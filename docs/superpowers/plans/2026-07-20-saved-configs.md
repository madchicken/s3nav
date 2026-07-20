# Saved Configurations Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let users persist named connection configurations (AWS profile + region + endpoint + initial bucket) and pick, create, or delete them from the TUI.

**Architecture:** A new `config.rs` module owns TOML persistence (no secrets stored — profiles are referenced by name). `s3::create_client` is refactored to take a `ConnectionParams` carrier so both CLI `Args` and a saved profile can build a client. `app.rs` gains two views (`ConfigSelector`, `ConfigForm`) and the ability to rebuild its client when a config is applied. `main.rs` routes to the selector at startup when no CLI connection flags are passed and at least one config exists.

**Tech Stack:** Rust (edition 2024), ratatui 0.29, aws-sdk-s3, clap, serde + toml (new), dirs.

## Global Constraints

- No secrets on disk: a `SavedProfile` stores only `name`, `profile` (AWS profile name), `region`, `endpoint_url`, `bucket`. Never access key / secret.
- Config file location: `dirs::config_dir()/s3nav/config.toml`.
- Follow existing code style: flat `View` enum, per-view key handlers named `handle_<view>_key`, footer success detection keys off message prefixes (`"Saved "`, `"Deleted "`, `"Created "`, etc.).
- Verify every task with `cargo build`, `cargo clippy` (no new warnings), and `cargo fmt`. Tasks with pure logic also add `cargo test`.
- TUI rendering/interaction has no automated test harness in this repo — verify those parts by build + clippy + the manual smoke check described in the task.

---

## File Structure

- Create: `src/config.rs` — `SavedProfile`, `Config`, `config_path`, `load`/`load_from`, `save`/`save_to`, unit tests.
- Modify: `src/s3.rs` — add `ConnectionParams` + conversions; change `create_client` signature; unit tests.
- Modify: `src/main.rs` — register `mod config`; build `ConnectionParams`; load configs; startup routing.
- Modify: `src/app.rs` — `View::ConfigSelector`/`View::ConfigForm`; `ConfigForm` struct; new `App` state and handlers; `apply_config`; `set_bucket_from_arg` helper; unit test for `ConfigForm::to_profile`.
- Modify: `src/ui.rs` — header/footer/list arms for the two new views; `draw_config_form`.
- Modify: `Cargo.toml` — add `serde`, `toml`.

---

### Task 1: `config.rs` persistence module

**Files:**
- Create: `src/config.rs`
- Modify: `src/main.rs:1-3` (add `mod config;`)
- Modify: `Cargo.toml:13-23` (add dependencies)

**Interfaces:**
- Produces:
  - `struct SavedProfile { pub name: String, pub profile: Option<String>, pub region: Option<String>, pub endpoint_url: Option<String>, pub bucket: Option<String> }` — derives `Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq`.
  - `struct Config { pub profiles: Vec<SavedProfile> }` — same derives.
  - `pub fn config_path() -> std::path::PathBuf`
  - `pub fn load() -> Result<Config, String>` and `pub fn load_from(path: &Path) -> Result<Config, String>`
  - `pub fn save(config: &Config) -> Result<(), String>` and `pub fn save_to(path: &Path, config: &Config) -> Result<(), String>`

- [ ] **Step 1: Add dependencies**

Run:
```bash
cargo add serde --features derive && cargo add toml
```
Expected: `Cargo.toml` gains `serde = { version = "...", features = ["derive"] }` and `toml = "..."`. (Use whatever versions resolve; do not hand-pin.)

- [ ] **Step 2: Create `src/config.rs` with the module and failing tests**

Create `src/config.rs`:

```rust
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
```

- [ ] **Step 3: Register the module in `main.rs`**

In `src/main.rs`, the current top is:
```rust
mod app;
mod s3;
mod ui;
```
Change to:
```rust
mod app;
mod config;
mod s3;
mod ui;
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test config::`
Expected: PASS — `round_trip_preserves_profiles`, `load_from_missing_file_is_empty`, `parses_minimal_profile`, `omits_none_fields_when_serializing`.

- [ ] **Step 5: Build and lint**

Run: `cargo build && cargo clippy && cargo fmt`
Expected: builds clean, no new clippy warnings. (`config` module is unused by non-test code so far — that is expected; Task 2/3 wire it in. If clippy flags dead code, proceed; it resolves in later tasks.)

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock src/config.rs src/main.rs
git commit -m "feat: add config persistence module for saved profiles"
```

---

### Task 2: `ConnectionParams` — decouple client creation from `Args`

**Files:**
- Modify: `src/s3.rs:1-25` (add struct + conversions, change `create_client`)
- Modify: `src/main.rs:29-40` (update call site)

**Interfaces:**
- Consumes: `crate::config::SavedProfile` (Task 1), `crate::Args` (existing, in `main.rs`).
- Produces:
  - `struct ConnectionParams { pub profile: Option<String>, pub region: Option<String>, pub endpoint_url: Option<String> }` — derives `Clone, Debug, Default`.
  - `impl ConnectionParams { pub fn from_args(args: &Args) -> Self; pub fn from_profile(p: &SavedProfile) -> Self }`
  - `pub async fn create_client(params: &ConnectionParams) -> Client` (signature change from `&Args`).

- [ ] **Step 1: Write failing conversion tests**

Append to `src/s3.rs`:

```rust
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
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test s3::tests`
Expected: FAIL to compile — `ConnectionParams` not found.

- [ ] **Step 3: Add `ConnectionParams` and refactor `create_client`**

In `src/s3.rs`, the current head is:
```rust
use std::path::Path;

use aws_sdk_s3::Client;
use aws_sdk_s3::primitives::ByteStream;

use crate::Args;

pub async fn create_client(args: &Args) -> Client {
    let mut config_loader = aws_config::defaults(aws_config::BehaviorVersion::latest());

    if let Some(profile) = &args.profile {
        config_loader = config_loader.profile_name(profile);
    }

    if let Some(region) = &args.region {
        config_loader = config_loader.region(aws_config::Region::new(region.clone()));
    }

    if let Some(endpoint) = &args.endpoint_url {
        config_loader = config_loader.endpoint_url(endpoint);
    }

    let config = config_loader.load().await;
    Client::new(&config)
}
```
Replace it with:
```rust
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
```

- [ ] **Step 4: Update the `main.rs` call site**

In `src/main.rs`, the current body is:
```rust
    let args = Args::parse();
    let client = s3::create_client(&args).await;
```
Change to:
```rust
    let args = Args::parse();
    let connection = s3::ConnectionParams::from_args(&args);
    let client = s3::create_client(&connection).await;
```
(The `connection` value is consumed further in Task 3; leaving it bound now is fine — if clippy warns "unused", that resolves in Task 3. To avoid a warning in the interim, you may temporarily pass `&connection` only; the App wiring lands in Task 3.)

- [ ] **Step 5: Run tests and build**

Run: `cargo test s3::tests && cargo build && cargo clippy && cargo fmt`
Expected: tests PASS, build clean.

- [ ] **Step 6: Commit**

```bash
git add src/s3.rs src/main.rs
git commit -m "refactor: build S3 client from ConnectionParams instead of Args"
```

---

### Task 3: ConfigSelector view — list, apply, delete, startup routing

**Files:**
- Modify: `src/app.rs` — `View` enum (L14-24), imports (L1-11), `App` struct fields (L26-62), `App::new` (L72-104), `item_count` (L110-117), `run` (L119-151), `handle_key` (L153-168); add `handle_config_selector_key`, `apply_config`, `set_bucket_from_arg`, `delete_selected_config`.
- Modify: `src/main.rs:29-40` — load configs, decide start view.
- Modify: `src/ui.rs` — `draw_header` (L41-76), `draw_list` (L78-134), `draw_footer` (L322-462).

**Interfaces:**
- Consumes: `config::{load, save, Config, SavedProfile}` (Task 1); `s3::{ConnectionParams, create_client}` (Task 2).
- Produces (used by Task 4):
  - `App` fields: `pub configs: Vec<SavedProfile>`, `pub connection: ConnectionParams`, `pub config_delete_pending: bool`.
  - `View::ConfigSelector`, `View::ConfigForm` (the `ConfigForm` variant is added here so match arms stay exhaustive; its handler is filled in Task 4).
  - `App::new(client: Client, connection: ConnectionParams, initial_bucket: Option<String>, configs: Vec<SavedProfile>) -> Self`
  - `fn set_bucket_from_arg(&mut self, bucket_arg: String)`
  - `async fn apply_config(&mut self, profile: SavedProfile, terminal: &mut DefaultTerminal) -> Result<()>`

- [ ] **Step 1: Add the two `View` variants**

In `src/app.rs`, the `View` enum currently ends:
```rust
    CreateFile,
    FilePicker,
}
```
Change to:
```rust
    CreateFile,
    FilePicker,
    ConfigSelector,
    ConfigForm,
}
```

- [ ] **Step 2: Extend imports and `App` fields**

In `src/app.rs`, the import block currently reads:
```rust
use crate::s3::{self, S3Entry};
use crate::ui;
```
Change to:
```rust
use crate::config::{self, SavedProfile};
use crate::s3::{self, ConnectionParams, S3Entry};
use crate::ui;
```

In the `App` struct, after the existing `pub picker_state: ListState,` field (L61), add:
```rust

    // Saved configurations
    pub configs: Vec<SavedProfile>,
    pub connection: ConnectionParams,
    pub config_delete_pending: bool,
    pub config_form: ConfigForm,
```
(`ConfigForm` is defined in Task 4. To keep Task 3 compiling on its own, add a minimal placeholder now and flesh it out in Task 4. Add this near `LocalEntry`, after its definition (L65-69):
```rust

#[derive(Default)]
pub struct ConfigForm {
    pub name: String,
    pub profile: String,
    pub region: String,
    pub endpoint_url: String,
    pub bucket: String,
    pub field: usize,
}
```
)

- [ ] **Step 3: Update `App::new`**

Change the signature and initializer. Current:
```rust
    pub fn new(client: Client, initial_bucket: Option<String>) -> Self {
        Self {
            client,
            ...
            picker_state: ListState::default(),
        }
    }
```
Change the signature to:
```rust
    pub fn new(
        client: Client,
        connection: ConnectionParams,
        initial_bucket: Option<String>,
        configs: Vec<SavedProfile>,
    ) -> Self {
```
and add these fields to the struct literal right after `picker_state: ListState::default(),`:
```rust
            configs,
            connection,
            config_delete_pending: false,
            config_form: ConfigForm::default(),
```

- [ ] **Step 4: Add the `ConfigSelector` arm to `item_count`**

Current:
```rust
    pub fn item_count(&self) -> usize {
        match self.view {
            View::Buckets => self.buckets.len(),
            View::Objects => self.entries.len(),
            View::FilePicker => self.picker_entries.len(),
            _ => 0,
        }
    }
```
Change the match to add:
```rust
            View::ConfigSelector => self.configs.len(),
```
(before the `_ => 0,` arm).

- [ ] **Step 5: Add `set_bucket_from_arg` helper and refactor `run`**

Add this method inside `impl App` (e.g. right after `current_prefix`):
```rust
    /// Parse a `bucket` or `bucket/prefix` argument into navigation state.
    fn set_bucket_from_arg(&mut self, bucket_arg: String) {
        let (bucket, prefix) = match bucket_arg.split_once('/') {
            Some((b, p)) => (b.to_string(), p.trim_matches('/').to_string()),
            None => (bucket_arg, String::new()),
        };
        self.current_bucket = bucket;
        self.prefix_stack.clear();
        self.prefix_stack.push(String::new());
        if !prefix.is_empty() {
            self.prefix_stack.push(format!("{prefix}/"));
        }
    }
```

In `run`, the current startup block is:
```rust
        self.loading = true;
        terminal.draw(|frame| ui::draw(frame, &mut self))?;

        if let Some(bucket_arg) = self.initial_bucket.take() {
            let (bucket, prefix) = match bucket_arg.split_once('/') {
                Some((b, p)) => (b.to_string(), p.trim_matches('/').to_string()),
                None => (bucket_arg, String::new()),
            };
            self.current_bucket = bucket;
            self.prefix_stack.push(String::new());
            if !prefix.is_empty() {
                self.prefix_stack.push(format!("{prefix}/"));
            }
            self.view = View::Objects;
            self.load_objects(&mut terminal).await?;
        } else {
            self.load_buckets(&mut terminal).await?;
        }
```
Replace with:
```rust
        self.loading = true;
        terminal.draw(|frame| ui::draw(frame, &mut self))?;

        if self.view == View::ConfigSelector {
            // Nothing to fetch yet; the selector is populated from self.configs.
            self.loading = false;
            if !self.configs.is_empty() {
                self.list_state.select(Some(0));
            }
        } else if let Some(bucket_arg) = self.initial_bucket.take() {
            self.set_bucket_from_arg(bucket_arg);
            self.view = View::Objects;
            self.load_objects(&mut terminal).await?;
        } else {
            self.load_buckets(&mut terminal).await?;
        }
```

- [ ] **Step 6: Dispatch keys for the new views**

In `handle_key`, the current match is:
```rust
        match self.view {
            View::FilePreview => self.handle_preview_key(key.code),
            ...
            View::FilePicker => self.handle_picker_key(key.code, terminal).await?,
            _ => {
                self.error = None;
                self.handle_list_key(key.code, terminal).await?;
            }
        }
```
Add these arms before the `_ =>` arm:
```rust
            View::ConfigSelector => self.handle_config_selector_key(key.code, terminal).await?,
            View::ConfigForm => self.handle_config_form_key(key, terminal).await?,
```
(`handle_config_form_key` is implemented in Task 4. To keep Task 3 compiling — and to avoid trapping the user in the form before Task 4 lands — add a stub now that at least handles `Esc`:
```rust
    async fn handle_config_form_key(
        &mut self,
        key: KeyEvent,
        _terminal: &mut DefaultTerminal,
    ) -> Result<()> {
        if key.code == KeyCode::Esc {
            self.view = View::ConfigSelector;
        }
        Ok(())
    }
```
Task 4 replaces this stub body.)

- [ ] **Step 7: Implement selector handler, `apply_config`, and delete**

Add these methods inside `impl App`:
```rust
    async fn handle_config_selector_key(
        &mut self,
        code: KeyCode,
        terminal: &mut DefaultTerminal,
    ) -> Result<()> {
        if self.config_delete_pending {
            match code {
                KeyCode::Char('y') | KeyCode::Char('Y') => self.delete_selected_config(),
                _ => self.config_delete_pending = false,
            }
            return Ok(());
        }
        match code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_exit = true,
            KeyCode::Down | KeyCode::Char('j') => self.select_next(),
            KeyCode::Up | KeyCode::Char('k') => self.select_previous(),
            KeyCode::Home | KeyCode::Char('g') => self.select_first(),
            KeyCode::End | KeyCode::Char('G') => self.select_last(),
            KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => {
                if let Some(i) = self.list_state.selected() {
                    if let Some(profile) = self.configs.get(i).cloned() {
                        self.error = None;
                        self.apply_config(profile, terminal).await?;
                    }
                }
            }
            KeyCode::Char('n') => {
                self.config_form = ConfigForm::default();
                self.error = None;
                self.view = View::ConfigForm;
            }
            KeyCode::Char('d') | KeyCode::Delete => {
                if !self.configs.is_empty() {
                    self.config_delete_pending = true;
                }
            }
            _ => {}
        }
        Ok(())
    }

    async fn apply_config(
        &mut self,
        profile: SavedProfile,
        terminal: &mut DefaultTerminal,
    ) -> Result<()> {
        self.connection = ConnectionParams::from_profile(&profile);
        self.client = s3::create_client(&self.connection).await;
        self.buckets.clear();
        self.entries.clear();
        self.prefix_stack.clear();
        self.list_state.select(None);

        match profile.bucket.filter(|b| !b.is_empty()) {
            Some(bucket_arg) => {
                self.set_bucket_from_arg(bucket_arg);
                self.view = View::Objects;
                self.load_objects(terminal).await?;
            }
            None => {
                self.view = View::Buckets;
                self.load_buckets(terminal).await?;
            }
        }
        Ok(())
    }

    fn delete_selected_config(&mut self) {
        self.config_delete_pending = false;
        let Some(i) = self.list_state.selected() else {
            return;
        };
        if i >= self.configs.len() {
            return;
        }
        let removed = self.configs.remove(i);
        let config = config::Config {
            profiles: self.configs.clone(),
        };
        match config::save(&config) {
            Ok(()) => {
                self.error = Some(format!("Deleted config {}", removed.name));
                if self.configs.is_empty() {
                    self.list_state.select(None);
                } else {
                    self.list_state.select(Some(i.min(self.configs.len() - 1)));
                }
            }
            Err(e) => {
                self.configs.insert(i, removed);
                self.error = Some(e);
            }
        }
    }
```

- [ ] **Step 8: Wire startup routing in `main.rs`**

Replace the body of `main` (from `let args = Args::parse();` through `result`) with:
```rust
    let args = Args::parse();

    let (configs, load_err) = match config::load() {
        Ok(c) => (c.profiles, None),
        Err(e) => (Vec::new(), Some(e)),
    };

    let cli_flags = args.profile.is_some()
        || args.region.is_some()
        || args.endpoint_url.is_some()
        || args.bucket.is_some();
    let start_in_selector = !cli_flags && !configs.is_empty();

    let connection = s3::ConnectionParams::from_args(&args);
    let client = s3::create_client(&connection).await;

    let terminal = ratatui::init();
    let mut app = app::App::new(client, connection, args.bucket, configs);
    if start_in_selector {
        app.view = app::View::ConfigSelector;
    }
    app.error = load_err;
    let result = app.run(terminal).await;
    ratatui::restore();

    result
```

- [ ] **Step 9: Render the selector — header, list, footer**

In `src/ui.rs` `draw_header`, the `match &app.view` is exhaustive. Add these arms (e.g. after the `View::FilePicker` arm):
```rust
        View::ConfigSelector => " Select configuration".to_string(),
        View::ConfigForm => " New configuration".to_string(),
```

In `draw_list`, the item-building `match &app.view` currently has `View::Buckets`, the `View::Objects | ...` group, and `_ => vec![]`. Add a `ConfigSelector` arm before `_ => vec![]`:
```rust
        View::ConfigSelector => app
            .configs
            .iter()
            .map(|c| {
                let mut detail = Vec::new();
                if let Some(p) = &c.profile {
                    detail.push(format!("profile={p}"));
                }
                if let Some(r) = &c.region {
                    detail.push(format!("region={r}"));
                }
                if let Some(b) = &c.bucket {
                    detail.push(format!("bucket={b}"));
                }
                ListItem::new(Line::from(vec![
                    Span::styled("  ", Style::default().fg(Color::Yellow)),
                    Span::styled(
                        &c.name,
                        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        if detail.is_empty() {
                            String::new()
                        } else {
                            format!("  ({})", detail.join(", "))
                        },
                        Style::default().fg(Color::DarkGray),
                    ),
                ]))
            })
            .collect(),
```

In `draw_footer`, add a delete-confirm early return near the top (after the existing `DeleteConfirm` block):
```rust
    if app.config_delete_pending {
        let name = app
            .list_state
            .selected()
            .and_then(|i| app.configs.get(i))
            .map(|c| c.name.as_str())
            .unwrap_or("");
        let prompt = Paragraph::new(Line::from(vec![
            Span::styled(
                format!(" Delete config \"{name}\"? "),
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ),
            Span::styled("y", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
            Span::raw(" confirm  "),
            Span::styled("any key", Style::default().fg(Color::Cyan)),
            Span::raw(" cancel"),
        ]));
        frame.render_widget(prompt, area);
        return;
    }
```
And add a `ConfigSelector` help arm to the final `match &app.view` (before `_ =>`):
```rust
        View::ConfigSelector => Paragraph::new(Line::from(vec![
            Span::styled(" ↑↓/jk", Style::default().fg(Color::Cyan)),
            Span::raw(" navigate  "),
            Span::styled("Enter/l", Style::default().fg(Color::Cyan)),
            Span::raw(" connect  "),
            Span::styled("n", Style::default().fg(Color::Cyan)),
            Span::raw(" new  "),
            Span::styled("d", Style::default().fg(Color::Cyan)),
            Span::raw(" delete  "),
            Span::styled("q", Style::default().fg(Color::Cyan)),
            Span::raw(" quit"),
        ])),
```

- [ ] **Step 10: Build, lint, smoke test**

Run: `cargo build && cargo clippy && cargo fmt`
Expected: clean build.

Manual smoke check (requires AWS credentials in env):
```bash
# 1) Write a throwaway config so the selector appears:
mkdir -p "$(cargo run -q -- --help >/dev/null 2>&1; echo "${XDG_CONFIG_HOME:-$HOME/.config}")/s3nav"
# (Or just create one via the form in Task 4 once implemented.)
cargo run
```
Expected: with at least one saved config and no CLI flags, the app opens on "Select configuration"; ↑↓ moves selection, Enter connects (loads buckets or the config's bucket), `d` then `y` deletes. With `--bucket X` or `--profile Y`, the selector is skipped (unchanged behavior). With no configs, it goes straight to the bucket list.

- [ ] **Step 11: Commit**

```bash
git add src/app.rs src/main.rs src/ui.rs
git commit -m "feat: add config selector view with apply and delete"
```

---

### Task 4: ConfigForm view — create config and save current session

**Files:**
- Modify: `src/app.rs` — flesh out `ConfigForm` (impl + `to_profile`), replace `handle_config_form_key` stub, add `save_config_form`; add `s` binding in `handle_list_key` (L170-217); unit test for `ConfigForm::to_profile`.
- Modify: `src/ui.rs` — add `draw_config_form`, dispatch it in `draw` (L23-36), add `ConfigForm` footer help.

**Interfaces:**
- Consumes: `ConfigForm` struct (Task 3), `config::{save, Config}`, `App.configs`, `App.connection` (Task 3).
- Produces: `ConfigForm::{to_profile, from_session, next_field, prev_field, active_buf, clear}`; `App::save_config_form`.

- [ ] **Step 1: Write the failing `to_profile` test**

Add to `src/app.rs` (a `#[cfg(test)]` module at the end of the file, or extend an existing one):
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_form_to_profile_trims_and_nullifies_empty() {
        let form = ConfigForm {
            name: "  prod ".into(),
            profile: "   ".into(),
            region: "eu-west-1".into(),
            endpoint_url: String::new(),
            bucket: "b/p".into(),
            field: 0,
        };
        let p = form.to_profile();
        assert_eq!(p.name, "prod");
        assert_eq!(p.profile, None);
        assert_eq!(p.region.as_deref(), Some("eu-west-1"));
        assert_eq!(p.endpoint_url, None);
        assert_eq!(p.bucket.as_deref(), Some("b/p"));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test app::tests::config_form_to_profile`
Expected: FAIL to compile — no method `to_profile`.

- [ ] **Step 3: Implement `ConfigForm` methods**

Add an `impl ConfigForm` block in `src/app.rs` (right after the `ConfigForm` struct from Task 3):
```rust
impl ConfigForm {
    const FIELDS: usize = 5;

    fn next_field(&mut self) {
        self.field = (self.field + 1) % Self::FIELDS;
    }

    fn prev_field(&mut self) {
        self.field = (self.field + Self::FIELDS - 1) % Self::FIELDS;
    }

    fn active_buf(&mut self) -> &mut String {
        match self.field {
            0 => &mut self.name,
            1 => &mut self.profile,
            2 => &mut self.region,
            3 => &mut self.endpoint_url,
            _ => &mut self.bucket,
        }
    }

    fn clear(&mut self) {
        *self = Self::default();
    }

    fn to_profile(&self) -> SavedProfile {
        let opt = |s: &str| {
            let t = s.trim();
            if t.is_empty() {
                None
            } else {
                Some(t.to_string())
            }
        };
        SavedProfile {
            name: self.name.trim().to_string(),
            profile: opt(&self.profile),
            region: opt(&self.region),
            endpoint_url: opt(&self.endpoint_url),
            bucket: opt(&self.bucket),
        }
    }

    /// Pre-fill from the currently active session (for "save current session").
    fn from_session(conn: &ConnectionParams, bucket: &str, prefix: &str) -> Self {
        let bucket_field = if bucket.is_empty() {
            String::new()
        } else if prefix.is_empty() {
            bucket.to_string()
        } else {
            format!("{}/{}", bucket, prefix.trim_end_matches('/'))
        };
        Self {
            name: String::new(),
            profile: conn.profile.clone().unwrap_or_default(),
            region: conn.region.clone().unwrap_or_default(),
            endpoint_url: conn.endpoint_url.clone().unwrap_or_default(),
            bucket: bucket_field,
            field: 0,
        }
    }
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test app::tests::config_form_to_profile`
Expected: PASS.

- [ ] **Step 5: Replace the `handle_config_form_key` stub and add `save_config_form`**

Replace the Task-3 stub `handle_config_form_key` with:
```rust
    async fn handle_config_form_key(
        &mut self,
        key: KeyEvent,
        terminal: &mut DefaultTerminal,
    ) -> Result<()> {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('s') {
            self.save_config_form(terminal).await?;
            return Ok(());
        }
        match key.code {
            KeyCode::Esc => {
                self.config_form.clear();
                self.error = None;
                self.view = View::ConfigSelector;
            }
            KeyCode::Tab | KeyCode::Down => self.config_form.next_field(),
            KeyCode::BackTab | KeyCode::Up => self.config_form.prev_field(),
            KeyCode::Enter => self.save_config_form(terminal).await?,
            KeyCode::Backspace => {
                self.config_form.active_buf().pop();
            }
            KeyCode::Char(c) => {
                self.config_form.active_buf().push(c);
            }
            _ => {}
        }
        Ok(())
    }

    async fn save_config_form(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        let profile = self.config_form.to_profile();
        if profile.name.is_empty() {
            self.error = Some("Config name is required".into());
            return Ok(());
        }

        let mut profiles = self.configs.clone();
        if let Some(existing) = profiles.iter_mut().find(|p| p.name == profile.name) {
            *existing = profile.clone();
        } else {
            profiles.push(profile.clone());
        }
        let config = config::Config {
            profiles: profiles.clone(),
        };

        match config::save(&config) {
            Ok(()) => {
                self.configs = profiles;
                self.error = Some(format!("Saved config {}", profile.name));
                self.config_form.clear();
                self.view = View::ConfigSelector;
                if let Some(i) = self.configs.iter().position(|p| p.name == profile.name) {
                    self.list_state.select(Some(i));
                }
            }
            Err(e) => self.error = Some(e),
        }
        // Redraw promptly so the result is visible.
        terminal.draw(|frame| ui::draw(frame, self))?;
        Ok(())
    }
```

- [ ] **Step 6: Add the `s` binding to save the current session**

In `handle_list_key`, add a new arm (e.g. after the `KeyCode::Char('c')` block):
```rust
            KeyCode::Char('s') => {
                if self.view == View::Buckets {
                    let prefix = self.current_prefix();
                    self.config_form = ConfigForm::from_session(
                        &self.connection,
                        &self.current_bucket,
                        &prefix,
                    );
                    self.error = None;
                    self.view = View::ConfigForm;
                }
            }
```
Note: `s` is only meaningful in `Buckets` (guarded). It does not collide — `Objects` uses `n`/`c`/`u`/`d`/`r`, none of which is `s`.

- [ ] **Step 7: Render the form — `draw` dispatch, `draw_config_form`, footer help**

In `src/ui.rs` `draw`, the current match has an arm `View::FilePicker => draw_file_picker(...)`. Add:
```rust
        View::ConfigForm => draw_config_form(frame, app, main_area),
```
(This keeps `ConfigForm` out of the `_ => draw_list` fallback.)

Add this function to `src/ui.rs`:
```rust
fn draw_config_form(frame: &mut Frame, app: &mut App, area: ratatui::layout::Rect) {
    let labels = ["Name", "AWS profile", "Region", "Endpoint URL", "Bucket/prefix"];
    let values = [
        &app.config_form.name,
        &app.config_form.profile,
        &app.config_form.region,
        &app.config_form.endpoint_url,
        &app.config_form.bucket,
    ];

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(""));
    for (i, (label, value)) in labels.iter().zip(values.iter()).enumerate() {
        let active = i == app.config_form.field;
        let marker = if active { "▶ " } else { "  " };
        let label_style = if active {
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Gray)
        };
        let cursor = if active { "█" } else { "" };
        lines.push(Line::from(vec![
            Span::styled(marker, Style::default().fg(Color::Cyan)),
            Span::styled(format!("{label:>13}: "), label_style),
            Span::raw(value.as_str()),
            Span::styled(cursor, Style::default().fg(Color::White)),
        ]));
        lines.push(Line::from(""));
    }
    lines.push(Line::from(Span::styled(
        "  Name is required. profile/region/endpoint/bucket are optional.",
        Style::default().fg(Color::DarkGray),
    )));

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(" Configuration ");
    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(paragraph, area);
}
```

Add a `ConfigForm` help arm to the final `match &app.view` in `draw_footer` (before `_ =>`):
```rust
        View::ConfigForm => Paragraph::new(Line::from(vec![
            Span::styled(" Tab/↑↓", Style::default().fg(Color::Cyan)),
            Span::raw(" field  "),
            Span::styled("Enter/Ctrl+S", Style::default().fg(Color::Cyan)),
            Span::raw(" save  "),
            Span::styled("Esc", Style::default().fg(Color::Cyan)),
            Span::raw(" cancel"),
        ])),
```

- [ ] **Step 8: Build, lint, test, smoke check**

Run: `cargo test && cargo build && cargo clippy && cargo fmt`
Expected: all tests PASS, clean build.

Manual smoke check:
```bash
cargo run
```
Expected: With no CLI flags and an existing config, opens the selector. Press `n` → the form appears with 5 fields; Tab/↑↓ moves the highlighted field; typing edits it; leaving Name empty and pressing Enter shows "Config name is required"; filling Name and pressing Enter (or Ctrl+S) writes `config.toml`, returns to the selector with the new entry selected and a green "Saved config …". From the bucket list, `s` opens the form pre-filled with the current profile/region/endpoint/bucket. Confirm the file at `dirs::config_dir()/s3nav/config.toml` contains the `[[profiles]]` entry and no secret fields.

- [ ] **Step 9: Commit**

```bash
git add src/app.rs src/ui.rs
git commit -m "feat: add config form to create and save configurations"
```

---

## Notes for the implementer

- Line numbers in "Files" reflect the repo at plan-writing time and shift as you edit; anchor on the quoted code, not the numbers.
- The footer already colors messages green when they start with `Saved `/`Deleted `/`Created `/`Uploaded `/`Downloaded to`. The new messages ("Saved config …", "Deleted config …") reuse those prefixes intentionally, so they render as success.
- README/CLAUDE.md updates are out of scope for this plan; the maintainer can note the new keys after merge.
