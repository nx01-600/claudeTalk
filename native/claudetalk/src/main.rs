//! claudetalk.exe: the plugin side of claudeTalk in one native binary.
//!
//!   claudetalk hook session-start|prompt|stop   Claude Code hooks (stdin JSON)
//!   claudetalk mcp                              MCP server with the `say` tool
//!   claudetalk toggle ACTION [SETTING] [VALUE]  /talk: talk mode and settings
//!   claudetalk dictation                        /dictation: start the daemon
//!   claudetalk preview VOICE RATE VOLUME [volume]  gear panel voice sample
//!   claudetalk speaker                          speech queue drainer (internal)

mod daemon;
mod edge;
mod hooks;
mod mcp;
mod speaker;
mod toggle;

pub const SAMPLE_VOICE: &str = "Hola, as\u{ed} sueno cuando te hablo.";
/// Played by the volume slider, so it sounds different from the voice sample.
pub const SAMPLE_VOLUME: &str = "Hola, este es el volumen de mi voz.";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let arg = |i: usize| args.get(i).map(String::as_str).unwrap_or("");
    match arg(0) {
        "hook" => match arg(1) {
            "session-start" => hooks::session_start(),
            "prompt" => hooks::prompt(),
            "stop" => hooks::stop(),
            other => ct_core::log::log(&format!("unknown hook '{other}'")),
        },
        "mcp" => mcp::run(),
        "speaker" => speaker::run(),
        "toggle" => {
            let (out, code) = toggle::run(&args[1..]);
            println!("{out}");
            std::process::exit(code);
        }
        "dictation" => {
            let msg = match daemon::ensure() {
                "running" => "claudeTalk: dictation was already running.",
                "started" => "claudeTalk: dictation started.",
                "not-installed" => "claudeTalk: bin\\claudetalk-dictation.exe is missing: reinstall or update the plugin.",
                _ => "claudeTalk: dictation could not be started (see %TEMP%\\claudetalk.log).",
            };
            println!("{msg}");
        }
        "preview" => {
            // Cut everything and play a sample with the voice just picked.
            let volume = arg(3).parse().unwrap_or(100);
            let sample = if arg(4) == "volume" { SAMPLE_VOLUME } else { SAMPLE_VOICE };
            ct_core::queue::stop(None);
            ct_core::queue::add(sample, arg(1), if arg(2).is_empty() { "+0%" } else { arg(2) }, volume, "");
        }
        "--version" | "-V" => println!("claudetalk {}", env!("CARGO_PKG_VERSION")),
        _ => {
            eprintln!("usage: claudetalk hook session-start|prompt|stop | mcp | toggle ... | dictation | preview VOICE RATE VOLUME [volume]");
            std::process::exit(2);
        }
    }
}
