# s3nav

A terminal UI file browser for Amazon S3, built with [ratatui](https://ratatui.rs).

Browse your S3 buckets and objects, preview and edit text files inline, upload and download files — all from the terminal.

## Features

- **Browse buckets and objects** with a familiar file-manager interface
- **Drill into folders** with stack-based prefix navigation
- **Detail panel** showing file metadata (size, last modified, storage class, ETag)
- **Preview text files** (json, yaml, xml, csv, markdown, source code, and many more) directly in the terminal with line numbers and scrolling
- **Edit text files** with a built-in editor and save back to S3 (`Ctrl+S`)
- **Upload files** via an interactive local file picker
- **Download binary files** to a local directory (defaults to `~/Downloads`)
- **Create and delete** files and folders
- **Vim-style keybindings** alongside arrow keys
- **S3-compatible** — works with AWS S3, MinIO, LocalStack, and other S3-compatible services via `--endpoint-url`

## Installation

### From crates.io

```bash
cargo install s3nav
```

### From GitHub Releases

Download a prebuilt binary from the [Releases](https://github.com/madchicken/s3nav/releases) page.

### From source

```bash
git clone https://github.com/madchicken/s3nav.git
cd s3nav
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
s3nav

# Specify a region
s3nav --region eu-west-1

# Jump directly into a bucket
s3nav --bucket my-bucket

# Use a custom S3 endpoint (MinIO, LocalStack, etc.)
s3nav --endpoint-url http://localhost:9000
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
| `n`             | New folder       |
| `c`             | New file         |
| `u`             | Upload file      |
| `d` / `Del`     | Delete           |
| `q` / `Esc`     | Quit             |

### File Preview

| Key              | Action          |
|------------------|-----------------|
| `j` / `↓`       | Scroll down     |
| `k` / `↑`       | Scroll up       |
| `d` / `PgDn`    | Page down       |
| `u` / `PgUp`    | Page up         |
| `g`              | Jump to top     |
| `e`              | Edit file       |
| `q` / `Esc` / `h`| Back to list   |

### Editing

| Key              | Action          |
|------------------|-----------------|
| `Ctrl+S`         | Save to S3      |
| `Esc`            | Cancel editing  |

### File Picker (Upload)

| Key              | Action          |
|------------------|-----------------|
| `j` / `↓`       | Move down       |
| `k` / `↑`       | Move up         |
| `Enter` / `l`   | Open dir / Upload file |
| `Backspace` / `h`| Parent directory |
| `.`              | Toggle hidden files |
| `q` / `Esc`     | Cancel          |

## Requirements

- AWS credentials via environment variables (`AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`)

## License

MIT
