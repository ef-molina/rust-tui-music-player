use crate::app::{AppState, SearchEntry};
use crate::search::filter_results;

pub fn refresh_search_results(app: &mut AppState) {
    app.search.results = filter_results(&app.search.index_entries, &app.search.query);

    if app.search.results.is_empty() {
        app.search.selected = 0;
    } else {
        app.search.selected = app.search.selected.min(app.search.results.len() - 1);
    }
}

pub fn merge_search_entries(app: &mut AppState, entries: Vec<SearchEntry>) {
    for entry in entries {
        if let Some(existing) = app
            .search
            .index_entries
            .iter_mut()
            .find(|existing| existing.path == entry.path)
        {
            *existing = entry;
        } else {
            app.search.index_entries.push(entry);
        }
    }
}
