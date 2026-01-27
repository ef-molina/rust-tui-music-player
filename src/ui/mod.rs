//! Terminal UI rendering module.
//!
//! Responsible for drawing the application UI using `ratatui`.
//!
//! Layout (top to bottom):
//! - Header: application title / track info
//! - Body: main content area
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

use crate::app::{AppState, FocusPane};
use crate::player::PlaybackState;
use unicode_width::UnicodeWidthStr;

// -----------------------------------------------------------------------------
// UI helpers
// -----------------------------------------------------------------------------
fn truncate_middle(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        return s.to_string();
    }
    let keep = max_len / 2 - 1;
    format!("{}…{}", &s[..keep], &s[s.len() - keep..])
}

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
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(5),
        ])
        .split(size);

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

    let playing_name = app
        .player
        .current_track
        .as_ref()
        .and_then(|p| p.file_name())
        .and_then(|s| s.to_str());

    // -------------------------------------------------------------------------
    // Left pane: Browser
    // -------------------------------------------------------------------------
    // Browser shows ONLY directories (for navigation).
    // Album tracks are shown in the right pane, not here.
    let browser_items: Vec<ListItem> = app
        .browser_entries
        .iter()
        .filter(|entry| entry.is_dir) // Show directories only
        .map(|entry| {
            let icon = "📁 ";
            ListItem::new(format!("{}{}", icon, entry.name)).style(Style::default())
        })
        .collect();

    // Browser pane: highlight with green border if focused, normal otherwise.
    let browser_block = Block::default()
        .title("Browser")
        .borders(Borders::ALL)
        .border_style(if app.focus == FocusPane::Browser {
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        });

    let browser = List::new(browser_items)
        .block(browser_block)
        .highlight_style(
            Style::default()
                .bg(Color::Blue)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("➤ ");

    let mut browser_state = ListState::default();
    browser_state.select(Some(app.selected_index));
    frame.render_stateful_widget(browser, body_chunks[0], &mut browser_state);

    // -------------------------------------------------------------------------
    // Right pane: Album / Playlist
    // -------------------------------------------------------------------------
    // Album pane is shown based on active_album_dir, NOT focus.
    // This allows album context to persist regardless of which pane is focused.
    let (album_title, track_entries): (String, Vec<_>) =
        if let Some(album_dir) = &app.active_album_dir {
            (
                album_dir
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("Album")
                    .to_string(),
                app.album_entries.clone(),
            )
        } else if app.current_dir == app.root_dir {
            (
                "Loose Tracks".to_string(),
                app.browser_entries
                    .iter()
                    .filter(|e| !e.is_dir)
                    .cloned()
                    .collect(),
            )
        } else {
            ("No Album".to_string(), Vec::new())
        };

    let album_items: Vec<ListItem> = track_entries
        .iter()
        .map(|entry| {
            let is_playing = playing_name.map(|n| n == entry.name).unwrap_or(false);

            let icon = if is_playing { "▶ " } else { "🎵 " };
            let style = if is_playing {
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            ListItem::new(format!("{icon}{}", entry.name)).style(style)
        })
        .collect();

    let album_block = Block::default()
        .title(album_title)
        .borders(Borders::ALL)
        .border_style(if app.focus == FocusPane::Album {
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        });

    if album_items.is_empty() {
        frame.render_widget(
            Paragraph::new("(No tracks to display)")
                .alignment(Alignment::Center)
                .style(Style::default().fg(Color::DarkGray))
                .block(album_block),
            body_chunks[1],
        );
    } else {
        let album = List::new(album_items)
            .block(album_block)
            .highlight_style(
                Style::default()
                    .bg(Color::Blue)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("➤ ");

        let mut album_state = ListState::default();
        album_state.select(Some(app.album_selected));
        frame.render_stateful_widget(album, body_chunks[1], &mut album_state);
    }

    // -------------------------------------------------------------------------
    // Footer
    // -------------------------------------------------------------------------
    let footer_block = Block::default().borders(Borders::ALL).title("Now Playing");
    let footer_inner = footer_block.inner(chunks[2]);
    frame.render_widget(footer_block, chunks[2]);

    let footer_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .split(footer_inner);

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

    frame.render_widget(
        Paragraph::new(
            "←/→ seek  < prev  > next   s stop   space pause   b browser   t tracks   q quit",
        )
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::DarkGray)),
        footer_rows[1],
    );

    let pos = app.player.metrics.position;
    let dur = app.player.metrics.duration;

    let progress = match (pos, dur) {
        (Some(p), Some(d)) if d > 0.0 => (p / d).clamp(0.0, 1.0),
        _ => 0.0,
    };

    let time_label = format!("{} / {}", format_time(pos), format_time(dur));
    let reserved = UnicodeWidthStr::width(time_label.as_str()) + 3;

    let bar_width = footer_rows[2].width.saturating_sub(reserved as u16).max(1) as usize;

    let filled = (progress * bar_width as f64).round() as usize;

    let bar = format!(
        "[{}{}]",
        "█".repeat(filled),
        "─".repeat(bar_width.saturating_sub(filled)),
    );

    let progress_line = Line::from(vec![
        Span::styled(bar, Style::default().fg(Color::Green)),
        Span::raw(" "),
        Span::styled(time_label, Style::default().fg(Color::Gray)),
    ]);

    frame.render_widget(
        Paragraph::new(progress_line).alignment(Alignment::Center),
        footer_rows[2],
    );
}
