//! Reads the turn that just ended from a Claude Code transcript (JSONL), the
//! way the Stop hook needs it (native/SPEC.md §5).

use serde_json::Value;
use std::path::Path;

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Turn {
    /// Claude called `say` at least once.
    pub spoke: bool,
    /// Claude used other tools after its last `say`.
    pub work_after_say: bool,
    /// Text written after the last `say` or tool call.
    pub text_after: String,
    /// The prompt that started the turn was dictated.
    pub spoken: bool,
}

pub fn is_say(block: &Value) -> bool {
    block.get("type").and_then(Value::as_str) == Some("tool_use")
        && block
            .get("name")
            .and_then(Value::as_str)
            .map(str::to_ascii_lowercase)
            .is_some_and(|n| n.starts_with("mcp__plugin_claudetalk") && n.ends_with("__say"))
}

fn flag(o: &Value, key: &str) -> bool {
    o.get(key).is_some_and(crate::settings::truthy)
}

/// A prompt the user typed (start of a turn), not a tool result, an injected
/// skill body or subagent traffic.
pub fn is_prompt(o: &Value) -> bool {
    if o.get("type").and_then(Value::as_str) != Some("user") || flag(o, "isMeta") || flag(o, "isSidechain") {
        return false;
    }
    match o.get("message").and_then(|m| m.get("content")) {
        Some(Value::Array(blocks)) => {
            !blocks.iter().any(|b| b.get("type").and_then(Value::as_str) == Some("tool_result"))
        }
        _ => true,
    }
}

fn prompt_text(content: Option<&Value>) -> String {
    match content {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(blocks)) => blocks
            .iter()
            .filter(|b| b.get("type").and_then(Value::as_str) == Some("text"))
            .filter_map(|b| b.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

pub fn read_turn(transcript: &Path) -> Turn {
    let raw = std::fs::read(transcript).unwrap_or_default();
    read_turn_str(&String::from_utf8_lossy(&raw))
}

pub fn read_turn_str(raw: &str) -> Turn {
    let mut turn: Vec<Value> = Vec::new();
    let mut prompt: Option<Value> = None;
    for line in raw.lines().rev() {
        if line.trim().is_empty() {
            continue;
        }
        let Ok(obj) = serde_json::from_str::<Value>(line) else { continue };
        if is_prompt(&obj) {
            prompt = obj.get("message").and_then(|m| m.get("content")).cloned();
            break;
        }
        if flag(&obj, "isSidechain") {
            continue;
        }
        if obj.get("message").and_then(|m| m.get("role")).and_then(Value::as_str) == Some("assistant") {
            turn.push(obj);
        }
    }
    turn.reverse();
    let mut info = Turn { spoken: crate::is_spoken(&prompt_text(prompt.as_ref())), ..Default::default() };
    for obj in &turn {
        let blocks: Vec<&Value> = match obj.get("message").and_then(|m| m.get("content")) {
            Some(Value::Array(b)) => b.iter().collect(),
            Some(other) => vec![other],
            None => continue,
        };
        for block in blocks {
            let kind = block.get("type").and_then(Value::as_str);
            if is_say(block) {
                info.spoke = true;
                info.work_after_say = false;
                info.text_after.clear();
            } else if kind == Some("tool_use") {
                // Notes between tool calls are progress chatter, not the
                // answer: only the text after the last tool gets read.
                info.work_after_say = true;
                info.text_after.clear();
            } else if kind == Some("text") {
                if let Some(t) = block.get("text").and_then(Value::as_str).filter(|t| !t.is_empty()) {
                    info.text_after.push_str(t);
                    info.text_after.push('\n');
                }
            }
        }
    }
    info
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn line(v: Value) -> String {
        v.to_string() + "\n"
    }
    fn user(text: &str) -> String {
        line(json!({"type":"user","message":{"role":"user","content":text}}))
    }
    fn asst(blocks: Value) -> String {
        line(json!({"type":"assistant","message":{"role":"assistant","content":blocks}}))
    }
    fn say() -> Value {
        json!({"type":"tool_use","name":"mcp__plugin_claudeTalk_voice__say","input":{"text":"hola"}})
    }

    #[test]
    fn short_answer() {
        let t = user("viejo") + &asst(json!([{"type":"text","text":"no"}])) + &user("hola") + &asst(json!([{"type":"text","text":"Hola!"}]));
        let r = read_turn_str(&t);
        assert_eq!(r, Turn { text_after: "Hola!\n".into(), ..Default::default() });
    }

    #[test]
    fn say_then_work_then_text() {
        let tool_result = line(json!({"type":"user","message":{"role":"user","content":[{"type":"tool_result","content":"x"}]}}));
        let t = user("\u{1F399}\u{FE0F} haz algo")
            + &asst(json!([say()]))
            + &asst(json!([{"type":"text","text":"nota"},{"type":"tool_use","name":"Bash"}]))
            + &tool_result
            + &asst(json!([{"type":"text","text":"Listo"}]));
        let r = read_turn_str(&t);
        assert!(r.spoke && r.work_after_say && r.spoken);
        assert_eq!(r.text_after, "Listo\n");
    }

    #[test]
    fn say_last_clears_text() {
        let t = user("hola") + &asst(json!([{"type":"text","text":"antes"}, say()]));
        let r = read_turn_str(&t);
        assert!(r.spoke && !r.work_after_say);
        assert_eq!(r.text_after, "");
    }

    #[test]
    fn meta_and_sidechain_are_not_prompts() {
        let t = user("hola")
            + &line(json!({"type":"user","isMeta":true,"message":{"role":"user","content":"skill"}}))
            + &line(json!({"type":"assistant","isSidechain":true,"message":{"role":"assistant","content":[{"type":"text","text":"sub"}]}}))
            + &asst(json!([{"type":"text","text":"fin"}]));
        assert_eq!(read_turn_str(&t).text_after, "fin\n");
    }
}
