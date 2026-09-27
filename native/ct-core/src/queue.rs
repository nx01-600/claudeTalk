//! The speech queue (native/SPEC.md §4). Phrases play in order instead of
//! cutting each other off:
//!   %TEMP%\claudetalk_queue\<ticks>.json   one item per phrase
//!   `claudetalk speaker`                    drains it; the named mutex
//!                                           `Local\claudetalk_speaker` keeps
//!                                           a single drainer alive
//! A cut (Stop-Speech) deletes queue items and signals the `Local\claudetalk_cut`
//! event, which the speaker checks while it plays.

use crate::fsutil::read_json;
use crate::lock::{wide, NamedMutex};
use crate::paths;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use windows_sys::Win32::Foundation::CloseHandle;
use windows_sys::Win32::System::Threading::{CreateEventW, SetEvent};

pub const SPEAKER_MUTEX: &str = "Local\\claudetalk_speaker";
pub const CUT_EVENT: &str = "Local\\claudetalk_cut";
/// Manual-reset, set by the speaker while a phrase plays.
pub const SPEAKING_EVENT: &str = "Local\\claudetalk_speaking";
/// Manual-reset, set by the dictation daemon while the user dictates.
pub const DUCKING_EVENT: &str = "Local\\claudetalk_ducking";

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Item {
    pub text: String,
    #[serde(default)]
    pub voice: String,
    #[serde(default)]
    pub rate: String,
    #[serde(default = "full_volume")]
    pub volume: i64,
    // v0.5 fields, kept so older readers still parse the item.
    #[serde(default)]
    pub edge: String,
    #[serde(default)]
    pub ffplay: String,
    #[serde(default, deserialize_with = "nullable")]
    pub session: String,
}

fn full_volume() -> i64 {
    100
}

fn nullable<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    Ok(Option::<String>::deserialize(d)?.unwrap_or_default())
}

/// Ticks since 0001-01-01 UTC, like .NET's DateTime.UtcNow.Ticks, so items
/// sort by name exactly like the ones v0.5 wrote.
fn utc_ticks() -> u128 {
    const EPOCH_TICKS: u128 = 621_355_968_000_000_000;
    let d = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    EPOCH_TICKS + d.as_nanos() / 100
}

/// Queues a phrase and makes sure a speaker is draining. Returns at once.
pub fn add(text: &str, voice: &str, rate: &str, volume: i64, session: &str) {
    let text = text.trim();
    if text.is_empty() {
        return;
    }
    let dir = paths::queue_dir();
    let _ = fs::create_dir_all(&dir);
    let item = Item {
        text: text.to_string(),
        voice: voice.to_string(),
        rate: rate.to_string(),
        volume,
        session: session.to_string(),
        ..Default::default()
    };
    // Two phrases in the same tick would overwrite each other.
    let mut ticks = utc_ticks();
    let mut path;
    loop {
        path = dir.join(format!("{ticks:020}.json"));
        if !path.exists() {
            break;
        }
        ticks += 1;
    }
    let tmp = path.with_extension("tmp");
    if fs::write(&tmp, serde_json::to_vec(&item).unwrap_or_default()).is_ok() {
        let _ = fs::rename(&tmp, &path);
    }
    ensure_speaker();
}

/// Queue files in play order.
pub fn pending() -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = fs::read_dir(paths::queue_dir())
        .map(|d| {
            d.flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|x| x == "json"))
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

pub fn read_item(path: &std::path::Path) -> Option<Item> {
    serde_json::from_value(read_json(path)?).ok()
}

/// Starts `claudetalk speaker` unless one is already draining.
pub fn ensure_speaker() {
    if let Some(m) = NamedMutex::new(SPEAKER_MUTEX) {
        // Free means nobody drains; the check is only a shortcut, the speaker
        // itself re-checks the queue after releasing it.
        if m.acquire(0).is_none() {
            return;
        }
    }
    spawn_self(&["speaker"]);
}

/// Runs this same executable detached and without a window.
pub fn spawn_self(args: &[&str]) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
    let Ok(exe) = std::env::current_exe() else { return };
    crate::procs::keep_std_handles();
    let mut cmd = std::process::Command::new(exe);
    cmd.args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    // Outlive the hook: Claude Code may kill the hook's job when it returns.
    if cmd.creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS | CREATE_BREAKAWAY_FROM_JOB).spawn().is_err() {
        let _ = cmd.creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS).spawn();
    }
}

/// Empties the queue and cuts what is playing. With `sid`, only that
/// session's phrases: the other sessions keep talking.
pub fn stop(sid: Option<&str>) {
    for p in pending() {
        let remove = match sid {
            None => true,
            Some(s) => read_item(&p).is_none_or(|it| it.session == s),
        };
        if remove {
            let _ = fs::remove_file(p);
        }
    }
    if !paths::player_pid_file().exists() {
        return;
    }
    if let Some(s) = sid {
        let playing = fs::read_to_string(paths::player_session_file()).unwrap_or_default();
        let playing = playing.lines().next().unwrap_or("").trim();
        if !playing.is_empty() && playing != s {
            return;
        }
    }
    signal_cut();
}

pub fn signal_cut() {
    unsafe {
        let h = CreateEventW(std::ptr::null(), 0, 0, wide(CUT_EVENT).as_ptr());
        if !h.is_null() {
            SetEvent(h);
            CloseHandle(h);
        }
    }
}

/// Rate for a phrase that starts while the user dictates: 15 points slower,
/// never below -50%.
pub fn ducked_rate(rate: &str) -> String {
    let pct: i64 = rate
        .strip_suffix('%')
        .and_then(|n| n.parse().ok())
        .unwrap_or(0);
    let r = (pct - 15).max(-50);
    if r >= 0 {
        format!("+{r}%")
    } else {
        format!("{r}%")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ducking_rate() {
        assert_eq!(ducked_rate("+0%"), "-15%");
        assert_eq!(ducked_rate("+20%"), "+5%");
        assert_eq!(ducked_rate("-40%"), "-50%");
        assert_eq!(ducked_rate("junk"), "-15%");
        assert_eq!(ducked_rate("+15%"), "+0%");
    }

    #[test]
    fn ticks_are_20_digits() {
        assert_eq!(format!("{:020}", utc_ticks()).len(), 20);
    }

    #[test]
    fn item_parses_v05_json() {
        let v: Item = serde_json::from_str(r#"{"text":"hola","voice":"v","rate":"+0%","volume":80,"edge":"","ffplay":"","session":null}"#).unwrap();
        assert_eq!(v.volume, 80);
        assert_eq!(v.session, "");
    }
}
