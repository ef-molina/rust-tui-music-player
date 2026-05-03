//! Command parsing and representation.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandSpec {
    pub name: &'static str,
    pub syntax: &'static str,
    pub description: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Download { url: String },
    SearchSong { query: String },
    SearchAlbum { query: String },
    SearchArtist { query: String },
    Unknown(String),
}

const COMMAND_SPECS: &[CommandSpec] = &[
    CommandSpec {
        name: "download",
        syntax: "download <url>",
        description: "Download and normalize a track from a URL",
    },
    CommandSpec {
        name: "ss",
        syntax: "ss <song>",
        description: "Search YouTube Music for a song",
    },
    CommandSpec {
        name: "salb",
        syntax: "salb <album>",
        description: "Search YouTube Music for an album",
    },
    CommandSpec {
        name: "sa",
        syntax: "sa <artist>",
        description: "Search YouTube Music for an artist",
    },
];

pub fn command_specs() -> &'static [CommandSpec] {
    COMMAND_SPECS
}

fn command_match_score(spec: &CommandSpec, query: &str) -> Option<usize> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return Some(0);
    }

    let haystack = format!("{} {} {}", spec.name, spec.syntax, spec.description).to_lowercase();

    if !haystack.contains(&query) {
        return None;
    }

    let mut score = 0;

    if spec.name == query {
        score += 120;
    } else if spec.name.starts_with(&query) {
        score += 100;
    }

    if spec.syntax.starts_with(&query) {
        score += 80;
    }

    if spec.description.contains(&query) {
        score += 20;
    }

    Some(score)
}

pub fn filtered_command_specs(query: &str) -> Vec<&'static CommandSpec> {
    let mut matches: Vec<_> = command_specs()
        .iter()
        .filter_map(|spec| command_match_score(spec, query).map(|score| (score, spec)))
        .collect();

    matches.sort_by(|(score_a, spec_a), (score_b, spec_b)| {
        score_b
            .cmp(score_a)
            .then_with(|| spec_a.name.cmp(spec_b.name))
    });

    matches.into_iter().map(|(_, spec)| spec).collect()
}

pub fn top_command_spec(query: &str) -> Option<&'static CommandSpec> {
    filtered_command_specs(query).into_iter().next()
}

pub fn active_command_spec(buffer: &str) -> Option<&'static CommandSpec> {
    let trimmed = buffer.trim_start();

    command_specs().iter().find(|spec| {
        trimmed == spec.name
            || trimmed
                .strip_prefix(spec.name)
                .is_some_and(|rest| rest.starts_with(' '))
    })
}

pub fn parse_command(input: &str) -> Command {
    let input = input.trim();

    // Try each prefix in order — longer prefixes first to avoid mis-matching
    for (prefix, builder) in &[
        ("download ", Command::Download { url: String::new() } ),
    ] {
        let _ = (prefix, builder);
    }

    if let Some(rest) = input.strip_prefix("download ") {
        let v = rest.trim();
        if !v.is_empty() {
            return Command::Download { url: v.to_string() };
        }
    }

    // Song: :ss or :songsearch
    for prefix in &["ss ", "songsearch "] {
        if let Some(rest) = input.strip_prefix(prefix) {
            let q = rest.trim();
            if !q.is_empty() {
                return Command::SearchSong { query: q.to_string() };
            }
        }
    }

    // Album: :salb or :albumsearch
    for prefix in &["salb ", "albumsearch "] {
        if let Some(rest) = input.strip_prefix(prefix) {
            let q = rest.trim();
            if !q.is_empty() {
                return Command::SearchAlbum { query: q.to_string() };
            }
        }
    }

    // Artist: :sa or :artistsearch
    for prefix in &["sa ", "artistsearch "] {
        if let Some(rest) = input.strip_prefix(prefix) {
            let q = rest.trim();
            if !q.is_empty() {
                return Command::SearchArtist { query: q.to_string() };
            }
        }
    }

    Command::Unknown(input.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_has_four_commands() {
        assert_eq!(command_specs().len(), 4);
        let names: Vec<_> = command_specs().iter().map(|s| s.name).collect();
        assert!(names.contains(&"download"));
        assert!(names.contains(&"ss"));
        assert!(names.contains(&"salb"));
        assert!(names.contains(&"sa"));
    }

    #[test]
    fn parse_song_search_shorthand() {
        assert_eq!(parse_command("ss God's Plan"), Command::SearchSong { query: "God's Plan".into() });
    }

    #[test]
    fn parse_album_search_shorthand() {
        assert_eq!(parse_command("salb Dark Side of the Moon"), Command::SearchAlbum { query: "Dark Side of the Moon".into() });
    }

    #[test]
    fn parse_artist_search_shorthand() {
        assert_eq!(parse_command("sa Drake"), Command::SearchArtist { query: "Drake".into() });
    }

    #[test]
    fn parse_song_search_full_word() {
        assert_eq!(parse_command("songsearch Hotline Bling"), Command::SearchSong { query: "Hotline Bling".into() });
    }

    #[test]
    fn parse_album_search_full_word() {
        assert_eq!(parse_command("albumsearch Take Care"), Command::SearchAlbum { query: "Take Care".into() });
    }

    #[test]
    fn parse_artist_search_full_word() {
        assert_eq!(parse_command("artistsearch Kendrick Lamar"), Command::SearchArtist { query: "Kendrick Lamar".into() });
    }

    #[test]
    fn bare_shorthand_without_query_is_unknown() {
        assert!(matches!(parse_command("ss"), Command::Unknown(_)));
        assert!(matches!(parse_command("salb"), Command::Unknown(_)));
        assert!(matches!(parse_command("sa"), Command::Unknown(_)));
    }

    #[test]
    fn parse_download_command() {
        let cmd = parse_command("download https://example.com");
        assert_eq!(cmd, Command::Download { url: "https://example.com".into() });
    }

    #[test]
    fn filtered_specs_find_ss_by_prefix() {
        let matches = filtered_command_specs("ss");
        assert!(matches.iter().any(|s| s.name == "ss"));
    }

    #[test]
    fn filtered_specs_find_sa_by_prefix() {
        let matches = filtered_command_specs("sa");
        assert!(matches.iter().any(|s| s.name == "sa"));
    }

    #[test]
    fn helper_returns_empty_for_unknown_query() {
        assert!(filtered_command_specs("xyzzy").is_empty());
    }

    #[test]
    fn top_command_returns_best_match() {
        assert_eq!(top_command_spec("d").map(|spec| spec.name), Some("download"));
        assert_eq!(top_command_spec("normalize").map(|spec| spec.name), Some("download"));
    }

    #[test]
    fn active_command_detects_prefilled_prefix() {
        assert_eq!(
            active_command_spec("download ").map(|spec| spec.name),
            Some("download")
        );
        assert_eq!(
            active_command_spec("download https://example.com").map(|spec| spec.name),
            Some("download")
        );
        assert_eq!(active_command_spec("d"), None);
    }
}
