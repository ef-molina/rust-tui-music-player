//! Command parsing and representation.
//!
//! Commands are higher-level user intents entered via command mode.
//! They are parsed from strings and handled by the application layer.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Download { url: String },
    Unknown(String),
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
