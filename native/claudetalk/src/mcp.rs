//! `claudetalk mcp`: the MCP server with the `say` tool (say-server.ps1 in
//! v0.5). stdio, JSON-RPC, one message per line. Claude Code starts one per
//! session; the session is looked up from the claude.exe it runs under.

use serde_json::{json, Value};
use std::io::{BufRead, Write};

fn say_tool() -> Value {
    json!({
        "name": "say",
        "description": "Speak a short phrase out loud to the user (claudeTalk talk mode). Plays in the background and returns at once; several calls play in order. Use only while talk mode is on, with 1-2 natural spoken sentences: no code, paths, symbols or markdown.",
        "inputSchema": {
            "type": "object",
            "properties": {"text": {"type": "string", "description": "What to say, in the user's language."}},
            "required": ["text"]
        }
    })
}

fn say(text: &str) -> &'static str {
    let dir = std::env::var_os("CLAUDE_PROJECT_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    let st = crate::hooks::talk_state(&dir, ct_core::sessions::session_id(None).as_deref());
    if !st.enabled {
        return "talk mode is off: nothing was spoken. Don't call say until the user turns talk mode on.";
    }
    crate::hooks::speak(text, &st);
    "spoken"
}

pub fn handle(msg: &Value) -> Option<Value> {
    let id = msg.get("id").cloned();
    let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
    let result = |r: Value| Some(json!({"jsonrpc": "2.0", "id": id, "result": r}));
    let error = |code: i64, m: String| Some(json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": m}}));
    match method {
        "initialize" => {
            let version = msg
                .pointer("/params/protocolVersion")
                .and_then(Value::as_str)
                .filter(|v| !v.is_empty())
                .unwrap_or("2024-11-05");
            result(json!({
                "protocolVersion": version,
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "claudeTalk-voice", "version": env!("CARGO_PKG_VERSION")}
            }))
        }
        "ping" => result(json!({})),
        "tools/list" => result(json!({"tools": [say_tool()]})),
        "tools/call" => {
            let name = msg.pointer("/params/name").and_then(Value::as_str).unwrap_or("");
            if name != "say" {
                return error(-32602, format!("unknown tool: {name}"));
            }
            let text = msg.pointer("/params/arguments/text").and_then(Value::as_str).unwrap_or("");
            result(json!({"content": [{"type": "text", "text": say(text)}]}))
        }
        // Notifications (no id) need no answer.
        _ if id.is_none() => None,
        _ => error(-32601, format!("method not found: {method}")),
    }
}

pub fn run() {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let Ok(msg) = serde_json::from_str::<Value>(&line) else { continue };
        if let Some(reply) = handle(&msg) {
            let _ = writeln!(stdout, "{reply}");
            let _ = stdout.flush();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol() {
        let r = handle(&json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}})).unwrap();
        assert_eq!(r["result"]["protocolVersion"], "2025-06-18");
        assert_eq!(r["result"]["serverInfo"]["name"], "claudeTalk-voice");
        let r = handle(&json!({"jsonrpc":"2.0","id":2,"method":"tools/list"})).unwrap();
        assert_eq!(r["result"]["tools"][0]["name"], "say");
        let r = handle(&json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"x"}})).unwrap();
        assert_eq!(r["error"]["code"], -32602);
        let r = handle(&json!({"jsonrpc":"2.0","id":"a","method":"nope"})).unwrap();
        assert_eq!(r["error"]["code"], -32601);
        assert!(handle(&json!({"jsonrpc":"2.0","method":"notifications/initialized"})).is_none());
    }
}
