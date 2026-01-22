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
    style::{Modifier, Style},
    widgets::{Block, Borders, Paragraph, Clear},
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

    // --- Body ---
    let body = Paragraph::new("Lyrics / file browser will live here")
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL));

    frame.render_widget(body, chunks[1]);

    // --- Footer ---
    let footer = Paragraph::new("q: quit   space: play/pause   ←/→: seek")
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL));

    frame.render_widget(footer, chunks[2]);
}
