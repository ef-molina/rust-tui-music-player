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
