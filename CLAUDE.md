# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Run Commands

- **Build:** `cargo build`
- **Run:** `cargo run -- --region eu-west-1`
- **Test:** `cargo test`
- **Run single test:** `cargo test <test_name>`
- **Lint:** `cargo clippy`
- **Format:** `cargo fmt`
- **Check (fast compile check):** `cargo check`

## AWS Credentials

Credentials are resolved by the AWS SDK (`aws-config`), so they may come from environment variables (`AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY`), a named AWS profile (`--profile` or a saved configuration referencing one), or any other source the SDK supports. The app does not itself require the env vars to be set.

## Architecture

TUI S3 file browser built with ratatui. Five modules:

- **main.rs** — Entry point. Parses CLI args (clap), builds `ConnectionParams` from them, creates the S3 client, loads saved configs, and decides the start view: if any connection flag (`--profile`/`--region`/`--endpoint-url`/`--bucket`) is passed it connects directly; otherwise, if configs exist, it starts in the configuration selector.
- **config.rs** — Persistence for saved configurations. `SavedProfile` (name + optional profile/region/endpoint_url/bucket — **no secrets**) and `Config { profiles }`, serialized as TOML to `dirs::config_dir()/s3nav/config.toml`. `load()` treats a missing file as empty and surfaces parse errors without overwriting.
- **s3.rs** — AWS S3 client creation and operations (`list_buckets`, `list_objects`, `get_object_bytes`, `put_object`, `download_object`, etc.). `ConnectionParams` (profile/region/endpoint_url) decouples client creation from CLI `Args` so a saved profile can also build a client; `create_client` takes it. Also contains `is_text_file` for extension-based file type detection.
- **app.rs** — Application state machine. Views include `Buckets`, `Objects`, `FilePreview`, `FileEdit`, `DownloadPrompt`, `DeleteConfirm`, `CreateFolder`, `CreateFile`, `FilePicker`, plus `ConfigSelector` and `ConfigForm`. Manages navigation with a `prefix_stack` for folder drill-down/back and handles keyboard events per-view. `ConfigForm` is a small multi-field text form with an in-field caret; `apply_config` rebuilds the client when a configuration is chosen. Uses `tui-textarea` for the file editor.
- **ui.rs** — Rendering layer. Draws the header (current path on the left, active connection summary on the right), the list (buckets/objects/configs with icons), file preview with line numbers, the editor, the config form, and the footer (keybindings, errors, and confirmation prompts). Uses `StatefulWidget` for list selection.

## Navigation Model

The app uses a stack-based prefix navigation. Entering a folder pushes the new prefix onto `prefix_stack`; going back pops it. `Backspace`/`h` go back through folders and, at a bucket root, return to the bucket list. `Esc` also goes back one folder level but, at the bucket root (or in the bucket list), asks for confirmation before quitting (`quit_pending`).
