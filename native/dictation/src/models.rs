//! Downloads the models the first time (Whisper ~0.9 GB, Silero VAD ~1 MB)
//! into %LOCALAPPDATA%\claudeTalk\models. Resumable, retried: the Hugging
//! Face CDN drops connections now and then.

use crate::stt::{models_dir, VAD_FILE, WHISPER_FILE};
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::time::Duration;

const SOURCES: [(&str, &str); 2] = [
    (VAD_FILE, "https://huggingface.co/ggml-org/whisper-vad/resolve/main/"),
    (WHISPER_FILE, "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/"),
];

pub fn missing() -> bool {
    SOURCES.iter().any(|(f, _)| !models_dir().join(f).exists())
}

/// Fetches every missing model. `progress(file, done_bytes, total_bytes)`.
pub fn ensure(mut progress: impl FnMut(&str, u64, u64)) -> Result<(), String> {
    std::fs::create_dir_all(models_dir()).map_err(|e| e.to_string())?;
    for (file, base) in SOURCES {
        let dest = models_dir().join(file);
        if dest.exists() {
            continue;
        }
        let part = dest.with_extension("part");
        let mut last_err = String::new();
        for attempt in 0..8 {
            match fetch(&format!("{base}{file}"), &part, &mut |d, t| progress(file, d, t)) {
                Ok(()) => {
                    std::fs::rename(&part, &dest).map_err(|e| e.to_string())?;
                    last_err.clear();
                    break;
                }
                Err(e) => {
                    last_err = e;
                    std::thread::sleep(Duration::from_secs(2 + attempt * 2));
                }
            }
        }
        if !last_err.is_empty() {
            return Err(format!("{file}: {last_err}"));
        }
    }
    Ok(())
}

fn fetch(url: &str, part: &std::path::Path, progress: &mut dyn FnMut(u64, u64)) -> Result<(), String> {
    let have = std::fs::metadata(part).map(|m| m.len()).unwrap_or(0);
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(20)))
        .timeout_recv_body(Some(Duration::from_secs(60)))
        .build()
        .into();
    let mut req = agent.get(url);
    if have > 0 {
        req = req.header("Range", &format!("bytes={have}-"));
    }
    let resp = req.call().map_err(|e| e.to_string())?;
    let resumed = resp.status() == 206;
    let body_len: u64 = resp
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let start = if resumed { have } else { 0 };
    let total = start + body_len;
    let mut out = OpenOptions::new()
        .create(true)
        .write(true)
        .append(resumed)
        .truncate(!resumed)
        .open(part)
        .map_err(|e| e.to_string())?;
    let mut reader = resp.into_body().into_reader();
    let mut buf = vec![0u8; 1 << 16];
    let mut done = start;
    loop {
        let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        out.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        done += n as u64;
        progress(done, total);
    }
    if total > 0 && done < total {
        return Err(format!("short read {done}/{total}"));
    }
    Ok(())
}
