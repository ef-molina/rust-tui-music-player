use super::LyricLine;

/// Runtime state for synced lyrics.
#[derive(Debug, Clone)]
pub struct LyricsState {
    pub lines: Vec<LyricLine>,
    pub current_index: usize,
}

impl LyricsState {
    /// Create a new lyrics state starting at the first line.
    pub fn new(lines: Vec<LyricLine>) -> Self {
        Self {
            lines,
            current_index: 0,
        }
    }

    /// Update the current lyric index based on playback time (seconds).
    ///
    /// This should be called on each Tick.
    pub fn update(&mut self, current_time: f64) {
        self.current_index = self
            .lines
            .iter()
            .rposition(|line| line.timestamp <= current_time)
            .unwrap_or(0);
    }

    /// Get the currently active lyric line, if any.
    pub fn current(&self) -> Option<&LyricLine> {
        self.lines.get(self.current_index)
    }

    /// Get the previous lyric line (for context), if any.
    pub fn previous(&self) -> Option<&LyricLine> {
        if self.current_index > 0 {
            self.lines.get(self.current_index - 1)
        } else {
            None
        }
    }

    /// Get the next lyric line (for context), if any.
    pub fn next(&self) -> Option<&LyricLine> {
        self.lines.get(self.current_index + 1)
    }
}

// ==============================================================
// Inline Unit Tests
// ==============================================================
#[cfg(test)]
mod tests {
    use super::*;

    fn line(ts: f64, text: &str) -> LyricLine {
        LyricLine {
            timestamp: ts,
            text: text.to_string(),
        }
    }

    #[test]
    fn advances_forward_in_time() {
        let lines = vec![line(0.0, "one"), line(5.0, "two"), line(10.0, "three")];

        let mut state = LyricsState::new(lines);

        state.update(6.0);
        assert_eq!(state.current().unwrap().text, "two");

        state.update(11.0);
        assert_eq!(state.current().unwrap().text, "three");
    }

    #[test]
    fn resyncs_on_backward_seek() {
        let lines = vec![line(0.0, "one"), line(5.0, "two"), line(10.0, "three")];

        let mut state = LyricsState::new(lines);

        state.update(10.0);
        assert_eq!(state.current().unwrap().text, "three");

        // simulate backward seek
        state.update(1.0);
        assert_eq!(state.current().unwrap().text, "one");
    }
}
