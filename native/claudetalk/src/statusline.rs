//! A "Talk" badge on Claude Code's status line while talk mode is on in that
//! session (native/SPEC.md, v0.8.7).
//!
//!   claudetalk statusline          the status line command: runs the user's
//!                                  previous one (if any) and appends the badge
//!   claudetalk toggle statusline on|off   installs / removes it in settings.json

use ct_core::fsutil::{read_json, write_json};
use ct_core::{paths, sessions};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};

const BADGE: &str = "\x1b[38;5;78mTalk\x1b[0m";

fn settings_path() -> PathBuf {
    let dir = std::env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("USERPROFILE").unwrap_or_default()).join(".claude"));
    dir.join("settings.json")
}

/// Where the user's own status line command is kept while ours wraps it.
fn wrapped_file() -> PathBuf {
    paths::state_dir().join("statusline-wrapped.txt")
}

fn asked_file() -> PathBuf {
    paths::state_dir().join("statusline-asked")
}

/// The command settings.json should hold; forward slashes survive any shell.
fn our_command() -> String {
    let exe = std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_else(|_| "claudetalk.exe".into());
    format!("\"{}\" statusline", exe.replace('\\', "/"))
}

fn is_ours(cmd: &str) -> bool {
    let c = cmd.to_lowercase();
    c.contains("claudetalk") && c.trim_end().ends_with(" statusline")
}

fn current_command(settings: &Value) -> Option<String> {
    settings.pointer("/statusLine/command").and_then(Value::as_str).map(str::to_string)
}

/// The status line command itself.
pub fn render() {
    let mut raw = String::new();
    let _ = std::io::stdin().read_to_string(&mut raw);
    let sid = serde_json::from_str::<Value>(&raw).ok().and_then(|v| v.get("session_id").and_then(Value::as_str).map(str::to_string));
    let mut line = String::new();
    if let Ok(prev) = std::fs::read_to_string(wrapped_file()) {
        line = run_previous(prev.trim(), &raw);
    }
    if sid.is_some_and(|s| sessions::get_state(Some(&s)).enabled) {
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(BADGE);
    }
    print!("{line}");
}

fn run_previous(cmd: &str, payload: &str) -> String {
    use std::os::windows::process::CommandExt;
    let child = Command::new("cmd")
        .raw_arg(format!("/S /C \"{cmd}\""))
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn();
    let Ok(mut child) = child else { return String::new() };
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(payload.as_bytes());
    }
    let out = child.wait_with_output().map(|o| o.stdout).unwrap_or_default();
    String::from_utf8_lossy(&out).trim_end_matches(['\r', '\n']).to_string()
}

/// `toggle statusline on|off`.
pub fn install(on: bool) -> (String, i32) {
    let path = settings_path();
    let mut settings = read_json(&path).filter(Value::is_object).unwrap_or_else(|| json!({}));
    if path.exists() && !path.with_extension("json.claudetalk.bak").exists() {
        let _ = std::fs::copy(&path, path.with_extension("json.claudetalk.bak"));
    }
    let cur = current_command(&settings);
    if on {
        if cur.as_deref().is_some_and(is_ours) {
            return ("claudeTalk: the status line badge is already installed.".into(), 0);
        }
        if let Some(prev) = &cur {
            let _ = std::fs::create_dir_all(paths::state_dir());
            let _ = std::fs::write(wrapped_file(), prev);
        }
        settings["statusLine"] = json!({"type": "command", "command": our_command()});
    } else {
        if !cur.as_deref().is_some_and(is_ours) {
            return ("claudeTalk: the status line badge is not installed.".into(), 0);
        }
        match std::fs::read_to_string(wrapped_file()) {
            Ok(prev) => settings["statusLine"] = json!({"type": "command", "command": prev.trim()}),
            Err(_) => {
                settings.as_object_mut().map(|o| o.remove("statusLine"));
            }
        }
        let _ = std::fs::remove_file(wrapped_file());
    }
    if let Err(e) = write_json(&path, &settings) {
        return (format!("claudeTalk: could not write {}: {e}", path.display()), 1);
    }
    let _ = std::fs::write(asked_file(), "");
    let msg = if on {
        "claudeTalk: the status line now shows \"Talk\" while talk mode is on (your previous status line, if any, is kept). Visible in new and restarted sessions."
    } else {
        "claudeTalk: status line badge removed; your previous status line is back."
    };
    (msg.into(), 0)
}

/// SessionStart: the plugin folder changes with each version, so a command
/// installed earlier must follow the exe that is running now.
pub fn follow_exe() {
    let path = settings_path();
    let Some(mut settings) = read_json(&path) else { return };
    let Some(cur) = current_command(&settings) else { return };
    if is_ours(&cur) && cur != our_command() {
        settings["statusLine"]["command"] = json!(our_command());
        let _ = write_json(&path, &settings);
    }
}

/// Said to Claude once, at SessionStart, so it offers the badge to the user.
pub fn ask_notice() -> Option<String> {
    if asked_file().exists() {
        return None;
    }
    let installed = read_json(&settings_path()).and_then(|s| current_command(&s)).is_some_and(|c| is_ours(&c));
    let _ = std::fs::create_dir_all(paths::state_dir());
    let _ = std::fs::write(asked_file(), "");
    if installed {
        return None;
    }
    let exe = std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_else(|_| "claudetalk.exe".into());
    Some(format!(
        "claudeTalk can show a \"Talk\" badge on Claude Code's status line while talk mode is on in a session. \
         At a natural moment, ask the user ONCE, in their language, if they want it (it keeps their current status line and only appends the badge). \
         Only after they say yes, run: \"{exe}\" toggle statusline on. To remove it later: \"{exe}\" toggle statusline off. \
         If they say no, do nothing and do not ask again."
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_its_own_command_only() {
        assert!(is_ours("\"C:/x/claudeTalk/0.8.7/bin/claudetalk.exe\" statusline"));
        assert!(!is_ours("node \"C:/Users/x/.claude/statusline.js\""));
        assert!(!is_ours("claudetalk.exe toggle on"));
    }
}
