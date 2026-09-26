//! Talk mode is per Claude Code session, and parallel sessions talk with
//! different voices (native/SPEC.md §3).
//!   sessions\<sid>.json   { enabled, voice, follows_default, updated, rules_* }
//!   live.json             { "<claude.exe PID>": "<sid>" }
//! Hooks get the session id in their payload; the `say` server and `toggle`
//! find the claude.exe they run under and look it up in live.json.

use crate::fsutil::{now_iso, read_json, write_json};
use crate::lock::with_session_lock;
use crate::paths;
use crate::procs::{claude_pid, Snapshot};
use crate::settings::{self, VOICE_POOL};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SessionState {
    pub enabled: bool,
    pub voice: Option<String>,
    pub follows_default: bool,
    /// Talk prompts since the full rules were last sent (native/SPEC.md,
    /// "Changes on purpose"). None = never sent in this conversation.
    pub rules_age: Option<u32>,
}

pub fn session_file(sid: &str) -> PathBuf {
    let safe: String = sid
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
        .collect();
    paths::sessions_dir().join(format!("{safe}.json"))
}

pub fn get_state(sid: Option<&str>) -> SessionState {
    let Some(sid) = sid else {
        return SessionState { follows_default: true, ..Default::default() };
    };
    let obj = read_json(&session_file(sid));
    let o = obj.as_ref();
    SessionState {
        enabled: o.and_then(|o| o.get("enabled")).is_some_and(settings::truthy),
        voice: o.and_then(|o| o.get("voice")).and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_string),
        follows_default: match o.and_then(|o| o.get("follows_default")) {
            None | Some(Value::Null) => true,
            Some(v) => settings::truthy(v),
        },
        rules_age: o.and_then(|o| o.get("rules_age")).and_then(Value::as_u64).map(|n| n as u32),
    }
}

pub fn set_state(sid: &str, s: &SessionState) {
    let mut m = Map::new();
    m.insert("enabled".into(), json!(s.enabled));
    m.insert("voice".into(), json!(s.voice));
    m.insert("follows_default".into(), json!(s.follows_default));
    m.insert("updated".into(), json!(now_iso()));
    if let Some(age) = s.rules_age {
        m.insert("rules_age".into(), json!(age));
    }
    let _ = write_json(&session_file(sid), &Value::Object(m));
}

/// live.json without dead sessions (PIDs that are no longer a claude.exe).
pub fn live_sessions(snap: &Snapshot) -> BTreeMap<u32, String> {
    let mut map = BTreeMap::new();
    if let Some(Value::Object(obj)) = read_json(&paths::live_file()) {
        for (k, v) in obj {
            if let (Ok(pid), Some(sid)) = (k.parse::<u32>(), v.as_str()) {
                if !sid.is_empty() && snap.is_claude(pid) {
                    map.insert(pid, sid.to_string());
                }
            }
        }
    }
    map
}

fn write_live(map: &BTreeMap<u32, String>) {
    let obj: Map<String, Value> = map.iter().map(|(k, v)| (k.to_string(), json!(v))).collect();
    let _ = write_json(&paths::live_file(), &Value::Object(obj));
}

/// Maps `claude` (this process's claude.exe) to `sid`. A new sid on a
/// claude.exe that had another one (/clear, /resume) inherits its talk mode
/// and voice: it is the same conversation window.
pub fn register(sid: &str, claude: u32) {
    with_session_lock(|| {
        let snap = Snapshot::take();
        let mut live = live_sessions(&snap);
        if let Some(old) = live.get(&claude) {
            if old != sid && !session_file(sid).exists() {
                let prev = get_state(Some(old));
                if prev.enabled || prev.voice.is_some() {
                    set_state(sid, &SessionState { rules_age: None, ..prev });
                }
            }
        }
        live.insert(claude, sid.to_string());
        write_live(&live);
        prune_old_sessions();
    });
}

fn prune_old_sessions() {
    let Ok(dir) = std::fs::read_dir(paths::sessions_dir()) else { return };
    let cutoff = SystemTime::now() - Duration::from_secs(30 * 86_400);
    for e in dir.flatten() {
        let old = e.metadata().and_then(|m| m.modified()).is_ok_and(|t| t < cutoff);
        if old && e.path().extension().is_some_and(|x| x == "json") {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

/// The session this process belongs to: env override, then the hook
/// payload's, then whatever live.json maps our claude.exe to.
pub fn session_id(payload_sid: Option<&str>) -> Option<String> {
    if let Ok(s) = std::env::var("CLAUDETALK_SESSION_ID") {
        if !s.is_empty() {
            return Some(s);
        }
    }
    if let Some(s) = payload_sid.filter(|s| !s.is_empty()) {
        return Some(s.to_string());
    }
    let pid = claude_pid()?;
    let obj = read_json(&paths::live_file())?;
    obj.get(pid.to_string())?.as_str().filter(|s| !s.is_empty()).map(str::to_string)
}

pub fn effective_voice(s: &SessionState, default: &str) -> String {
    match &s.voice {
        Some(v) if !s.follows_default => v.clone(),
        _ => default.to_string(),
    }
}

/// Voices used by the OTHER live sessions that have talk mode on.
pub fn other_voices(sid: &str, snap: &Snapshot, default: &str) -> Vec<String> {
    let mut seen = Vec::new();
    let mut voices = Vec::new();
    for other in live_sessions(snap).into_values() {
        if other == sid || seen.contains(&other) {
            continue;
        }
        let s = get_state(Some(&other));
        if s.enabled {
            voices.push(effective_voice(&s, default));
        }
        seen.push(other);
    }
    voices
}

pub struct Enabled {
    pub voice: String,
    /// Other sessions are talking and this one doesn't use the gear voice.
    pub own: bool,
    pub others: Vec<String>,
}

/// Picks the voice for a session turning talk mode on while `others` talk.
pub fn pick_voice(sess: &mut SessionState, others: &[String], default: &str) -> String {
    let current = effective_voice(sess, default);
    let voice = if sess.voice.is_some() && !others.contains(&current) {
        current
    } else if !others.iter().any(|v| v == default) {
        sess.follows_default = true;
        default.to_string()
    } else {
        sess.follows_default = false;
        match VOICE_POOL.iter().find(|v| !others.iter().any(|o| o == *v)) {
            Some(v) => v.to_string(),
            // More sessions than voices: repeat the least used one.
            None => VOICE_POOL
                .iter()
                .min_by_key(|v| others.iter().filter(|o| o == v).count())
                .unwrap()
                .to_string(),
        }
    };
    if voice != default {
        sess.follows_default = false;
    }
    voice
}

pub fn enable(sid: &str) -> Enabled {
    with_session_lock(|| {
        let snap = Snapshot::take();
        let mut sess = get_state(Some(sid));
        let default = settings::gear().voice;
        let others = other_voices(sid, &snap, &default);
        let voice = pick_voice(&mut sess, &others, &default);
        sess.voice = Some(voice.clone());
        sess.enabled = true;
        sess.rules_age = None;
        set_state(sid, &sess);
        update_talk_flag(&snap);
        Enabled { own: voice != default && !others.is_empty(), voice, others }
    })
}

pub fn disable(sid: &str) {
    with_session_lock(|| {
        let mut sess = get_state(Some(sid));
        sess.enabled = false;
        set_state(sid, &sess);
        update_talk_flag(&Snapshot::take());
    });
}

/// "Oye Claude" listens while at least one live session has talk mode on.
pub fn update_talk_flag(snap: &Snapshot) {
    let mut any = live_sessions(snap).values().any(|s| get_state(Some(s)).enabled);
    if let Ok(env_sid) = std::env::var("CLAUDETALK_SESSION_ID") {
        if !env_sid.is_empty() && get_state(Some(&env_sid)).enabled {
            any = true;
        }
    }
    set_talk_flag(any);
}

pub fn set_talk_flag(on: bool) {
    let path = paths::talk_flag();
    if on {
        let _ = std::fs::create_dir_all(paths::state_dir());
        let _ = std::fs::write(path, format!("{}\r\n", now_iso()));
    } else {
        let _ = std::fs::remove_file(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(voice: Option<&str>, follows: bool) -> SessionState {
        SessionState { voice: voice.map(str::to_string), follows_default: follows, ..Default::default() }
    }
    const G: &str = "es-CO-GonzaloNeural";
    const SAL: &str = "es-CO-SalomeNeural";

    #[test]
    fn first_session_gets_gear_voice() {
        let mut st = s(None, true);
        assert_eq!(pick_voice(&mut st, &[], G), G);
        assert!(st.follows_default);
    }

    #[test]
    fn second_session_gets_next_free_pool_voice() {
        let mut st = s(None, true);
        assert_eq!(pick_voice(&mut st, &[G.into()], G), SAL);
        assert!(!st.follows_default);
    }

    #[test]
    fn keeps_own_voice_when_free() {
        let mut st = s(Some("es-MX-DaliaNeural"), false);
        assert_eq!(pick_voice(&mut st, &[G.into()], G), "es-MX-DaliaNeural");
    }

    #[test]
    fn own_voice_taken_falls_back_to_default() {
        let mut st = s(Some(SAL), false);
        assert_eq!(pick_voice(&mut st, &[SAL.into()], G), G);
        assert!(st.follows_default);
    }

    #[test]
    fn all_taken_repeats_least_used() {
        let mut others: Vec<String> = VOICE_POOL.iter().map(|v| v.to_string()).collect();
        others.push(G.into());
        let mut st = s(None, true);
        assert_eq!(pick_voice(&mut st, &others, G), SAL);
    }

    #[test]
    fn sanitized_file_name() {
        assert!(session_file("ab/c:d").ends_with("ab_c_d.json"));
    }
}
