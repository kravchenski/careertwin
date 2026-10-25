//! Colouring code in the editor. It is a plain tokenizer, not a parser: enough to tell keywords,
//! strings, numbers and comments apart in the languages of the tasks, and always exact about the
//! text, which the editor needs (the coloured text must be the text that is edited).

use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, FontId};

use super::theme::{self, BLUSH, FAINT, GOOD, SNOW};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Word,
    Keyword,
    String,
    Number,
    Comment,
    Other,
}

fn keywords(language: &str) -> &'static [&'static str] {
    match language {
        "python" => &[
            "def", "class", "return", "if", "elif", "else", "for", "while", "in", "not", "and", "or", "import", "from",
            "as", "with", "try", "except", "finally", "raise", "lambda", "yield", "pass", "break", "continue", "None",
            "True", "False", "is", "async", "await", "self",
        ],
        "javascript" | "typescript" => &[
            "function",
            "const",
            "let",
            "var",
            "return",
            "if",
            "else",
            "for",
            "while",
            "of",
            "in",
            "new",
            "class",
            "extends",
            "import",
            "from",
            "export",
            "default",
            "async",
            "await",
            "try",
            "catch",
            "finally",
            "throw",
            "null",
            "undefined",
            "true",
            "false",
            "this",
            "typeof",
            "interface",
            "type",
            "enum",
            "switch",
            "case",
            "break",
            "continue",
        ],
        "java" | "csharp" => &[
            "public",
            "private",
            "protected",
            "static",
            "final",
            "class",
            "interface",
            "void",
            "int",
            "long",
            "double",
            "boolean",
            "bool",
            "string",
            "String",
            "return",
            "if",
            "else",
            "for",
            "foreach",
            "while",
            "new",
            "import",
            "using",
            "namespace",
            "try",
            "catch",
            "finally",
            "throw",
            "throws",
            "null",
            "true",
            "false",
            "this",
            "extends",
            "implements",
            "var",
            "switch",
            "case",
            "break",
            "continue",
            "in",
        ],
        "go" => &[
            "func",
            "package",
            "import",
            "return",
            "if",
            "else",
            "for",
            "range",
            "var",
            "const",
            "type",
            "struct",
            "interface",
            "map",
            "chan",
            "go",
            "defer",
            "switch",
            "case",
            "break",
            "continue",
            "nil",
            "true",
            "false",
        ],
        "rust" => &[
            "fn", "let", "mut", "pub", "struct", "enum", "impl", "trait", "use", "mod", "return", "if", "else", "for",
            "while", "loop", "match", "in", "as", "ref", "self", "Self", "true", "false", "const", "static", "where",
            "async", "await", "move", "break", "continue",
        ],
        "cpp" => &[
            "int",
            "long",
            "double",
            "float",
            "char",
            "bool",
            "void",
            "auto",
            "const",
            "static",
            "class",
            "struct",
            "public",
            "private",
            "return",
            "if",
            "else",
            "for",
            "while",
            "new",
            "delete",
            "namespace",
            "using",
            "template",
            "typename",
            "include",
            "nullptr",
            "true",
            "false",
            "this",
            "switch",
            "case",
            "break",
            "continue",
            "std",
        ],
        "sql" => &[
            "select", "from", "where", "group", "by", "order", "having", "join", "left", "right", "inner", "outer",
            "on", "as", "and", "or", "not", "in", "is", "null", "insert", "into", "values", "update", "set", "delete",
            "create", "table", "index", "limit", "distinct", "union", "case", "when", "then", "else", "end", "count",
            "sum", "avg", "min", "max", "SELECT", "FROM", "WHERE", "GROUP", "BY", "ORDER", "HAVING", "JOIN", "LEFT",
            "RIGHT", "INNER", "ON", "AS", "AND", "OR", "NOT", "IN", "IS", "NULL", "LIMIT", "DISTINCT",
        ],
        _ => &[],
    }
}

fn line_comment(language: &str) -> &'static str {
    match language {
        "python" => "#",
        "sql" => "--",
        _ => "//",
    }
}

/// Cuts `text` into pieces that, put together, are exactly `text`.
pub fn tokens<'a>(text: &'a str, language: &str) -> Vec<(Kind, &'a str)> {
    let keywords = keywords(language);
    let comment = line_comment(language);
    let block_comments = !matches!(language, "python" | "sql");
    let mut out: Vec<(Kind, &str)> = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        let first = rest.chars().next().unwrap_or(' ');
        let (kind, length) = if rest.starts_with(comment) {
            (Kind::Comment, rest.find('\n').unwrap_or(rest.len()))
        } else if block_comments && rest.starts_with("/*") {
            (Kind::Comment, rest.find("*/").map_or(rest.len(), |end| end + 2))
        } else if matches!(first, '"' | '\'' | '`') {
            let mut end = first.len_utf8();
            let mut escaped = false;
            for (offset, ch) in rest.char_indices().skip(1) {
                end = offset + ch.len_utf8();
                if escaped {
                    escaped = false;
                } else if ch == '\\' {
                    escaped = true;
                } else if ch == first || (ch == '\n' && first != '`') {
                    break;
                }
            }
            (Kind::String, end)
        } else if first.is_ascii_digit() {
            let end = rest.find(|c: char| !(c.is_ascii_alphanumeric() || c == '.' || c == '_')).unwrap_or(rest.len());
            (Kind::Number, end)
        } else if first.is_alphabetic() || first == '_' {
            let end = rest.find(|c: char| !(c.is_alphanumeric() || c == '_')).unwrap_or(rest.len());
            let kind = if keywords.contains(&&rest[..end]) { Kind::Keyword } else { Kind::Word };
            (kind, end)
        } else {
            (Kind::Other, first.len_utf8())
        };
        out.push((kind, &rest[..length]));
        rest = &rest[length..];
    }
    // neighbours of one kind are one piece
    let mut merged: Vec<(Kind, &str)> = Vec::new();
    let mut start = 0;
    let mut offset = 0;
    for (index, (kind, piece)) in out.iter().enumerate() {
        let next_same = out.get(index + 1).is_some_and(|(k, _)| k == kind && matches!(kind, Kind::Other));
        offset += piece.len();
        if !next_same {
            merged.push((*kind, &text[start..offset]));
            start = offset;
        }
    }
    merged
}

fn color(kind: Kind) -> Color32 {
    match kind {
        Kind::Keyword => theme::mix(theme::BERRY, BLUSH, 0.35),
        Kind::String => GOOD,
        Kind::Number => BLUSH,
        Kind::Comment => FAINT,
        Kind::Word | Kind::Other => SNOW,
    }
}

/// The text of the editor as coloured pieces, wrapped at `wrap`.
pub fn highlight(text: &str, language: &str, font: &FontId, wrap: f32) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.wrap.max_width = wrap;
    for (kind, piece) in tokens(text, language) {
        job.append(piece, 0.0, TextFormat { font_id: font.clone(), color: color(kind), ..Default::default() });
    }
    job
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(text: &str, language: &str) -> Vec<(Kind, String)> {
        tokens(text, language).into_iter().map(|(k, t)| (k, t.to_owned())).collect()
    }

    #[test]
    fn the_pieces_always_add_up_to_the_text() {
        let samples = [
            ("python", "def f(x):\n    # note\n    return \"a\\\"b\" + 'c' + str(12.5)\n"),
            ("javascript", "const a = `x ${y}`; /* c */ // d\nlet é = 1;"),
            ("sql", "SELECT name FROM t -- all\nWHERE x = 'it''s';"),
            ("rust", "fn main() { let s = \"unterminated"),
            ("go", ""),
            ("cpp", "int main() { /* never closed"),
            ("python", "naïve = 'ünï' # комментарий"),
        ];
        for (language, text) in samples {
            let joined: String = tokens(text, language).into_iter().map(|(_, piece)| piece).collect();
            assert_eq!(joined, text, "{language}");
        }
    }

    #[test]
    fn keywords_strings_numbers_and_comments_are_told_apart() {
        let python = kinds("def f(x): return 'a' # hi\nn = 42", "python");
        assert!(python.contains(&(Kind::Keyword, "def".into())));
        assert!(python.contains(&(Kind::Keyword, "return".into())));
        assert!(python.contains(&(Kind::String, "'a'".into())));
        assert!(python.contains(&(Kind::Comment, "# hi".into())));
        assert!(python.contains(&(Kind::Number, "42".into())));
        assert!(python.contains(&(Kind::Word, "f".into())));
    }

    #[test]
    fn comment_styles_follow_the_language() {
        assert!(kinds("a // b", "python").iter().all(|(k, _)| *k != Kind::Comment), "// is not a comment in Python");
        assert!(kinds("a # b", "javascript").iter().all(|(k, _)| *k != Kind::Comment));
        assert!(kinds("a -- b", "sql").contains(&(Kind::Comment, "-- b".into())));
        assert!(kinds("a /* b */ c", "rust").contains(&(Kind::Comment, "/* b */".into())));
        assert!(kinds("a /* b */ c", "python").iter().all(|(k, _)| *k != Kind::Comment));
    }

    #[test]
    fn a_string_stops_at_the_end_of_its_line_unless_it_is_a_template() {
        let broken = kinds("x = 'abc\ny = 2", "python");
        assert!(broken.contains(&(Kind::Number, "2".into())), "{broken:?}");
        let template = kinds("`a\nb` c", "javascript");
        assert_eq!(template[0], (Kind::String, "`a\nb`".into()));
    }

    #[test]
    fn an_escaped_quote_does_not_end_a_string() {
        let pieces = kinds(r#"s = "a\"b" + t"#, "python");
        assert!(pieces.contains(&(Kind::String, r#""a\"b""#.into())), "{pieces:?}");
    }

    #[test]
    fn the_job_has_the_same_text_and_a_colour_per_kind() {
        let font = theme::mono(15.0);
        let job = highlight("def x(): return 1", "python", &font, 400.0);
        assert_eq!(job.text, "def x(): return 1");
        let piece = |s: &egui::text::LayoutSection| job.text[s.byte_range.start.0..s.byte_range.end.0].to_owned();
        let keyword = job.sections.iter().find(|s| piece(s) == "def").unwrap();
        let word = job.sections.iter().find(|s| piece(s).contains('x')).unwrap();
        assert_ne!(keyword.format.color, word.format.color);
        assert_eq!(highlight("", "rust", &font, 100.0).text, "");
    }
}
