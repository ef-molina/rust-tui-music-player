//! Terminal UI rendering module.
//!
//! Responsible for drawing the application UI using `ratatui`.
//!
//! Layout (top to bottom):
//! - Header: application title / track info
//! - Body: main content area (future lyrics / browser)
//! - Footer: controls and status
//!
//! Design rules:
//! - Pure rendering only
//! - Read-only access to AppState
//! - No input handling or side effects

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
};

use crate::app::AppState;

pub fn draw(frame: &mut Frame, app: &AppState) {
    let size = frame.size();

    frame.render_widget(Clear, size); // clear the background

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header
            Constraint::Min(1),    // body
            Constraint::Length(3), // footer
        ])
        .split(size);

    // --- Header ---
    let title = "Rust TUI Music Player";

    // Compute relative path for display
    let path_display = app
        .current_dir
        .strip_prefix(&app.root_dir)
        .ok()
        .and_then(|p| {
            if p.as_os_str().is_empty() {
                None
            } else {
                Some(p)
            }
        })
        .map(|p| format!("~/{}", p.display()))
        .unwrap_or_else(|| "~/".to_string());

    // Final header text
    let header_text = format!("{title} — {path_display}");

    let header = Paragraph::new(header_text)
        .alignment(Alignment::Center)
        .style(Style::default().add_modifier(Modifier::BOLD))
        .block(Block::default().borders(Borders::ALL));

    frame.render_widget(header, chunks[0]);

    // --- Body (split plane) ---
    let body_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(30), // left pane (file browser)
            Constraint::Percentage(70), // right pane (track details)
        ])
        .split(chunks[1]);

    // Left pane: filesystem browser
    let items: Vec<ListItem> = app
        .browser_entries
        .iter()
        .map(|entry| {
            let prefix = if entry.is_dir { "📁 " } else { "🎵 " };
            ListItem::new(format!("{}{}", prefix, entry.name))
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().title("Browser").borders(Borders::ALL))
        .highlight_style(
            Style::default()
                .bg(Color::Blue)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("➤ ");

    let mut state = ListState::default();
    state.select(Some(app.selected_index));
    frame.render_stateful_widget(list, body_chunks[0], &mut state);

    // Right pane: Preview / lyrics / Metadata placeholder
    let detail_text = match &app.player.current_track {
        Some(path) => format!(
            "Selected for playback:\n\n{}",
            path.file_name().unwrap_or_default().to_string_lossy()
        ),
        None => "Nothing selected".to_string(),
    };

    let right_pane = Paragraph::new(detail_text)
        .alignment(Alignment::Center)
        .block(Block::default().title("Details").borders(Borders::ALL));

    frame.render_widget(right_pane, body_chunks[1]);

    // --- Footer ---
    let status = app.player.status_text();

    let footer = Paragraph::new(format!(
        "{}    q: quit   space: play/pause   ←/→: seek",
        status
    ))
    .alignment(Alignment::Center)
    .block(Block::default().borders(Borders::ALL));

    frame.render_widget(footer, chunks[2]);
}
