//! Shared state and logic of claudeTalk: the files every piece reads and
//! writes (see native/SPEC.md §2), sessions and voices, the speech queue, and
//! the transcript rules the Stop hook uses.

pub mod fsutil;
pub mod http;
pub mod lang;
pub mod lock;
pub mod log;
pub mod paths;
pub mod procs;
pub mod queue;
pub mod sessions;
pub mod settings;
pub mod speech_text;
pub mod transcript;
pub mod voice_link;

/// First UTF-16 units of the mark the dictation daemon puts in front of every
/// message it sends to Claude Code (U+1F399 U+FE0F and a space).
pub const SPOKEN_MARK: &str = "\u{1F399}\u{FE0F} ";

/// True when a prompt starts with the microphone mark, i.e. it was dictated.
///
/// Claude Code wraps a long paste in `<pasted_content id="..">` tags, so the
/// mark can come after one or more opening tags.
pub fn is_spoken(prompt: &str) -> bool {
    let mut rest = prompt.trim_start();
    while rest.starts_with('<') {
        match rest.find('>') {
            Some(end) => rest = rest[end + 1..].trim_start(),
            None => break,
        }
    }
    rest.starts_with('\u{1F399}')
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
        assert!(is_spoken("\n\n<pasted_content id=\"79e7\">\n\u{1F399}\u{FE0F} hola\n</pasted_content id=\"79e7\">\n"));
        assert!(!is_spoken("<pasted_content id=\"1\">\nhola \u{1F399}"));
    }
}
