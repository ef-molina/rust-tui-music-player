#[derive(Debug, Clone)]
pub struct TrackMetadata {
    pub title: String,
    pub artist: String,
    /// Album-level artist — set when multiple performers appear on individual tracks
    /// but the album belongs to a single primary artist (e.g. YouTube Music embeds this).
    pub album_artist: Option<String>,
    pub album: Option<String>,
    pub duration_secs: f64,

    // Optional fields used by download normalization (safe to ignore elsewhere)
    pub date: Option<String>,     // e.g. "20180916" or "2017-03-10"
    pub track: Option<String>,    // e.g. "1", "01/12"
    pub purl: Option<String>,     // e.g. https://www.youtube.com/watch?v=...
    pub comment: Option<String>,  // often contains Provided-to-YouTube block
    pub synopsis: Option<String>, // sometimes same as comment

    pub confidence: MetadataConfidence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetadataConfidence {
    /// Metadata came directly from tags (ffprobe / exiftool)
    Exact,

    /// Only filename-based metadata; do not fetch lyrics
    FilenameOnly,
}

impl TrackMetadata {
    /// Returns true if we have the minimum required fields
    /// to safely attempt a lyrics lookup.
    pub fn is_complete(&self) -> bool {
        !self.title.is_empty() && !self.artist.is_empty() && self.duration_secs > 0.0
    }
}
