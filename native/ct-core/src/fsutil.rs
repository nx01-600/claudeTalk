//! JSON files shared with other processes: readers retry while a writer
//! swaps the file, writers write a temp file and rename it over.

use serde_json::Value;
use std::fs;
use std::path::Path;
use std::thread::sleep;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Reads a JSON file, tolerating a UTF-8 BOM. None when it is missing or
/// still unreadable after a few tries (someone is rewriting it).
pub fn read_json(path: &Path) -> Option<Value> {
    for _ in 0..5 {
        let bytes = match fs::read(path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
            Err(_) => {
                sleep(Duration::from_millis(40));
                continue;
            }
        };
        let body = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&bytes);
        match serde_json::from_slice(body) {
            Ok(v) => return Some(v),
            Err(_) => sleep(Duration::from_millis(40)),
        }
    }
    None
}

/// Writes pretty JSON without a BOM, atomically.
pub fn write_json(path: &Path, value: &Value) -> std::io::Result<()> {
    let text = serde_json::to_string_pretty(value).unwrap_or_default();
    write_atomic(path, text.as_bytes())
}

pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(format!(".{}.tmp", std::process::id()));
    fs::write(&tmp, bytes)?;
    // Another process may hold the target open for a moment.
    for attempt in 0..5 {
        match fs::rename(&tmp, path) {
            Ok(()) => return Ok(()),
            Err(e) if attempt == 4 => {
                let _ = fs::remove_file(&tmp);
                return Err(e);
            }
            Err(_) => sleep(Duration::from_millis(20)),
        }
    }
    Ok(())
}

/// Current UTC time as ISO 8601 (like PowerShell's `Get-Date -Format o`).
pub fn now_iso() -> String {
    let d = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    let secs = d.as_secs() as i64;
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    let (y, m, dd) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{dd:02}T{:02}:{:02}:{:02}.{:07}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60,
        d.subsec_nanos() / 100
    )
}

/// Days since 1970-01-01 to (year, month, day). Howard Hinnant's algorithm.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(20_722), (2026, 9, 26));
    }

    #[test]
    fn bom_and_roundtrip() {
        let dir = std::env::temp_dir().join(format!("ct-fsutil-{}", std::process::id()));
        let file = dir.join("a.json");
        fs::create_dir_all(&dir).unwrap();
        fs::write(&file, b"\xEF\xBB\xBF{\"a\": 1}").unwrap();
        assert_eq!(read_json(&file).unwrap()["a"], 1);
        write_json(&file, &serde_json::json!({"b": "\u{f1}"})).unwrap();
        let raw = fs::read(&file).unwrap();
        assert!(!raw.starts_with(b"\xEF\xBB\xBF"));
        assert_eq!(read_json(&file).unwrap()["b"], "\u{f1}");
        fs::remove_dir_all(&dir).unwrap();
    }
}
