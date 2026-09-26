//! The dictation daemon (claudetalk-dictation.exe, ~60 MB: whisper.cpp with
//! every Vulkan shader) is not in the git repo, so the plugin stays light.
//! It is downloaded once per version from the GitHub release of that same
//! version into %LOCALAPPDATA%\claudeTalk\bin\<version>\.

use std::io::{Read, Write};
use std::path::PathBuf;
use std::time::Duration;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
const REPO: &str = "nx01-600/claudeTalk";
const EXE: &str = "claudetalk-dictation.exe";

pub fn versions_dir() -> PathBuf {
    ct_core::paths::local_dir().join("bin")
}

fn downloaded_path() -> PathBuf {
    versions_dir().join(VERSION).join(EXE)
}

/// Where the daemon is: next to this exe (a local build), else the copy
/// downloaded for this version.
pub fn dictation_exe() -> Option<PathBuf> {
    let beside = std::env::current_exe().ok()?.parent()?.join(EXE);
    if beside.exists() {
        return Some(beside);
    }
    let d = downloaded_path();
    d.exists().then_some(d)
}

/// Remembers where this claudetalk.exe lives, for the daemon (voice
/// previews) and the Start Menu shortcut.
pub fn remember_self() {
    if let Ok(me) = std::env::current_exe() {
        let file = ct_core::paths::state_dir().join("claudetalk-path.txt");
        let text = me.display().to_string();
        if std::fs::read_to_string(&file).ok().as_deref() != Some(text.as_str()) {
            let _ = std::fs::create_dir_all(ct_core::paths::state_dir());
            let _ = std::fs::write(file, text);
        }
    }
}

/// Downloads the daemon for this version. One download at a time
/// machine-wide; resumable; checks it got a Windows executable.
pub fn download() -> Result<PathBuf, String> {
    let dest = downloaded_path();
    let lock = ct_core::lock::NamedMutex::new("Local\\claudetalk_fetch").ok_or("no mutex")?;
    let _g = lock.acquire(15 * 60 * 1000).ok_or("another download is still running")?;
    if dest.exists() {
        return Ok(dest); // the other download finished it
    }
    std::fs::create_dir_all(dest.parent().unwrap()).map_err(|e| e.to_string())?;
    let url = format!("https://github.com/{REPO}/releases/download/v{VERSION}/{EXE}");
    let part = dest.with_extension("part");
    let mut last = String::new();
    for attempt in 0..6u64 {
        match fetch(&url, &part) {
            Ok(()) => {
                let mut head = [0u8; 2];
                let ok = std::fs::File::open(&part).and_then(|mut f| f.read_exact(&mut head)).is_ok() && &head == b"MZ";
                if !ok || std::fs::metadata(&part).map(|m| m.len()).unwrap_or(0) < 1_000_000 {
                    let _ = std::fs::remove_file(&part);
                    return Err(format!("{url} is not the dictation executable"));
                }
                std::fs::rename(&part, &dest).map_err(|e| e.to_string())?;
                return Ok(dest);
            }
            Err(e) => {
                last = e;
                std::thread::sleep(Duration::from_secs(2 + attempt * 3));
            }
        }
    }
    Err(format!("{url}: {last}"))
}

fn fetch(url: &str, part: &std::path::Path) -> Result<(), String> {
    let have = std::fs::metadata(part).map(|m| m.len()).unwrap_or(0);
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(20)))
        .timeout_recv_body(Some(Duration::from_secs(60)))
        .tls_config(ureq::tls::TlsConfig::builder().provider(ureq::tls::TlsProvider::NativeTls).build())
        .build()
        .into();
    let mut req = agent.get(url).header("User-Agent", "claudeTalk");
    if have > 0 {
        req = req.header("Range", &format!("bytes={have}-"));
    }
    let resp = req.call().map_err(|e| e.to_string())?;
    let resumed = resp.status() == 206;
    let mut out = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .append(resumed)
        .truncate(!resumed)
        .open(part)
        .map_err(|e| e.to_string())?;
    let mut reader = resp.into_body().into_reader();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        out.write_all(&buf[..n]).map_err(|e| e.to_string())?;
    }
    Ok(())
}
