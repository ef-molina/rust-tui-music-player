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

use crate::app::{AppState, FocusPane, InputMode, LyricsStatus};
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

fn display_album_name(raw: &str) -> &str {
    if let Some((year, rest)) = raw.split_once(" - ")
        && year.len() == 4
        && year.chars().all(|c| c.is_ascii_digit())
    {
        return rest;
    }
    raw
}

fn marquee_text(text: &str, max_width: usize, ui_tick: u64, anchor_tick: u64) -> String {
    let text_width = UnicodeWidthStr::width(text);
    if text_width <= max_width {
        return text.to_string();
    }

    let gap = "   ";
    let full = format!("{text}{gap}");
    let full_width = UnicodeWidthStr::width(full.as_str());

    // ----- timing -------
    let tick_rate = 100u64; // ~100 ticks/sec
    let start_delay = tick_rate / 2; // 0.5s
    let end_delay = tick_rate / 2; // 0.5s
    let speed = 8u64; // ticks per column (higher = slower)

    let max_offset = full_width.saturating_sub(max_width) as u64;
    let scroll_duration = max_offset * speed;
    let total_duration = start_delay + scroll_duration + end_delay;

    let elapsed = (ui_tick.saturating_sub(anchor_tick)) % total_duration;

    let offset = if elapsed < start_delay {
        // start pause
        0
    } else if elapsed < start_delay + scroll_duration {
        // scrolling phase
        (elapsed - start_delay) / speed
    } else {
        // end pause (hold final position)
        max_offset
    };

    let offset = offset.min(max_offset) as usize;

    // ----- render window -----
    let mut out = String::new();
    let mut skipped = 0;

    for ch in full.chars() {
        let w = UnicodeWidthStr::width(ch.to_string().as_str());

        if skipped + w <= offset {
            skipped += w;
            continue;
        }

        if UnicodeWidthStr::width(out.as_str()) + w > max_width {
            break;
        }

        out.push(ch);
    }

    out
}

// helper to wrap text into multiple lines fitting a given width, preserving word boundaries
fn wrap_text_to_width(text: &str, max_width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut width = 0;

    for word in text.split_whitespace() {
        let w = UnicodeWidthStr::width(word);

        if width > 0 && width + 1 + w > max_width {
            lines.push(current);
            current = word.to_string();
            width = w;
        } else {
            if width > 0 {
                current.push(' ');
                width += 1;
            }
            current.push_str(word);
            width += w;
        }
    }

    if !current.is_empty() {
        lines.push(current);
    }

    // Preserve empty lines (instrumentals etc.)
    if lines.is_empty() {
        lines.push(String::new());
    }

    lines
}

fn render_command_bar(frame: &mut Frame, area: Rect, app: &AppState) {
    use crate::app::InputMode;

    let InputMode::Command(cmd) = &app.input_mode else {
        return;
    };

    let cursor_visible = (app.ui_tick / 25) % 2 == 0;
    let cursor = if cursor_visible { "█" } else { " " };

    let text = format!("/{buffer}{cursor}", buffer = cmd.buffer, cursor = cursor,);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow))
        .title("Command");

    frame.render_widget(
        Paragraph::new(text)
            .style(Style::default().fg(Color::White))
            .block(block),
        area,
    );
}

// -----------------------------------------------------------------------------
// Pane renderers
// -----------------------------------------------------------------------------
fn render_browser(frame: &mut Frame, area: Rect, app: &AppState) {
    let items: Vec<ListItem> = app
        .browser_entries
        .iter()
        .enumerate()
        .filter(|(_, e)| e.is_dir)
        .map(|(i, e)| {
            let display = display_album_name(&e.name);
            let available = area.width.saturating_sub(6) as usize;

            let name = if app.focus == FocusPane::Browser && i == app.selected_index {
                marquee_text(display, available, app.ui_tick, app.selection_anchor_tick)
            } else {
                display.to_string()
            };

            ListItem::new(format!("📁 {}", name))
        })
        .collect();

    let block = Block::default()
        .title("[B]rowser")
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

// -----------------------------------------------------------------------------
// Album / Playlist renderer
// -----------------------------------------------------------------------------
fn render_album(frame: &mut Frame, area: Rect, app: &AppState) {
    let (title, tracks) = if let Some(dir) = &app.active_album_dir {
        (
            dir.file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("[T]racks")
                .to_string(),
            &app.album_entries,
        )
    } else {
        ("[T]racks".to_string(), &Vec::new())
    };

    let playing_name = app
        .player
        .current_track
        .as_ref()
        .and_then(|p| p.file_name())
        .and_then(|s| s.to_str());

    let items: Vec<ListItem> = tracks
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let is_playing = playing_name.map(|n| n == e.name).unwrap_or(false);
            let icon = if is_playing { "▶ " } else { "🎵 " };

            let style = if is_playing {
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            // ListItem::new(format!("{icon}{}", e.name)).style(style)
            let available = area.width.saturating_sub(6) as usize;

            let name = if app.focus == FocusPane::Album && i == app.album_selected {
                marquee_text(&e.name, available, app.ui_tick, app.selection_anchor_tick)
            } else {
                e.name.clone()
            };

            ListItem::new(format!("{icon}{name}")).style(style)
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

// -----------------------------------------------------------------------------
// Mini lyric renderer
// -----------------------------------------------------------------------------
fn render_lyrics_mini(frame: &mut Frame, area: Rect, app: &AppState) {
    let block = Block::default().title("[L]yrics").borders(Borders::ALL);

    let paragraph = match &app.lyrics {
        LyricsStatus::Loading => Paragraph::new("Loading lyrics…")
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::DarkGray))
            .block(block),

        LyricsStatus::None => Paragraph::new("No lyrics available")
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::DarkGray))
            .block(block),

        LyricsStatus::Loaded(lyrics) => {
            let max_width = area.width.saturating_sub(2) as usize;
            let max_height = area.height.saturating_sub(2) as usize;

            let mut out: Vec<Line> = Vec::new();

            // Wrap current lyric first (highest priority)
            if let Some(cur) = lyrics.current() {
                for row in wrap_text_to_width(&cur.text, max_width) {
                    out.push(Line::from(Span::styled(
                        row,
                        Style::default()
                            .fg(Color::Green)
                            .add_modifier(Modifier::BOLD),
                    )));
                }
            }

            // Prepend previous lyric if space allows
            if let Some(prev) = lyrics.previous() {
                let wrapped_prev = wrap_text_to_width(&prev.text, max_width);
                if wrapped_prev.len() + out.len() <= max_height {
                    let mut prev_lines = Vec::new();
                    for row in wrapped_prev {
                        prev_lines.push(Line::from(Span::styled(
                            row,
                            Style::default().fg(Color::DarkGray),
                        )));
                    }
                    prev_lines.extend(out);
                    out = prev_lines;
                }
            }

            // Append next lyric if space allows
            if let Some(next) = lyrics.next() {
                let wrapped_next = wrap_text_to_width(&next.text, max_width);
                if out.len() + wrapped_next.len() <= max_height {
                    for row in wrapped_next {
                        out.push(Line::from(Span::styled(
                            row,
                            Style::default().fg(Color::DarkGray),
                        )));
                    }
                }
            }

            Paragraph::new(out)
                .alignment(Alignment::Center)
                .block(block)
        }
    };

    frame.render_widget(paragraph, area);
}

// -----------------------------------------------------------------------------
// Full lyrics pane
// -----------------------------------------------------------------------------
fn render_lyrics_full(frame: &mut Frame, area: Rect, app: &AppState) {
    struct VisualLine {
        logical_index: usize,
        text: String,
    }

    let block = Block::default()
        .title("[L]yrics")
        .borders(Borders::ALL)
        .border_style(
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        );

    let paragraph = match &app.lyrics {
        LyricsStatus::Loading => Paragraph::new("Loading lyrics…")
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::DarkGray))
            .block(block),

        LyricsStatus::None => Paragraph::new("No lyrics available")
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::DarkGray))
            .block(block),

        LyricsStatus::Loaded(lyrics) => {
            let lines = &lyrics.lines;

            if lines.is_empty() {
                Paragraph::new("No lyrics available")
                    .alignment(Alignment::Center)
                    .style(Style::default().fg(Color::DarkGray))
                    .block(block)
            } else {
                let logical_center = app.lyric_scroll.min(lines.len() - 1);

                let max_width = area.width.saturating_sub(2) as usize;

                let mut visual_lines: Vec<VisualLine> = Vec::new();
                let mut logical_to_visual_start: Vec<usize> = Vec::new();

                for (i, line) in lines.iter().enumerate() {
                    logical_to_visual_start.push(visual_lines.len());

                    let wrapped = wrap_text_to_width(&line.text, max_width);
                    for row in wrapped {
                        visual_lines.push(VisualLine {
                            logical_index: i,
                            text: row,
                        });
                    }
                }

                let visual_center = logical_to_visual_start[logical_center];

                let inner_height = area.height.saturating_sub(2) as usize;
                let half = inner_height / 2;

                let start = visual_center.saturating_sub(half);
                let end = (start + inner_height).min(visual_lines.len());

                let text: Vec<Line> = (start..end)
                    .map(|i| {
                        let v = &visual_lines[i];
                        let is_active = v.logical_index == logical_center;

                        let style = if is_active {
                            Style::default()
                                .fg(Color::Green)
                                .add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(Color::Gray)
                        };

                        Line::from(Span::styled(v.text.clone(), style))
                    })
                    .collect();

                Paragraph::new(text)
                    .alignment(Alignment::Center)
                    .block(block)
            }
        }
    };

    frame.render_widget(paragraph, area);
}

// -----------------------------------------------------------------------------
// Main draw
// -----------------------------------------------------------------------------
pub fn draw(frame: &mut Frame, app: &AppState) {
    let size = frame.size();
    frame.render_widget(Clear, size);

    let chunks = if matches!(app.input_mode, InputMode::Command(_)) {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // header
                Constraint::Min(1),    // body
                Constraint::Length(3), // command bar
                Constraint::Length(7), // footer
            ])
            .split(size)
    } else {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // header
                Constraint::Min(1),    // body
                Constraint::Length(7), // footer
            ])
            .split(size)
    };
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

    // If in command mode, we need to render the command bar above the footer
    let footer_index = if matches!(app.input_mode, InputMode::Command(_)) {
        render_command_bar(frame, chunks[2], app);
        3
    } else {
        2
    };

    let footer_block = Block::default()
        .borders(Borders::ALL)
        .title("[N]ow Playing");
    let footer_inner = footer_block.inner(chunks[footer_index]);
    frame.render_widget(footer_block, chunks[footer_index]);

    let footer_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // track title
            Constraint::Length(1), // artist + album
            Constraint::Length(1), // controls
            Constraint::Min(1),    // progress bar
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

    let artist_label = app
        .now_playing
        .as_ref()
        .map(|n| n.artist.as_str())
        .unwrap_or("Unknown Artist");

    // let album_label = app
    //     .now_playing
    //     .as_ref()
    //     .map(|n| n.album.as_str())
    //     .unwrap_or("");

    // Track title + playback symbol
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

    // Artist name
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            artist_label,
            Style::default().fg(Color::DarkGray),
        )))
        .alignment(Alignment::Center),
        footer_rows[1],
    );

    // Controls hint
    frame.render_widget(
        Paragraph::new(
            "←/→ seek  < prev  > next   s stop   space pause   b browser   t tracks   q quit",
        )
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::DarkGray)),
        footer_rows[2],
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
        footer_rows[3],
    );
}
