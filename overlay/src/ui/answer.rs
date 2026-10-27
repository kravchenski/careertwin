//! Showing an answer: the lines to say with their **key terms**, the paragraph, code blocks and details.

use egui::text::{LayoutJob, TextFormat};
use egui::{Frame, Margin, RichText, ScrollArea, Ui};

use super::theme::{ACCENT, CODE, LINE, MUTED, STRONG, TEXT, mono, semibold, sans};
use crate::prompts::Sections;

/// One line with `**bold**` and `` `code` `` in it.
pub fn rich_line(ui: &mut Ui, text: &str, size: f32, color: egui::Color32) {
    let mut job = LayoutJob::default();
    job.wrap.max_width = ui.available_width();
    let plain = TextFormat { font_id: sans(size), color, ..TextFormat::default() };
    let bold = TextFormat { font_id: semibold(size), color: STRONG, ..TextFormat::default() };
    let code = TextFormat { font_id: mono(size * 0.94), color: ACCENT, ..TextFormat::default() };
    for (kind, piece) in tokens(text) {
        let format = match kind {
            Token::Plain => &plain,
            Token::Bold => &bold,
            Token::Code => &code,
        };
        job.append(piece, 0.0, format.clone());
    }
    ui.label(job);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Token {
    Plain,
    Bold,
    Code,
}

/// Split a line at `**` and backticks. A marker that is never closed stays as text, so half a streamed line
/// does not lose its asterisks.
pub fn tokens(text: &str) -> Vec<(Token, &str)> {
    let mut out = Vec::new();
    let mut plain_start = 0usize;
    let mut position = 0usize;
    while position < text.len() {
        let here = &text[position..];
        let marker = if here.starts_with("**") { Some(("**", Token::Bold)) } else if here.starts_with('`') { Some(("`", Token::Code)) } else { None };
        if let Some((mark, kind)) = marker {
            let inner = position + mark.len();
            if let Some(close) = text[inner..].find(mark).filter(|close| *close > 0) {
                if plain_start < position {
                    out.push((Token::Plain, &text[plain_start..position]));
                }
                out.push((kind, &text[inner..inner + close]));
                position = inner + close + mark.len();
                plain_start = position;
                continue;
            }
        }
        position += here.chars().next().map_or(1, char::len_utf8);
    }
    if plain_start < text.len() {
        out.push((Token::Plain, &text[plain_start..]));
    }
    out
}

/// The answer text with its fenced code blocks drawn as code.
pub fn answer_text(ui: &mut Ui, text: &str, size: f32) {
    for piece in split_fences(text) {
        match piece {
            Piece::Prose(prose) => {
                for paragraph in prose.split("\n\n").map(str::trim).filter(|paragraph| !paragraph.is_empty()) {
                    rich_line(ui, paragraph, size, TEXT);
                }
            }
            Piece::Code { language, body } => code_block(ui, &language, &body, size),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Piece {
    Prose(String),
    Code { language: String, body: String },
}

/// Prose and fenced blocks in order; a block still being written ends at the end of the text.
pub fn split_fences(text: &str) -> Vec<Piece> {
    let mut pieces = Vec::new();
    let mut prose = String::new();
    let mut code: Option<(String, Vec<&str>)> = None;
    for line in text.lines() {
        if let Some((language, body)) = code.as_mut() {
            if line.trim_start().starts_with("```") {
                pieces.push(Piece::Code { language: std::mem::take(language), body: body.join("\n") });
                code = None;
            } else {
                body.push(line);
            }
        } else if let Some(fence) = line.trim_start().strip_prefix("```") {
            if !prose.trim().is_empty() {
                pieces.push(Piece::Prose(std::mem::take(&mut prose)));
            }
            prose.clear();
            code = Some((fence.trim().to_owned(), Vec::new()));
        } else {
            prose.push_str(line);
            prose.push('\n');
        }
    }
    if let Some((language, body)) = code {
        pieces.push(Piece::Code { language, body: body.join("\n") });
    } else if !prose.trim().is_empty() {
        pieces.push(Piece::Prose(prose));
    }
    pieces
}

pub fn code_block(ui: &mut Ui, language: &str, body: &str, size: f32) {
    Frame::new().fill(CODE).stroke(egui::Stroke::new(1.0, LINE)).corner_radius(8).inner_margin(Margin::symmetric(12, 10)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        if !language.is_empty() {
            ui.label(RichText::new(language).font(sans(size * 0.8)).color(MUTED));
        }
        ScrollArea::horizontal().id_salt(("code", body.len(), language)).show(ui, |ui| {
            ui.label(RichText::new(body).font(mono(size * 0.94)).color(TEXT));
        });
    });
}

/// The whole reply: lines to say (when the layout has them), the answer, and the details.
pub fn show(ui: &mut Ui, sections: &Sections, main_only: bool, details_open: &mut bool, size: f32, t: &dyn Fn(&str) -> String) {
    if !main_only && !sections.points.is_empty() {
        for (number, point) in sections.points.iter().enumerate() {
            ui.horizontal_top(|ui| {
                ui.label(RichText::new(format!("{}", number + 1)).font(semibold(size * 0.9)).color(ACCENT));
                ui.vertical(|ui| rich_line(ui, point, size + 1.0, TEXT));
            });
        }
        ui.add_space(6.0);
    }
    let details = sections.has_details();
    if main_only || *details_open || sections.points.is_empty() {
        if !sections.answer.is_empty() {
            answer_text(ui, &sections.answer, size);
        }
        if !main_only && *details_open {
            heading_and_lines(ui, &t("key_terms"), &sections.key, size);
            if !sections.context.is_empty() {
                ui.add_space(6.0);
                ui.label(RichText::new(t("context")).font(semibold(size * 0.9)).color(MUTED));
                rich_line(ui, &sections.context, size, TEXT);
            }
            heading_and_lines(ui, &t("next_questions"), &sections.follow_ups, size);
        }
    }
    if !main_only && details && !sections.points.is_empty() {
        let label = t(if *details_open { "hide_details" } else { "details" });
        if ui.add(egui::Button::new(RichText::new(label).color(ACCENT)).frame(false)).clicked() {
            *details_open = !*details_open;
        }
    }
}

fn heading_and_lines(ui: &mut Ui, heading: &str, lines: &[String], size: f32) {
    if lines.is_empty() {
        return;
    }
    ui.add_space(6.0);
    ui.label(RichText::new(heading).font(semibold(size * 0.9)).color(MUTED));
    for line in lines {
        rich_line(ui, line, size, TEXT);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bold_and_code_are_found_and_the_text_between_stays() {
        assert_eq!(tokens("I build **APIs** with `FastAPI` daily"), [(Token::Plain, "I build "), (Token::Bold, "APIs"), (Token::Plain, " with "), (Token::Code, "FastAPI"), (Token::Plain, " daily")]);
        assert_eq!(tokens("plain only"), [(Token::Plain, "plain only")]);
        assert_eq!(tokens("**a****b**"), [(Token::Bold, "a"), (Token::Bold, "b")]);
    }

    #[test]
    fn a_marker_that_is_never_closed_stays_text() {
        assert_eq!(tokens("I use **Dock"), [(Token::Plain, "I use **Dock")]);
        assert_eq!(tokens("a ` b"), [(Token::Plain, "a ` b")]);
        assert_eq!(tokens("emoji ✓ **ok** é"), [(Token::Plain, "emoji ✓ "), (Token::Bold, "ok"), (Token::Plain, " é")]);
    }

    #[test]
    fn prose_and_fenced_code_are_told_apart_and_a_streaming_block_is_kept() {
        let pieces = split_fences("Use a heap.\n\n```python\ndef f(x):\n    return x\n```\nIt is O(n).");
        assert_eq!(pieces.len(), 3);
        assert_eq!(pieces[1], Piece::Code { language: "python".into(), body: "def f(x):\n    return x".into() });
        assert!(matches!(&pieces[2], Piece::Prose(text) if text.trim() == "It is O(n)."));
        assert_eq!(split_fences("```sql\nselect 1"), [Piece::Code { language: "sql".into(), body: "select 1".into() }]);
        assert!(split_fences("   \n").is_empty());
    }
}
