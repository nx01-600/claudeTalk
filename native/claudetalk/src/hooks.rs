//! The three Claude Code hooks (native/SPEC.md §5-7).

use ct_core::procs::{command_line, is_headless_cmdline, Snapshot};
use ct_core::sessions::{self, SessionState};
use ct_core::settings::{self, Gear};
use ct_core::{log::log, paths, queue, speech_text, transcript};
use serde_json::{json, Value};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

const MAX_SPOKEN_CHARS: usize = 400;
/// Talk prompts between two full copies of the rules (see RULES).
const RULES_EVERY: u32 = 8;
pub const LEFT_ON_SCREEN: &str = "Te dej\u{e9} la respuesta en pantalla.";

const RULES: &str = "claudeTalk talk mode is ON: the user hears you through text-to-speech. Answer for a listener:
- LANGUAGE: write AND speak in the language of the user's messages (usually Spanish). These rules are in English only for you; that is not a reason to switch. Never change language unless the user asks for it.
- If the answer fits in 2-3 plain sentences, just write it, conversational, no markdown. It is read aloud automatically. Do not call say.
- If the answer needs code, tables, lists or more than ~3 sentences: first call the claudeTalk say tool with 1-2 sentences that give the gist or point to the screen (e.g. \"Te deje en pantalla los tres pasos\"), then write the full detail. Never say the same thing you write.
- For work with tools: call say briefly when you start (\"Voy a revisar el hook\") and, if the result is long, again before the final write-up.
- Never put code, paths, symbols or markdown in say.
- If the user asks to stop talking or to turn talk mode off/on, or to change any claudeTalk setting (voice, speed, volume, silence, sensitivity, hotkey, Enter, wake word, speak only to spoken messages, screen share, glass, position, language), use the claudeTalk talk skill.
";

const SPOKEN_RULE: &str = "- This message starts with a microphone mark: the user SPOKE it and Whisper transcribed it. Read it charitably: expect misheard words (e.g. Cloud for Claude), and stray phrases at the end that are really your own voice picked up by the mic; ignore those. Ask only if the meaning is truly unclear.\n";

const REMINDER: &str = "claudeTalk talk mode is still ON: keep following its rules (user's language; short plain answers are read aloud; for long ones call say first with the gist; no code or markdown in say).\n";

const SPOKEN_REMINDER: &str = "- Spoken message (mic mark): expect misheard words and stray echoes of your own voice at the end; ignore those.\n";

const TYPED_ONLY: &str = "claudeTalk talk mode is ON, but this message was typed, not spoken, and the user asked for spoken answers only to spoken messages. Answer this one in text only: do not call the claudeTalk say tool. Nothing will be read aloud. Keep writing in the language of the user's messages.
";

/// Hook payloads are UTF-8 regardless of the console code page.
fn read_payload() -> Value {
    let mut raw = Vec::new();
    let _ = std::io::stdin().read_to_end(&mut raw);
    let body = raw.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&raw);
    serde_json::from_slice(body).unwrap_or(Value::Null)
}

fn s<'a>(p: &'a Value, key: &str) -> Option<&'a str> {
    p.get(key).and_then(Value::as_str).filter(|s| !s.is_empty())
}

fn cwd_of(p: &Value) -> PathBuf {
    s(p, "cwd").map(PathBuf::from).unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
}

/// Everything needed to speak for a session.
pub struct TalkState {
    pub session: String,
    pub enabled: bool,
    pub voice: String,
    pub gear: Gear,
    pub skip_code: bool,
    pub sess: SessionState,
}

pub fn talk_state(cwd: &Path, sid: Option<&str>) -> TalkState {
    let gear = settings::gear();
    let sess = sessions::get_state(sid);
    TalkState {
        session: sid.unwrap_or("").to_string(),
        enabled: sid.is_some() && sess.enabled,
        voice: sessions::effective_voice(&sess, &gear.voice),
        skip_code: settings::skip_code(cwd),
        gear,
        sess,
    }
}

pub fn speak(text: &str, st: &TalkState) {
    queue::add(text, &st.voice, &st.gear.rate, st.gear.volume, &st.session);
}

/// UserPromptSubmit.
pub fn prompt() {
    let p = read_payload();
    let cwd = cwd_of(&p);
    let sid = sessions::session_id(s(&p, "session_id"));
    let env_override = std::env::var("CLAUDETALK_SESSION_ID").is_ok_and(|v| !v.is_empty());
    // The `say` server and /talk find their session through live.json. If
    // SessionStart didn't record it (plugin updated mid-session), do it now.
    if let (Some(sid), false) = (&sid, env_override) {
        let known = ct_core::fsutil::read_json(&paths::live_file())
            .and_then(|v| v.as_object().map(|o| o.values().any(|x| x.as_str() == Some(sid))))
            .unwrap_or(false);
        if !known {
            if let Some(pid) = ct_core::procs::claude_pid() {
                sessions::register(sid, pid);
            }
        }
    }
    let st = talk_state(&cwd, sid.as_deref());
    if !st.enabled {
        return;
    }
    // Talk mode may have been left on from an earlier run: re-arm "Oye Claude".
    sessions::set_talk_flag(true);

    let spoken = ct_core::is_spoken(s(&p, "prompt").unwrap_or(""));
    let context = if st.gear.only_spoken && !spoken {
        TYPED_ONLY.to_string()
    } else {
        let full = st.sess.rules_age.is_none_or(|age| age + 1 >= RULES_EVERY);
        let mut sess = st.sess.clone();
        sess.rules_age = Some(if full { 0 } else { sess.rules_age.unwrap_or(0) + 1 });
        ct_core::lock::with_session_lock(|| {
            // Re-read under the lock so a concurrent /talk off isn't undone.
            let mut now = sessions::get_state(Some(&st.session));
            now.rules_age = sess.rules_age;
            sessions::set_state(&st.session, &now);
        });
        match (full, spoken) {
            (true, true) => format!("{RULES}{SPOKEN_RULE}"),
            (true, false) => RULES.to_string(),
            (false, true) => format!("{REMINDER}{SPOKEN_REMINDER}"),
            (false, false) => REMINDER.to_string(),
        }
    };
    let out = json!({"hookSpecificOutput": {"hookEventName": "UserPromptSubmit", "additionalContext": context}});
    println!("{out}");
}

/// Stop: read the end of the turn out loud when it is short.
pub fn stop() {
    let p = read_payload();
    let Some(transcript) = s(&p, "transcript_path").map(PathBuf::from).filter(|t| t.exists()) else {
        return;
    };
    let st = talk_state(&cwd_of(&p), sessions::session_id(s(&p, "session_id")).as_deref());
    if !st.enabled {
        return;
    }
    // Claude Code can fire Stop a moment before the final message reaches the
    // transcript: wait until it shows last_assistant_message (or, without it,
    // any text), up to ~2 s.
    let last = s(&p, "last_assistant_message").unwrap_or("").trim().to_string();
    let probe: String = last.chars().take(40).collect();
    let mut turn = transcript::read_turn(&transcript);
    for _ in 0..14 {
        let found = if probe.is_empty() {
            turn.spoke || !turn.text_after.trim().is_empty()
        } else {
            turn.text_after.contains(&probe)
        };
        if found {
            break;
        }
        std::thread::sleep(Duration::from_millis(150));
        turn = transcript::read_turn(&transcript);
    }
    // "Speak only when I talk": a typed prompt gets a silent answer.
    if st.gear.only_spoken && !turn.spoken {
        return;
    }
    let (mut text_after, mut work_after_say) = (turn.text_after.clone(), turn.work_after_say);
    if !probe.is_empty() && !text_after.contains(&probe) {
        // Still not written: it is the final message, after any say or tool.
        text_after = last;
        work_after_say = false;
    }
    let clean = speech_text::to_speech(&text_after, st.skip_code);
    if clean.is_empty() {
        return;
    }
    // .NET string length counts UTF-16 units.
    let fits = clean.encode_utf16().count() <= MAX_SPOKEN_CHARS && !text_after.contains("```");
    if fits {
        speak(&clean, &st);
    } else if !turn.spoke || work_after_say {
        speak(LEFT_ON_SCREEN, &st);
    }
}

/// SessionStart: register the session and leave dictation running.
pub fn session_start() {
    let p = read_payload();
    let snap = Snapshot::take();
    let Some(claude) = snap.claude_ancestor(std::process::id()) else { return };
    if command_line(claude).is_some_and(|c| is_headless_cmdline(&c)) {
        return; // headless session: not a dictation target
    }
    if let Some(sid) = s(&p, "session_id") {
        sessions::register(sid, claude);
        // A compacted, cleared or resumed conversation lost the rules.
        if matches!(s(&p, "source"), Some("compact" | "clear" | "resume")) {
            ct_core::lock::with_session_lock(|| {
                let mut st = sessions::get_state(Some(sid));
                if st.rules_age.is_some() {
                    st.rules_age = None;
                    sessions::set_state(sid, &st);
                }
            });
        }
    }
    sessions::update_talk_flag(&snap);
    remember_session_pid(claude);
    crate::daemon::ensure();
    // Leftovers of an older version (Python venv, old plugin copies): let
    // Claude offer the cleanup. Costs tokens only while they exist.
    if crate::cleanup::leftovers_present() {
        let out = json!({"hookSpecificOutput": {"hookEventName": "SessionStart", "additionalContext": crate::cleanup::session_notice()}});
        println!("{out}");
    }
}

fn remember_session_pid(pid: u32) {
    let file = paths::sessions_txt();
    let known = std::fs::read_to_string(&file).unwrap_or_default();
    if known.lines().any(|l| l.trim() == pid.to_string()) {
        return;
    }
    let _ = std::fs::create_dir_all(paths::state_dir());
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&file) {
        if let Err(e) = writeln!(f, "{pid}") {
            log(&format!("session-start: {e}"));
        }
    }
}
