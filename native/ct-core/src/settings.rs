//! The gear panel's settings (dictation.json) as the plugin side reads and
//! writes them, plus the per-project .claude/claudetalk.local.md.

use crate::fsutil::{read_json, write_json};
use crate::paths;
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

/// The default voice of the current language.
pub fn default_voice() -> String {
    crate::lang::current().default_voice().to_string()
}

pub fn voice_name(voice: &str) -> String {
    crate::lang::voice_name(voice)
}

/// dictation.json as an ordered map (missing or broken file = empty).
pub fn read_all() -> Map<String, Value> {
    match read_json(&paths::settings_file()) {
        Some(Value::Object(m)) => m,
        _ => Map::new(),
    }
}

/// Sets one key, keeping every other key the daemon wrote.
pub fn set(key: &str, value: Value) -> std::io::Result<()> {
    let mut all = read_all();
    all.insert(key.to_string(), value);
    write_json(&paths::settings_file(), &Value::Object(all))
}

/// The voice-related settings every hook needs.
#[derive(Clone, Debug)]
pub struct Gear {
    pub voice: String,
    pub rate: String,
    pub volume: i64,
    pub only_spoken: bool,
}

pub fn gear() -> Gear {
    let all = read_all();
    let s = |k: &str| all.get(k).and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_string);
    Gear {
        voice: s("tts_voice").unwrap_or_else(default_voice),
        rate: s("tts_rate").unwrap_or_else(|| "+0%".to_string()),
        volume: all.get("tts_volume").and_then(as_int).unwrap_or(100),
        only_spoken: all.get("speak_only_spoken").is_some_and(truthy),
    }
}

fn as_int(v: &Value) -> Option<i64> {
    v.as_i64().or_else(|| v.as_f64().map(|f| f.round() as i64)).or_else(|| v.as_str()?.trim().parse().ok())
}

/// PowerShell's [bool] cast of a JSON value.
pub fn truthy(v: &Value) -> bool {
    match v {
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
        Value::String(s) => !s.is_empty(),
        Value::Null => false,
        _ => true,
    }
}

/// The project's .claude/claudetalk.local.md: looked up from cwd to the drive
/// root, otherwise where it would be created (project root or cwd).
pub fn project_state_file(cwd: &Path) -> PathBuf {
    let mut dir = Some(cwd);
    while let Some(d) = dir {
        let candidate = d.join(".claude").join("claudetalk.local.md");
        if candidate.exists() {
            return candidate;
        }
        dir = d.parent();
    }
    let root = std::env::var_os("CLAUDE_PROJECT_DIR").map(PathBuf::from).unwrap_or_else(|| cwd.to_path_buf());
    root.join(".claude").join("claudetalk.local.md")
}

/// `skip_code` from the project file (default true).
pub fn skip_code(cwd: &Path) -> bool {
    let raw = std::fs::read_to_string(project_state_file(cwd)).unwrap_or_default();
    let re = regex::Regex::new(r#"(?m)^\s*skip_code\s*:\s*"?([^"\r\n]+?)"?\s*$"#).unwrap();
    match re.captures(&raw) {
        Some(c) => c[1].trim().eq_ignore_ascii_case("true"),
        None => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truthiness() {
        assert!(truthy(&Value::Bool(true)));
        assert!(!truthy(&Value::Null));
        assert!(!truthy(&serde_json::json!(0)));
        assert!(truthy(&serde_json::json!("x")));
    }

    #[test]
    fn skip_code_default_and_value() {
        let dir = std::env::temp_dir().join(format!("ct-skip-{}", std::process::id()));
        let sub = dir.join("a").join("b");
        std::fs::create_dir_all(&sub).unwrap();
        assert!(skip_code(&sub) || std::env::var_os("CLAUDE_PROJECT_DIR").is_some());
        std::fs::create_dir_all(dir.join(".claude")).unwrap();
        std::fs::write(dir.join(".claude/claudetalk.local.md"), "---\nskip_code: \"false\"\n---\n").unwrap();
        assert!(!skip_code(&sub));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
