//! claudetalk.exe: the plugin side of claudeTalk in one native binary.
//!
//!   claudetalk hook session-start|prompt|stop   Claude Code hooks (stdin JSON)
//!   claudetalk mcp                              MCP server with the `say` tool
//!   claudetalk toggle ACTION [SETTING] [VALUE]  /talk: talk mode and settings
//!   claudetalk dictation                        /dictation: start the daemon
//!   claudetalk preview VOICE RATE VOLUME [volume]  gear panel voice sample
//!   claudetalk voices CODE                      Edge voices of a language (for language packs)
//!   claudetalk cleanup [--yes]                  remove what older versions left
//!   claudetalk fetch-dictation [--start]        download the dictation app for this version
//!   claudetalk speaker                          speech queue drainer (internal)

mod cleanup;
mod daemon;
mod edge;
mod fetch;
mod hooks;
mod mcp;
mod speaker;
mod toggle;

/// Models the dictation daemon loads (kept by `cleanup`).
pub const MODEL_FILES: [&str; 2] = ["ggml-large-v3-turbo-q8_0.bin", "ggml-silero-v6.2.0.bin"];


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
        "fetch-dictation" => std::process::exit(daemon::fetch_command(arg(1) == "--start")),
        "cleanup" => std::process::exit(cleanup::run(arg(1) == "--yes")),
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
                "downloading" => "claudeTalk: downloading the dictation app for this version (~60 MB, once); it starts by itself when done.",
                _ => "claudeTalk: dictation could not be started (see %TEMP%\\claudetalk.log).",
            };
            println!("{msg}");
        }
        "preview" => {
            // Cut everything and play a sample with the voice just picked.
            let volume = arg(3).parse().unwrap_or(100);
            // The volume slider plays another line, so it sounds different.
            let pack = ct_core::lang::current();
            let sample = if arg(4) == "volume" { &pack.sample_volume } else { &pack.sample_voice };
            ct_core::queue::stop(None);
            ct_core::queue::add(sample, arg(1), if arg(2).is_empty() { "+0%" } else { arg(2) }, volume, "");
        }
        "voices" => std::process::exit(edge::print_voices(arg(1))),
        "--version" | "-V" => println!("claudetalk {}", env!("CARGO_PKG_VERSION")),
        _ => {
            eprintln!("usage: claudetalk hook session-start|prompt|stop | mcp | toggle ... | dictation | cleanup [--yes] | voices CODE | preview VOICE RATE VOLUME [volume]");
            std::process::exit(2);
        }
    }
}
