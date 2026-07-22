use std::path::Path;

use aws_sdk_s3::Client;
use aws_sdk_s3::primitives::ByteStream;

use crate::Args;
use crate::config::SavedProfile;

/// Connection settings usable to build an S3 client, from either CLI args or a
/// saved profile.
#[derive(Clone, Debug, Default)]
pub struct ConnectionParams {
    pub profile: Option<String>,
    pub region: Option<String>,
    pub endpoint_url: Option<String>,
}

impl ConnectionParams {
    pub fn from_args(args: &Args) -> Self {
        Self {
            profile: args.profile.clone(),
            region: args.region.clone(),
            endpoint_url: args.endpoint_url.clone(),
        }
    }

    pub fn from_profile(p: &SavedProfile) -> Self {
        Self {
            profile: p.profile.clone(),
            region: p.region.clone(),
            endpoint_url: p.endpoint_url.clone(),
        }
    }

    /// One-line summary of the active connection for display in the header.
    /// Missing values fall back to "AWS" (endpoint) or "default" (region/profile).
    pub fn summary(&self) -> String {
        let endpoint = self.endpoint_url.as_deref().unwrap_or("AWS");
        let region = self.region.as_deref().unwrap_or("default");
        let profile = self.profile.as_deref().unwrap_or("default");
        format!("{endpoint} · {region} · {profile}")
    }
}

pub async fn create_client(params: &ConnectionParams) -> Client {
    let mut config_loader = aws_config::defaults(aws_config::BehaviorVersion::latest());

    if let Some(profile) = &params.profile {
        config_loader = config_loader.profile_name(profile);
    }

    if let Some(region) = &params.region {
        config_loader = config_loader.region(aws_config::Region::new(region.clone()));
    }

    if let Some(endpoint) = &params.endpoint_url {
        config_loader = config_loader.endpoint_url(endpoint);
    }

    let config = config_loader.load().await;
    Client::new(&config)
}

pub async fn list_buckets(client: &Client) -> Result<Vec<String>, String> {
    let output = client
        .list_buckets()
        .send()
        .await
        .map_err(|e| format!("Failed to list buckets: {e}"))?;

    Ok(output
        .buckets()
        .iter()
        .filter_map(|b| b.name().map(String::from))
        .collect())
}

pub async fn list_objects(
    client: &Client,
    bucket: &str,
    prefix: &str,
) -> Result<Vec<S3Entry>, String> {
    let mut entries = Vec::new();
    let mut continuation_token: Option<String> = None;

    loop {
        let mut req = client.list_objects_v2().bucket(bucket).delimiter("/");

        if !prefix.is_empty() {
            req = req.prefix(prefix);
        }
        if let Some(token) = continuation_token {
            req = req.continuation_token(token);
        }

        let resp = req
            .send()
            .await
            .map_err(|e| format!("Failed to list objects: {e}"))?;

        // Folders (common prefixes)
        for cp in resp.common_prefixes() {
            if let Some(p) = cp.prefix() {
                let name = p.strip_prefix(prefix).unwrap_or(p).trim_end_matches('/');
                if !name.is_empty() {
                    entries.push(S3Entry {
                        name: name.to_string(),
                        is_dir: true,
                        size: 0,
                        last_modified: None,
                        storage_class: None,
                        e_tag: None,
                    });
                }
            }
        }

        // Files (objects)
        for obj in resp.contents() {
            if let Some(key) = obj.key() {
                let name = key.strip_prefix(prefix).unwrap_or(key);
                // Skip the prefix itself (shows up as empty string)
                if !name.is_empty() && !name.ends_with('/') {
                    let last_modified = obj.last_modified().map(|dt| {
                        dt.fmt(aws_sdk_s3::primitives::DateTimeFormat::DateTime)
                            .unwrap_or_default()
                    });
                    let storage_class = obj.storage_class().map(|sc| sc.as_str().to_string());
                    let e_tag = obj.e_tag().map(|s| s.trim_matches('"').to_string());

                    entries.push(S3Entry {
                        name: name.to_string(),
                        is_dir: false,
                        size: obj.size().unwrap_or(0),
                        last_modified,
                        storage_class,
                        e_tag,
                    });
                }
            }
        }

        if resp.is_truncated() == Some(true) {
            continuation_token = resp.next_continuation_token().map(String::from);
        } else {
            break;
        }
    }

    entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.cmp(&b.name)));
    Ok(entries)
}

pub async fn get_object_bytes(client: &Client, bucket: &str, key: &str) -> Result<Vec<u8>, String> {
    let resp = client
        .get_object()
        .bucket(bucket)
        .key(key)
        .send()
        .await
        .map_err(|e| format!("Failed to get object: {e}"))?;

    let bytes = resp
        .body
        .collect()
        .await
        .map_err(|e| format!("Failed to read body: {e}"))?
        .into_bytes();

    Ok(bytes.to_vec())
}

pub async fn download_object(
    client: &Client,
    bucket: &str,
    key: &str,
    dest: &Path,
) -> Result<(), String> {
    let bytes = get_object_bytes(client, bucket, key).await?;
    std::fs::write(dest, &bytes).map_err(|e| format!("Failed to write file: {e}"))?;
    Ok(())
}

/// Path of `key` relative to `prefix`, or `None` when `key` is a "directory"
/// placeholder — the prefix itself or any key ending in `/`, which have no
/// local file to write.
fn relative_key(prefix: &str, key: &str) -> Option<String> {
    let rel = key.strip_prefix(prefix).unwrap_or(key);
    if rel.is_empty() || rel.ends_with('/') {
        return None;
    }
    Some(rel.to_string())
}

/// Recursively download every object under a prefix (i.e. a "folder") into
/// `dest_root`, recreating the subdirectory structure. Returns the number of
/// files written.
pub async fn download_prefix(
    client: &Client,
    bucket: &str,
    prefix: &str,
    dest_root: &Path,
) -> Result<u32, String> {
    let mut downloaded = 0u32;
    let mut continuation_token: Option<String> = None;

    loop {
        let mut req = client.list_objects_v2().bucket(bucket).prefix(prefix);
        if let Some(token) = continuation_token {
            req = req.continuation_token(token);
        }

        let resp = req
            .send()
            .await
            .map_err(|e| format!("Failed to list objects for download: {e}"))?;

        for obj in resp.contents() {
            let Some(key) = obj.key() else { continue };
            let Some(rel) = relative_key(prefix, key) else {
                continue;
            };
            let mut dest = dest_root.to_path_buf();
            for part in rel.split('/') {
                dest.push(part);
            }
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("Failed to create {}: {e}", parent.display()))?;
            }
            let bytes = get_object_bytes(client, bucket, key).await?;
            std::fs::write(&dest, &bytes)
                .map_err(|e| format!("Failed to write {}: {e}", dest.display()))?;
            downloaded += 1;
        }

        if resp.is_truncated() == Some(true) {
            continuation_token = resp.next_continuation_token().map(String::from);
        } else {
            break;
        }
    }

    Ok(downloaded)
}

pub async fn put_object(
    client: &Client,
    bucket: &str,
    key: &str,
    content: &str,
) -> Result<(), String> {
    client
        .put_object()
        .bucket(bucket)
        .key(key)
        .body(ByteStream::from(content.as_bytes().to_vec()))
        .content_type("text/plain; charset=utf-8")
        .send()
        .await
        .map_err(|e| format!("Failed to upload object: {e}"))?;
    Ok(())
}

pub async fn delete_object(client: &Client, bucket: &str, key: &str) -> Result<(), String> {
    client
        .delete_object()
        .bucket(bucket)
        .key(key)
        .send()
        .await
        .map_err(|e| format!("Failed to delete object: {e}"))?;
    Ok(())
}

/// Recursively delete all objects under a prefix (i.e. a "folder").
pub async fn delete_prefix(client: &Client, bucket: &str, prefix: &str) -> Result<u32, String> {
    let mut deleted = 0u32;
    let mut continuation_token: Option<String> = None;

    loop {
        let mut req = client.list_objects_v2().bucket(bucket).prefix(prefix);
        if let Some(token) = continuation_token {
            req = req.continuation_token(token);
        }

        let resp = req
            .send()
            .await
            .map_err(|e| format!("Failed to list objects for deletion: {e}"))?;

        for obj in resp.contents() {
            if let Some(key) = obj.key() {
                client
                    .delete_object()
                    .bucket(bucket)
                    .key(key)
                    .send()
                    .await
                    .map_err(|e| format!("Failed to delete {key}: {e}"))?;
                deleted += 1;
            }
        }

        if resp.is_truncated() == Some(true) {
            continuation_token = resp.next_continuation_token().map(String::from);
        } else {
            break;
        }
    }

    Ok(deleted)
}

pub async fn upload_file(
    client: &Client,
    bucket: &str,
    key: &str,
    path: &Path,
) -> Result<(), String> {
    let body = ByteStream::from_path(path)
        .await
        .map_err(|e| format!("Failed to read file: {e}"))?;

    client
        .put_object()
        .bucket(bucket)
        .key(key)
        .body(body)
        .send()
        .await
        .map_err(|e| format!("Failed to upload: {e}"))?;

    Ok(())
}

const TEXT_EXTENSIONS: &[&str] = &[
    "txt",
    "json",
    "yaml",
    "yml",
    "xml",
    "csv",
    "tsv",
    "md",
    "markdown",
    "html",
    "htm",
    "css",
    "js",
    "ts",
    "jsx",
    "tsx",
    "py",
    "rb",
    "rs",
    "go",
    "java",
    "c",
    "h",
    "cpp",
    "hpp",
    "cs",
    "sh",
    "bash",
    "zsh",
    "fish",
    "toml",
    "ini",
    "cfg",
    "conf",
    "properties",
    "env",
    "log",
    "sql",
    "graphql",
    "gql",
    "proto",
    "tf",
    "hcl",
    "lua",
    "pl",
    "pm",
    "r",
    "scala",
    "kt",
    "kts",
    "swift",
    "m",
    "mm",
    "zig",
    "nim",
    "ex",
    "exs",
    "erl",
    "hrl",
    "hs",
    "ml",
    "mli",
    "lisp",
    "cl",
    "el",
    "clj",
    "cljs",
    "cljc",
    "edn",
    "svelte",
    "vue",
    "php",
    "twig",
    "erb",
    "haml",
    "slim",
    "pug",
    "jade",
    "sass",
    "scss",
    "less",
    "styl",
    "dockerfile",
    "makefile",
    "cmake",
    "gitignore",
    "gitattributes",
    "editorconfig",
    "prettierrc",
    "eslintrc",
    "babelrc",
];

pub fn is_text_file(name: &str) -> bool {
    let lower = name.to_lowercase();
    // Files with no extension but known names
    let basename = lower.rsplit('/').next().unwrap_or(&lower);
    if matches!(
        basename,
        "dockerfile" | "makefile" | "rakefile" | "gemfile" | "procfile" | "license" | "readme"
    ) {
        return true;
    }
    // Check extension
    if let Some(ext) = lower.rsplit('.').next() {
        TEXT_EXTENSIONS.contains(&ext)
    } else {
        false
    }
}

#[derive(Clone, Debug)]
pub struct S3Entry {
    pub name: String,
    pub is_dir: bool,
    pub size: i64,
    pub last_modified: Option<String>,
    pub storage_class: Option<String>,
    pub e_tag: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Args;
    use crate::config::SavedProfile;

    #[test]
    fn connection_params_from_profile_maps_fields() {
        let p = SavedProfile {
            name: "x".into(),
            profile: Some("prod".into()),
            region: Some("eu-west-1".into()),
            endpoint_url: None,
            bucket: Some("b".into()),
        };
        let c = ConnectionParams::from_profile(&p);
        assert_eq!(c.profile.as_deref(), Some("prod"));
        assert_eq!(c.region.as_deref(), Some("eu-west-1"));
        assert_eq!(c.endpoint_url, None);
    }

    #[test]
    fn connection_params_from_args_maps_fields() {
        let args = Args {
            region: Some("us-east-1".into()),
            profile: Some("dev".into()),
            endpoint_url: Some("http://localhost:9000".into()),
            bucket: Some("ignored".into()),
        };
        let c = ConnectionParams::from_args(&args);
        assert_eq!(c.profile.as_deref(), Some("dev"));
        assert_eq!(c.region.as_deref(), Some("us-east-1"));
        assert_eq!(c.endpoint_url.as_deref(), Some("http://localhost:9000"));
    }

    #[test]
    fn summary_uses_values_when_present() {
        let c = ConnectionParams {
            profile: Some("dev".into()),
            region: Some("eu-west-1".into()),
            endpoint_url: Some("http://localhost:9000".into()),
        };
        assert_eq!(c.summary(), "http://localhost:9000 · eu-west-1 · dev");
    }

    #[test]
    fn summary_falls_back_to_defaults_when_absent() {
        let c = ConnectionParams::default();
        assert_eq!(c.summary(), "AWS · default · default");
    }

    #[test]
    fn relative_key_strips_prefix_and_skips_markers() {
        // Folder marker equal to the prefix has no file to write.
        assert_eq!(relative_key("foo/bar/", "foo/bar/"), None);
        // A nested "directory" placeholder is skipped too.
        assert_eq!(relative_key("foo/bar/", "foo/bar/baz/"), None);
        // A file directly in the folder keeps just its name.
        assert_eq!(
            relative_key("foo/bar/", "foo/bar/file.txt").as_deref(),
            Some("file.txt")
        );
        // A nested file keeps its relative subpath.
        assert_eq!(
            relative_key("foo/bar/", "foo/bar/baz/x.txt").as_deref(),
            Some("baz/x.txt")
        );
    }
}
