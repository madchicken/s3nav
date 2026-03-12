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

## Required Environment Variables

The app requires `AWS_ACCESS_KEY_ID` and `AWS_SECRET_ACCESS_KEY` to be set. It will exit with an error if either is missing.

## Architecture

TUI S3 file browser built with ratatui. Four modules:

- **main.rs** — Entry point. Validates env vars, parses CLI args (clap), creates S3 client, launches TUI.
- **s3.rs** — AWS S3 client creation and operations (`list_buckets`, `list_objects`, `get_object_bytes`, `put_object`, `download_object`). Uses `aws-sdk-s3` with credentials loaded from environment via `aws-config`. Also contains `is_text_file` for extension-based file type detection.
- **app.rs** — Application state machine. Five views: `Buckets`, `Objects`, `FilePreview`, `FileEdit`, `DownloadPrompt`. Manages navigation with a `prefix_stack` for folder drill-down/back. Handles keyboard events per-view. Uses `tui-textarea` for the editor.
- **ui.rs** — Rendering layer. Draws header (current path), list (buckets or objects with icons), file preview with line numbers, editor via tui-textarea, and footer (keybindings/errors). Uses `StatefulWidget` for list selection.

## Navigation Model

The app uses a stack-based prefix navigation. Entering a folder pushes the new prefix onto `prefix_stack`; going back pops it. When the stack is empty and the user goes back, it returns to the bucket list view.
