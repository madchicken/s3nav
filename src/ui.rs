use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, HighlightSpacing, List, ListItem, Paragraph, Scrollbar,
    ScrollbarOrientation, ScrollbarState, StatefulWidget, Wrap,
};

use crate::app::{App, View};
use crate::s3;

pub fn draw(frame: &mut Frame, app: &mut App) {
    let [header_area, main_area, footer_area] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(1),
    ])
    .areas(frame.area());

    draw_header(frame, app, header_area);

    match &app.view {
        View::FilePreview => draw_preview(frame, app, main_area),
        View::FileEdit => draw_editor(frame, app, main_area),
        View::Objects | View::DownloadPrompt | View::DeleteConfirm | View::CreateFolder | View::CreateFile => {
            let [list_area, detail_area] = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Fill(1), Constraint::Length(40)])
                .areas(main_area);
            draw_list(frame, app, list_area);
            draw_detail_panel(frame, app, detail_area);
        }
        _ => draw_list(frame, app, main_area),
    }

    draw_footer(frame, app, footer_area);
}

fn draw_header(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let title = match &app.view {
        View::Buckets => " S3 Buckets".to_string(),
        View::Objects | View::DownloadPrompt | View::DeleteConfirm | View::CreateFolder | View::CreateFile => {
            let prefix = app.current_prefix();
            if prefix.is_empty() {
                format!(" s3://{}", app.current_bucket)
            } else {
                format!(" s3://{}/{}", app.current_bucket, prefix)
            }
        }
        View::FilePreview => {
            format!(" {}", app.preview_name)
        }
        View::FileEdit => {
            let modified = if app.editor_modified { " [modified]" } else { "" };
            format!(" EDITING: {}{}", app.editor_name, modified)
        }
    };

    let status = if app.loading {
        " Loading..."
    } else {
        ""
    };

    let header = Paragraph::new(Line::from(vec![
        Span::styled(title, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::styled(status, Style::default().fg(Color::Yellow)),
    ]));

    frame.render_widget(header, area);
}

fn draw_list(frame: &mut Frame, app: &mut App, area: ratatui::layout::Rect) {
    let items: Vec<ListItem> = match &app.view {
        View::Buckets => app
            .buckets
            .iter()
            .map(|name| {
                ListItem::new(Line::from(vec![
                    Span::styled("  ", Style::default().fg(Color::Yellow)),
                    Span::raw(name),
                ]))
            })
            .collect(),
        View::Objects | View::DownloadPrompt | View::DeleteConfirm | View::CreateFolder | View::CreateFile => app
            .entries
            .iter()
            .map(|entry| {
                if entry.is_dir {
                    ListItem::new(Line::from(vec![
                        Span::styled("  ", Style::default().fg(Color::Yellow)),
                        Span::styled(
                            &entry.name,
                            Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD),
                        ),
                        Span::styled("/", Style::default().fg(Color::DarkGray)),
                    ]))
                } else {
                    let icon = if s3::is_text_file(&entry.name) { "  " } else { "  " };
                    ListItem::new(Line::from(vec![
                        Span::styled(icon, Style::default().fg(Color::DarkGray)),
                        Span::raw(&entry.name),
                        Span::styled(
                            format!("  {}", format_size(entry.size)),
                            Style::default().fg(Color::DarkGray),
                        ),
                    ]))
                }
            })
            .collect(),
        _ => vec![],
    };

    let block = Block::default().borders(Borders::ALL).border_style(
        Style::default().fg(Color::DarkGray),
    );

    let list = List::new(items)
        .block(block)
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ")
        .highlight_spacing(HighlightSpacing::Always);

    StatefulWidget::render(list, area, frame.buffer_mut(), &mut app.list_state);
}

fn draw_preview(frame: &mut Frame, app: &mut App, area: ratatui::layout::Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let lines: Vec<Line> = app
        .preview_content
        .lines()
        .enumerate()
        .map(|(i, line)| {
            Line::from(vec![
                Span::styled(
                    format!("{:>4} ", i + 1),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::raw(line),
            ])
        })
        .collect();

    let total_lines = lines.len();
    let paragraph = Paragraph::new(lines)
        .scroll((app.preview_scroll, 0))
        .wrap(Wrap { trim: false });

    frame.render_widget(paragraph, inner);

    // Scrollbar
    let mut scrollbar_state = ScrollbarState::new(total_lines.saturating_sub(inner.height as usize))
        .position(app.preview_scroll as usize);
    let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight);
    frame.render_stateful_widget(scrollbar, area, &mut scrollbar_state);
}

fn draw_editor(frame: &mut Frame, app: &mut App, area: ratatui::layout::Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    app.editor.set_block(block);
    app.editor.set_line_number_style(Style::default().fg(Color::DarkGray));
    app.editor.set_cursor_line_style(Style::default());
    app.editor.set_cursor_style(Style::default().bg(Color::White).fg(Color::Black));

    frame.render_widget(&app.editor, area);
}

fn draw_detail_panel(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(Span::styled(" Details ", Style::default().fg(Color::Cyan)));

    let selected = app.list_state.selected();
    let entry = selected.and_then(|i| app.entries.get(i));

    let lines = match entry {
        Some(entry) => {
            let mut lines = vec![
                Line::from(vec![
                    Span::styled("Name: ", Style::default().fg(Color::Cyan)),
                    Span::raw(&entry.name),
                ]),
                Line::from(""),
                Line::from(vec![
                    Span::styled("Type: ", Style::default().fg(Color::Cyan)),
                    Span::raw(if entry.is_dir { "Folder" } else { "File" }),
                ]),
            ];

            if !entry.is_dir {
                lines.push(Line::from(vec![
                    Span::styled("Size: ", Style::default().fg(Color::Cyan)),
                    Span::raw(format_size(entry.size)),
                ]));
            }

            if let Some(modified) = &entry.last_modified {
                // Format: "2024-01-15T10:30:00Z" -> "2024-01-15 10:30:00"
                let display = modified
                    .replace('T', " ")
                    .trim_end_matches('Z')
                    .to_string();
                lines.push(Line::from(vec![
                    Span::styled("Modified: ", Style::default().fg(Color::Cyan)),
                    Span::raw(display),
                ]));
            }

            if let Some(class) = &entry.storage_class {
                lines.push(Line::from(vec![
                    Span::styled("Storage: ", Style::default().fg(Color::Cyan)),
                    Span::raw(class),
                ]));
            }

            if let Some(etag) = &entry.e_tag {
                lines.push(Line::from(vec![
                    Span::styled("ETag: ", Style::default().fg(Color::Cyan)),
                    Span::raw(etag),
                ]));
            }

            lines.push(Line::from(""));

            let full_key = if entry.is_dir {
                format!("s3://{}/{}{}/", app.current_bucket, app.current_prefix(), entry.name)
            } else {
                format!("s3://{}/{}{}", app.current_bucket, app.current_prefix(), entry.name)
            };
            lines.push(Line::from(vec![
                Span::styled("Path: ", Style::default().fg(Color::Cyan)),
            ]));
            lines.push(Line::from(Span::styled(
                full_key,
                Style::default().fg(Color::DarkGray),
            )));

            lines
        }
        None => {
            vec![Line::from(Span::styled(
                "No item selected",
                Style::default().fg(Color::DarkGray),
            ))]
        }
    };

    let paragraph = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false });

    frame.render_widget(paragraph, area);
}

fn draw_footer(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    if app.view == View::DeleteConfirm {
        let kind = if app.delete_is_dir { "folder" } else { "file" };
        let prompt = Paragraph::new(Line::from(vec![
            Span::styled(
                format!(" Delete {kind} \"{}\"? ", app.delete_target_name),
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

    if app.view == View::CreateFolder {
        let prompt = Paragraph::new(Line::from(vec![
            Span::styled(" New folder: ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw(&app.new_folder_input),
            Span::styled("█", Style::default().fg(Color::White)),
            Span::raw("  "),
            Span::styled("Enter", Style::default().fg(Color::Cyan)),
            Span::raw(" create  "),
            Span::styled("Esc", Style::default().fg(Color::Cyan)),
            Span::raw(" cancel"),
        ]));
        frame.render_widget(prompt, area);
        return;
    }

    if app.view == View::CreateFile {
        let prompt = Paragraph::new(Line::from(vec![
            Span::styled(" New file: ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw(&app.new_file_input),
            Span::styled("█", Style::default().fg(Color::White)),
            Span::raw("  "),
            Span::styled("Enter", Style::default().fg(Color::Cyan)),
            Span::raw(" create  "),
            Span::styled("Esc", Style::default().fg(Color::Cyan)),
            Span::raw(" cancel"),
        ]));
        frame.render_widget(prompt, area);
        return;
    }

    if app.view == View::DownloadPrompt {
        let prompt = Paragraph::new(Line::from(vec![
            Span::styled(" Save to: ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw(&app.download_input),
            Span::styled("█", Style::default().fg(Color::White)),
            Span::styled(
                format!("  (default: ~/Downloads/{}) ", app.download_name),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled("Esc", Style::default().fg(Color::Cyan)),
            Span::raw(" cancel"),
        ]));
        frame.render_widget(prompt, area);
        return;
    }

    if let Some(msg) = &app.error {
        let is_success = msg.starts_with("Downloaded to") || msg.starts_with("Saved ") || msg.starts_with("Deleted ") || msg.starts_with("Created ");
        let color = if is_success { Color::Green } else { Color::Red };
        let label = if is_success { " OK: " } else { " ERROR: " };
        let line = Paragraph::new(Line::from(vec![
            Span::styled(label, Style::default().fg(color).add_modifier(Modifier::BOLD)),
            Span::styled(msg.as_str(), Style::default().fg(color)),
        ]));
        frame.render_widget(line, area);
        return;
    }

    let help = match &app.view {
        View::FilePreview => Paragraph::new(Line::from(vec![
            Span::styled(" ↑↓/jk", Style::default().fg(Color::Cyan)),
            Span::raw(" scroll  "),
            Span::styled("PgDn/d PgUp/u", Style::default().fg(Color::Cyan)),
            Span::raw(" page  "),
            Span::styled("g", Style::default().fg(Color::Cyan)),
            Span::raw(" top  "),
            Span::styled("e", Style::default().fg(Color::Cyan)),
            Span::raw(" edit  "),
            Span::styled("q/Esc/h", Style::default().fg(Color::Cyan)),
            Span::raw(" back"),
        ])),
        View::FileEdit => Paragraph::new(Line::from(vec![
            Span::styled(" Ctrl+S", Style::default().fg(Color::Cyan)),
            Span::raw(" save  "),
            Span::styled("Esc", Style::default().fg(Color::Cyan)),
            Span::raw(" cancel"),
        ])),
        View::Objects => Paragraph::new(Line::from(vec![
            Span::styled(" ↑↓/jk", Style::default().fg(Color::Cyan)),
            Span::raw(" navigate  "),
            Span::styled("Enter/l", Style::default().fg(Color::Cyan)),
            Span::raw(" open  "),
            Span::styled("n", Style::default().fg(Color::Cyan)),
            Span::raw(" new folder  "),
            Span::styled("c", Style::default().fg(Color::Cyan)),
            Span::raw(" new file  "),
            Span::styled("d", Style::default().fg(Color::Cyan)),
            Span::raw(" delete  "),
            Span::styled("h", Style::default().fg(Color::Cyan)),
            Span::raw(" back  "),
            Span::styled("q", Style::default().fg(Color::Cyan)),
            Span::raw(" quit"),
        ])),
        _ => Paragraph::new(Line::from(vec![
            Span::styled(" ↑↓/jk", Style::default().fg(Color::Cyan)),
            Span::raw(" navigate  "),
            Span::styled("Enter/l", Style::default().fg(Color::Cyan)),
            Span::raw(" open  "),
            Span::styled("Backspace/h", Style::default().fg(Color::Cyan)),
            Span::raw(" back  "),
            Span::styled("q/Esc", Style::default().fg(Color::Cyan)),
            Span::raw(" quit"),
        ])),
    };
    frame.render_widget(help, area);
}

fn format_size(bytes: i64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;

    let b = bytes as f64;
    if b >= GB {
        format!("{:.1} GB", b / GB)
    } else if b >= MB {
        format!("{:.1} MB", b / MB)
    } else if b >= KB {
        format!("{:.1} KB", b / KB)
    } else {
        format!("{bytes} B")
    }
}
