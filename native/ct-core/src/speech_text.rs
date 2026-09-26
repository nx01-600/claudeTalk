//! Markdown to something a voice can read (ConvertTo-Speech in v0.5).

use fancy_regex::Regex as FRegex;
use regex::Regex;
use std::sync::OnceLock;

struct Rules {
    fences: Regex,
    inline: Regex,
    links: Regex,
    headings: Regex,
    quotes: Regex,
    bullets: Regex,
    rules: Regex,
    tables: Regex,
    emphasis: FRegex,
    spaces: Regex,
    paragraphs: Regex,
}

fn rules() -> &'static Rules {
    static R: OnceLock<Rules> = OnceLock::new();
    R.get_or_init(|| Rules {
        fences: Regex::new(r"(?s)```.*?```").unwrap(),
        inline: Regex::new(r"`([^`]+)`").unwrap(),
        links: Regex::new(r"!?\[([^\]]+)\]\([^)]+\)").unwrap(),
        headings: Regex::new(r"(?m)^\s{0,3}#{1,6}\s*").unwrap(),
        quotes: Regex::new(r"(?m)^\s*>\s?").unwrap(),
        bullets: Regex::new(r"(?m)^\s*[-*+]\s+").unwrap(),
        rules: Regex::new(r"(?m)^\s*[-*_]{3,}\s*$").unwrap(),
        tables: Regex::new(r"(?m)^\s*\|.*\|\s*$").unwrap(),
        // keeps snake_case: only markers not glued to a word on that side
        emphasis: FRegex::new(r"(?<!\w)[*_]{1,3}|[*_]{1,3}(?!\w)").unwrap(),
        spaces: Regex::new(r"[ \t]+").unwrap(),
        paragraphs: Regex::new(r"(\r?\n){2,}").unwrap(),
    })
}

/// Same rules, same order as v0.5. Like .NET, `(?m)$` only matches before
/// `\n`, so CRLF text comes out the same in both.
pub fn to_speech(text: &str, skip_code: bool) -> String {
    let r = rules();
    let mut t = if skip_code { r.fences.replace_all(text, " ").into_owned() } else { text.replace("```", "") };
    t = r.inline.replace_all(&t, "$1").into_owned();
    t = r.links.replace_all(&t, "$1").into_owned();
    t = r.headings.replace_all(&t, "").into_owned();
    t = r.quotes.replace_all(&t, "").into_owned();
    t = r.bullets.replace_all(&t, "").into_owned();
    t = r.rules.replace_all(&t, "").into_owned();
    t = r.tables.replace_all(&t, "").into_owned();
    t = r.emphasis.replace_all(&t, "").into_owned();
    t = t.replace('`', "");
    t = r.spaces.replace_all(&t, " ").into_owned();
    t = r.paragraphs.replace_all(&t, ". ").into_owned();
    t.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basics() {
        assert_eq!(to_speech("**Hola** _mundo_", true), "Hola mundo");
        assert_eq!(to_speech("usa `snake_case` aqui", true), "usa snake_case aqui");
        assert_eq!(to_speech("# Titulo\n\nTexto", true), "Titulo. Texto");
        assert_eq!(to_speech("- uno\n- dos", true), "uno\ndos");
        assert_eq!(to_speech("mira [esto](http://x) ya", true), "mira esto ya");
        assert_eq!(to_speech("a\n```rust\nfn x(){}\n```\nb", true), "a\n \nb");
        assert_eq!(to_speech("a ```x``` b", false), "a x b");
        assert_eq!(to_speech("| a | b |\n|---|---|\nfin", true), ". fin");
        assert_eq!(to_speech("> cita", true), "cita");
        assert_eq!(to_speech("x\n---\ny", true), "x\n\ny".replace("\n\n", ". "));
    }

    #[test]
    fn crlf_like_dotnet() {
        assert_eq!(to_speech("- uno\r\n- dos\r\n\r\nfin", true), "uno\r\ndos. fin");
    }
}
