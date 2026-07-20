use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct SavedProfile {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bucket: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct Config {
    #[serde(default)]
    pub profiles: Vec<SavedProfile>,
}

/// Path to the config file: `dirs::config_dir()/s3nav/config.toml`.
pub fn config_path() -> PathBuf {
    let base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("s3nav").join("config.toml")
}

/// Load configs from the default path. Missing file => empty config.
pub fn load() -> Result<Config, String> {
    load_from(&config_path())
}

pub fn load_from(path: &Path) -> Result<Config, String> {
    if !path.exists() {
        return Ok(Config::default());
    }
    let text = fs::read_to_string(path).map_err(|e| format!("Failed to read config: {e}"))?;
    toml::from_str(&text).map_err(|e| format!("Failed to parse config: {e}"))
}

/// Save configs to the default path, creating the parent directory.
pub fn save(config: &Config) -> Result<(), String> {
    save_to(&config_path(), config)
}

pub fn save_to(path: &Path, config: &Config) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Failed to create config dir: {e}"))?;
    }
    let text =
        toml::to_string_pretty(config).map_err(|e| format!("Failed to serialize config: {e}"))?;
    fs::write(path, text).map_err(|e| format!("Failed to write config: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_preserves_profiles() {
        let config = Config {
            profiles: vec![SavedProfile {
                name: "prod".into(),
                profile: Some("prod-acct".into()),
                region: Some("eu-west-1".into()),
                endpoint_url: None,
                bucket: Some("my-bucket/data".into()),
            }],
        };
        let text = toml::to_string_pretty(&config).unwrap();
        let parsed: Config = toml::from_str(&text).unwrap();
        assert_eq!(parsed, config);
    }

    #[test]
    fn load_from_missing_file_is_empty() {
        let path = std::env::temp_dir().join("s3nav-test-missing-abc123.toml");
        let _ = std::fs::remove_file(&path);
        let config = load_from(&path).unwrap();
        assert!(config.profiles.is_empty());
    }

    #[test]
    fn parses_minimal_profile() {
        let text = "[[profiles]]\nname = \"only\"\n";
        let config: Config = toml::from_str(text).unwrap();
        assert_eq!(config.profiles.len(), 1);
        assert_eq!(config.profiles[0].name, "only");
        assert_eq!(config.profiles[0].profile, None);
    }

    #[test]
    fn omits_none_fields_when_serializing() {
        let config = Config {
            profiles: vec![SavedProfile {
                name: "only".into(),
                ..Default::default()
            }],
        };
        let text = toml::to_string_pretty(&config).unwrap();
        assert!(!text.contains("profile ="));
        assert!(!text.contains("region ="));
    }
}
