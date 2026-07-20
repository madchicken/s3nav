use std::path::PathBuf;

use aws_sdk_s3::Client;
use color_eyre::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::DefaultTerminal;
use ratatui::widgets::ListState;
use tui_textarea::TextArea;

use crate::config::{self, SavedProfile};
use crate::s3::{self, ConnectionParams, S3Entry};
use crate::ui;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum View {
    Buckets,
    Objects,
    FilePreview,
    FileEdit,
    DownloadPrompt,
    DeleteConfirm,
    CreateFolder,
    CreateFile,
    FilePicker,
    ConfigSelector,
    ConfigForm,
}

pub struct App<'a> {
    pub client: Client,
    pub should_exit: bool,
    pub view: View,
    pub buckets: Vec<String>,
    pub entries: Vec<S3Entry>,
    pub current_bucket: String,
    pub prefix_stack: Vec<String>,
    pub list_state: ListState,
    pub loading: bool,
    pub error: Option<String>,
    pub initial_bucket: Option<String>,
    // File preview state
    pub preview_content: String,
    pub preview_name: String,
    pub preview_scroll: u16,
    // File edit state
    pub editor: TextArea<'a>,
    pub editor_key: String,
    pub editor_name: String,
    pub editor_modified: bool,
    // Download prompt state
    pub download_input: String,
    pub download_key: String,
    pub download_name: String,
    // Delete confirm state
    pub delete_target_name: String,
    pub delete_target_key: String,
    pub delete_is_dir: bool,
    // Create folder/file state
    pub new_folder_input: String,
    pub new_file_input: String,
    // File picker state
    pub picker_dir: PathBuf,
    pub picker_entries: Vec<LocalEntry>,
    pub picker_state: ListState,

    // Saved configurations
    pub configs: Vec<SavedProfile>,
    pub connection: ConnectionParams,
    pub config_delete_pending: bool,
    pub config_form: ConfigForm,
    pub config_form_origin: View,
    /// When editing an existing config, its index in `configs`; `None` for a new config.
    pub config_form_edit_index: Option<usize>,
}

#[derive(Clone, Debug)]
pub struct LocalEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
}

#[derive(Default)]
pub struct ConfigForm {
    pub name: String,
    pub profile: String,
    pub region: String,
    pub endpoint_url: String,
    pub bucket: String,
    pub field: usize,
}

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

    /// Pre-fill from an existing saved config (for editing it).
    fn from_profile(p: &SavedProfile) -> Self {
        Self {
            name: p.name.clone(),
            profile: p.profile.clone().unwrap_or_default(),
            region: p.region.clone().unwrap_or_default(),
            endpoint_url: p.endpoint_url.clone().unwrap_or_default(),
            bucket: p.bucket.clone().unwrap_or_default(),
            field: 0,
        }
    }
}

impl<'a> App<'a> {
    pub fn new(
        client: Client,
        connection: ConnectionParams,
        initial_bucket: Option<String>,
        configs: Vec<SavedProfile>,
    ) -> Self {
        Self {
            client,
            should_exit: false,
            view: View::Buckets,
            buckets: vec![],
            entries: vec![],
            current_bucket: String::new(),
            prefix_stack: vec![],
            list_state: ListState::default(),
            loading: false,
            error: None,
            initial_bucket,
            preview_content: String::new(),
            preview_name: String::new(),
            preview_scroll: 0,
            editor: TextArea::default(),
            editor_key: String::new(),
            editor_name: String::new(),
            editor_modified: false,
            download_input: String::new(),
            download_key: String::new(),
            download_name: String::new(),
            delete_target_name: String::new(),
            delete_target_key: String::new(),
            delete_is_dir: false,
            new_folder_input: String::new(),
            new_file_input: String::new(),
            picker_dir: PathBuf::new(),
            picker_entries: vec![],
            picker_state: ListState::default(),
            configs,
            connection,
            config_delete_pending: false,
            config_form: ConfigForm::default(),
            config_form_origin: View::ConfigSelector,
            config_form_edit_index: None,
        }
    }

    pub fn current_prefix(&self) -> String {
        self.prefix_stack.last().cloned().unwrap_or_default()
    }

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

    pub fn item_count(&self) -> usize {
        match self.view {
            View::Buckets => self.buckets.len(),
            View::Objects => self.entries.len(),
            View::FilePicker => self.picker_entries.len(),
            View::ConfigSelector => self.configs.len(),
            _ => 0,
        }
    }

    pub async fn run(mut self, mut terminal: DefaultTerminal) -> Result<()> {
        // Initial load
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

        while !self.should_exit {
            terminal.draw(|frame| ui::draw(frame, &mut self))?;
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                self.handle_key(key, &mut terminal).await?;
            }
        }

        Ok(())
    }

    async fn handle_key(&mut self, key: KeyEvent, terminal: &mut DefaultTerminal) -> Result<()> {
        match self.view {
            View::FilePreview => self.handle_preview_key(key.code),
            View::FileEdit => self.handle_edit_key(key, terminal).await?,
            View::DownloadPrompt => self.handle_download_key(key.code, terminal).await?,
            View::DeleteConfirm => self.handle_delete_confirm_key(key.code, terminal).await?,
            View::CreateFolder => self.handle_create_folder_key(key.code, terminal).await?,
            View::CreateFile => self.handle_create_file_key(key.code, terminal).await?,
            View::FilePicker => self.handle_picker_key(key.code, terminal).await?,
            View::ConfigSelector => self.handle_config_selector_key(key.code, terminal).await?,
            View::ConfigForm => self.handle_config_form_key(key, terminal).await?,
            _ => {
                self.error = None;
                self.handle_list_key(key.code, terminal).await?;
            }
        }
        Ok(())
    }

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
                self.config_form_edit_index = None;
                self.view = self.config_form_origin.clone();
            }
            KeyCode::Tab | KeyCode::Down => self.config_form.next_field(),
            KeyCode::BackTab | KeyCode::Up => self.config_form.prev_field(),
            KeyCode::Enter => self.save_config_form(terminal).await?,
            KeyCode::Backspace => {
                self.config_form.active_buf().pop();
            }
            KeyCode::Char(c)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
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
        match self.config_form_edit_index {
            // Editing an existing config: replace it in place (allows renaming).
            Some(i) if i < profiles.len() => profiles[i] = profile.clone(),
            // New config (or save-session): replace by name if present, else append.
            _ => {
                if let Some(existing) = profiles.iter_mut().find(|p| p.name == profile.name) {
                    *existing = profile.clone();
                } else {
                    profiles.push(profile.clone());
                }
            }
        }
        let config = config::Config {
            profiles: profiles.clone(),
        };

        match config::save(&config) {
            Ok(()) => {
                self.configs = profiles;
                self.error = Some(format!("Saved config {}", profile.name));
                self.config_form.clear();
                self.config_form_edit_index = None;
                self.view = self.config_form_origin.clone();
                if self.config_form_origin == View::ConfigSelector
                    && let Some(i) = self.configs.iter().position(|p| p.name == profile.name)
                {
                    self.list_state.select(Some(i));
                }
            }
            Err(e) => self.error = Some(e),
        }
        // Redraw promptly so the result is visible.
        terminal.draw(|frame| ui::draw(frame, self))?;
        Ok(())
    }

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
                if let Some(i) = self.list_state.selected()
                    && let Some(profile) = self.configs.get(i).cloned()
                {
                    self.error = None;
                    self.apply_config(profile, terminal).await?;
                }
            }
            KeyCode::Char('n') => {
                self.config_form = ConfigForm::default();
                self.error = None;
                self.config_form_origin = View::ConfigSelector;
                self.config_form_edit_index = None;
                self.view = View::ConfigForm;
            }
            KeyCode::Char('e') => {
                if let Some(i) = self.list_state.selected()
                    && let Some(profile) = self.configs.get(i)
                {
                    self.config_form = ConfigForm::from_profile(profile);
                    self.error = None;
                    self.config_form_origin = View::ConfigSelector;
                    self.config_form_edit_index = Some(i);
                    self.view = View::ConfigForm;
                }
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

    async fn handle_list_key(
        &mut self,
        code: KeyCode,
        terminal: &mut DefaultTerminal,
    ) -> Result<()> {
        match code {
            KeyCode::Char('q') | KeyCode::Esc => self.go_back_or_quit(),
            KeyCode::Down | KeyCode::Char('j') => self.select_next(),
            KeyCode::Up | KeyCode::Char('k') => self.select_previous(),
            KeyCode::Home | KeyCode::Char('g') => self.select_first(),
            KeyCode::End | KeyCode::Char('G') => self.select_last(),
            KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => {
                self.enter(terminal).await?;
            }
            KeyCode::Backspace | KeyCode::Left | KeyCode::Char('h') => {
                self.go_back(terminal).await?;
            }
            KeyCode::Char('d') | KeyCode::Delete => {
                self.prompt_delete();
            }
            KeyCode::Char('n') => {
                if self.view == View::Objects {
                    self.new_folder_input.clear();
                    self.view = View::CreateFolder;
                }
            }
            KeyCode::Char('c') => {
                if self.view == View::Objects {
                    self.new_file_input.clear();
                    self.view = View::CreateFile;
                }
            }
            KeyCode::Char('s') => {
                if self.view == View::Buckets {
                    let prefix = self.current_prefix();
                    self.config_form =
                        ConfigForm::from_session(&self.connection, &self.current_bucket, &prefix);
                    self.error = None;
                    self.config_form_origin = View::Buckets;
                    self.config_form_edit_index = None;
                    self.view = View::ConfigForm;
                }
            }
            KeyCode::Char('u') => {
                if self.view == View::Objects {
                    self.open_file_picker();
                }
            }
            KeyCode::Char('r') => match self.view {
                View::Objects => self.load_objects(terminal).await?,
                View::Buckets => self.load_buckets(terminal).await?,
                _ => {}
            },
            _ => {}
        }
        Ok(())
    }

    fn handle_preview_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Char('e') => {
                self.open_editor_from_preview();
            }
            KeyCode::Char('q')
            | KeyCode::Esc
            | KeyCode::Backspace
            | KeyCode::Left
            | KeyCode::Char('h') => {
                self.view = View::Objects;
                self.preview_content.clear();
                self.preview_scroll = 0;
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.preview_scroll = self.preview_scroll.saturating_add(1);
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.preview_scroll = self.preview_scroll.saturating_sub(1);
            }
            KeyCode::PageDown | KeyCode::Char('d') => {
                self.preview_scroll = self.preview_scroll.saturating_add(20);
            }
            KeyCode::PageUp | KeyCode::Char('u') => {
                self.preview_scroll = self.preview_scroll.saturating_sub(20);
            }
            KeyCode::Home | KeyCode::Char('g') => {
                self.preview_scroll = 0;
            }
            _ => {}
        }
    }

    async fn handle_edit_key(
        &mut self,
        key: KeyEvent,
        terminal: &mut DefaultTerminal,
    ) -> Result<()> {
        // Ctrl+S to save
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('s') {
            self.save_editor(terminal).await?;
            return Ok(());
        }
        // Esc to cancel
        if key.code == KeyCode::Esc {
            self.close_editor();
            return Ok(());
        }
        // Forward everything else to the textarea
        self.editor.input(key);
        self.editor_modified = true;
        Ok(())
    }

    async fn save_editor(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        let content = self.editor.lines().join("\n");
        let key = self.editor_key.clone();

        self.loading = true;
        terminal.draw(|frame| ui::draw(frame, self))?;

        match s3::put_object(&self.client, &self.current_bucket, &key, &content).await {
            Ok(()) => {
                self.error = Some(format!("Saved {}", self.editor_name));
                self.editor_modified = false;
                self.preview_content = content;
                self.preview_scroll = 0;
                self.view = View::FilePreview;
                // Reload objects so the detail panel shows updated metadata
                self.load_objects(terminal).await?;
            }
            Err(e) => {
                self.error = Some(e);
                self.loading = false;
            }
        }
        Ok(())
    }

    fn close_editor(&mut self) {
        // Go back to preview with original content
        self.preview_scroll = 0;
        self.view = View::FilePreview;
    }

    fn open_editor_from_preview(&mut self) {
        let lines: Vec<String> = self.preview_content.lines().map(String::from).collect();
        self.editor = TextArea::new(if lines.is_empty() {
            vec![String::new()]
        } else {
            lines
        });
        self.editor_key = format!("{}{}", self.current_prefix(), self.preview_name);
        self.editor_name = self.preview_name.clone();
        self.editor_modified = false;
        self.view = View::FileEdit;
    }

    async fn handle_download_key(
        &mut self,
        code: KeyCode,
        terminal: &mut DefaultTerminal,
    ) -> Result<()> {
        match code {
            KeyCode::Esc => {
                self.view = View::Objects;
                self.download_input.clear();
            }
            KeyCode::Enter => {
                let dir = if self.download_input.is_empty() {
                    dirs::download_dir()
                        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join("Downloads"))
                } else {
                    let expanded = shellexpand::tilde(&self.download_input).into_owned();
                    std::path::PathBuf::from(expanded)
                };
                let dest = dir.join(&self.download_name);
                let key = self.download_key.clone();

                self.loading = true;
                self.view = View::Objects;
                terminal.draw(|frame| ui::draw(frame, self))?;

                match s3::download_object(&self.client, &self.current_bucket, &key, &dest).await {
                    Ok(()) => {
                        self.error = None;
                        self.download_input.clear();
                        self.error = Some(format!("Downloaded to {}", dest.display()));
                    }
                    Err(e) => self.error = Some(e),
                }
                self.loading = false;
            }
            KeyCode::Backspace => {
                self.download_input.pop();
            }
            KeyCode::Char(c) => {
                self.download_input.push(c);
            }
            _ => {}
        }
        Ok(())
    }

    fn go_back_or_quit(&mut self) {
        if self.view == View::Buckets {
            self.should_exit = true;
        }
        if self.view == View::Objects {
            self.should_exit = true;
        }
    }

    fn select_next(&mut self) {
        if self.item_count() == 0 {
            return;
        }
        self.list_state.select_next();
    }

    fn select_previous(&mut self) {
        if self.item_count() == 0 {
            return;
        }
        self.list_state.select_previous();
    }

    fn select_first(&mut self) {
        if self.item_count() > 0 {
            self.list_state.select_first();
        }
    }

    fn select_last(&mut self) {
        if self.item_count() > 0 {
            self.list_state.select_last();
        }
    }

    async fn enter(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        let selected = match self.list_state.selected() {
            Some(i) => i,
            None => return Ok(()),
        };

        match self.view {
            View::Buckets => {
                if selected < self.buckets.len() {
                    self.current_bucket = self.buckets[selected].clone();
                    self.prefix_stack.clear();
                    self.prefix_stack.push(String::new());
                    self.load_objects(terminal).await?;
                    self.view = View::Objects;
                }
            }
            View::Objects => {
                if selected >= self.entries.len() {
                    return Ok(());
                }
                let entry = self.entries[selected].clone();
                if entry.is_dir {
                    let new_prefix = format!("{}{}/", self.current_prefix(), entry.name);
                    self.prefix_stack.push(new_prefix);
                    self.load_objects(terminal).await?;
                } else {
                    let key = format!("{}{}", self.current_prefix(), entry.name);
                    if s3::is_text_file(&entry.name) {
                        self.open_preview(&key, &entry.name, terminal).await?;
                    } else {
                        self.open_download_prompt(key, entry.name);
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    async fn open_preview(
        &mut self,
        key: &str,
        name: &str,
        terminal: &mut DefaultTerminal,
    ) -> Result<()> {
        self.loading = true;
        terminal.draw(|frame| ui::draw(frame, self))?;

        match s3::get_object_bytes(&self.client, &self.current_bucket, key).await {
            Ok(bytes) => {
                self.preview_content = String::from_utf8_lossy(&bytes).into_owned();
                self.preview_name = name.to_string();
                self.preview_scroll = 0;
                self.view = View::FilePreview;
            }
            Err(e) => self.error = Some(e),
        }
        self.loading = false;
        Ok(())
    }

    fn open_download_prompt(&mut self, key: String, name: String) {
        self.download_key = key;
        self.download_name = name;
        self.download_input.clear();
        self.view = View::DownloadPrompt;
    }

    fn prompt_delete(&mut self) {
        if self.view != View::Objects {
            return;
        }
        let selected = match self.list_state.selected() {
            Some(i) => i,
            None => return,
        };
        if selected >= self.entries.len() {
            return;
        }
        let entry = &self.entries[selected];
        self.delete_target_name = entry.name.clone();
        self.delete_is_dir = entry.is_dir;
        self.delete_target_key = if entry.is_dir {
            format!("{}{}/", self.current_prefix(), entry.name)
        } else {
            format!("{}{}", self.current_prefix(), entry.name)
        };
        self.view = View::DeleteConfirm;
    }

    async fn handle_delete_confirm_key(
        &mut self,
        code: KeyCode,
        terminal: &mut DefaultTerminal,
    ) -> Result<()> {
        match code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                self.loading = true;
                self.view = View::Objects;
                terminal.draw(|frame| ui::draw(frame, self))?;

                let key = self.delete_target_key.clone();
                let name = self.delete_target_name.clone();

                let result = if self.delete_is_dir {
                    match s3::delete_prefix(&self.client, &self.current_bucket, &key).await {
                        Ok(count) => Ok(format!("Deleted {name}/ ({count} objects)")),
                        Err(e) => Err(e),
                    }
                } else {
                    match s3::delete_object(&self.client, &self.current_bucket, &key).await {
                        Ok(()) => Ok(format!("Deleted {name}")),
                        Err(e) => Err(e),
                    }
                };

                match result {
                    Ok(msg) => {
                        self.error = Some(msg);
                        self.load_objects(terminal).await?;
                    }
                    Err(e) => {
                        self.error = Some(e);
                        self.loading = false;
                    }
                }
            }
            _ => {
                // Any other key cancels
                self.view = View::Objects;
            }
        }
        Ok(())
    }

    async fn handle_create_folder_key(
        &mut self,
        code: KeyCode,
        terminal: &mut DefaultTerminal,
    ) -> Result<()> {
        match code {
            KeyCode::Esc => {
                self.view = View::Objects;
                self.new_folder_input.clear();
            }
            KeyCode::Enter => {
                let name = self.new_folder_input.trim().to_string();
                if name.is_empty() {
                    self.view = View::Objects;
                    return Ok(());
                }
                let key = format!("{}{}/", self.current_prefix(), name);

                self.loading = true;
                self.view = View::Objects;
                terminal.draw(|frame| ui::draw(frame, self))?;

                match s3::put_object(&self.client, &self.current_bucket, &key, "").await {
                    Ok(()) => {
                        self.error = Some(format!("Created folder {name}/"));
                        self.new_folder_input.clear();
                        self.load_objects(terminal).await?;
                    }
                    Err(e) => {
                        self.error = Some(e);
                        self.loading = false;
                    }
                }
            }
            KeyCode::Backspace => {
                self.new_folder_input.pop();
            }
            KeyCode::Char(c) => {
                self.new_folder_input.push(c);
            }
            _ => {}
        }
        Ok(())
    }

    async fn handle_create_file_key(
        &mut self,
        code: KeyCode,
        terminal: &mut DefaultTerminal,
    ) -> Result<()> {
        match code {
            KeyCode::Esc => {
                self.view = View::Objects;
                self.new_file_input.clear();
            }
            KeyCode::Enter => {
                let name = self.new_file_input.trim().to_string();
                if name.is_empty() {
                    self.view = View::Objects;
                    return Ok(());
                }
                let key = format!("{}{}", self.current_prefix(), name);

                self.loading = true;
                self.view = View::Objects;
                terminal.draw(|frame| ui::draw(frame, self))?;

                match s3::put_object(&self.client, &self.current_bucket, &key, "").await {
                    Ok(()) => {
                        self.error = Some(format!("Created file {name}"));
                        self.new_file_input.clear();
                        self.load_objects(terminal).await?;
                    }
                    Err(e) => {
                        self.error = Some(e);
                        self.loading = false;
                    }
                }
            }
            KeyCode::Backspace => {
                self.new_file_input.pop();
            }
            KeyCode::Char(c) => {
                self.new_file_input.push(c);
            }
            _ => {}
        }
        Ok(())
    }

    fn open_file_picker(&mut self) {
        let start_dir = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
        self.picker_dir = start_dir;
        self.refresh_picker();
        self.view = View::FilePicker;
    }

    fn refresh_picker(&mut self) {
        let mut entries = Vec::new();
        if let Ok(read_dir) = std::fs::read_dir(&self.picker_dir) {
            for entry in read_dir.flatten() {
                let Ok(meta) = entry.metadata() else {
                    continue;
                };
                let Some(name) = entry.file_name().to_str().map(String::from) else {
                    continue;
                };
                // Skip hidden files
                if name.starts_with('.') {
                    continue;
                }
                if meta.is_dir() || meta.is_file() {
                    entries.push(LocalEntry {
                        name,
                        is_dir: meta.is_dir(),
                        size: meta.len(),
                    });
                }
            }
        }
        entries.sort_by(|a, b| {
            b.is_dir
                .cmp(&a.is_dir)
                .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        self.picker_entries = entries;
        if self.picker_entries.is_empty() {
            self.picker_state.select(None);
        } else {
            self.picker_state.select(Some(0));
        }
    }

    async fn handle_picker_key(
        &mut self,
        code: KeyCode,
        terminal: &mut DefaultTerminal,
    ) -> Result<()> {
        match code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.view = View::Objects;
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if !self.picker_entries.is_empty() {
                    self.picker_state.select_next();
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if !self.picker_entries.is_empty() {
                    self.picker_state.select_previous();
                }
            }
            KeyCode::Home | KeyCode::Char('g') => {
                if !self.picker_entries.is_empty() {
                    self.picker_state.select_first();
                }
            }
            KeyCode::End | KeyCode::Char('G') => {
                if !self.picker_entries.is_empty() {
                    self.picker_state.select_last();
                }
            }
            KeyCode::Char('.') => {
                // Toggle hidden files: re-read including hidden
                self.refresh_picker_toggle_hidden();
            }
            KeyCode::Backspace | KeyCode::Left | KeyCode::Char('h') => {
                if let Some(parent) = self.picker_dir.parent() {
                    self.picker_dir = parent.to_path_buf();
                    self.refresh_picker();
                }
            }
            KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => {
                if let Some(idx) = self.picker_state.selected()
                    && idx < self.picker_entries.len()
                {
                    let entry = self.picker_entries[idx].clone();
                    if entry.is_dir {
                        self.picker_dir = self.picker_dir.join(&entry.name);
                        self.refresh_picker();
                    } else {
                        // Upload the selected file
                        let local_path = self.picker_dir.join(&entry.name);
                        let s3_key = format!("{}{}", self.current_prefix(), entry.name);

                        self.loading = true;
                        self.view = View::Objects;
                        terminal.draw(|frame| ui::draw(frame, self))?;

                        match s3::upload_file(
                            &self.client,
                            &self.current_bucket,
                            &s3_key,
                            &local_path,
                        )
                        .await
                        {
                            Ok(()) => {
                                self.error = Some(format!("Uploaded {}", entry.name));
                                self.load_objects(terminal).await?;
                            }
                            Err(e) => {
                                self.error = Some(e);
                                self.loading = false;
                            }
                        }
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn refresh_picker_toggle_hidden(&mut self) {
        let mut entries = Vec::new();
        let show_hidden = !self.picker_entries.iter().any(|e| e.name.starts_with('.'));
        if let Ok(read_dir) = std::fs::read_dir(&self.picker_dir) {
            for entry in read_dir.flatten() {
                let Ok(meta) = entry.metadata() else {
                    continue;
                };
                let Some(name) = entry.file_name().to_str().map(String::from) else {
                    continue;
                };
                if !show_hidden && name.starts_with('.') {
                    continue;
                }
                if meta.is_dir() || meta.is_file() {
                    entries.push(LocalEntry {
                        name,
                        is_dir: meta.is_dir(),
                        size: meta.len(),
                    });
                }
            }
        }
        entries.sort_by(|a, b| {
            b.is_dir
                .cmp(&a.is_dir)
                .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        self.picker_entries = entries;
        if self.picker_entries.is_empty() {
            self.picker_state.select(None);
        } else {
            self.picker_state.select(Some(0));
        }
    }

    async fn go_back(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        match self.view {
            View::Buckets => {}
            View::Objects => {
                if self.prefix_stack.len() > 1 {
                    self.prefix_stack.pop();
                    self.load_objects(terminal).await?;
                } else {
                    self.view = View::Buckets;
                    self.entries.clear();
                    if self.buckets.is_empty() {
                        self.load_buckets(terminal).await?;
                    } else {
                        self.list_state.select(Some(0));
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    async fn load_buckets(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        self.loading = true;
        terminal.draw(|frame| ui::draw(frame, self))?;

        match s3::list_buckets(&self.client).await {
            Ok(buckets) => {
                self.buckets = buckets;
                if !self.buckets.is_empty() {
                    self.list_state.select(Some(0));
                } else {
                    self.list_state.select(None);
                }
            }
            Err(e) => self.error = Some(e),
        }
        self.loading = false;
        Ok(())
    }

    async fn load_objects(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        self.loading = true;
        terminal.draw(|frame| ui::draw(frame, self))?;

        let prefix = self.current_prefix();
        match s3::list_objects(&self.client, &self.current_bucket, &prefix).await {
            Ok(entries) => {
                self.entries = entries;
                if self.entries.is_empty() {
                    self.list_state.select(None);
                } else {
                    self.list_state.select(Some(0));
                }
            }
            Err(e) => {
                self.error = Some(e);
                self.entries.clear();
                self.list_state.select(None);
            }
        }
        self.loading = false;
        Ok(())
    }
}

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

    #[test]
    fn config_form_from_profile_round_trips() {
        let original = SavedProfile {
            name: "prod".into(),
            profile: Some("prod-acct".into()),
            region: Some("eu-west-1".into()),
            endpoint_url: None,
            bucket: Some("my-bucket/data".into()),
        };
        let form = ConfigForm::from_profile(&original);
        // Absent optional fields become empty strings in the form.
        assert_eq!(form.name, "prod");
        assert_eq!(form.endpoint_url, "");
        assert_eq!(form.field, 0);
        // Editing nothing and saving reproduces the original profile.
        assert_eq!(form.to_profile(), original);
    }
}
