#[derive(Debug, Clone)]
pub struct TrackMetadata {
    pub title: String,
    pub artist: String,
    /// Album-level artist — set when multiple performers appear on individual tracks
    /// but the album belongs to a single primary artist (e.g. YouTube Music embeds this).
    /// Read by the normalization pipeline; not needed elsewhere.
    #[allow(dead_code)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrustedSearchMetadataScope {
    /// Trusted metadata from song search enrichment (`:ss`).
    /// Embedded auto-generated album markers still take precedence.
    Song,
    /// Trusted metadata from album preview downloads.
    /// Album artist/album should stay consistent across the selected release.
    Album,
}

impl TrackMetadata {
    /// Returns true if we have the minimum required fields
    /// to safely attempt a lyrics lookup.
    pub fn is_complete(&self) -> bool {
        !self.title.is_empty() && !self.artist.is_empty() && self.duration_secs > 0.0
    }
}

/// Trusted metadata from YouTube search enrichment, carried as a sidecar
/// alongside a download so the normalization pipeline can prefer it over
/// weaker embedded tags.
///
/// This is a neutral type that does not depend on the `youtube` module,
/// keeping `download` and `fs::normalize` decoupled from search concerns.
#[derive(Debug, Clone)]
pub struct TrustedSearchMetadata {
    pub track: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    /// True when the search enrichment had High or Medium confidence
    /// (i.e. multiple core fields were present in the direct metadata fetch).
    pub is_trusted: bool,
    pub scope: TrustedSearchMetadataScope,
}
