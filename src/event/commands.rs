//! Command parsing and representation.
//!
//! Commands are higher-level user intents entered via command mode.
//! They are parsed from strings and handled by the application layer.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandSpec {
    pub name: &'static str,
    pub syntax: &'static str,
    pub description: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Download { url: String },
    Unknown(String),
}

const COMMAND_SPECS: &[CommandSpec] = &[CommandSpec {
    name: "download",
    syntax: "download <url>",
    description: "Download and normalize a track from a URL",
}];

pub fn command_specs() -> &'static [CommandSpec] {
    COMMAND_SPECS
}

fn command_match_score(spec: &CommandSpec, query: &str) -> Option<usize> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return Some(0);
    }

    let haystack = format!(
        "{} {} {}",
        spec.name, spec.syntax, spec.description
    )
    .to_lowercase();

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

    if let Some(rest) = input.strip_prefix("download ") {
        let url = rest.trim();
        if !url.is_empty() {
            return Command::Download {
                url: url.to_string(),
            };
        }
    }

    Command::Unknown(input.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_registry_exposes_download_command() {
        let specs = command_specs();
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].name, "download");
        assert_eq!(specs[0].syntax, "download <url>");
    }

    #[test]
    fn helper_filters_by_prefix_and_description() {
        let prefix_matches = filtered_command_specs("down");
        assert_eq!(prefix_matches.len(), 1);
        assert_eq!(prefix_matches[0].name, "download");

        let description_matches = filtered_command_specs("normalize");
        assert_eq!(description_matches.len(), 1);
        assert_eq!(description_matches[0].name, "download");
    }

    #[test]
    fn helper_returns_empty_for_unknown_query() {
        assert!(filtered_command_specs("nonsense").is_empty());
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
