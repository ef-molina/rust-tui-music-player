//! Terminal UI rendering module.
//!
//! Responsible for drawing the application UI using `ratatui`.
//!
//! Layout (top to bottom):
//! - Header: application title / current path
//! - Body: browser (left) + album/lyrics (right)
//! - Footer: playback controls and progress
//!
//! Design rules:
//! - Pure rendering only
//! - Read-only access to AppState
//! - No input handling or side effects

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
};

use crate::app::{AppState, FocusPane};
use crate::player::PlaybackState;
use unicode_width::UnicodeWidthStr;

// -----------------------------------------------------------------------------
// Small helpers
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

// -----------------------------------------------------------------------------
// Pane renderers
// -----------------------------------------------------------------------------
fn render_browser(frame: &mut Frame, area: Rect, app: &AppState) {
    let items: Vec<ListItem> = app
        .browser_entries
        .iter()
        .filter(|e| e.is_dir)
        .map(|e| ListItem::new(format!("📁 {}", e.name)))
        .collect();

    let block = Block::default()
        .title("Browser")
        .borders(Borders::ALL)
        .border_style(if app.focus == FocusPane::Browser {
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        });

    let list = List::new(items)
        .block(block)
        .highlight_style(
            Style::default()
                .bg(Color::Blue)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("➤ ");

    let mut state = ListState::default();
    state.select(Some(app.selected_index));
    frame.render_stateful_widget(list, area, &mut state);
}

fn render_album(frame: &mut Frame, area: Rect, app: &AppState) {
    let (title, tracks) = if let Some(dir) = &app.active_album_dir {
        (
            dir.file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("Album")
                .to_string(),
            &app.album_entries,
        )
    } else {
        ("No Album".to_string(), &Vec::new())
    };

    let playing_name = app
        .player
        .current_track
        .as_ref()
        .and_then(|p| p.file_name())
        .and_then(|s| s.to_str());

    let items: Vec<ListItem> = tracks
        .iter()
        .map(|e| {
            let is_playing = playing_name.map(|n| n == e.name).unwrap_or(false);
            let icon = if is_playing { "▶ " } else { "🎵 " };

            let style = if is_playing {
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            ListItem::new(format!("{icon}{}", e.name)).style(style)
        })
        .collect();

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(if app.focus == FocusPane::Album {
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        });

    if items.is_empty() {
        frame.render_widget(
            Paragraph::new("(No tracks)")
                .alignment(Alignment::Center)
                .style(Style::default().fg(Color::DarkGray))
                .block(block),
            area,
        );
    } else {
        let list = List::new(items)
            .block(block)
            .highlight_style(
                Style::default()
                    .bg(Color::Blue)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("➤ ");

        let mut state = ListState::default();
        state.select(Some(app.album_selected));
        frame.render_stateful_widget(list, area, &mut state);
    }
}

fn render_lyrics_mini(frame: &mut Frame, area: Rect, app: &AppState) {
    let block = Block::default().title("Lyrics").borders(Borders::ALL);

    let Some(lyrics) = &app.lyrics else {
        frame.render_widget(
            Paragraph::new("No lyrics")
                .alignment(Alignment::Center)
                .style(Style::default().fg(Color::DarkGray))
                .block(block),
            area,
        );
        return;
    };

    let mut lines: Vec<Line> = Vec::new();

    if let Some(prev) = lyrics.previous() {
        lines.push(Line::from(Span::styled(
            prev.text.clone(),
            Style::default().fg(Color::DarkGray),
        )));
    }

    if let Some(cur) = lyrics.current() {
        lines.push(Line::from(Span::styled(
            cur.text.clone(),
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        )));
    }

    if let Some(next) = lyrics.next() {
        lines.push(Line::from(Span::styled(
            next.text.clone(),
            Style::default().fg(Color::DarkGray),
        )));
    }

    frame.render_widget(
        Paragraph::new(lines)
            .alignment(Alignment::Center)
            .block(block),
        area,
    );
}

fn render_lyrics_full(frame: &mut Frame, area: Rect, app: &AppState) {
    let block = Block::default()
        .title("Lyrics")
        .borders(Borders::ALL)
        .border_style(
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        );

    let Some(lyrics) = &app.lyrics else {
        frame.render_widget(
            Paragraph::new("No lyrics available")
                .alignment(Alignment::Center)
                .style(Style::default().fg(Color::DarkGray))
                .block(block),
            area,
        );
        return;
    };

    let lines = &lyrics.lines;
    if lines.is_empty() {
        frame.render_widget(
            Paragraph::new("No lyrics available")
                .alignment(Alignment::Center)
                .block(block),
            area,
        );
        return;
    }

    let center = app.lyric_scroll.min(lines.len() - 1);

    // How many lines can we render inside the block?
    let inner_height = area.height.saturating_sub(2) as usize; // minus borders
    let half = inner_height / 2;

    // Compute window bounds
    let start = center.saturating_sub(half);
    let end = (start + inner_height).min(lines.len());

    let text: Vec<Line> = (start..end)
        .map(|i| {
            let line = &lines[i];
            let is_active = i == center;

            let style = if is_active {
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray)
            };

            Line::from(Span::styled(line.text.clone(), style))
        })
        .collect();

    frame.render_widget(
        Paragraph::new(text)
            .alignment(Alignment::Center)
            .block(block),
        area,
    );
}

// -----------------------------------------------------------------------------
// Main draw
// -----------------------------------------------------------------------------
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
    // Header
    // -------------------------------------------------------------------------
    let path_display = app
        .current_dir
        .strip_prefix(&app.root_dir)
        .ok()
        .and_then(|p| (!p.as_os_str().is_empty()).then_some(p))
        .map(|p| format!("~/{}", p.display()))
        .unwrap_or_else(|| "~/".to_string());

    frame.render_widget(
        Paragraph::new(format!("Rust TUI Music Player — {path_display}"))
            .alignment(Alignment::Center)
            .style(Style::default().add_modifier(Modifier::BOLD))
            .block(Block::default().borders(Borders::ALL)),
        chunks[0],
    );

    // -------------------------------------------------------------------------
    // Body
    // -------------------------------------------------------------------------
    let body_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(30), // browser
            Constraint::Percentage(70), // album + lyrics
        ])
        .split(chunks[1]);

    render_browser(frame, body_chunks[0], app);

    // Right pane layout depends on focus
    let right_chunks = match app.focus {
        FocusPane::Lyrics => Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1)])
            .split(body_chunks[1]),

        _ => Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(5),    // album
                Constraint::Length(5), // lyrics mini
            ])
            .split(body_chunks[1]),
    };

    match app.focus {
        FocusPane::Lyrics => {
            render_lyrics_full(frame, right_chunks[0], app);
        }
        _ => {
            render_album(frame, right_chunks[0], app);
            render_lyrics_mini(frame, right_chunks[1], app);
        }
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

    let playing_name = app
        .player
        .current_track
        .as_ref()
        .and_then(|p| p.file_name())
        .and_then(|s| s.to_str());

    let track_label = playing_name
        .map(|s| truncate_middle(s, 40))
        .unwrap_or_else(|| "Stopped".to_string());

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                symbol,
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            Span::raw(track_label),
        ]))
        .alignment(Alignment::Center),
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

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(bar, Style::default().fg(Color::Green)),
            Span::raw(" "),
            Span::styled(time_label, Style::default().fg(Color::Gray)),
        ]))
        .alignment(Alignment::Center),
        footer_rows[2],
    );
}
