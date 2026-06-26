use crate::app::{AppState, FocusPane, NavigationState};
use crate::fs;
use std::path::PathBuf;

const NAVIGATION_HISTORY_LIMIT: usize = 20;

pub fn load_browser_dir(app: &mut AppState, dir: PathBuf) {
    app.browser_state.current_dir = dir;
    app.browser_state.entries = fs::read_dir(&app.browser_state.current_dir).unwrap_or_default();
    app.browser_state.selected_index = 0;
    app.ui.selection_anchor_tick = app.ui.ui_tick;
}

pub fn sync_album_for_directory(app: &mut AppState, dir: &std::path::Path) {
    if let Ok(Some(tracks)) = fs::detect_loose_tracks(dir) {
        app.album.dir = Some(dir.to_path_buf());
        app.album.entries = tracks;
        app.album.selected = 0;
    } else {
        app.album.dir = None;
        app.album.entries.clear();
        app.album.selected = 0;
    }
}

pub fn push_navigation_history(app: &mut AppState) {
    let snapshot = app.current_navigation_state();
    if app.navigation_history.last() == Some(&snapshot) {
        return;
    }

    if app.navigation_history.len() == NAVIGATION_HISTORY_LIMIT {
        app.navigation_history.remove(0);
    }
    app.navigation_history.push(snapshot);
}

pub fn restore_navigation_state(app: &mut AppState, state: &NavigationState) {
    load_browser_dir(app, state.current_dir.clone());

    let dir_count = app
        .browser_state
        .entries
        .iter()
        .filter(|entry| entry.is_dir)
        .count();
    app.browser_state.selected_index = if dir_count == 0 {
        0
    } else {
        state.selected_index.min(dir_count - 1)
    };

    app.album.dir = state.active_album_dir.clone();
    app.album.entries = state.album_entries.clone();
    app.album.selected = if app.album.entries.is_empty() {
        0
    } else {
        state.album_selected.min(app.album.entries.len() - 1)
    };
    app.ui.focus = state.focus;
    app.ui.selection_anchor_tick = app.ui.ui_tick;
}

pub fn pop_navigation_history(app: &mut AppState) -> bool {
    while let Some(state) = app.navigation_history.pop() {
        if state != app.current_navigation_state() {
            restore_navigation_state(app, &state);
            return true;
        }
    }

    false
}

pub fn restore_search_context(app: &mut AppState) {
    load_browser_dir(app, app.search.last_browser_dir.clone());
    let dir_count = app
        .browser_state
        .entries
        .iter()
        .filter(|entry| entry.is_dir)
        .count();
    app.browser_state.selected_index = if dir_count == 0 {
        0
    } else {
        app.search.last_browser_selected.min(dir_count - 1)
    };
    app.album.dir = app.search.last_active_album_dir.clone();
    app.album.entries = app.search.last_album_entries.clone();
    app.album.selected = if app.album.entries.is_empty() {
        0
    } else {
        app.search
            .last_album_selected
            .min(app.album.entries.len() - 1)
    };
    app.ui.focus = app.search.last_focus;
}

pub fn jump_to_track_path(app: &mut AppState, track_path: &std::path::Path) {
    let Some(track_dir) = track_path.parent() else {
        return;
    };

    if !track_dir.starts_with(&app.browser_state.root_dir) {
        return;
    }

    let track_name = track_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or_default();

    if let Ok(Some(tracks)) = fs::detect_album(track_dir) {
        let browser_dir = track_dir
            .parent()
            .unwrap_or(&app.browser_state.root_dir)
            .to_path_buf();
        load_browser_dir(app, browser_dir);

        let browser_dirs: Vec<_> = app
            .browser_state
            .entries
            .iter()
            .filter(|e| e.is_dir)
            .collect();

        if let Some(dir_name) = track_dir.file_name().and_then(|s| s.to_str())
            && let Some(index) = browser_dirs.iter().position(|e| e.name == dir_name)
        {
            app.browser_state.selected_index = index;
        }

        app.album.dir = Some(track_dir.to_path_buf());
        app.album.entries = tracks;
        app.album.selected = app
            .album
            .entries
            .iter()
            .position(|entry| entry.name == track_name)
            .unwrap_or(0);
    } else {
        load_browser_dir(app, track_dir.to_path_buf());
        sync_album_for_directory(app, track_dir);
        app.album.selected = app
            .album
            .entries
            .iter()
            .position(|entry| entry.name == track_name)
            .unwrap_or(0);
    }

    app.ui.focus = FocusPane::Album;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;

    fn test_app() -> AppState {
        let (lyrics_tx, lyrics_rx) = channel();
        let (search_tx, search_rx) = channel();
        let (jobs_tx, jobs_rx) = channel();
        AppState::new(
            &crate::config::Config::default(),
            crate::app::Channels {
                lyrics_rx,
                lyrics_tx,
                search_rx,
                search_tx,
                jobs_rx,
                jobs_tx,
            },
        )
    }

    #[test]
    fn navigation_history_dedupes_and_stays_bounded() {
        let mut app = test_app();
        app.browser_state.current_dir = PathBuf::from("/tmp/root");

        push_navigation_history(&mut app);
        push_navigation_history(&mut app);
        assert_eq!(app.navigation_history.len(), 1);

        for index in 0..(NAVIGATION_HISTORY_LIMIT + 5) {
            app.browser_state.current_dir = PathBuf::from(format!("/tmp/root/{index}"));
            push_navigation_history(&mut app);
        }

        assert_eq!(app.navigation_history.len(), NAVIGATION_HISTORY_LIMIT);
        assert_eq!(
            app.navigation_history
                .first()
                .map(|state| state.current_dir.clone()),
            Some(PathBuf::from("/tmp/root/5"))
        );
        assert_eq!(
            app.navigation_history
                .last()
                .map(|state| state.current_dir.clone()),
            Some(PathBuf::from(format!(
                "/tmp/root/{}",
                NAVIGATION_HISTORY_LIMIT + 4
            )))
        );
    }
}
