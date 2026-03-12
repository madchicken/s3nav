use aws_sdk_s3::Client;
use color_eyre::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::DefaultTerminal;
use ratatui::widgets::ListState;

use crate::s3::{self, S3Entry};
use crate::ui;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum View {
    Buckets,
    Objects,
    FilePreview,
    DownloadPrompt,
}

pub struct App {
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
    // Download prompt state
    pub download_input: String,
    pub download_key: String,
    pub download_name: String,
}

impl App {
    pub fn new(client: Client, initial_bucket: Option<String>) -> Self {
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
            download_input: String::new(),
            download_key: String::new(),
            download_name: String::new(),
        }
    }

    pub fn current_prefix(&self) -> String {
        self.prefix_stack.last().cloned().unwrap_or_default()
    }

    pub fn item_count(&self) -> usize {
        match self.view {
            View::Buckets => self.buckets.len(),
            View::Objects => self.entries.len(),
            _ => 0,
        }
    }

    pub async fn run(mut self, mut terminal: DefaultTerminal) -> Result<()> {
        // Initial load
        self.loading = true;
        terminal.draw(|frame| ui::draw(frame, &mut self))?;

        if let Some(bucket) = self.initial_bucket.take() {
            self.current_bucket = bucket;
            self.prefix_stack.push(String::new());
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
                self.handle_key(key.code, &mut terminal).await?;
            }
        }

        Ok(())
    }

    async fn handle_key(
        &mut self,
        code: KeyCode,
        terminal: &mut DefaultTerminal,
    ) -> Result<()> {
        match self.view {
            View::FilePreview => self.handle_preview_key(code),
            View::DownloadPrompt => self.handle_download_key(code, terminal).await?,
            _ => {
                self.error = None;
                self.handle_list_key(code, terminal).await?;
            }
        }
        Ok(())
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
            _ => {}
        }
        Ok(())
    }

    fn handle_preview_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Char('q') | KeyCode::Esc | KeyCode::Backspace | KeyCode::Left | KeyCode::Char('h') => {
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
                        // Show success briefly via error field (green would be nice but reuse is fine)
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
                    let new_prefix =
                        format!("{}{}/", self.current_prefix(), entry.name);
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
