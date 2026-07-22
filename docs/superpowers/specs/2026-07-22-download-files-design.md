# Download files/folders from S3 — design

## Problem

Download plumbing already exists (`s3::download_object`, `DownloadPrompt` view,
`handle_download_key`), but it is only reachable implicitly: pressing `Enter` on
a **non-text** file opens the download prompt, while text files open the
preview instead. There is no explicit "download this" action, no way to
download a file shown in the preview, and no way to download a whole folder.

## Goals

1. An explicit download key (`D`, Shift+d) in the Objects list that works on any
   selected entry — text file, binary file, or folder.
2. Download from the `FilePreview` view with the same `D` key.
3. Recursive download of a folder (prefix), recreating the subdirectory tree
   locally.

`d` stays mapped to delete (Objects) / PageDown (preview). Both delete and
download go through a prompt, so an accidental `D`/`d` mixup is recoverable.

## Changes

### s3.rs
- New `download_prefix(client, bucket, prefix, dest_root) -> Result<u32, String>`
  modeled on `delete_prefix`: paginate all objects under `prefix`, write each to
  `dest_root` joined with the object key stripped of `prefix`, creating parent
  directories as needed. Skip folder-marker keys (those ending in `/`, or equal
  to the prefix). Returns the number of files written.
- Pure helper `relative_key(prefix, key) -> Option<String>` returning the path
  of `key` relative to `prefix`, or `None` for folder markers. Unit-tested.

### app.rs
- New state fields: `download_is_dir: bool` and `download_origin: View`
  (mirroring `delete_is_dir` and `config_form_origin`).
- `open_download_prompt` gains `is_dir` and records the origin view so cancel /
  completion return to the caller (Objects or FilePreview).
- `handle_list_key`: `KeyCode::Char('D')` in Objects → open the download prompt
  for the selected entry (file or folder).
- `handle_preview_key`: `KeyCode::Char('D')` → open the download prompt for the
  previewed file.
- `handle_download_key` (Enter): if `download_is_dir`, call `download_prefix`
  into `dir.join(download_name)` and report `"Downloaded N files to <path>"`;
  otherwise keep the single-file behavior (`"Downloaded to <path>"`). On
  Esc/Enter return to `download_origin` instead of always `Objects`.

### ui.rs
- Objects footer: add `D download` next to `u upload`.
- FilePreview footer: add `D download`.
- Download prompt: wording reflects folder vs file when `download_is_dir`.

## Testing

- Unit test `relative_key` (folder marker → None, nested key → stripped path).
- S3 network operations remain untested (consistent with the rest of s3.rs);
  verified via `cargo build` + `cargo clippy`.

## Out of scope

- Progress bar / cancellation of an in-flight recursive download.
- Overwrite confirmation for existing local files.
