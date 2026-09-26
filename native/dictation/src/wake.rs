//! Hands-free start: listens for "Oye Claude" (or the user's own phrase)
//! and starts a dictation (voice-input/wake.py).
//!
//! A cheap energy check splits the mic stream into short bursts. v0.5 sent
//! every burst to Whisper; now Silero VAD first checks it is a voice, so
//! knocks, music beats and fans never reach the GPU. The Whisper model
//! already resident for dictation then reads the burst, and the same
//! matching as v0.5 decides.
//!
//! Listens only while `should_listen()` (gear toggle AND talk mode on) and
//! steps aside while a dictation runs. While Claude is talking it keeps
//! listening, but only the full "oye Claude" form counts then.

use crate::audio::{self, Mic, BLOCK_MS};
use crate::stt::{Transcriber, Vad};
use regex::Regex;
use std::collections::VecDeque;
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use unicode_normalization::UnicodeNormalization;

const PREROLL_MS: usize = 300;
const BURST_END_MS: usize = 350;
const BURST_MIN_MS: usize = 300;
const BURST_MAX_MS: usize = 2500;
const FLOOR_WINDOW_MS: usize = 4000;
const MIN_FLOOR: f32 = 1e-4;
const TTS_TAIL: Duration = Duration::from_millis(800);
const SHORT_WORDS: usize = 3;
const MAX_WAKE_MARGIN: f32 = 3.0;
pub const DEFAULT_PHRASE: &str = "oye claude";
const PHRASE_SIMILARITY: f64 = 0.75;
const PHRASE_LEAD_WORDS: usize = 2;

const NAMES: &str = r"(claude|claud|clod|clode|cloud|clau|claus|klaus|claudio|glod|klod|clo)";
const CALLS: &str = r"(oye|oje|oyi|oj|oy|oi|hey|ey|ei|oiga|okay|ok|roger)";

fn wake_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(&format!(r"^(?:\w+\s+){{0,2}}{CALLS}\s+{NAMES}\b|{CALLS}\s+{NAMES}$")).unwrap())
}

fn short_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(&format!("^{NAMES}$")).unwrap())
}

/// Lowercase, accents and punctuation off, spaces collapsed.
pub fn normalize(text: &str) -> String {
    let plain: String = text
        .to_lowercase()
        .nfkd()
        .filter(|c| !unicode_normalization::char::is_combining_mark(*c))
        .map(|c| if c.is_alphanumeric() || c == '_' || c.is_whitespace() { c } else { ' ' })
        .collect();
    plain.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// difflib.SequenceMatcher(None, a, b).ratio(), on characters. autojunk
/// never kicks in for strings this short (< 200 chars).
pub fn seq_ratio(a: &str, b: &str) -> f64 {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    fn matches(a: &[char], b: &[char]) -> usize {
        // longest common substring, earliest in a, then earliest in b
        let (mut best, mut bi, mut bj) = (0usize, 0usize, 0usize);
        let mut prev = vec![0usize; b.len() + 1];
        for i in 0..a.len() {
            let mut cur = vec![0usize; b.len() + 1];
            for j in 0..b.len() {
                if a[i] == b[j] {
                    cur[j + 1] = prev[j] + 1;
                    let len = cur[j + 1];
                    let (si, sj) = (i + 1 - len, j + 1 - len);
                    if len > best || (len == best && (si < bi || (si == bi && sj < bj))) {
                        best = len;
                        bi = si;
                        bj = sj;
                    }
                }
            }
            prev = cur;
        }
        if best == 0 {
            return 0;
        }
        best + matches(&a[..bi], &b[..bj]) + matches(&a[bi + best..], &b[bj + best..])
    }
    2.0 * matches(&a, &b) as f64 / (a.len() + b.len()) as f64
}

fn matches_custom(text: &str, phrase: &str) -> bool {
    let words: Vec<&str> = text.split(' ').filter(|w| !w.is_empty()).collect();
    let n = phrase.split(' ').count() as isize;
    let spare = words.len() as isize - n;
    // the first positions (after up to two stray words) and the very end
    let mut starts: Vec<isize> = if spare >= 0 { (0..=spare.min(PHRASE_LEAD_WORDS as isize)).collect() } else { Vec::new() };
    starts.push(spare);
    for start in starts {
        if start < 0 {
            continue;
        }
        let end = ((start + n) as usize).min(words.len());
        let window = words[start as usize..end].join(" ");
        if seq_ratio(&window, phrase) >= PHRASE_SIMILARITY {
            return true;
        }
    }
    false
}

/// strict: only the full "oye/hey + name" form (while Claude is talking).
/// `phrase` is the one set in the gear; "Oye Claude" has its own tuned
/// matching.
pub fn is_wake_phrase(text: &str, strict: bool, phrase: &str) -> bool {
    let text = normalize(text);
    let mut phrase = normalize(phrase);
    if phrase.is_empty() {
        phrase = DEFAULT_PHRASE.into();
    }
    if phrase != DEFAULT_PHRASE {
        return matches_custom(&text, &phrase);
    }
    if wake_re().is_match(&text) {
        return true;
    }
    if strict {
        return false;
    }
    let words: Vec<&str> = text.split(' ').filter(|w| !w.is_empty()).collect();
    !words.is_empty() && words.len() <= SHORT_WORDS && short_re().is_match(words[words.len() - 1])
}

/// Claude's voice may be on the speakers: a phrase is playing or queued.
pub fn claude_is_talking() -> bool {
    crate::signals::speaking() || !ct_core::queue::pending().is_empty()
}

pub struct Hooks {
    pub should_listen: Box<dyn Fn() -> bool + Send>,
    pub is_busy: Box<dyn Fn() -> bool + Send>,
    pub margin: Box<dyn Fn() -> f32 + Send>,
    pub language: Box<dyn Fn() -> Option<String> + Send>,
    pub phrase: Box<dyn Fn() -> String + Send>,
    pub on_wake: Box<dyn Fn() + Send>,
}

pub fn spawn(transcriber: Transcriber, hooks: Hooks) {
    std::thread::spawn(move || {
        // Loaded on first use: it starts the GPU drivers, which an idle
        // daemon with the wake phrase off shouldn't pay for.
        let mut vad: Option<Vad> = None;
        let mut was_on = false;
        loop {
            let on = (hooks.should_listen)();
            if on != was_on {
                println!("[wake] {}", if on { format!("listening for {:?}", (hooks.phrase)()) } else { "off".into() });
                was_on = on;
            }
            if !on || (hooks.is_busy)() {
                std::thread::sleep(Duration::from_millis(500));
                continue;
            }
            match Mic::open() {
                Ok(mic) => listen(&mic, &transcriber, vad.get_or_insert_with(Vad::load), &hooks),
                Err(e) => {
                    println!("[wake] mic error: {e}; retrying");
                    std::thread::sleep(Duration::from_secs(2));
                }
            }
        }
    });
}

/// Holds the mic open until paused; returns so a dictation can use it.
fn listen(mic: &Mic, transcriber: &Transcriber, vad: &mut Vad, h: &Hooks) {
    let paused = || !(h.should_listen)() || (h.is_busy)();
    let mut preroll: VecDeque<Vec<f32>> = VecDeque::with_capacity(PREROLL_MS / BLOCK_MS);
    let mut recent: VecDeque<f32> = VecDeque::with_capacity(FLOOR_WINDOW_MS / BLOCK_MS);
    let mut burst: Vec<Vec<f32>> = Vec::new();
    let (mut loud_ms, mut quiet_ms) = (0usize, 0usize);
    let mut talking_until = Instant::now();
    let mut strict = false;
    let mut checked_at = Instant::now();
    let mut talking = false;
    while !paused() {
        let Some(block) = mic.read() else { return };
        let level = audio::rms(&block);
        // The queue folder is read at most every 150 ms (v0.5: every block).
        if checked_at.elapsed() > Duration::from_millis(150) {
            talking = claude_is_talking();
            checked_at = Instant::now();
        } else if crate::signals::speaking() {
            talking = true;
        }
        if talking {
            talking_until = Instant::now() + TTS_TAIL;
        }
        let over_claude = Instant::now() < talking_until;

        if recent.len() == FLOOR_WINDOW_MS / BLOCK_MS {
            recent.pop_front();
        }
        recent.push_back(level);
        let push_pre = |pre: &mut VecDeque<Vec<f32>>, b: Vec<f32>| {
            if pre.len() == PREROLL_MS / BLOCK_MS {
                pre.pop_front();
            }
            pre.push_back(b);
        };
        if recent.len() < (FLOOR_WINDOW_MS / BLOCK_MS) / 4 {
            push_pre(&mut preroll, block);
            continue;
        }
        let floor = audio::percentile(recent.make_contiguous(), 20.0).max(MIN_FLOOR);
        let loud = level > floor * (h.margin)().min(MAX_WAKE_MARGIN);

        if burst.is_empty() {
            if loud {
                burst = preroll.iter().cloned().collect();
                burst.push(block);
                loud_ms = BLOCK_MS;
                quiet_ms = 0;
                strict = over_claude;
            } else {
                push_pre(&mut preroll, block);
            }
            continue;
        }
        burst.push(block);
        strict = strict || over_claude;
        if loud {
            loud_ms += BLOCK_MS;
            quiet_ms = 0;
        } else {
            quiet_ms += BLOCK_MS;
        }
        let length_ms = burst.len() * BLOCK_MS;
        if quiet_ms < BURST_END_MS && length_ms < BURST_MAX_MS {
            continue;
        }
        let pcm: Vec<f32> = burst.concat();
        burst.clear();
        preroll.clear();
        if loud_ms < BURST_MIN_MS {
            continue;
        }
        if check(&pcm, strict, transcriber, vad, h) {
            return; // the dictation takes the mic from here
        }
    }
}

fn check(pcm: &[f32], strict: bool, transcriber: &Transcriber, vad: &mut Vad, h: &Hooks) -> bool {
    if !(h.should_listen)() || (h.is_busy)() || !vad.has_speech(pcm) {
        return false;
    }
    let lang = (h.language)();
    let text = match transcriber.transcribe(pcm, lang.as_deref(), "") {
        Ok(t) => t,
        Err(e) => {
            println!("[wake] transcription failed: {e}");
            return false;
        }
    };
    if text.is_empty() {
        return false;
    }
    let hit = is_wake_phrase(&text, strict, &(h.phrase)());
    println!("[wake] heard {text:?}{}", if hit { " -> wake" } else { "" });
    if hit && (h.should_listen)() && !(h.is_busy)() {
        (h.on_wake)();
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_phrase() {
        assert!(is_wake_phrase("Oye, Claude.", false, "Oye Claude"));
        assert!(is_wake_phrase("y digo oye cloud", false, "Oye Claude"));
        assert!(is_wake_phrase("Hey Claude, ¿qué tal?", true, "Oye Claude"));
        assert!(is_wake_phrase("Claude.", false, "Oye Claude"));
        assert!(!is_wake_phrase("Claude.", true, "Oye Claude"));
        assert!(!is_wake_phrase("le pregunté a Claude ayer por la tarde", false, "Oye Claude"));
    }

    #[test]
    fn custom_phrase() {
        assert!(is_wake_phrase("Hola, Jarvis.", false, "Hola Jarvis"));
        assert!(is_wake_phrase("eh bueno hola jarvi", false, "Hola Jarvis"));
        assert!(is_wake_phrase("hola yarvis", false, "Hola Jarvis"));
        assert!(!is_wake_phrase("hola cómo estás", false, "Hola Jarvis"));
        assert!(!is_wake_phrase("", false, "Hola Jarvis"));
    }

    #[test]
    fn ratio_like_difflib() {
        // difflib.SequenceMatcher(None, "hola yarvis", "hola jarvis").ratio()
        assert!((seq_ratio("hola yarvis", "hola jarvis") - 0.9090909).abs() < 1e-6);
        assert!((seq_ratio("abcd", "bcda") - 0.75).abs() < 1e-9);
        assert_eq!(seq_ratio("", "x"), 0.0);
    }
}
