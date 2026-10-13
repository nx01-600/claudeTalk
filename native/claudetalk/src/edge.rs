//! Microsoft Edge "Read aloud" TTS over its websocket, without Python: the
//! same protocol as the edge-tts package (rany2/edge-tts 7.2). v0.5 paid
//! ~2 s of Python start-up per phrase for it.
//!
//! If Microsoft rotates the accepted Edge version, set `edge_version` in
//! dictation.json (e.g. "143.0.3650.75") without waiting for a new build.
//! The Sec-MS-GEC token is in edge_drm.rs (v2 since edge-tts 7.3: the old
//! SHA-256 one gets a 403).

use crate::edge_drm::sec_ms_gec;
use sha2::{Digest, Sha256};
use std::net::TcpStream;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::mpsc::Sender;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tungstenite::client::IntoClientRequest;
use tungstenite::http::HeaderValue;
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket};

const TRUSTED_CLIENT_TOKEN: &str = "6A5AA1D4EAFF4E9FB37E23D68491D6F4";
const WSS_URL: &str = "wss://speech.platform.bing.com/consumer/speech/synthesize/readaloud/edge/v1";
const DEFAULT_EDGE_VERSION: &str = "143.0.3650.75";
/// The service rejects SSML requests bigger than this (edge-tts splits too).
const MAX_CHUNK_BYTES: usize = 4096;

fn edge_version() -> String {
    ct_core::settings::read_all()
        .get("edge_version")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or(DEFAULT_EDGE_VERSION)
        .to_string()
}

/// Seconds to add to this computer's clock to match Microsoft's: the token
/// depends on the time, and a 403 carries the server's `Date`.
static CLOCK_SKEW: AtomicI64 = AtomicI64::new(0);

fn unix_now() -> u64 {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() as i64;
    (now + CLOCK_SKEW.load(Ordering::Relaxed)).max(0) as u64
}

/// "Tue, 13 Oct 2026 23:00:00 GMT" to unix seconds.
fn parse_http_date(s: &str) -> Option<i64> {
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let p: Vec<&str> = s.split_whitespace().collect();
    let (day, month, year) = (p.get(1)?.parse::<i64>().ok()?, { let m = *p.get(2)?; MONTHS.iter().position(|x| *x == m)? as i64 + 1 }, p.get(3)?.parse::<i64>().ok()?);
    let hms: Vec<i64> = p.get(4)?.split(':').filter_map(|x| x.parse().ok()).collect();
    if hms.len() != 3 {
        return None;
    }
    // days from civil (Howard Hinnant)
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(days * 86_400 + hms[0] * 3_600 + hms[1] * 60 + hms[2])
}

fn random_hex(bytes: usize) -> String {
    // Good enough for connection ids: time + address entropy through SHA-256.
    let seed = format!(
        "{:?}{:p}{}",
        SystemTime::now(),
        &bytes as *const usize,
        std::process::id()
    );
    let d = Sha256::digest(seed.as_bytes());
    d.iter().take(bytes).map(|b| format!("{b:02x}")).collect()
}

fn js_date() -> String {
    // "Thu Sep 26 2026 12:00:00 GMT+0000 (Coordinated Universal Time)"
    const DAYS: [&str; 7] = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"];
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let iso = ct_core::fsutil::now_iso(); // 2026-09-26T12:00:00.0000000Z
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    let dow = DAYS[((secs / 86_400) % 7) as usize];
    let month: usize = iso[5..7].parse().unwrap_or(1);
    format!(
        "{dow} {} {} {} {} GMT+0000 (Coordinated Universal Time)",
        MONTHS[month - 1],
        &iso[8..10],
        &iso[0..4],
        &iso[11..19]
    )
}

fn escape_xml(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            // Control characters the service refuses (edge-tts does the same).
            '\u{0}'..='\u{8}' | '\u{b}' | '\u{c}' | '\u{e}'..='\u{1f}' => out.push(' '),
            c => out.push(c),
        }
    }
    out
}

/// Splits escaped text into pieces under the size limit, at a space when
/// possible and never inside an `&...;` entity.
fn split_chunks(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while rest.len() > MAX_CHUNK_BYTES {
        let mut cut = MAX_CHUNK_BYTES;
        while !rest.is_char_boundary(cut) {
            cut -= 1;
        }
        if let Some(sp) = rest[..cut].rfind(' ') {
            cut = sp + 1;
        }
        if let Some(amp) = rest[..cut].rfind('&') {
            if !rest[amp..cut].contains(';') {
                cut = amp;
            }
        }
        out.push(rest[..cut].to_string());
        rest = &rest[cut..];
    }
    if !rest.trim().is_empty() {
        out.push(rest.to_string());
    }
    out
}

fn connect() -> Result<WebSocket<MaybeTlsStream<TcpStream>>, String> {
    let version = edge_version();
    let major = version.split('.').next().unwrap_or("143").to_string();
    let now = unix_now();
    let url = format!(
        "{WSS_URL}?TrustedClientToken={TRUSTED_CLIENT_TOKEN}&ConnectionId={}&Sec-MS-GEC={}&Sec-MS-GEC-Version=2-{version}",
        random_hex(16),
        sec_ms_gec(now)
    );
    let mut req = url.into_client_request().map_err(|e| e.to_string())?;
    let h = req.headers_mut();
    let ua = format!(
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/{major}.0.0.0 Safari/537.36 Edg/{major}.0.0.0"
    );
    let muid = random_hex(16).to_uppercase();
    for (k, v) in [
        ("Pragma", "no-cache".to_string()),
        ("Cache-Control", "no-cache".to_string()),
        ("Origin", "chrome-extension://jdiccldimpdaibmpdkjnbmckianbfold".to_string()),
        ("User-Agent", ua),
        ("Accept-Encoding", "gzip, deflate, br, zstd".to_string()),
        ("Accept-Language", "en-US,en;q=0.9".to_string()),
        ("Cookie", format!("muid={muid};")),
    ] {
        if let Ok(v) = HeaderValue::from_str(&v) {
            h.insert(k, v);
        }
    }
    let (ws, _) = tungstenite::connect(req).map_err(|e| {
        // 403: usually a wrong clock. Learn the server's, and the retry in
        // `synthesize` uses it.
        if let tungstenite::Error::Http(resp) = &e {
            let date = resp.headers().get("date").and_then(|v| v.to_str().ok()).and_then(parse_http_date);
            if let Some(server) = date {
                let local = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() as i64;
                CLOCK_SKEW.store(server - local, Ordering::Relaxed);
            }
        }
        e.to_string()
    })?;
    let tcp = match ws.get_ref() {
        MaybeTlsStream::NativeTls(s) => Some(s.get_ref()),
        MaybeTlsStream::Plain(s) => Some(s),
        _ => None,
    };
    if let Some(tcp) = tcp {
        let _ = tcp.set_read_timeout(Some(Duration::from_secs(10)));
        let _ = tcp.set_nodelay(true);
    }
    Ok(ws)
}

/// Synthesizes `text` and sends the mp3 bytes to `out` as they arrive.
/// Returns an error if nothing could be synthesized.
pub fn synthesize(text: &str, voice: &str, rate: &str, out: &Sender<Vec<u8>>) -> Result<(), String> {
    let mut ws = match connect() {
        Ok(ws) => ws,
        // a rejected token with a wrong clock: connect() just learned the skew
        Err(_) if CLOCK_SKEW.load(Ordering::Relaxed).abs() > 60 => connect()?,
        Err(e) => return Err(e),
    };
    ws.send(Message::text(format!(
        "X-Timestamp:{}\r\nContent-Type:application/json; charset=utf-8\r\nPath:speech.config\r\n\r\n\
         {{\"context\":{{\"synthesis\":{{\"audio\":{{\"metadataoptions\":{{\"sentenceBoundaryEnabled\":\"true\",\"wordBoundaryEnabled\":\"false\"}},\
         \"outputFormat\":\"audio-24khz-48kbitrate-mono-mp3\"}}}}}}}}\r\n",
        js_date()
    )))
    .map_err(|e| e.to_string())?;
    let mut got_audio = false;
    for chunk in split_chunks(&escape_xml(text)) {
        let ssml = format!(
            "<speak version='1.0' xmlns='http://www.w3.org/2001/10/synthesis' xml:lang='en-US'>\
             <voice name='{voice}'><prosody pitch='+0Hz' rate='{rate}' volume='+0%'>{chunk}</prosody></voice></speak>"
        );
        ws.send(Message::text(format!(
            "X-RequestId:{}\r\nContent-Type:application/ssml+xml\r\nX-Timestamp:{}Z\r\nPath:ssml\r\n\r\n{ssml}",
            random_hex(16),
            js_date()
        )))
        .map_err(|e| e.to_string())?;
        loop {
            match ws.read().map_err(|e| e.to_string())? {
                Message::Text(t) => {
                    if t.as_str().contains("Path:turn.end") {
                        break;
                    }
                }
                Message::Binary(b) => {
                    if b.len() < 2 {
                        continue;
                    }
                    let hlen = u16::from_be_bytes([b[0], b[1]]) as usize;
                    if hlen + 2 > b.len() {
                        continue;
                    }
                    let headers = String::from_utf8_lossy(&b[2..2 + hlen]);
                    let data = &b[2 + hlen..];
                    if headers.contains("Path:audio") && headers.contains("Content-Type:audio/mpeg") && !data.is_empty() {
                        got_audio = true;
                        if out.send(data.to_vec()).is_err() {
                            return Ok(()); // the player was cut
                        }
                    }
                }
                Message::Close(_) => return Err("connection closed".into()),
                _ => {}
            }
        }
    }
    let _ = ws.close(None);
    if got_audio {
        Ok(())
    } else {
        Err("no audio received".into())
    }
}

/// `claudetalk voices CODE`: the Edge voices of a language, one per line as
/// `id  gender  locale`, for Claude to pick from when writing a language
/// pack. Returns the exit code.
pub fn print_voices(code: &str) -> i32 {
    let Some(code) = ct_core::lang::normalize_code(code) else {
        eprintln!("usage: claudetalk voices CODE   (a language code like fr, pt, de)");
        return 2;
    };
    let now = unix_now();
    let url = format!(
        "https://speech.platform.bing.com/consumer/speech/synthesize/readaloud/voices/list?trustedclienttoken={TRUSTED_CLIENT_TOKEN}&Sec-MS-GEC={}&Sec-MS-GEC-Version=2-{}",
        sec_ms_gec(now),
        edge_version()
    );
    let body = ct_core::http::agent().get(&url).call().and_then(|mut r| r.body_mut().read_to_string());
    let list: serde_json::Value = match body.map_err(|e| e.to_string()).and_then(|b| serde_json::from_str(&b).map_err(|e| e.to_string())) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("claudeTalk: could not get the voice list: {e}");
            return 1;
        }
    };
    let prefix = format!("{code}-");
    let mut n = 0;
    for v in list.as_array().into_iter().flatten() {
        let s = |k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("");
        if s("Locale").to_ascii_lowercase().starts_with(&prefix) {
            println!("{}  {}  {}", s("ShortName"), s("Gender"), s("Locale"));
            n += 1;
        }
    }
    if n == 0 {
        eprintln!("claudeTalk: Edge has no voices for '{code}'.");
        return 1;
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Talks to Microsoft: `cargo test -- --ignored live_synthesis`.
    #[test]
    #[ignore]
    fn live_synthesis() {
        let (tx, rx) = std::sync::mpsc::channel();
        synthesize("Hola, prueba.", "es-CO-SalomeNeural", "+0%", &tx).expect("synthesis");
        drop(tx);
        let bytes: usize = rx.iter().map(|c| c.len()).sum();
        assert!(bytes > 1000, "only {bytes} bytes of audio");
    }

    #[test]
    fn http_date() {
        assert_eq!(parse_http_date("Thu, 01 Jan 1970 00:00:10 GMT"), Some(10));
        assert_eq!(parse_http_date("Tue, 13 Oct 2026 23:00:00 GMT"), Some(1_791_932_400));
        assert_eq!(parse_http_date("garbage"), None);
    }

    #[test]
    fn escape_and_split() {
        assert_eq!(escape_xml("a<b & 'c'\u{b}"), "a&lt;b &amp; &apos;c&apos; ");
        let long = "palabra ".repeat(1200);
        let parts = split_chunks(&long);
        assert!(parts.len() > 1 && parts.iter().all(|p| p.len() <= MAX_CHUNK_BYTES));
        assert_eq!(parts.concat(), long);
    }

    #[test]
    fn date_shape() {
        let d = js_date();
        assert!(d.ends_with("GMT+0000 (Coordinated Universal Time)"), "{d}");
    }
}
