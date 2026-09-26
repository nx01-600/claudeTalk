//! Where claudeTalk keeps its state. Same locations as v0.5 so the Python
//! daemon and the Rust pieces can run side by side.

use std::env;
use std::path::PathBuf;

fn env_dir(name: &str) -> PathBuf {
    PathBuf::from(env::var_os(name).unwrap_or_default())
}

/// %APPDATA%\claudeTalk
pub fn state_dir() -> PathBuf {
    env_dir("APPDATA").join("claudeTalk")
}

pub fn sessions_dir() -> PathBuf {
    state_dir().join("sessions")
}

pub fn live_file() -> PathBuf {
    state_dir().join("live.json")
}

/// The gear panel's settings (dictation.json).
pub fn settings_file() -> PathBuf {
    state_dir().join("dictation.json")
}

pub fn sessions_txt() -> PathBuf {
    state_dir().join("sessions.txt")
}

pub fn talk_flag() -> PathBuf {
    state_dir().join("talk-active.flag")
}

pub fn ducking_flag() -> PathBuf {
    state_dir().join("ducking.flag")
}

pub fn temp_dir() -> PathBuf {
    env::temp_dir()
}

pub fn queue_dir() -> PathBuf {
    temp_dir().join("claudetalk_queue")
}

/// PID of whatever is playing right now; only exists while a phrase plays.
/// The wake listener reads it as a bare number.
pub fn player_pid_file() -> PathBuf {
    temp_dir().join("claudetalk_player.pid")
}

/// Session id of the phrase playing right now.
pub fn player_session_file() -> PathBuf {
    temp_dir().join("claudetalk_player.session")
}

pub fn log_file() -> PathBuf {
    temp_dir().join("claudetalk.log")
}

/// %LOCALAPPDATA%\claudeTalk
pub fn local_dir() -> PathBuf {
    env_dir("LOCALAPPDATA").join("claudeTalk")
}

pub fn tts_cache_dir() -> PathBuf {
    local_dir().join("tts-cache")
}
