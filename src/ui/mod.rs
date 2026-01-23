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
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
};

use crate::app::AppState;
use crate::player::PlaybackState; // NEW: needed for footer + playing indicator

// -----------------------------------------------------------------------------
// UI-only helper to truncate long filenames nicely (middle truncation)
// -----------------------------------------------------------------------------
fn truncate_middle(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        return s.to_string();
    }

    let keep = max_len / 2 - 1;
    format!("{}…{}", &s[..keep], &s[s.len() - keep..])
}

pub fn draw(frame: &mut Frame, app: &AppState) {
    let size = frame.size();

    frame.render_widget(Clear, size); // clear the background

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header
            Constraint::Min(1),    // body
            Constraint::Length(5), // footer
        ])
        .split(size);

    // -------------------------------------------------------------------------
    // Extract currently playing filename ONCE for reuse everywhere in UI
    // -------------------------------------------------------------------------
    let playing_name: Option<&str> = app
        .player
        .current_track
        .as_ref()
        .and_then(|p| p.file_name())
        .and_then(|s| s.to_str());

    // --- Header ---------------------------------------------------------------
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

    let header_text = format!("{title} — {path_display}");

    let header = Paragraph::new(header_text)
        .alignment(Alignment::Center)
        .style(Style::default().add_modifier(Modifier::BOLD))
        .block(Block::default().borders(Borders::ALL));

    frame.render_widget(header, chunks[0]);

    // --- Body (split plane) ----------------------------------------------------
    let body_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(30), // left pane (file browser)
            Constraint::Percentage(70), // right pane (track details)
        ])
        .split(chunks[1]);

    // --- Left pane: filesystem browser ----------------------------------------

    // browser entries highlight the *playing* track
    let items: Vec<ListItem> = app
        .browser_entries
        .iter()
        .map(|entry| {
            let is_playing = playing_name
                .map(|name| !entry.is_dir && entry.name == name)
                .unwrap_or(false);

            let icon = if entry.is_dir {
                "📁 "
            } else if is_playing {
                "▶ "
            } else {
                "🎵 "
            };

            let style = if is_playing {
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            ListItem::new(format!("{}{}", icon, entry.name)).style(style)
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

    // --- Right pane: Details ---------------------------------------------------

    // reuse playing_name instead of re-deriving it
    let detail_text = match playing_name {
        Some(name) => format!("Selected for playback:\n\n{}", name),
        None => "Nothing selected".to_string(),
    };

    let right_pane = Paragraph::new(detail_text)
        .alignment(Alignment::Center)
        .block(Block::default().title("Details").borders(Borders::ALL));

    frame.render_widget(right_pane, body_chunks[1]);

    // --- Footer ---------------------------------------------------------------

    // 1) Draw a single outer footer block that owns ALL borders
    let footer_block = Block::default().borders(Borders::ALL).title("Now Playing");

    // 2) Get the inner area so content does not overwrite borders
    let footer_inner = footer_block.inner(chunks[2]);

    frame.render_widget(footer_block, chunks[2]);

    // 3) Split footer content vertically (stacked layout)
    let footer_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // now playing line
            Constraint::Length(1), // controls line
            Constraint::Min(1),    // reserved (progress bar later)
        ])
        .split(footer_inner);

    // -------------------------------------------------------------------------
    // Row 1: Now Playing (centered, no borders)
    // -------------------------------------------------------------------------

    let (symbol, color) = match app.player.state {
        PlaybackState::Playing => ("▶", Color::Green),
        PlaybackState::Paused => ("⏸", Color::Yellow),
        PlaybackState::Stopped => ("■", Color::Gray),
    };

    let footer_track = playing_name
        .map(|s| truncate_middle(s, 40))
        .unwrap_or_else(|| "Stopped".to_string());

    let status_line = Line::from(vec![
        Span::styled(
            symbol,
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::raw(footer_track),
    ]);

    let status_widget = Paragraph::new(status_line).alignment(Alignment::Center);

    frame.render_widget(status_widget, footer_rows[0]);

    // -------------------------------------------------------------------------
    // Row 2: Controls (centered, subtle color)
    // -------------------------------------------------------------------------

    let controls = Paragraph::new("←/→ seek   s stop   space pause   n now playing   q quit")
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::DarkGray));

    frame.render_widget(controls, footer_rows[1]);

    // -------------------------------------------------------------------------
    // Row 3: Reserved for progress bar / timing (empty for now)
    // -------------------------------------------------------------------------
    // frame.render_widget(progress_bar, footer_rows[2]); // later
}
