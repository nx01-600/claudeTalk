use std::io::Write;

/// Appends a line to %TEMP%\claudetalk.log. Never fails.
pub fn log(msg: &str) {
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(crate::paths::log_file())
    {
        let _ = writeln!(f, "[{}] {msg}", crate::fsutil::now_iso());
    }
}
