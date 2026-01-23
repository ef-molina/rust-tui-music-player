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
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Modifier, Style, Color},
    widgets::{Block, Borders, Paragraph, Clear, List, ListItem, ListState},
    Frame,
};

use crate::app::AppState;

pub fn draw(frame: &mut Frame, _app: &AppState) {
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
    let header = Paragraph::new("Rust TUI Music Player")
        .alignment(Alignment::Center)
        .style(Style::default().add_modifier(Modifier::BOLD))
        .block(Block::default().borders(Borders::ALL));


    frame.render_widget(header, chunks[0]);

    // --- Body (split plane) ---
    let body_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(30), // left pane (e.g., lyrics)
            Constraint::Percentage(70), // right pane (e.g., browser)
        ])
        .split(chunks[1]);


    // Left pane: filesystem browser
    let items: Vec<ListItem> = _app
        .browser_entries
        .iter()
        .map(|entry| {
            let prefix = if entry.is_dir { "📁 " } else { "🎵 " };
            ListItem::new(format!("{}{}", prefix, entry.name))
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .title("Browser")
                .borders(Borders::ALL),
        )
        .highlight_style(
            Style::default()
                .bg(Color::Blue)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("➤ ");

    
    let mut state = ListState::default();
    state.select(Some(_app.selected_index));
    frame.render_stateful_widget(list, body_chunks[0], &mut state);



    // Right pane: Preview / lyrics / Metadata placeholder
    let detail_text = if let Some(file) = &_app.active_file {
        format!("Selected for playback:\n\n{}", file)
    } else {
        "Track Preview / Lyrics / Metadata\n\n[No file selected]".to_string()
    };
    
    let right_pane = Paragraph::new(detail_text)
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .title("Details")
                .borders(Borders::ALL),
        );


    frame.render_widget(right_pane, body_chunks[1]);

    // --- Footer ---
    let footer = Paragraph::new("q: quit   space: play/pause   ←/→: seek")
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL));

    frame.render_widget(footer, chunks[2]);
}
