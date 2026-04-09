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

use crate::app::{AppState, FocusPane, InputMode, LyricsStatus, SearchStatus, StatusLevel};
use crate::event::commands::filtered_command_specs;
use crate::player::PlaybackState;
use unicode_width::UnicodeWidthStr;

const ACCENT: Color = Color::Rgb(102, 187, 160);
const HIGHLIGHT_BG: Color = Color::Rgb(38, 68, 94);
const HIGHLIGHT_FG: Color = Color::Rgb(245, 247, 250);
const MUTED: Color = Color::Rgb(132, 145, 160);
const SUBTLE: Color = Color::Rgb(84, 94, 108);
const WARNING: Color = Color::Rgb(242, 201, 76);
const DANGER: Color = Color::Rgb(242, 107, 107);
const SCRIM: Color = Color::Rgb(8, 10, 14);
const BADGE_BG: Color = Color::Rgb(28, 34, 42);

// -----------------------------------------------------------------------------
// Small helpers
// -----------------------------------------------------------------------------
fn pane_border_style(active: bool) -> Style {
    if active {
        Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(SUBTLE)
    }
}

fn selection_style() -> Style {
    Style::default()
        .bg(HIGHLIGHT_BG)
        .fg(HIGHLIGHT_FG)
        .add_modifier(Modifier::BOLD)
}

fn muted_style() -> Style {
    Style::default().fg(MUTED)
}

fn badge_span(text: &str, fg: Color) -> Span<'static> {
    Span::styled(
        format!(" {text} "),
        Style::default().fg(fg).bg(BADGE_BG).add_modifier(Modifier::BOLD),
    )
}

fn truncate_middle(s: &str, max_len: usize) -> String {
    let width = UnicodeWidthStr::width(s);
    if width <= max_len {
        return s.to_string();
    }

    if max_len <= 1 {
        return "…".to_string();
    }

    let target = max_len.saturating_sub(1);
    let front_target = target / 2;
    let back_target = target.saturating_sub(front_target);

    let mut front = String::new();
    let mut front_width = 0;
    for ch in s.chars() {
        let ch_width = UnicodeWidthStr::width(ch.to_string().as_str());
        if front_width + ch_width > front_target {
            break;
        }
        front.push(ch);
        front_width += ch_width;
    }

    let chars: Vec<char> = s.chars().collect();
    let mut back = String::new();
    let mut back_width = 0;
    for ch in chars.iter().rev() {
        let ch_width = UnicodeWidthStr::width(ch.to_string().as_str());
        if back_width + ch_width > back_target {
            break;
        }
        back.insert(0, *ch);
        back_width += ch_width;
    }

    format!("{front}…{back}")
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

fn display_track_name(raw: &str) -> String {
    let stem = raw.rsplit_once('.').map(|(stem, _)| stem).unwrap_or(raw);
    let trimmed = stem.trim();

    if let Some((prefix, rest)) = trimmed.split_once(". ")
        && !prefix.is_empty()
        && prefix.chars().all(|ch| ch.is_ascii_digit())
    {
        return rest.trim().to_string();
    }

    trimmed.to_string()
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let vertical_margin = 100u16.saturating_sub(percent_y) / 2;
    let horizontal_margin = 100u16.saturating_sub(percent_x) / 2;

    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(vertical_margin),
            Constraint::Percentage(percent_y),
            Constraint::Percentage(vertical_margin),
        ])
        .split(r);

    let body = popup_layout[1];

    let body_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(horizontal_margin),
            Constraint::Percentage(percent_x),
            Constraint::Percentage(horizontal_margin),
        ])
        .split(body);

    body_layout[1]
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
    let start_delay = tick_rate; // 1.0s
    let end_delay = tick_rate; // 1.0s
    let speed = 14u64; // ticks per column (higher = slower)

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

fn mode_label(app: &AppState) -> &'static str {
    match app.input_mode {
        InputMode::Normal => "Normal",
        InputMode::Command(_) => "Command",
        InputMode::Search => "Search",
    }
}

fn focus_label(app: &AppState) -> &'static str {
    match app.focus {
        FocusPane::Browser => "Browser",
        FocusPane::Album => "Tracks",
        FocusPane::Lyrics => "Lyrics",
    }
}

fn playback_badge(app: &AppState) -> (&'static str, Color) {
    match app.player.state {
        PlaybackState::Playing => ("Playing", ACCENT),
        PlaybackState::Paused => ("Paused", WARNING),
        PlaybackState::Stopped => ("Stopped", MUTED),
    }
}

fn lyrics_title(app: &AppState) -> &'static str {
    match app.lyrics {
        LyricsStatus::Loading => "[L]yrics · Loading",
        LyricsStatus::None => "[L]yrics · Unavailable",
        LyricsStatus::Loaded(_) => "[L]yrics · Synced",
    }
}

fn status_level_color(level: StatusLevel) -> Color {
    match level {
        StatusLevel::Info => ACCENT,
        StatusLevel::Success => ACCENT,
        StatusLevel::Warning => WARNING,
        StatusLevel::Error => DANGER,
    }
}

fn current_status(app: &AppState) -> (StatusLevel, String) {
    if let Some(status) = &app.status_message {
        return (status.level, status.text.clone());
    }

    if let Some(url) = &app.active_download_url {
        return (
            StatusLevel::Info,
            format!("Downloading media from {}", truncate_middle(url, 56)),
        );
    }

    match &app.search.status {
        SearchStatus::Indexing { scanned } => {
            return (
                StatusLevel::Info,
                format!("Indexing library • {scanned} tracks discovered"),
            );
        }
        SearchStatus::Failed(error) => {
            return (StatusLevel::Error, format!("Search indexing failed: {error}"));
        }
        SearchStatus::Idle | SearchStatus::Ready => {}
    }

    match app.lyrics {
        LyricsStatus::Loading => (StatusLevel::Info, "Fetching lyrics…".to_string()),
        LyricsStatus::Loaded(_) => (StatusLevel::Success, "Lyrics synced".to_string()),
        LyricsStatus::None if !app.search.index_entries.is_empty() => (
            StatusLevel::Success,
            format!("Library ready • {} indexed tracks", app.search.index_entries.len()),
        ),
        LyricsStatus::None => (StatusLevel::Info, "Ready".to_string()),
    }
}

fn render_command_bar(frame: &mut Frame, area: Rect, app: &AppState) {
    let cursor_visible = (app.ui_tick / 25).is_multiple_of(2);
    let cursor = if cursor_visible { '█' } else { ' ' };

    let (title, prefix, buffer, cursor_index) = match &app.input_mode {
        InputMode::Command(cmd) => ("Command".to_string(), ":", cmd.buffer.as_str(), cmd.cursor),
        InputMode::Search => {
            let title = match &app.search.status {
                SearchStatus::Idle => "Search".to_string(),
                SearchStatus::Indexing { scanned } => {
                    format!("Search ({}, indexing {scanned})", app.search.results.len())
                }
                SearchStatus::Ready => format!("Search ({})", app.search.results.len()),
                SearchStatus::Failed(_) => "Search (failed)".to_string(),
            };
            (title, "/", app.search.query.as_str(), app.search.cursor)
        }
        InputMode::Normal => return,
    };

    render_text_bar(frame, area, &title, prefix, buffer, cursor_index, cursor);
}

fn render_text_bar(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    prefix: &str,
    buffer: &str,
    cursor_index: usize,
    cursor: char,
) {
    let mut text = format!("{prefix}{buffer}");
    let insert_at = prefix.len() + cursor_index.min(buffer.len());
    text.insert(insert_at, cursor);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(WARNING).add_modifier(Modifier::BOLD))
        .title(title);

    frame.render_widget(
        Paragraph::new(text)
            .style(Style::default().fg(Color::White))
            .block(block),
        area,
    );
}

fn render_command_helper(frame: &mut Frame, area: Rect, app: &AppState) {
    let query = match &app.input_mode {
        InputMode::Command(cmd) => cmd.buffer.as_str(),
        _ => return,
    };

    let matches = filtered_command_specs(query);
    let title = if query.trim().is_empty() {
        format!("Commands ({})", matches.len())
    } else {
        format!("Commands ({}) for '{}'", matches.len(), query.trim())
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(ACCENT));

    if matches.is_empty() {
        frame.render_widget(
            Paragraph::new("No commands match")
                .alignment(Alignment::Center)
                .style(muted_style())
                .block(block),
            area,
        );
        return;
    }

    let items: Vec<ListItem> = matches
        .into_iter()
        .map(|spec| {
            ListItem::new(vec![
                Line::from(vec![
                    Span::styled(
                        spec.syntax,
                        Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
                    ),
                ]),
                Line::from(Span::styled(
                    spec.description,
                    muted_style(),
                )),
            ])
        })
        .collect();

    let list = List::new(items)
        .block(block)
        .highlight_style(selection_style())
        .highlight_symbol("➤ ");

    let mut state = ListState::default();
    state.select(Some(0));
    frame.render_stateful_widget(list, area, &mut state);
}

fn render_search_results_content(frame: &mut Frame, area: Rect, app: &AppState) {
    if let SearchStatus::Failed(error) = &app.search.status {
        frame.render_widget(
            Paragraph::new(error.as_str())
                .alignment(Alignment::Center)
                .style(Style::default().fg(DANGER))
                .block(Block::default()),
            area,
        );
        return;
    }

    if app.search.query.trim().is_empty() {
        frame.render_widget(
            Paragraph::new("Type to search by artist, title, album, file name, or path")
                .alignment(Alignment::Center)
                .style(muted_style())
                .block(Block::default()),
            area,
        );
        return;
    }

    if app.search.results.is_empty() {
        frame.render_widget(
            Paragraph::new("No matches")
                .alignment(Alignment::Center)
                .style(muted_style())
                .block(Block::default()),
            area,
        );
        return;
    }

    let items: Vec<ListItem> = app
        .search
        .results
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            let primary_width = area.width.saturating_sub(6) as usize;
            let path_width = area.width.saturating_sub(4) as usize;
            let primary = match (&entry.artist, &entry.title) {
                (Some(artist), Some(title)) => format!("{artist} - {title}"),
                (None, Some(title)) => title.clone(),
                (Some(artist), None) => {
                    format!("{artist} - {}", display_track_name(&entry.file_name))
                }
                (None, None) => display_track_name(&entry.file_name),
            };
            let primary_text = if i == app.search.selected {
                marquee_text(&primary, primary_width, app.ui_tick, app.selection_anchor_tick)
            } else {
                truncate_middle(&primary, primary_width)
            };
            let path_text = truncate_middle(&entry.relative_path, path_width);

            ListItem::new(vec![
                Line::from(vec![
                    Span::raw("🎵 "),
                    Span::styled(
                        primary_text,
                        Style::default().add_modifier(Modifier::BOLD),
                    ),
                ]),
                Line::from(Span::styled(
                    format!("   {path_text}"),
                    muted_style(),
                )),
            ])
        })
        .collect();

    let list = List::new(items)
        .highlight_style(selection_style())
        .highlight_symbol("➤ ");

    let mut state = ListState::default();
    state.select(Some(app.search.selected));
    frame.render_stateful_widget(list, area, &mut state);
}

fn render_modal_backdrop(frame: &mut Frame, area: Rect) {
    frame.render_widget(
        Block::default().style(
            Style::default()
                .bg(SCRIM)
                .add_modifier(Modifier::DIM),
        ),
        area,
    );
}

fn render_search_picker(frame: &mut Frame, area: Rect, app: &AppState) {
    let modal_area = centered_rect(86, 78, area);
    let selection_label = if app.search.results.is_empty() {
        "0/0".to_string()
    } else {
        format!("{}/{}", app.search.selected + 1, app.search.results.len())
    };

    let title = match &app.search.status {
        SearchStatus::Idle => format!("Search Results ({selection_label}) - Enter plays"),
        SearchStatus::Indexing { scanned } => {
            format!("Search Results ({selection_label}, indexing {scanned}) - Enter plays")
        }
        SearchStatus::Ready => format!("Search Results ({selection_label}) - Enter plays"),
        SearchStatus::Failed(_) => "Search Results (failed) - Enter plays".to_string(),
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(pane_border_style(true));

    render_modal_backdrop(frame, area);
    frame.render_widget(Clear, modal_area);

    let inner = block.inner(modal_area);
    frame.render_widget(block, modal_area);

    let picker_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // search bar
            Constraint::Min(1),    // results
        ])
        .split(inner);

    render_text_bar(
        frame,
        picker_chunks[0],
        "Search",
        "/",
        app.search.query.as_str(),
        app.search.cursor,
        if (app.ui_tick / 25).is_multiple_of(2) {
            '█'
        } else {
            ' '
        },
    );

    render_search_results_content(frame, picker_chunks[1], app);
}

// -----------------------------------------------------------------------------
// Pane renderers
// -----------------------------------------------------------------------------
fn render_browser(frame: &mut Frame, area: Rect, app: &AppState) {
    let dir_count = app.browser_entries.iter().filter(|entry| entry.is_dir).count();
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
        .title(format!("[B]rowser · {dir_count}"))
        .borders(Borders::ALL)
        .border_style(pane_border_style(app.focus == FocusPane::Browser));

    let list = List::new(items)
        .block(block)
        .highlight_style(selection_style())
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

            let display_name = display_track_name(&e.name);
            let available = area.width.saturating_sub(6) as usize;

            let name = if app.focus == FocusPane::Album && i == app.album_selected {
                marquee_text(&display_name, available, app.ui_tick, app.selection_anchor_tick)
            } else {
                display_name
            };

            ListItem::new(format!("{icon}{name}")).style(style)
        })
        .collect();

    let block = Block::default()
        .title(format!("{title} · {}", tracks.len()))
        .borders(Borders::ALL)
        .border_style(pane_border_style(app.focus == FocusPane::Album));

    if items.is_empty() {
        frame.render_widget(
            Paragraph::new("(No tracks)")
                .alignment(Alignment::Center)
                .style(muted_style())
                .block(block),
            area,
        );
    } else {
        let list = List::new(items)
            .block(block)
            .highlight_style(selection_style())
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
    let block = Block::default()
        .title(lyrics_title(app))
        .borders(Borders::ALL)
        .border_style(pane_border_style(app.focus == FocusPane::Lyrics));

    let paragraph = match &app.lyrics {
        LyricsStatus::Loading => Paragraph::new("Loading lyrics…")
            .alignment(Alignment::Center)
            .style(muted_style())
            .block(block),

        LyricsStatus::None => Paragraph::new("No lyrics available")
            .alignment(Alignment::Center)
            .style(muted_style())
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

            if out.len() > max_height {
                out.truncate(max_height);
            } else {
                let mut prev_index = lyrics.current_index.checked_sub(1);
                let mut next_index = if lyrics.current_index + 1 < lyrics.lines.len() {
                    Some(lyrics.current_index + 1)
                } else {
                    None
                };
                let mut add_previous = true;

                while out.len() < max_height && (prev_index.is_some() || next_index.is_some()) {
                    let mut appended = false;

                    if add_previous {
                        if let Some(index) = prev_index {
                            let wrapped_prev = wrap_text_to_width(&lyrics.lines[index].text, max_width);
                            if out.len() + wrapped_prev.len() <= max_height {
                                let mut prev_lines = Vec::new();
                                for row in wrapped_prev {
                                    prev_lines.push(Line::from(Span::styled(row, muted_style())));
                                }
                                prev_lines.extend(out);
                                out = prev_lines;
                                appended = true;
                            }
                            prev_index = index.checked_sub(1);
                        }
                    } else if let Some(index) = next_index {
                        let wrapped_next = wrap_text_to_width(&lyrics.lines[index].text, max_width);
                        if out.len() + wrapped_next.len() <= max_height {
                            for row in wrapped_next {
                                out.push(Line::from(Span::styled(row, muted_style())));
                            }
                            appended = true;
                        }
                        next_index = if index + 1 < lyrics.lines.len() {
                            Some(index + 1)
                        } else {
                            None
                        };
                    }

                    add_previous = !add_previous;

                    if !appended && prev_index.is_none() && next_index.is_none() {
                        break;
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
        .title(lyrics_title(app))
        .borders(Borders::ALL)
        .border_style(pane_border_style(app.focus == FocusPane::Lyrics));

    let paragraph = match &app.lyrics {
        LyricsStatus::Loading => Paragraph::new("Loading lyrics…")
            .alignment(Alignment::Center)
            .style(muted_style())
            .block(block),

        LyricsStatus::None => Paragraph::new("No lyrics available")
            .alignment(Alignment::Center)
            .style(muted_style())
            .block(block),

        LyricsStatus::Loaded(lyrics) => {
            let lines = &lyrics.lines;

            if lines.is_empty() {
                Paragraph::new("No lyrics available")
                    .alignment(Alignment::Center)
                    .style(muted_style())
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
                                .fg(ACCENT)
                                .add_modifier(Modifier::BOLD)
                        } else {
                            muted_style()
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

fn render_statusline(frame: &mut Frame, area: Rect, app: &AppState) {
    let (level, message) = current_status(app);
    let color = status_level_color(level);
    let right_text = match app.input_mode {
        InputMode::Search => "Esc close  Enter play  ↑/↓ move",
        InputMode::Command(_) => "Enter run  Esc close",
        InputMode::Normal => "Backspace back  / search  : command",
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(SUBTLE));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(10), Constraint::Length(34)])
        .split(inner);

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            badge_span(
                match level {
                    StatusLevel::Info => "Info",
                    StatusLevel::Success => "Ready",
                    StatusLevel::Warning => "Warn",
                    StatusLevel::Error => "Error",
                },
                color,
            ),
            Span::raw(" "),
            Span::styled(truncate_middle(&message, chunks[0].width.saturating_sub(10) as usize), muted_style()),
        ])),
        chunks[0],
    );

    frame.render_widget(
        Paragraph::new(Span::styled(right_text, muted_style())).alignment(Alignment::Right),
        chunks[1],
    );
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
                Constraint::Length(3), // statusline
                Constraint::Length(7), // footer
            ])
            .split(size)
    } else {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // header
                Constraint::Min(1),    // body
                Constraint::Length(3), // statusline
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

    let header_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(SUBTLE));
    let header_inner = header_block.inner(chunks[0]);
    frame.render_widget(header_block, chunks[0]);

    let header_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(24),
            Constraint::Min(12),
            Constraint::Length(30),
        ])
        .split(header_inner);

    let (playback_text, playback_color) = playback_badge(app);

    frame.render_widget(
        Paragraph::new("Rust TUI Music Player")
            .alignment(Alignment::Left)
            .style(Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        header_chunks[0],
    );

    frame.render_widget(
        Paragraph::new(path_display)
            .alignment(Alignment::Center)
            .style(muted_style()),
        header_chunks[1],
    );

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            badge_span(playback_text, playback_color),
            Span::raw(" "),
            badge_span(mode_label(app), WARNING),
            Span::raw(" "),
            badge_span(focus_label(app), ACCENT),
        ]))
        .alignment(Alignment::Right),
        header_chunks[2],
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

    let (main_body_area, helper_area) = if matches!(app.input_mode, InputMode::Command(_)) {
        let available_height = body_chunks[1].height.saturating_sub(1);
        let helper_height = available_height.min(11);

        if helper_height >= 5 {
            let helper_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(1), Constraint::Length(helper_height)])
                .split(body_chunks[1]);
            (helper_chunks[0], Some(helper_chunks[1]))
        } else {
            (body_chunks[1], None)
        }
    } else {
        (body_chunks[1], None)
    };

    render_browser(frame, body_chunks[0], app);

    // Right pane layout depends on focus
    let right_chunks = match app.focus {
        FocusPane::Lyrics => Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1)])
            .split(main_body_area),

        _ => Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(5),    // album
                Constraint::Length(9), // lyrics mini
            ])
            .split(main_body_area),
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

    if let Some(helper_area) = helper_area {
        render_command_helper(frame, helper_area, app);
    }

    // -------------------------------------------------------------------------
    // Footer
    // -------------------------------------------------------------------------

    // If in command mode, we need to render the command bar above the statusline/footer
    let footer_index = if matches!(app.input_mode, InputMode::Command(_)) {
        render_command_bar(frame, chunks[2], app);
        render_statusline(frame, chunks[3], app);
        4
    } else {
        render_statusline(frame, chunks[2], app);
        3
    };

    let footer_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(SUBTLE))
        .title(format!("[N]ow Playing · {playback_text}"));
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

    let title_width = footer_rows[0].width.saturating_sub(6) as usize;
    let track_label = app
        .now_playing
        .as_ref()
        .map(|n| n.title.trim())
        .filter(|title| !title.is_empty())
        .map(|title| truncate_middle(title, title_width.max(1)))
        .or_else(|| {
            playing_name
                .map(display_track_name)
                .map(|s| truncate_middle(&s, title_width.max(1)))
        })
        .unwrap_or_else(|| "Stopped".to_string());

    let secondary_label = app
        .now_playing
        .as_ref()
        .map(|n| match (n.artist.trim(), n.album.trim()) {
            ("", "") => "Unknown Artist".to_string(),
            (artist, "") => artist.to_string(),
            ("", album) => album.to_string(),
            (artist, album) => format!("{artist} • {album}"),
        })
        .unwrap_or_else(|| "Unknown Artist".to_string());

    // Track title + playback symbol
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                symbol,
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            Span::styled(track_label, Style::default().fg(Color::White)),
        ]))
        .alignment(Alignment::Center),
        footer_rows[0],
    );

    // Artist + album metadata
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            secondary_label,
            muted_style(),
        )))
        .alignment(Alignment::Center),
        footer_rows[1],
    );

    // Controls hint
    frame.render_widget(
        Paragraph::new(
            "←/→ seek  < prev  > next   s stop   space pause   / search   : command   q quit",
        )
        .alignment(Alignment::Center)
        .style(muted_style()),
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

    let bar_width = footer_rows[3].width.saturating_sub(reserved as u16).max(1) as usize;

    let filled = (progress * bar_width as f64).round() as usize;

    let bar = format!(
        "[{}{}]",
        "█".repeat(filled),
        "─".repeat(bar_width.saturating_sub(filled)),
    );

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(bar, Style::default().fg(ACCENT)),
            Span::raw(" "),
            Span::styled(time_label, muted_style()),
        ]))
        .alignment(Alignment::Center),
        footer_rows[3],
    );

    if matches!(app.input_mode, InputMode::Search) {
        render_search_picker(frame, chunks[1], app);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::layout::Rect;

    #[test]
    fn strips_numeric_prefix_and_extension_from_track_labels() {
        assert_eq!(display_track_name("01. Brain Damage.flac"), "Brain Damage");
        assert_eq!(display_track_name("8. Role Model.mp3"), "Role Model");
    }

    #[test]
    fn leaves_non_numbered_track_labels_readable() {
        assert_eq!(display_track_name("Infinite.flac"), "Infinite");
        assert_eq!(display_track_name("Lose Yourself"), "Lose Yourself");
    }

    #[test]
    fn centered_rect_stays_within_bounds() {
        let area = Rect::new(0, 0, 100, 40);
        let rect = centered_rect(86, 78, area);

        assert_eq!(rect.width, 86);
        assert_eq!(rect.height, 32);
        assert_eq!(rect.x, 7);
        assert_eq!(rect.y, 4);
    }
}
