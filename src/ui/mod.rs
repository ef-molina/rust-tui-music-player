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
use crate::player::PlaybackState;

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

// -----------------------------------------------------------------------------
// UI-only helper to format seconds as mm:ss
// -----------------------------------------------------------------------------
fn format_time(seconds: Option<f64>) -> String {
    let secs = match seconds {
        Some(s) => s as u64,
        None => return "--:--".to_string(),
    };

    format!("{:02}:{:02}", secs / 60, secs % 60)
}

pub fn draw(frame: &mut Frame, app: &AppState) {
    let size = frame.size();

    frame.render_widget(Clear, size);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header
            Constraint::Min(1),    // body
            Constraint::Length(5), // footer
        ])
        .split(size);

    // -------------------------------------------------------------------------
    // Extract currently playing filename ONCE
    // -------------------------------------------------------------------------
    let playing_name: Option<&str> = app
        .player
        .current_track
        .as_ref()
        .and_then(|p| p.file_name())
        .and_then(|s| s.to_str());

    // -------------------------------------------------------------------------
    // Header
    // -------------------------------------------------------------------------
    let title = "Rust TUI Music Player";

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

    let header = Paragraph::new(format!("{title} — {path_display}"))
        .alignment(Alignment::Center)
        .style(Style::default().add_modifier(Modifier::BOLD))
        .block(Block::default().borders(Borders::ALL));

    frame.render_widget(header, chunks[0]);

    // -------------------------------------------------------------------------
    // Body
    // -------------------------------------------------------------------------
    let body_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
        .split(chunks[1]);

    // --- Left pane: filesystem browser ----------------------------------------
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

    // --- Right pane: details ---------------------------------------------------
    let detail_text = match playing_name {
        Some(name) => format!("Selected for playback:\n\n{}", name),
        None => "Nothing selected".to_string(),
    };

    let right_pane = Paragraph::new(detail_text)
        .alignment(Alignment::Center)
        .block(Block::default().title("Details").borders(Borders::ALL));

    frame.render_widget(right_pane, body_chunks[1]);

    // -------------------------------------------------------------------------
    // Footer
    // -------------------------------------------------------------------------
    let footer_block = Block::default().borders(Borders::ALL).title("Now Playing");

    let footer_inner = footer_block.inner(chunks[2]);
    frame.render_widget(footer_block, chunks[2]);

    let footer_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // now playing
            Constraint::Length(1), // controls
            Constraint::Min(1),    // progress
        ])
        .split(footer_inner);

    // --- Row 1: Now Playing ----------------------------------------------------
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

    frame.render_widget(
        Paragraph::new(status_line).alignment(Alignment::Center),
        footer_rows[0],
    );

    // --- Row 2: Controls -------------------------------------------------------
    frame.render_widget(
        Paragraph::new("←/→ seek   s stop   space pause   n now playing   q quit")
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::DarkGray)),
        footer_rows[1],
    );

    // --- Row 3: Progress bar ---------------------------------------------------
    let pos = app.player.metrics.position;
    let dur = app.player.metrics.duration;

    let progress = match (pos, dur) {
        (Some(p), Some(d)) if d > 0.0 => (p / d).clamp(0.0, 1.0),
        _ => 0.0,
    };

    // Calculate the width of the progress bar, reserving space for the time label
    let time_label = format!("{} / {}", format_time(pos), format_time(dur));
    let reserved = time_label.len() + 3; // space + brackets

    let bar_width = footer_rows[2].width.saturating_sub(reserved as u16).max(1) as usize;

    let filled = (progress * bar_width as f64).round() as usize;

    let bar = format!(
        "[{}{}]",
        "█".repeat(filled),
        "─".repeat(bar_width.saturating_sub(filled)),
    );

    let timing = time_label;

    let progress_line = Line::from(vec![
        Span::styled(bar, Style::default().fg(Color::Green)),
        Span::raw(" "),
        Span::styled(timing, Style::default().fg(Color::Gray)),
    ]);

    frame.render_widget(
        Paragraph::new(progress_line).alignment(Alignment::Center),
        footer_rows[2],
    );
}
