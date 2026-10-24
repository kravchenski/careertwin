//! Models wrap JSON in prose and code fences. This finds the object and reads it.

use serde_json::Value;

/// The first JSON object in `text`, however it is wrapped.
pub fn extract_object(text: &str) -> Option<Value> {
    let text = text.trim();
    if let Ok(value) = serde_json::from_str::<Value>(text)
        && value.is_object()
    {
        return Some(value);
    }
    let bytes = text.as_bytes();
    let mut start = 0;
    while let Some(offset) = text[start..].find('{') {
        let begin = start + offset;
        if let Some(end) = matching_brace(bytes, begin)
            && let Ok(value) = serde_json::from_str::<Value>(&text[begin..=end])
            && value.is_object()
        {
            return Some(value);
        }
        start = begin + 1;
    }
    None
}

/// The index of the `}` that closes the `{` at `begin`, skipping braces inside strings.
fn matching_brace(bytes: &[u8], begin: usize) -> Option<usize> {
    let (mut depth, mut in_string, mut escaped) = (0usize, false, false);
    for (index, &byte) in bytes.iter().enumerate().skip(begin) {
        if in_string {
            match byte {
                _ if escaped => escaped = false,
                b'\\' => escaped = true,
                b'"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match byte {
            b'"' => in_string = true,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

pub fn text(value: &Value, key: &str) -> String {
    value.get(key).and_then(Value::as_str).unwrap_or_default().trim().to_owned()
}

/// A list of non-empty strings, at most `limit` of them; a lone string counts as a list of one.
pub fn strings(value: &Value, key: &str, limit: usize) -> Vec<String> {
    match value.get(key) {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|item| item.as_str().map(|s| s.trim().to_owned()))
            .filter(|s| !s.is_empty())
            .take(limit)
            .collect(),
        Some(Value::String(one)) if !one.trim().is_empty() => vec![one.trim().to_owned()],
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_json() {
        assert_eq!(extract_object(r#"{"a": 1}"#).unwrap()["a"], 1);
    }

    #[test]
    fn fenced_json_with_prose_around_it() {
        let text = "Sure! Here you go:\n```json\n{\"a\": {\"b\": [1, 2]}}\n```\nHope that helps.";
        assert_eq!(extract_object(text).unwrap()["a"]["b"][1], 2);
    }

    #[test]
    fn braces_inside_strings_do_not_confuse_it() {
        let text = r#"note {x} then {"code": "fn main() { println!(\"}\"); }", "n": 2} done"#;
        let value = extract_object(text).unwrap();
        assert_eq!(value["n"], 2);
        assert!(value["code"].as_str().unwrap().contains("println"));
    }

    #[test]
    fn the_first_valid_object_wins_over_an_earlier_broken_one() {
        let value = extract_object("{broken: yes} {\"ok\": true}").unwrap();
        assert_eq!(value["ok"], true);
    }

    #[test]
    fn nothing_usable_gives_none() {
        assert!(extract_object("no json here").is_none());
        assert!(extract_object("[1, 2, 3]").is_none());
        assert!(extract_object("{unterminated").is_none());
        assert!(extract_object("").is_none());
    }

    #[test]
    fn unbalanced_closing_braces_do_not_panic() {
        assert!(extract_object("} } { \"a\": 1 }").is_some());
        assert!(extract_object("}}}}").is_none());
    }

    #[test]
    fn text_and_strings_are_forgiving() {
        let value: Value =
            serde_json::from_str(r#"{"a": " hi ", "b": ["x", "", 5, " y "], "c": "solo", "d": 3}"#).unwrap();
        assert_eq!(text(&value, "a"), "hi");
        assert_eq!(text(&value, "missing"), "");
        assert_eq!(text(&value, "d"), "");
        assert_eq!(strings(&value, "b", 10), vec!["x", "y"]);
        assert_eq!(strings(&value, "b", 1), vec!["x"]);
        assert_eq!(strings(&value, "c", 10), vec!["solo"]);
        assert!(strings(&value, "d", 10).is_empty());
    }
}
