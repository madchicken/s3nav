# ts3

A terminal UI file browser for Amazon S3, built with [ratatui](https://ratatui.rs).

Browse your S3 buckets and objects, preview text files inline, and download binary files — all from the terminal.

## Features

- **Browse buckets and objects** with a familiar file-manager interface
- **Drill into folders** with stack-based prefix navigation
- **Preview text files** (json, yaml, xml, csv, markdown, source code, and many more) directly in the terminal with line numbers and scrolling
- **Download binary files** to a local directory (defaults to `~/Downloads`)
- **Vim-style keybindings** alongside arrow keys
- **S3-compatible** — works with AWS S3, MinIO, LocalStack, and other S3-compatible services via `--endpoint-url`

## Installation

```
cargo install --path .
```

## Usage

Export your AWS credentials:

```bash
export AWS_ACCESS_KEY_ID=your-key
export AWS_SECRET_ACCESS_KEY=your-secret
```

Then run:

```bash
# Browse all buckets in us-east-1 (default region)
ts3

# Specify a region
ts3 --region eu-west-1

# Jump directly into a bucket
ts3 --bucket my-bucket

# Use a custom S3 endpoint (MinIO, LocalStack, etc.)
ts3 --endpoint-url http://localhost:9000
```

## Keybindings

### Browsing

| Key              | Action          |
|------------------|-----------------|
| `j` / `↓`       | Move down       |
| `k` / `↑`       | Move up         |
| `Enter` / `l`   | Open            |
| `Backspace` / `h`| Go back        |
| `g` / `Home`    | Jump to first   |
| `G` / `End`     | Jump to last    |
| `q` / `Esc`     | Quit            |

### File Preview

| Key              | Action          |
|------------------|-----------------|
| `j` / `↓`       | Scroll down     |
| `k` / `↑`       | Scroll up       |
| `d` / `PgDn`    | Page down       |
| `u` / `PgUp`    | Page up         |
| `g`              | Jump to top     |
| `q` / `Esc` / `h`| Back to list   |

### Download Prompt

| Key              | Action          |
|------------------|-----------------|
| `Enter`          | Confirm download|
| `Esc`            | Cancel          |

## Requirements

- Rust 2024 edition (1.85+)
- AWS credentials via environment variables (`AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`)

## License

MIT
