//! `claudetalk toggle ACTION [SETTING] [VALUE]` (voice-toggle.ps1 in v0.5):
//! talk mode on/off for this session, and every gear panel setting
//! (native/SPEC.md §1.1).

use ct_core::procs::Snapshot;
use ct_core::sessions;
use ct_core::settings::{self, voice_name};
use serde_json::{json, Value};
use unicode_normalization::UnicodeNormalization;

pub const HELP: &str = r#"Settings (set <setting> <value>):
  voice        Salome | Gonzalo | Dalia | Jorge | Elena | Alonso   (Claude's voice in this session)
  rate         slow | normal | fast | faster | +N% | -N%   (Claude's speed)
  volume       0-100                                     (Claude's volume)
  silence      seconds, 0.5 to 10   (pause that ends a dictation)
  sensitivity  0 to 100   (mic sensitivity; higher picks up a softer voice)
  hotkey       keys joined by +, e.g. ctrl+shift+space, alt+f2, lctrl+lshift+space
  sound        on | off   (chime when recording starts)
  enter        on | off   (press Enter after pasting = send the message)
  wake         on | off   (start dictating by saying the wake phrase)
  phrase       any words, e.g. "Hola Jarvis"   (the wake phrase; default "Oye Claude")
  spoken       on | off   (talk mode answers out loud only dictated messages)
  share        on | off   (overlay visible in screen sharing)
  glass        0 to 100   (glass effect intensity)
  position     bottom | top   (also forgets a dragged spot)
  drag         on | off   (the pill comes back where you last dragged it)
  language     es | en | auto   (dictation language)"#;

/// Lowercase without accents, so "Salomé", "salome" and "SALOME" all match.
pub fn plain(s: &str) -> String {
    s.trim()
        .to_lowercase()
        .nfd()
        .filter(|c| !unicode_normalization::char::is_combining_mark(*c))
        .collect()
}

fn to_bool(v: &str) -> Result<bool, String> {
    let p = plain(v);
    let starts = |list: &[&str]| list.iter().any(|w| p.starts_with(w));
    if starts(&["on", "true", "yes", "si", "1", "activ", "encend", "prend"]) {
        Ok(true)
    } else if starts(&["off", "false", "no", "0", "desactiv", "apag"]) {
        Ok(false)
    } else {
        Err(format!("'{v}' is not on/off."))
    }
}

fn to_number(v: &str, min: f64, max: f64) -> Result<f64, String> {
    let t = v.trim().replace(',', ".").replace('%', "");
    let n: f64 = t.trim().parse().map_err(|_| format!("'{v}' is not a number."))?;
    Ok(n.clamp(min, max))
}

/// .NET's [int] cast rounds half to even.
fn to_int(x: f64) -> i64 {
    let r = x.round();
    if (x - x.trunc()).abs() == 0.5 && r as i64 % 2 != 0 {
        (r - x.signum()) as i64
    } else {
        r as i64
    }
}

fn key_code(k: &str) -> Option<i64> {
    Some(match k {
        "ctrl" | "control" => 0x11,
        "shift" => 0x10,
        "alt" => 0x12,
        "win" | "windows" | "lwin" => 0x5B,
        "lctrl" => 0xA2,
        "rctrl" => 0xA3,
        "lshift" => 0xA0,
        "rshift" => 0xA1,
        "lalt" => 0xA4,
        "ralt" | "altgr" => 0xA5,
        "rwin" => 0x5C,
        "space" | "espacio" => 0x20,
        "tab" => 0x09,
        "enter" => 0x0D,
        "esc" => 0x1B,
        "backspace" => 0x08,
        "capslock" => 0x14,
        "insert" => 0x2D,
        "delete" => 0x2E,
        "home" => 0x24,
        "end" => 0x23,
        "pageup" => 0x21,
        "pagedown" => 0x22,
        "left" => 0x25,
        "up" => 0x26,
        "right" => 0x27,
        "down" => 0x28,
        "menu" => 0x5D,
        _ => return None,
    })
}

fn to_hotkey(v: &str) -> Result<Vec<i64>, String> {
    let mut vks = Vec::new();
    for part in plain(v).split('+').filter(|p| !p.trim().is_empty()) {
        let k: String = part.chars().filter(|c| !c.is_whitespace()).collect();
        let fkey = regex::Regex::new(r"^f([1-9]|1[0-2])$").unwrap();
        let vk = if let Some(code) = key_code(&k) {
            code
        } else if let Some(c) = fkey.captures(&k) {
            0x6F + c[1].parse::<i64>().unwrap()
        } else if k.len() == 1 && k.chars().all(|c| c.is_ascii_lowercase()) {
            k.to_ascii_uppercase().as_bytes()[0] as i64
        } else if k.len() == 1 && k.chars().all(|c| c.is_ascii_digit()) {
            0x30 + k.parse::<i64>().unwrap()
        } else {
            return Err(format!("unknown key '{part}'."));
        };
        vks.push(vk);
    }
    if vks.len() < 2 {
        return Err("a hotkey needs at least two keys, e.g. ctrl+shift+space.".into());
    }
    Ok(vks)
}

fn on_off(b: bool) -> &'static str {
    if b {
        "on"
    } else {
        "off"
    }
}

/// (json key, value, description) for `set NAME VALUE`, or the reason why not.
pub fn resolve(name: &str, v: &str) -> Result<(&'static str, Value, String), String> {
    let p = plain(v);
    let n = plain(name);
    let is = |names: &[&str]| names.contains(&n.as_str());
    if is(&["voice", "voz"]) {
        let voices = [
            ("salom", "es-CO-SalomeNeural"),
            ("gonzalo", "es-CO-GonzaloNeural"),
            ("dalia", "es-MX-DaliaNeural"),
            ("jorge", "es-MX-JorgeNeural"),
            ("elena", "es-AR-ElenaNeural"),
            ("alonso", "es-US-AlonsoNeural"),
        ];
        if let Some((_, code)) = voices.iter().find(|(k, _)| p.starts_with(k)) {
            return Ok(("tts_voice", json!(code), format!("voice {code}")));
        }
        let re = regex::Regex::new(r"(?i)^[a-z]{2}-[a-z]{2}-\w+Neural$").unwrap();
        if re.is_match(v) {
            return Ok(("tts_voice", json!(v), format!("voice {v}")));
        }
        return Err(format!("unknown voice '{v}'. Options: Salome, Gonzalo, Dalia, Jorge, Elena, Alonso."));
    }
    if is(&["rate", "speed", "velocidad"]) {
        let rate = match p.as_str() {
            "slow" | "lenta" => Some("-15%"),
            "normal" => Some("+0%"),
            "fast" | "rapida" => Some("+20%"),
            "faster" | "muy rapida" => Some("+40%"),
            _ => None,
        };
        if let Some(r) = rate {
            return Ok(("tts_rate", json!(r), format!("speed {r}")));
        }
        if regex::Regex::new(r"^[+-]\d{1,3}%$").unwrap().is_match(&p) {
            return Ok(("tts_rate", json!(p), format!("speed {p}")));
        }
        return Err(format!("unknown speed '{v}'. Options: slow, normal, fast, faster, or like +10%."));
    }
    if is(&["volume", "volumen"]) {
        let x = to_int(to_number(v, 0.0, 100.0)?);
        return Ok(("tts_volume", json!(x), format!("Claude's volume {x}")));
    }
    if is(&["silence", "silencio"]) {
        // [math]::Round is also half-to-even.
        let ms = to_int(to_number(v, 0.5, 10.0)? * 4.0) * 250;
        let secs = format!("{:.2}", ms as f64 / 1000.0);
        let secs = secs.trim_end_matches('0').trim_end_matches('.');
        return Ok(("silence_ms", json!(ms), format!("silence cutoff {secs} s")));
    }
    if is(&["sensitivity", "sensibilidad"]) {
        let x = to_int(to_number(v, 0.0, 100.0)?);
        return Ok(("sensitivity", json!(x), format!("mic sensitivity {x}")));
    }
    if is(&["glass", "vidrio"]) {
        let x = to_int(to_number(v, 0.0, 100.0)?);
        return Ok(("glass", json!(x), format!("glass {x}")));
    }
    if is(&["hotkey", "keys", "teclas", "atajo"]) {
        return Ok(("hotkey", json!(to_hotkey(v)?), format!("hotkey {v}")));
    }
    let boolean = |key: &'static str, label: &str| -> Result<(&'static str, Value, String), String> {
        let b = to_bool(v)?;
        Ok((key, json!(b), format!("{label} {}", on_off(b))))
    };
    if is(&["sound", "sonido"]) {
        return boolean("sound", "start chime");
    }
    if is(&["enter", "auto_enter", "send"]) {
        return boolean("auto_enter", "send with Enter");
    }
    if is(&["wake", "wake_word", "oye"]) {
        return boolean("wake_word", "Oye Claude");
    }
    if is(&["phrase", "wake_phrase", "frase"]) {
        let phrase = v.split_whitespace().collect::<Vec<_>>().join(" ");
        if phrase.is_empty() {
            return Err("the wake phrase can't be empty.".into());
        }
        if phrase.encode_utf16().count() > 40 {
            return Err("the wake phrase is 40 characters at most.".into());
        }
        return Ok(("wake_phrase", json!(phrase), format!("wake phrase \"{phrase}\"")));
    }
    if is(&["spoken", "speak_only_spoken", "solo_voz"]) {
        return boolean("speak_only_spoken", "speak only to dictated messages");
    }
    if is(&["share", "show_in_capture", "capture"]) {
        return boolean("show_in_capture", "visible in screen share");
    }
    if is(&["theme", "tema"]) {
        return Err("there is no theme setting any more: the overlay is always dark glass.".into());
    }
    if is(&["position", "posicion"]) {
        if p.starts_with("bottom") || p.starts_with("abajo") {
            return Ok(("position", json!("bottom"), "overlay at the bottom".into()));
        }
        if p.starts_with("top") || p.starts_with("arriba") {
            return Ok(("position", json!("top"), "overlay at the top".into()));
        }
        return Err("position is bottom or top.".into());
    }
    if is(&["drag", "remember_drag", "arrastre"]) {
        return boolean("remember_drag", "remember dragged spot");
    }
    if is(&["language", "idioma"]) {
        if ["es", "span", "espa"].iter().any(|w| p.starts_with(w)) {
            return Ok(("language", json!("es"), "dictation in Spanish".into()));
        }
        if ["en", "engl", "ingl"].iter().any(|w| p.starts_with(w)) {
            return Ok(("language", json!("en"), "dictation in English".into()));
        }
        if p.starts_with("auto") {
            return Ok(("language", json!("auto"), "dictation language auto".into()));
        }
        return Err("language is es, en or auto.".into());
    }
    Err(format!("unknown setting '{name}'.\n{HELP}"))
}

/// Runs the action; returns the text to print and the exit code.
pub fn run(args: &[String]) -> (String, i32) {
    let mut action = plain(args.first().map(String::as_str).unwrap_or(""));
    let mut value = args.get(1).cloned().unwrap_or_default();
    let mut extra = args.get(2..).map(|a| a.join(" ")).unwrap_or_default();
    const ACTIONS: [&str; 11] = ["on", "off", "toggle", "status", "stop", "settings", "set", "voice", "rate", "volume", "silence"];
    if !ACTIONS.contains(&action.as_str()) {
        return (format!("claudeTalk: unknown action '{action}'. Use on, off, toggle, status, stop, settings or set.\n{HELP}"), 1);
    }
    if ["voice", "rate", "volume", "silence"].contains(&action.as_str()) {
        extra = value;
        value = action.clone();
        action = "set".into();
    }
    match action.as_str() {
        "set" => return set(&value, &extra),
        "settings" => {
            let mut out = format!("claudeTalk settings ({}):\n", ct_core::paths::settings_file().display());
            for (k, v) in settings::read_all() {
                out += &format!("  {k} = {v}\n");
            }
            out += HELP;
            return (out, 0);
        }
        "stop" => {
            ct_core::queue::stop(sessions::session_id(None).as_deref());
            return ("claudeTalk: stopped talking (talk mode stays on).".into(), 0);
        }
        _ => {}
    }
    let Some(sid) = sessions::session_id(None) else {
        return ("claudeTalk: can't tell which Claude Code session this is (run it from inside Claude Code).".into(), 1);
    };
    let sess = sessions::get_state(Some(&sid));
    if action == "toggle" {
        action = if sess.enabled { "off" } else { "on" }.into();
    }
    match action.as_str() {
        "on" => {
            let r = sessions::enable(&sid);
            let name = voice_name(&r.voice);
            if r.own {
                let mut busy: Vec<String> = Vec::new();
                for v in &r.others {
                    let n = voice_name(v);
                    if !busy.contains(&n) {
                        busy.push(n);
                    }
                }
                (format!("claudeTalk: talk mode ON for this session with its OWN voice: {name} ({}). Other sessions are talking with: {}. Announce the voice (see the skill).", r.voice, busy.join(", ")), 0)
            } else {
                (format!("claudeTalk: talk mode ON for this session (voice: {name}). Claude talks to you until you run /talk again."), 0)
            }
        }
        "off" => {
            sessions::disable(&sid);
            ct_core::queue::stop(Some(&sid));
            ("claudeTalk: talk mode OFF for this session (silence).".into(), 0)
        }
        _ => {
            let gear = settings::gear();
            let voice = sessions::effective_voice(&sess, &gear.voice);
            let others = sessions::other_voices(&sid, &Snapshot::take(), &gear.voice);
            (format!(
                "claudeTalk: talk mode {} in this session | voice: {} ({voice}) | speed: {} | other sessions talking: {}",
                if sess.enabled { "ON" } else { "OFF" },
                voice_name(&voice),
                gear.rate,
                others.len()
            ), 0)
        }
    }
}

fn set(name: &str, value: &str) -> (String, i32) {
    let (key, val, desc) = match resolve(name, value) {
        Ok(r) => r,
        Err(e) => return (format!("claudeTalk: {e}"), 1),
    };
    let sid = sessions::session_id(None);
    if let (true, Some(sid)) = (key == "tts_voice", sid) {
        // The voice belongs to this session. The session that speaks with the
        // gear voice (or the only one talking) also moves the gear along.
        let gear = settings::gear();
        let others = sessions::other_voices(&sid, &Snapshot::take(), &gear.voice);
        let voice = val.as_str().unwrap_or_default().to_string();
        let follow = ct_core::lock::with_session_lock(|| {
            let mut sess = sessions::get_state(Some(&sid));
            let follow = sess.follows_default || others.is_empty();
            sess.voice = Some(voice.clone());
            sess.follows_default = follow;
            sessions::set_state(&sid, &sess);
            follow
        });
        if follow {
            let _ = settings::set(key, val);
        }
        let note = if others.contains(&voice) { " Another session already talks with that voice." } else { "" };
        return (format!("claudeTalk: {desc} for this session.{note} Applied now."), 0);
    }
    if let Err(e) = settings::set(key, val) {
        return (format!("claudeTalk: could not save the setting: {e}"), 1);
    }
    (format!("claudeTalk: {desc}. Applied now."), 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn val(name: &str, v: &str) -> Value {
        resolve(name, v).unwrap().1
    }

    #[test]
    fn settings_parse_like_v05() {
        assert_eq!(val("voz", "Salomé"), json!("es-CO-SalomeNeural"));
        assert_eq!(val("velocidad", "Rápida"), json!("+20%"));
        assert_eq!(val("rate", "muy rapida"), json!("+40%"));
        assert_eq!(val("rate", "-5%"), json!("-5%"));
        assert_eq!(val("volumen", "150"), json!(100));
        assert_eq!(val("volume", "42,5"), json!(42));
        assert_eq!(val("silencio", "4,6"), json!(4500));
        assert_eq!(resolve("silence", "2").unwrap().2, "silence cutoff 2 s");
        assert_eq!(resolve("silence", "4.5").unwrap().2, "silence cutoff 4.5 s");
        assert_eq!(val("atajo", "ctrl+alt+space"), json!([0x11, 0x12, 0x20]));
        assert_eq!(val("hotkey", "lctrl + f2 + k + 5"), json!([0xA2, 0x71, 0x4B, 0x35]));
        assert!(resolve("hotkey", "ctrl").is_err());
        assert_eq!(val("sonido", "apagado"), json!(false));
        assert_eq!(val("oye", "Encendido"), json!(true));
        assert!(resolve("enter", "maybe").is_err());
        assert_eq!(val("frase", "  Hola   Jarvis "), json!("Hola Jarvis"));
        assert_eq!(val("posicion", "arriba"), json!("top"));
        assert_eq!(val("idioma", "inglés"), json!("en"));
        assert!(resolve("tema", "x").is_err());
        assert!(resolve("nada", "x").is_err());
    }

    #[test]
    fn bankers_rounding() {
        assert_eq!(to_int(42.5), 42);
        assert_eq!(to_int(43.5), 44);
        assert_eq!(to_int(7.2), 7);
    }
}
