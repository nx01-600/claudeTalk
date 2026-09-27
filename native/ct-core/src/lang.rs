//! claudeTalk's language: the voices, the spoken sample lines, the default
//! wake phrase, the dictation language and the gear panel's text all follow
//! one setting, `lang` in dictation.json (see docs/LANGUAGES.md).
//!
//! Spanish and English are built in. Any other language is a "pack": a JSON
//! file in %APPDATA%\claudeTalk\lang\<code>.json that Claude writes when the
//! user asks for that language (`claudetalk toggle template <code>` prints
//! the blank to translate, `claudetalk voices <code>` lists the voices,
//! `claudetalk toggle pack <file>` installs it).

use crate::fsutil::{read_json, write_json};
use crate::{paths, settings};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Voice {
    /// Edge voice id, e.g. "fr-FR-DeniseNeural"
    pub id: String,
    /// What the user calls it, e.g. "Denise"
    pub name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Pack {
    /// ISO 639-1 code, also the Whisper language: "es", "en", "fr"...
    pub code: String,
    /// The language's own name: "Español", "English", "Français"
    pub name: String,
    /// 2 to 9 voices. The first is the default; the rest are handed out, in
    /// order, to sessions that talk at the same time. Alternate men and
    /// women and mix accents so sessions are easy to tell apart.
    pub voices: Vec<Voice>,
    /// Default wake phrase, e.g. "Oye Claude"
    pub wake_phrase: String,
    /// Played when a voice is picked in the gear panel
    pub sample_voice: String,
    /// Played while the volume slider moves
    pub sample_volume: String,
    /// Spoken when a long answer stays on screen
    pub left_on_screen: String,
    /// Whisper's initial prompt: a sentence in the language with technical
    /// words the user may dictate
    pub whisper_prompt: String,
    /// The gear panel and tray text: English text -> translation. Missing
    /// entries stay in English.
    #[serde(default)]
    pub ui: BTreeMap<String, String>,
}

/// Every piece of text the dictation app shows, in English. `{…}` parts are
/// placeholders that must stay as they are.
pub const UI_TEXT: &[&str] = &[
    "Dictation",
    "Claude's voice",
    "Activation",
    "Keys",
    "Press the keys",
    "Silence cutoff",
    "Mic sensitivity",
    "Sound on start",
    "Send with Enter",
    "Appearance",
    "Glass",
    "Position",
    "Bottom",
    "Top",
    "Remember dragged spot",
    "Show in screen share",
    "Transcription",
    "Language",
    "Detect language automatically",
    "Talk mode",
    "Voice",
    "Speed",
    "Slow",
    "Normal",
    "Fast",
    "Faster",
    "Volume",
    "Start with \u{201c}{phrase}\u{201d}",
    "Wake phrase",
    "Speak only when I talk",
    "Show when Claude talks",
    "Turn off dictation",
    "Turn off?",
    "Cancel",
    "Turn off",
    "Settings",
    "Dictation: {label}",
    "Turn off dictation completely?",
    "downloading the voice model {pct}%",
    "voice model download failed (see log)",
];

fn voices(list: &[(&str, &str)]) -> Vec<Voice> {
    list.iter().map(|(id, name)| Voice { id: id.to_string(), name: name.to_string() }).collect()
}

fn spanish() -> Pack {
    let ui: &[(&str, &str)] = &[
        ("Dictation", "Dictado"),
        ("Claude's voice", "Voz de Claude"),
        ("Activation", "Activación"),
        ("Keys", "Teclas"),
        ("Press the keys", "Pulsa las teclas"),
        ("Silence cutoff", "Silencio para cortar"),
        ("Mic sensitivity", "Sensibilidad del mic"),
        ("Sound on start", "Sonido al empezar"),
        ("Send with Enter", "Enviar con Enter"),
        ("Appearance", "Apariencia"),
        ("Glass", "Vidrio"),
        ("Position", "Posición"),
        ("Bottom", "Abajo"),
        ("Top", "Arriba"),
        ("Remember dragged spot", "Recordar dónde lo arrastré"),
        ("Show in screen share", "Mostrar al compartir pantalla"),
        ("Transcription", "Transcripción"),
        ("Language", "Idioma"),
        ("Detect language automatically", "Detectar el idioma solo"),
        ("Talk mode", "Modo conversación"),
        ("Voice", "Voz"),
        ("Speed", "Velocidad"),
        ("Slow", "Lenta"),
        ("Normal", "Normal"),
        ("Fast", "Rápida"),
        ("Faster", "Muy rápida"),
        ("Volume", "Volumen"),
        ("Start with \u{201c}{phrase}\u{201d}", "Empezar con \u{201c}{phrase}\u{201d}"),
        ("Wake phrase", "Frase de activación"),
        ("Speak only when I talk", "Hablar solo cuando yo hablo"),
        ("Show when Claude talks", "Mostrar cuando Claude habla"),
        ("Turn off dictation", "Apagar el dictado"),
        ("Turn off?", "¿Apagar?"),
        ("Cancel", "Cancelar"),
        ("Turn off", "Apagar"),
        ("Settings", "Ajustes"),
        ("Dictation: {label}", "Dictado: {label}"),
        ("Turn off dictation completely?", "¿Apagar el dictado por completo?"),
        ("downloading the voice model {pct}%", "descargando el modelo de voz {pct}%"),
        ("voice model download failed (see log)", "falló la descarga del modelo de voz (ver el log)"),
    ];
    Pack {
        code: "es".into(),
        name: "Español".into(),
        voices: voices(&[
            ("es-CO-GonzaloNeural", "Gonzalo"),
            ("es-CO-SalomeNeural", "Salomé"),
            ("es-MX-JorgeNeural", "Jorge"),
            ("es-MX-DaliaNeural", "Dalia"),
            ("es-US-AlonsoNeural", "Alonso"),
            ("es-AR-ElenaNeural", "Elena"),
        ]),
        wake_phrase: "Oye Claude".into(),
        sample_voice: "Hola, así sueno cuando te hablo.".into(),
        sample_volume: "Hola, este es el volumen de mi voz.".into(),
        left_on_screen: "Te dejé la respuesta en pantalla.".into(),
        whisper_prompt: "Dictado en espanol para Claude Code: commit, repositorio, hook, pull request, branch, terminal, script, Elementor, Rails, TypeScript, Docker, WordPress.".into(),
        ui: ui.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
    }
}

fn english() -> Pack {
    Pack {
        code: "en".into(),
        name: "English".into(),
        voices: voices(&[
            ("en-US-AndrewNeural", "Andrew"),
            ("en-US-AvaNeural", "Ava"),
            ("en-US-BrianNeural", "Brian"),
            ("en-US-EmmaNeural", "Emma"),
            ("en-GB-RyanNeural", "Ryan"),
            ("en-GB-SoniaNeural", "Sonia"),
        ]),
        wake_phrase: "Hey Claude".into(),
        sample_voice: "Hi, this is how I sound when I talk to you.".into(),
        sample_volume: "Hi, this is how loud my voice is.".into(),
        left_on_screen: "I left the answer on screen.".into(),
        whisper_prompt: "Dictation in English for Claude Code: commit, repository, hook, pull request, branch, terminal, script, TypeScript, Docker, WordPress.".into(),
        ui: BTreeMap::new(),
    }
}

pub fn builtin(code: &str) -> Option<Pack> {
    match code {
        "es" => Some(spanish()),
        "en" => Some(english()),
        _ => None,
    }
}

pub fn packs_dir() -> PathBuf {
    paths::state_dir().join("lang")
}

fn pack_file(code: &str) -> PathBuf {
    packs_dir().join(format!("{code}.json"))
}

/// A language code as claudeTalk stores it: "fr", "pt", "zh"... Accepts
/// "fr-FR" too (keeps "fr").
pub fn normalize_code(code: &str) -> Option<String> {
    let c = code.trim().to_ascii_lowercase();
    let c = c.split(['-', '_']).next().unwrap_or("");
    (c.len() >= 2 && c.len() <= 3 && c.chars().all(|ch| ch.is_ascii_lowercase())).then(|| c.to_string())
}

/// The pack for `code`: built in, or installed by Claude.
pub fn load(code: &str) -> Option<Pack> {
    builtin(code).or_else(|| {
        let p: Pack = serde_json::from_value(read_json(&pack_file(code))?).ok()?;
        validate(&p).ok()?;
        Some(p)
    })
}

/// Codes of every language that can be picked right now, built-in first.
pub fn available() -> Vec<(String, String)> {
    let mut out = vec![("es".to_string(), "Español".to_string()), ("en".to_string(), "English".to_string())];
    if let Ok(dir) = std::fs::read_dir(packs_dir()) {
        let mut extra: Vec<(String, String)> = dir
            .flatten()
            .filter_map(|e| {
                let code = e.path().file_stem()?.to_str()?.to_string();
                let p = load(&code)?;
                (builtin(&code).is_none()).then_some((p.code, p.name))
            })
            .collect();
        extra.sort();
        out.extend(extra);
    }
    out
}

pub fn validate(p: &Pack) -> Result<(), String> {
    if normalize_code(&p.code).as_deref() != Some(p.code.as_str()) {
        return Err(format!("code '{}' is not a language code like \"fr\".", p.code));
    }
    if p.voices.len() < 2 || p.voices.len() > 9 {
        return Err("a pack needs 2 to 9 voices.".into());
    }
    let re = regex::Regex::new(r"^[a-z]{2,3}-[A-Za-z]{2,4}(-[A-Za-z]+)?-\w+Neural$").unwrap();
    if p.name.starts_with('<') || p.voices.iter().any(|v| v.id.contains("-XX-")) {
        return Err("the pack still has the template's placeholders (name, voices).".into());
    }
    if let Some(v) = p.voices.iter().find(|v| !re.is_match(&v.id) || v.name.trim().is_empty()) {
        return Err(format!("'{}' is not an Edge voice id like fr-FR-DeniseNeural (with a name).", v.id));
    }
    for (field, text) in [
        ("name", &p.name),
        ("wake_phrase", &p.wake_phrase),
        ("sample_voice", &p.sample_voice),
        ("sample_volume", &p.sample_volume),
        ("left_on_screen", &p.left_on_screen),
    ] {
        if text.trim().is_empty() {
            return Err(format!("'{field}' is empty."));
        }
    }
    for (k, v) in &p.ui {
        for hole in ["{phrase}", "{label}", "{pct}"] {
            if k.contains(hole) && !v.contains(hole) {
                return Err(format!("the translation of \"{k}\" lost {hole}."));
            }
        }
    }
    Ok(())
}

/// The blank Claude fills in for a new language, with English as the
/// starting text.
pub fn template(code: &str) -> Value {
    let mut p = english();
    p.code = code.to_string();
    p.name = "<the language's own name>".into();
    p.voices = vec![
        Voice { id: format!("{code}-XX-FirstNeural"), name: "First".into() },
        Voice { id: format!("{code}-XX-SecondNeural"), name: "Second".into() },
    ];
    p.ui = UI_TEXT.iter().map(|t| (t.to_string(), t.to_string())).collect();
    serde_json::to_value(p).unwrap()
}

/// Saves a pack Claude wrote; returns it after checking it.
pub fn install(raw: &str) -> Result<Pack, String> {
    let body = raw.trim_start_matches('\u{feff}');
    let p: Pack = serde_json::from_str(body).map_err(|e| format!("not a valid pack: {e}"))?;
    validate(&p)?;
    if builtin(&p.code).is_some() {
        return Err(format!("'{}' is built in and can't be replaced.", p.code));
    }
    std::fs::create_dir_all(packs_dir()).map_err(|e| e.to_string())?;
    write_json(&pack_file(&p.code), &serde_json::to_value(&p).unwrap()).map_err(|e| e.to_string())?;
    Ok(p)
}

/// Windows' display language, if it's one claudeTalk ships.
fn system_language() -> Option<String> {
    let id = unsafe { windows_sys::Win32::Globalization::GetUserDefaultUILanguage() };
    match id & 0x3ff {
        0x0a => Some("es".into()),
        0x09 => Some("en".into()),
        _ => None,
    }
}

/// The current language code. Before v0.8 there was no `lang`: the dictation
/// language (es/en) stood for it.
pub fn current_code() -> String {
    let all = settings::read_all();
    let s = |k: &str| all.get(k).and_then(Value::as_str).and_then(normalize_code);
    s("lang")
        .or_else(|| s("language").filter(|c| c != "aut"))
        .or_else(system_language)
        .unwrap_or_else(|| "en".into())
}

/// The current pack. A missing or broken custom pack falls back to English
/// (Claude still talks in the user's language; only claudeTalk's own lines
/// and voices go English).
pub fn current() -> Pack {
    load(&current_code()).unwrap_or_else(english)
}

static CACHE: Mutex<Option<Arc<Pack>>> = Mutex::new(None);

/// current(), read once per process until `reload()`: for the long-running
/// dictation app, which asks on every paint.
pub fn cached() -> Arc<Pack> {
    let mut c = CACHE.lock().unwrap();
    c.get_or_insert_with(|| Arc::new(current())).clone()
}

pub fn reload() {
    *CACHE.lock().unwrap() = None;
}

impl Pack {
    /// The translation of an English UI text (itself when there is none).
    pub fn tr(&self, english: &str) -> String {
        self.ui.get(english).filter(|t| !t.trim().is_empty()).cloned().unwrap_or_else(|| english.to_string())
    }

    pub fn default_voice(&self) -> &str {
        &self.voices[0].id
    }

    pub fn has_voice(&self, id: &str) -> bool {
        self.voices.iter().any(|v| v.id == id)
    }
}

/// The short name of a voice id, from any known pack.
pub fn voice_name(id: &str) -> String {
    let mut packs = vec![current()];
    packs.extend(["es", "en"].iter().filter_map(|c| builtin(c)));
    packs
        .iter()
        .flat_map(|p| p.voices.iter())
        .find(|v| v.id == id)
        .map(|v| v.name.clone())
        .unwrap_or_else(|| id.to_string())
}

/// Switches claudeTalk to `code`: the pack must exist. Moves along the
/// voice, the dictation language and the wake phrase (unless the user wrote
/// their own), and hands the talking sessions voices of the new language.
/// Returns the pack.
pub fn apply(code: &str) -> Result<Pack, String> {
    let code = normalize_code(code).ok_or_else(|| format!("'{code}' is not a language code like \"fr\"."))?;
    let new = load(&code).ok_or_else(|| format!("no pack for '{code}' yet."))?;
    let old = current();
    let mut all = settings::read_all();
    all.insert("lang".into(), json!(new.code));
    // The dictation follows unless it detects the language by itself.
    if all.get("language").and_then(Value::as_str) != Some("auto") {
        all.insert("language".into(), json!(new.code));
    }
    let voice = all.get("tts_voice").and_then(Value::as_str).unwrap_or("").to_string();
    if !new.has_voice(&voice) {
        all.insert("tts_voice".into(), json!(new.default_voice()));
    }
    let phrase = all.get("wake_phrase").and_then(Value::as_str).unwrap_or("").to_string();
    let stock = [old.wake_phrase.as_str(), "Oye Claude", "Hey Claude", ""];
    if stock.iter().any(|p| p.eq_ignore_ascii_case(phrase.trim())) {
        all.insert("wake_phrase".into(), json!(new.wake_phrase));
    }
    write_json(&paths::settings_file(), &Value::Object(all)).map_err(|e| e.to_string())?;
    crate::sessions::reassign_voices(&new);
    reload();
    Ok(new)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_are_valid_and_complete() {
        for code in ["es", "en"] {
            let p = builtin(code).unwrap();
            validate(&p).unwrap();
            assert_eq!(p.code, code);
        }
        let es = spanish();
        for t in UI_TEXT {
            assert!(es.ui.contains_key(*t), "Spanish misses {t:?}");
        }
        assert_eq!(es.ui.len(), UI_TEXT.len());
    }

    #[test]
    fn codes() {
        assert_eq!(normalize_code("FR-fr").as_deref(), Some("fr"));
        assert_eq!(normalize_code("pt_BR").as_deref(), Some("pt"));
        assert_eq!(normalize_code("auto").as_deref(), None);
        assert_eq!(normalize_code("x").as_deref(), None);
    }

    #[test]
    fn template_round_trips_and_is_rejected_until_filled() {
        let t = template("fr");
        let p: Pack = serde_json::from_value(t).unwrap();
        assert_eq!(p.ui.len(), UI_TEXT.len());
        assert!(validate(&p).is_err(), "an unfilled template must not install");
        let mut p = p;
        p.name = "Fran\u{e7}ais".into();
        p.voices = voices(&[("fr-FR-DeniseNeural", "Denise"), ("fr-FR-HenriNeural", "Henri")]);
        validate(&p).unwrap();
        let mut bad = p.clone();
        bad.ui.insert("Dictation: {label}".into(), "Dictée".into());
        assert!(validate(&bad).is_err());
        bad = p;
        bad.voices.truncate(1);
        assert!(validate(&bad).is_err());
    }

    #[test]
    fn tr_falls_back_to_english() {
        assert_eq!(spanish().tr("Volume"), "Volumen");
        assert_eq!(spanish().tr("Something new"), "Something new");
        assert_eq!(english().tr("Volume"), "Volume");
    }
}
