//! Shared state and logic of claudeTalk: the files every piece reads and
//! writes (see native/SPEC.md §2), sessions and voices, the speech queue, and
//! the transcript rules the Stop hook uses.

pub mod fsutil;
pub mod http;
pub mod lock;
pub mod log;
pub mod paths;
pub mod procs;
pub mod queue;
pub mod sessions;
pub mod settings;
pub mod speech_text;
pub mod transcript;

/// First UTF-16 units of the mark the dictation daemon puts in front of every
/// message it sends to Claude Code (U+1F399 U+FE0F and a space).
pub const SPOKEN_MARK: &str = "\u{1F399}\u{FE0F} ";

/// True when a prompt starts with the microphone mark, i.e. it was dictated.
pub fn is_spoken(prompt: &str) -> bool {
    prompt.trim_start().starts_with('\u{1F399}')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spoken_mark() {
        assert!(is_spoken("\u{1F399}\u{FE0F} hola"));
        assert!(is_spoken("  \u{1F399} hola"));
        assert!(!is_spoken("hola \u{1F399}"));
        assert!(!is_spoken(""));
    }
}
