//! Parity with the v0.5 PowerShell code on real transcripts. The fixtures are
//! private, so they are generated locally by native/tests/gen-golden.ps1; the
//! test passes vacuously when they are missing (CI).

use ct_core::{speech_text::to_speech, transcript::read_turn};
use serde_json::Value;
use std::path::PathBuf;

#[test]
fn matches_powershell() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../tests/golden-local");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        eprintln!("no golden-local fixtures, skipped");
        return;
    };
    let mut checked = 0;
    let mut failures = Vec::new();
    for e in entries.flatten() {
        let path = e.path();
        if !path.to_string_lossy().ends_with(".jsonl") {
            continue;
        }
        let exp_path = PathBuf::from(path.to_string_lossy().replace(".jsonl", ".expected.json"));
        let raw = std::fs::read(&exp_path).unwrap();
        let exp: Value = serde_json::from_slice(raw.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&raw)).unwrap();
        let turn = read_turn(&path);
        let got = serde_json::json!({
            "spoke": turn.spoke,
            "work_after_say": turn.work_after_say,
            "text_after": turn.text_after,
            "spoken": turn.spoken,
            "speech": to_speech(&turn.text_after, true),
            "speech_code": to_speech(&turn.text_after, false),
        });
        for key in ["spoke", "work_after_say", "text_after", "spoken", "speech", "speech_code"] {
            if got[key] != exp[key] {
                failures.push(format!("{}: {key}\n  ps:   {:?}\n  rust: {:?}", path.display(), exp[key], got[key]));
            }
        }
        checked += 1;
    }
    eprintln!("{checked} golden cases checked");
    assert!(failures.is_empty(), "{} mismatches:\n{}", failures.len(), failures.join("\n"));
}
