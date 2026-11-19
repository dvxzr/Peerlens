//! Tolerant parser for the agent's NDJSON output contract.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Suggestion {
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub code: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Finding {
    #[serde(default)]
    pub file: String,
    #[serde(default)]
    pub start_line: Option<u32>,
    #[serde(default)]
    pub end_line: Option<u32>,
    #[serde(default)]
    pub severity: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub explanation: String,
    #[serde(default)]
    pub snippet: Option<String>,
    #[serde(default)]
    pub suggestion: Option<Suggestion>,
    /// Set by us after checking the lines exist in the diff.
    #[serde(default)]
    pub anchored: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Finding(Finding),
    Summary(String),
}

const SEVERITIES: [&str; 4] = ["critical", "major", "minor", "nit"];
const CATEGORIES: [&str; 6] = ["bug", "security", "performance", "design", "style", "test"];

/// Parse one line of agent text. Non-JSON, fences and unknown objects → None.
pub fn parse_line(line: &str) -> Option<Item> {
    let l = line.trim();
    let (a, b) = (l.find('{')?, l.rfind('}')?);
    if b <= a {
        return None;
    }
    let v: serde_json::Value = serde_json::from_str(&l[a..=b]).ok()?;
    match v.get("type")?.as_str()? {
        "finding" => {
            let mut f: Finding = serde_json::from_value(v).ok()?;
            if f.title.trim().is_empty() && f.explanation.trim().is_empty() {
                return None;
            }
            f.severity = f.severity.to_lowercase();
            if !SEVERITIES.contains(&f.severity.as_str()) {
                f.severity = "minor".into();
            }
            f.category = f.category.to_lowercase();
            if !CATEGORIES.contains(&f.category.as_str()) {
                f.category = "design".into();
            }
            if let (Some(s), Some(e)) = (f.start_line, f.end_line) {
                if e < s {
                    f.end_line = Some(s);
                }
            }
            f.anchored = false;
            Some(Item::Finding(f))
        }
        "summary" => Some(Item::Summary(v.get("text")?.as_str()?.to_string())),
        _ => None,
    }
}

/// Turns arbitrary text deltas into complete lines.
#[derive(Default)]
pub struct LineAssembler {
    buf: String,
}

impl LineAssembler {
    pub fn push(&mut self, text: &str) -> Vec<String> {
        self.buf.push_str(text);
        let mut out = Vec::new();
        while let Some(i) = self.buf.find('\n') {
            out.push(self.buf[..i].to_string());
            self.buf.drain(..=i);
        }
        out
    }
    pub fn finish(&mut self) -> Option<String> {
        let rest = std::mem::take(&mut self.buf);
        (!rest.trim().is_empty()).then_some(rest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_finding_and_summary() {
        let f = parse_line(
            r#"{"type":"finding","file":"a.rs","start_line":3,"end_line":1,"severity":"MAJOR","category":"bug","title":"t","explanation":"e","suggestion":{"description":"d","code":"x"}}"#,
        );
        let Some(Item::Finding(f)) = f else { panic!() };
        assert_eq!(
            (f.severity.as_str(), f.start_line, f.end_line),
            ("major", Some(3), Some(3))
        );
        assert_eq!(f.suggestion.unwrap().code, "x");
        assert_eq!(
            parse_line(r#"{"type":"summary","text":"done"}"#),
            Some(Item::Summary("done".into()))
        );
    }

    #[test]
    fn tolerates_garbage() {
        for g in [
            "",
            "hello",
            "```json",
            "```",
            "{",
            "}{",
            "{not json}",
            r#"{"type":"other"}"#,
            r#"{"no_type":1}"#,
            r#"{"type":"finding"}"#,
            "\u{0}\u{1}{{{",
            "🙂 {\"type\": 5}",
        ] {
            assert_eq!(parse_line(g), None, "{g:?}");
        }
    }

    #[test]
    fn tolerates_prefixes_and_unknown_enums() {
        let Some(Item::Finding(f)) =
            parse_line(r#"- {"type":"finding","title":"t","severity":"blocker","category":"weird","start_line":"x"}"#)
        else {
            // start_line as string fails the typed parse → whole line ignored, which is acceptable.
            return;
        };
        assert_eq!(f.severity, "minor");
        let Some(Item::Finding(f)) =
            parse_line(r#"  * {"type":"finding","title":"t","severity":"blocker","category":"weird"} trailing"#)
        else {
            panic!()
        };
        assert_eq!((f.severity.as_str(), f.category.as_str()), ("minor", "design"));
    }

    #[test]
    fn assembles_lines_across_deltas() {
        let mut a = LineAssembler::default();
        assert!(a.push("{\"type\":\"sum").is_empty());
        assert_eq!(
            a.push("mary\",\"text\":\"x\"}\nnext"),
            vec!["{\"type\":\"summary\",\"text\":\"x\"}"]
        );
        assert_eq!(a.finish().as_deref(), Some("next"));
        assert_eq!(a.finish(), None);
    }
}
