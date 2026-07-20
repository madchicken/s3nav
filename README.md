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
- **Saved configurations** — store named connections (AWS profile, region, endpoint, initial bucket) and pick one from a startup selector; no secrets are written to disk
- **Connection indicator** — the header always shows the active endpoint, region, and profile
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

# Jump directly into a bucket (optionally with a prefix)
s3nav --bucket my-bucket
s3nav --bucket my-bucket/some/prefix

# Use a named AWS profile from ~/.aws/credentials or ~/.aws/config
s3nav --profile my-profile

# Use a custom S3 endpoint (MinIO, LocalStack, etc.)
s3nav --endpoint-url http://localhost:9000
```

Instead of AWS environment variables you can rely on a named profile with
`--profile`, or save a reusable configuration and pick it from the startup
selector (see [Saved configurations](#saved-configurations)).

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
| `s`             | Save current session as a configuration |
| `Esc`           | Go back one level; at the bucket root, ask to quit |
| `q`             | Quit             |

`Backspace` / `h` also go back and, unlike `Esc`, return to the bucket list from a bucket root.

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

## Saved configurations

s3nav can remember named connections so you don't have to pass flags every
time. A configuration stores a name plus an optional AWS profile, region,
endpoint URL, and initial bucket/prefix. **No credentials are stored** — a
configuration references an AWS profile by name and lets the AWS SDK resolve
the secrets.

Configurations are saved to `config.toml` in your platform config directory:

- macOS: `~/Library/Application Support/s3nav/config.toml`
- Linux: `~/.config/s3nav/config.toml`

**Startup behaviour:**

- If you pass any connection flag (`--profile`, `--region`, `--endpoint-url`, `--bucket`), s3nav connects directly and skips the selector.
- Otherwise, if at least one configuration is saved, s3nav opens the configuration selector.
- If none are saved, it goes straight to the bucket list (press `s` there to save the current session as a configuration).

### Configuration selector

| Key            | Action                          |
|----------------|---------------------------------|
| `j` / `↓`, `k` / `↑` | Move selection            |
| `Enter` / `l`  | Connect with this configuration |
| `n`            | Create a new configuration       |
| `e`            | Edit the selected configuration  |
| `d` / `Del`    | Delete the selected configuration |
| `q` / `Esc`    | Quit                             |

### Configuration form

| Key                  | Action                              |
|----------------------|-------------------------------------|
| `Tab` / `↑` / `↓`    | Move between fields                 |
| `←` / `→`            | Move the caret within a field       |
| `Home` / `End`       | Jump to start / end of the field    |
| `Backspace` / `Del`  | Delete before / at the caret        |
| `Enter` / `Ctrl+S`   | Save                                |
| `Esc`                | Cancel                              |

Only the name is required; the other fields are optional. Editing keeps the
entry in place, so you can rename a configuration without creating a duplicate.

## Requirements

- AWS credentials via environment variables (`AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`), a named AWS profile (`--profile` or a saved configuration), or any other source the AWS SDK resolves

## License

MIT
