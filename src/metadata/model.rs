#[derive(Debug, Clone)]
pub struct TrackMetadata {
    pub title: String,
    pub artist: String,
    pub album: Option<String>,
    pub duration_secs: f64,
    pub confidence: MetadataConfidence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetadataConfidence {
    /// Metadata came directly from tags (ffprobe / exiftool)
    Exact,

    /// Metadata required filesystem or filename heuristics
    Heuristic,

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
