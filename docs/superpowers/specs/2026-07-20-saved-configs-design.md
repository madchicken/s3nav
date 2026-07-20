# Saved Configurations — Design

**Date:** 2026-07-20
**Status:** Approved for planning

## Summary

Add the ability to persist named connection configurations and pick one from
the TUI at startup. Each saved configuration references an AWS profile by name
(no secrets stored on disk) plus optional region, endpoint URL, and an initial
bucket/prefix. Configurations can be created and deleted from within the TUI.

## Goals

- Save named configurations to a config file on disk.
- Present a startup selector to choose a saved configuration.
- Create a new configuration from the TUI via a multi-field form.
- Save the current session as a configuration on the fly.
- Delete a saved configuration from the TUI.

## Non-goals

- Storing raw AWS credentials (access key / secret) on disk. Configurations
  reference AWS profiles by name only, delegating secret management to the AWS
  credential/config files.
- Editing an existing configuration in place (v1: delete + recreate). May be
  added later.

## Credentials model

A saved configuration **never** stores secrets. It references an AWS profile
name from `~/.aws/credentials` / `~/.aws/config`. Region, endpoint URL, and
initial bucket are stored inline because they are not sensitive.

## Data model & storage

**Format:** TOML.

**Location:** `dirs::config_dir()/s3nav/config.toml`
- macOS: `~/Library/Application Support/s3nav/config.toml`
- Linux: `~/.config/s3nav/config.toml`

**Schema:**

```toml
[[profiles]]
name = "prod"                  # required — identifier shown in the selector
profile = "prod-account"       # optional — AWS profile name
region = "eu-west-1"           # optional
endpoint_url = "https://..."   # optional
bucket = "my-bucket/prefix"    # optional — initial bucket, optional prefix
```

All fields except `name` are optional. A configuration with only a `name` is
valid (uses AWS defaults).

## Architecture

New module **`config.rs`** owns persistence. It is independent of the TUI and
the S3 client, testable on its own.

```
config.rs   — SavedProfile, Config, load(), save(), config_path()
s3.rs       — create_client refactored to accept connection params
app.rs      — ConfigSelector + ConfigForm views and their state
ui.rs       — rendering for selector and form
main.rs     — startup routing (CLI flags vs. selector)
```

### `config.rs`

```rust
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
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

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Config {
    #[serde(default)]
    pub profiles: Vec<SavedProfile>,
}

pub fn config_path() -> PathBuf;      // dirs::config_dir()/s3nav/config.toml
pub fn load() -> Result<Config, String>;  // missing file => Config::default()
pub fn save(config: &Config) -> Result<(), String>;  // creates parent dir
```

- `load`: missing file returns an empty `Config` (not an error). A corrupt file
  returns an `Err(String)` describing the parse failure; the app surfaces this
  and continues with an empty config in memory, and does **not** overwrite the
  file until the user explicitly saves.
- `save`: creates the parent directory if needed, then writes serialized TOML.

### `s3.rs`

Refactor `create_client` so it does not depend on `Args` directly. Introduce a
small connection-parameter carrier that both `Args` and `SavedProfile` can
produce:

```rust
pub struct ConnectionParams {
    pub profile: Option<String>,
    pub region: Option<String>,
    pub endpoint_url: Option<String>,
}

pub async fn create_client(params: &ConnectionParams) -> Client;
```

`Args` and `SavedProfile` each get a small conversion into `ConnectionParams`.
This lets the app rebuild the S3 client when a configuration is selected.

### `app.rs`

Add two `View` variants:

- `ConfigSelector` — list of saved configurations with actions.
- `ConfigForm` — multi-field form to create a configuration.

New `App` state:

- `configs: Vec<SavedProfile>` — loaded configurations.
- `config_state: ListState` — selection in the selector.
- Form state: the five field buffers (`name`, `profile`, `region`,
  `endpoint_url`, `bucket`) and a `form_field` cursor indicating the active
  field. A flag distinguishes "delete pending confirmation" reusing the
  existing confirmation pattern where practical.

The app gains the ability to rebuild `self.client`:

```rust
async fn apply_config(&mut self, profile: &SavedProfile);
```

which builds `ConnectionParams` from the profile, calls
`s3::create_client`, replaces `self.client`, sets the initial bucket/prefix,
and transitions into `Buckets` (or directly into `Objects` if a bucket is set).

## Flow

1. **Startup** (`main.rs`):
   - If any of `--profile`, `--bucket`, `--region`, `--endpoint-url` is passed,
     behavior is unchanged: build the client from `Args`, skip the selector.
   - Otherwise load `config.toml`. If it has entries, start in
     `ConfigSelector`. If empty, go to the bucket list as today, using a
     default client.
   - A default client is always constructed up front so `App.client` stays
     non-optional; it is replaced when a configuration is applied.

2. **Selector** (`ConfigSelector`):
   - `↑/↓` navigate, `Enter` apply the selected configuration.
   - `n` open the form to create a new configuration.
   - `d` delete the selected configuration (with confirmation).
   - `q` quit.

3. **Form** (`ConfigForm`):
   - `Tab` / `↑↓` move between fields.
   - Typing edits the active field; `Backspace` deletes.
   - `Enter` / `Ctrl+S` validate (require non-empty `name`), save to file,
     reload `configs`, return to the selector.
   - `Esc` cancel without saving.

4. **Save current session**:
   - From the `Buckets` view, `s` opens the form pre-filled with the current
     profile/region/endpoint/bucket so the running session can be persisted.

## Error handling

- File read/write and TOML parse errors are shown in the footer, matching the
  existing `error: Option<String>` pattern.
- A corrupt config file yields an error message and an empty in-memory config;
  the file is not overwritten until the user saves.
- Form validation error (empty `name`) is shown in the footer; the form stays
  open.

## Dependencies

Add to `Cargo.toml`:

- `serde = { version = "1", features = ["derive"] }`
- `toml = "1"`

(`dirs` is already a dependency.)

## Testing

Unit tests in `config.rs`:

- Round-trip: serialize a `Config` and deserialize it back to an equal value.
- Load from a missing file returns an empty `Config`.
- Deserialize TOML with optional fields omitted (only `name`).
- Serialize omits `None` optional fields (no empty keys written).

TUI interaction (selector navigation, form editing) remains manually tested,
consistent with the rest of the codebase.

## Keybindings summary

| View            | Key       | Action                                   |
|-----------------|-----------|------------------------------------------|
| ConfigSelector  | ↑/↓       | Navigate                                 |
| ConfigSelector  | Enter     | Apply selected configuration             |
| ConfigSelector  | n         | New configuration (open form)            |
| ConfigSelector  | d         | Delete selected configuration            |
| ConfigSelector  | q         | Quit                                     |
| ConfigForm      | Tab / ↑↓  | Move between fields                      |
| ConfigForm      | Enter/^S  | Save and return to selector              |
| ConfigForm      | Esc       | Cancel                                   |
| Buckets         | s         | Save current session as a configuration  |
