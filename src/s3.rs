use std::path::Path;

use aws_sdk_s3::Client;

use crate::Args;

pub async fn create_client(args: &Args) -> Client {
    let mut config_loader = aws_config::defaults(aws_config::BehaviorVersion::latest()).region(
        aws_config::Region::new(args.region.clone()),
    );

    if let Some(endpoint) = &args.endpoint_url {
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
        let mut req = client
            .list_objects_v2()
            .bucket(bucket)
            .delimiter("/");

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
                let name = p
                    .strip_prefix(prefix)
                    .unwrap_or(p)
                    .trim_end_matches('/');
                if !name.is_empty() {
                    entries.push(S3Entry {
                        name: name.to_string(),
                        is_dir: true,
                        size: 0,
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
                    entries.push(S3Entry {
                        name: name.to_string(),
                        is_dir: false,
                        size: obj.size().unwrap_or(0),
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

pub async fn get_object_bytes(
    client: &Client,
    bucket: &str,
    key: &str,
) -> Result<Vec<u8>, String> {
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

const TEXT_EXTENSIONS: &[&str] = &[
    "txt", "json", "yaml", "yml", "xml", "csv", "tsv", "md", "markdown",
    "html", "htm", "css", "js", "ts", "jsx", "tsx", "py", "rb", "rs",
    "go", "java", "c", "h", "cpp", "hpp", "cs", "sh", "bash", "zsh",
    "fish", "toml", "ini", "cfg", "conf", "properties", "env", "log",
    "sql", "graphql", "gql", "proto", "tf", "hcl", "lua", "pl", "pm",
    "r", "scala", "kt", "kts", "swift", "m", "mm", "zig", "nim", "ex",
    "exs", "erl", "hrl", "hs", "ml", "mli", "lisp", "cl", "el", "clj",
    "cljs", "cljc", "edn", "svelte", "vue", "php", "twig", "erb",
    "haml", "slim", "pug", "jade", "sass", "scss", "less", "styl",
    "dockerfile", "makefile", "cmake", "gitignore", "gitattributes",
    "editorconfig", "prettierrc", "eslintrc", "babelrc",
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
}
