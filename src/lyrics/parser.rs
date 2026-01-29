use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::path::Path;

use regex::Regex;

#[derive(Debug, Clone)]
pub struct LyricLine {
    pub timestamp: f64, // seconds
    pub text: String,
}

/// Parse an .lrc file and return a sorted list of timestamped lyric lines.
///
/// - Skips metadata tags like [ar:], [al:], [ti:], etc.
/// - Supports multiple timestamps per line
/// - Skips empty lyric text lines
pub fn parse_lrc(path: &Path) -> io::Result<Vec<LyricLine>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);

    // [mm:ss.xx] — seconds can include decimals
    let ts_re = Regex::new(r"\[(\d+):(\d+(?:\.\d+)?)\]").expect("valid timestamp regex");
    // [ar:artist], [ti:title], etc.
    let meta_re = Regex::new(r"^\[[a-zA-Z]+:").expect("valid metadata regex");

    let mut out: Vec<LyricLine> = Vec::new();

    for line in reader.lines() {
        let line = line?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if meta_re.is_match(line) {
            continue;
        }

        // collect all timestamps
        let mut stamps: Vec<f64> = Vec::new();
        for cap in ts_re.captures_iter(line) {
            let minutes: u64 = match cap.get(1).and_then(|m| m.as_str().parse().ok()) {
                Some(v) => v,
                None => continue,
            };
            let seconds: f64 = match cap.get(2).and_then(|s| s.as_str().parse().ok()) {
                Some(v) => v,
                None => continue,
            };

            stamps.push(minutes as f64 * 60.0 + seconds);
        }

        if stamps.is_empty() {
            continue;
        }

        // remove timestamps to get lyric text
        let text = ts_re.replace_all(line, "").trim().to_string();
        if text.is_empty() {
            continue;
        }

        for t in stamps {
            out.push(LyricLine {
                timestamp: t,
                text: text.clone(),
            });
        }
    }

    out.sort_by(|a, b| {
        a.timestamp
            .partial_cmp(&b.timestamp)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    Ok(out)
}

// ==============================================================
// Inline Unit Tests
// ==============================================================
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn write_lrc(contents: &str) -> NamedTempFile {
        let mut file = NamedTempFile::new().expect("temp file");
        write!(file, "{contents}").expect("write temp lrc");
        file
    }

    #[test]
    fn parses_single_timestamp_line() {
        let file = write_lrc("[00:10.00]Hello world\n");

        let lines = parse_lrc(file.path()).unwrap();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].timestamp, 10.0);
        assert_eq!(lines[0].text, "Hello world");
    }

    #[test]
    fn parses_multiple_timestamps_per_line() {
        let file = write_lrc("[00:01.00][00:02.00]Hi\n");

        let lines = parse_lrc(file.path()).unwrap();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].timestamp, 1.0);
        assert_eq!(lines[1].timestamp, 2.0);
        assert_eq!(lines[0].text, "Hi");
        assert_eq!(lines[1].text, "Hi");
    }

    #[test]
    fn ignores_metadata_lines() {
        let file = write_lrc("[ar:Artist]\n[ti:Title]\n[00:05.00]Lyric line\n");

        let lines = parse_lrc(file.path()).unwrap();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text, "Lyric line");
    }

    #[test]
    fn sorts_lines_by_timestamp() {
        let file = write_lrc("[00:10.00]Ten\n[00:02.00]Two\n[00:05.00]Five\n");

        let lines = parse_lrc(file.path()).unwrap();
        let texts: Vec<_> = lines.iter().map(|l| l.text.as_str()).collect();

        assert_eq!(texts, vec!["Two", "Five", "Ten"]);
    }

    #[test]
    fn skips_malformed_lines_safely() {
        let file = write_lrc("not lyrics\n[xx:yy]bad\n[00:03.00]Good\n");

        let lines = parse_lrc(file.path()).unwrap();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text, "Good");
    }
}
