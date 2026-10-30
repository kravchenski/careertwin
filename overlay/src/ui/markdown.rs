//! Markdown in an answer: headings, emphasis, lists, quotes, tables, links, rules and code blocks.
//!
//! The model writes Markdown whether or not it was asked to, and an answer is shown while it is still being written,
//! so a half-finished construct (an unclosed `**`, an open fence) has to look like what it will become or like plain
//! text, never like an error. `pulldown-cmark` does the parsing; this draws what it finds in the window's own style.

use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, Frame, Margin, RichText, Stroke, Ui};
use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};

use super::answer::code_block;
use super::theme::{accent, mono, muted, sans, semibold, strong, text};

/// A run of text with one style.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Span {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub strike: bool,
    pub code: bool,
    /// Where the text leads, when it is a link.
    pub link: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Block {
    Paragraph(Vec<Span>),
    Heading(u8, Vec<Span>),
    Code { language: String, body: String },
    Quote(Vec<Block>),
    List { first: Option<u64>, items: Vec<Item> },
    Rule,
    Table { header: Vec<Vec<Span>>, rows: Vec<Vec<Vec<Span>>> },
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Item {
    /// `Some` for a task list item: whether it is ticked.
    pub checked: Option<bool>,
    pub blocks: Vec<Block>,
}

#[derive(Clone, Copy, Default)]
struct Style {
    bold: bool,
    italic: bool,
    strike: bool,
}

/// The blocks of a Markdown text.
pub fn parse(source: &str) -> Vec<Block> {
    let options = Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TABLES | Options::ENABLE_TASKLISTS;
    let mut events = Parser::new_ext(source, options);
    blocks(&mut events, None)
}

fn level(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

/// The blocks up to the end of `end` (or of the text). Inline events with no paragraph around them, as in a tight list
/// item, are gathered into a paragraph of their own.
fn blocks(events: &mut Parser<'_>, end: Option<TagEnd>) -> Vec<Block> {
    blocks_and_task(events, end, &mut None)
}

/// As [`blocks`], and `task` is set when the first thing in them is a task list marker (`[x]` or `[ ]`).
fn blocks_and_task(events: &mut Parser<'_>, end: Option<TagEnd>, task: &mut Option<bool>) -> Vec<Block> {
    let mut out: Vec<Block> = Vec::new();
    let mut spans: Vec<Span> = Vec::new();
    let mut style = Style::default();
    let mut link: Option<String> = None;
    fn flush(out: &mut Vec<Block>, spans: &mut Vec<Span>) {
        if spans.iter().any(|span| !span.text.trim().is_empty()) {
            out.push(Block::Paragraph(std::mem::take(spans)));
        } else {
            spans.clear();
        }
    }
    while let Some(event) = events.next() {
        match event {
            Event::End(tag) if Some(tag) == end => break,
            Event::TaskListMarker(done) => *task = Some(done),
            Event::End(TagEnd::Paragraph) => flush(&mut out, &mut spans),
            Event::Start(Tag::Paragraph) => {}
            Event::Start(Tag::Heading { level: heading, .. }) => {
                flush(&mut out, &mut spans);
                let inner = inlines(events, TagEnd::Heading(heading));
                out.push(Block::Heading(level(heading), inner));
            }
            Event::Start(Tag::BlockQuote(_)) => {
                flush(&mut out, &mut spans);
                out.push(Block::Quote(blocks(events, Some(TagEnd::BlockQuote(None)))));
            }
            Event::Start(Tag::CodeBlock(kind)) => {
                flush(&mut out, &mut spans);
                let language = match kind {
                    pulldown_cmark::CodeBlockKind::Fenced(name) => {
                        name.split_whitespace().next().unwrap_or("").to_owned()
                    }
                    pulldown_cmark::CodeBlockKind::Indented => String::new(),
                };
                let mut body = String::new();
                for inner in events.by_ref() {
                    match inner {
                        Event::Text(piece) => body.push_str(&piece),
                        Event::End(TagEnd::CodeBlock) => break,
                        _ => {}
                    }
                }
                out.push(Block::Code { language, body: body.trim_end_matches('\n').to_owned() });
            }
            Event::Start(Tag::List(first)) => {
                flush(&mut out, &mut spans);
                out.push(list(events, first));
            }
            Event::Start(Tag::Table(_)) => {
                flush(&mut out, &mut spans);
                out.push(table(events));
            }
            Event::Rule => {
                flush(&mut out, &mut spans);
                out.push(Block::Rule);
            }
            // inline events with no paragraph around them
            Event::Start(Tag::Strong) => style.bold = true,
            Event::End(TagEnd::Strong) => style.bold = false,
            Event::Start(Tag::Emphasis) => style.italic = true,
            Event::End(TagEnd::Emphasis) => style.italic = false,
            Event::Start(Tag::Strikethrough) => style.strike = true,
            Event::End(TagEnd::Strikethrough) => style.strike = false,
            Event::Start(Tag::Link { dest_url, .. }) => link = Some(dest_url.to_string()),
            Event::End(TagEnd::Link) => link = None,
            Event::Text(piece) | Event::Html(piece) | Event::InlineHtml(piece) => {
                spans.push(Span {
                    text: piece.to_string(),
                    bold: style.bold,
                    italic: style.italic,
                    strike: style.strike,
                    code: false,
                    link: link.clone(),
                });
            }
            Event::Code(piece) => spans.push(Span {
                text: piece.to_string(),
                code: true,
                bold: style.bold,
                link: link.clone(),
                ..Span::default()
            }),
            Event::SoftBreak => spans.push(Span { text: " ".to_owned(), ..Span::default() }),
            Event::HardBreak => spans.push(Span { text: "\n".to_owned(), ..Span::default() }),
            _ => {}
        }
    }
    flush(&mut out, &mut spans);
    out
}

/// The styled runs up to the end of `end`.
fn inlines(events: &mut Parser<'_>, end: TagEnd) -> Vec<Span> {
    let mut spans: Vec<Span> = Vec::new();
    let mut style = Style::default();
    let mut link: Option<String> = None;
    for event in events.by_ref() {
        match event {
            Event::End(tag) if tag == end => break,
            Event::Start(Tag::Strong) => style.bold = true,
            Event::End(TagEnd::Strong) => style.bold = false,
            Event::Start(Tag::Emphasis) => style.italic = true,
            Event::End(TagEnd::Emphasis) => style.italic = false,
            Event::Start(Tag::Strikethrough) => style.strike = true,
            Event::End(TagEnd::Strikethrough) => style.strike = false,
            Event::Start(Tag::Link { dest_url, .. }) => link = Some(dest_url.to_string()),
            Event::End(TagEnd::Link) => link = None,
            Event::Text(piece) | Event::Html(piece) | Event::InlineHtml(piece) => {
                spans.push(Span {
                    text: piece.to_string(),
                    bold: style.bold,
                    italic: style.italic,
                    strike: style.strike,
                    code: false,
                    link: link.clone(),
                });
            }
            Event::Code(piece) => spans.push(Span {
                text: piece.to_string(),
                code: true,
                bold: style.bold,
                link: link.clone(),
                ..Span::default()
            }),
            Event::SoftBreak => spans.push(Span { text: " ".to_owned(), ..Span::default() }),
            Event::HardBreak => spans.push(Span { text: "\n".to_owned(), ..Span::default() }),
            _ => {}
        }
    }
    spans
}

fn list(events: &mut Parser<'_>, first: Option<u64>) -> Block {
    let mut items: Vec<Item> = Vec::new();
    while let Some(event) = events.next() {
        match event {
            Event::End(TagEnd::List(_)) => break,
            Event::Start(Tag::Item) => {
                let mut checked = None;
                let inner = blocks_and_task(events, Some(TagEnd::Item), &mut checked);
                items.push(Item { checked, blocks: inner });
            }
            _ => {}
        }
    }
    Block::List { first, items }
}

fn table(events: &mut Parser<'_>) -> Block {
    let mut header: Vec<Vec<Span>> = Vec::new();
    let mut rows: Vec<Vec<Vec<Span>>> = Vec::new();
    let mut row: Vec<Vec<Span>> = Vec::new();
    while let Some(event) = events.next() {
        match event {
            Event::End(TagEnd::Table) => break,
            Event::Start(Tag::TableCell) => row.push(inlines(events, TagEnd::TableCell)),
            Event::End(TagEnd::TableHead) => header = std::mem::take(&mut row),
            Event::End(TagEnd::TableRow) => rows.push(std::mem::take(&mut row)),
            _ => {}
        }
    }
    Block::Table { header, rows }
}

// ---- drawing -----------------------------------------------------------------------------------------------------------

/// A Markdown text in the window's style. `size` is the body text size.
pub fn show(ui: &mut Ui, source: &str, size: f32) {
    for block in parse(source) {
        block_ui(ui, &block, size, text(), false);
    }
}

/// A line or two of Markdown with a given colour (the lines to say, the key terms): the paragraphs only, close together.
pub fn inline(ui: &mut Ui, source: &str, size: f32, color: Color32) {
    for block in parse(source) {
        match block {
            Block::Paragraph(spans) | Block::Heading(_, spans) => spans_ui(ui, &spans, size, color, false),
            other => block_ui(ui, &other, size, color, true),
        }
    }
}

fn block_ui(ui: &mut Ui, block: &Block, size: f32, color: Color32, tight: bool) {
    match block {
        Block::Paragraph(spans) => {
            spans_ui(ui, spans, size, color, false);
            if !tight {
                ui.add_space(size * 0.3);
            }
        }
        Block::Heading(depth, spans) => {
            let scale = match depth {
                1 => 1.45,
                2 => 1.28,
                3 => 1.14,
                _ => 1.04,
            };
            ui.add_space(size * 0.25);
            spans_ui(ui, spans, size * scale, strong(), true);
            ui.add_space(size * 0.2);
        }
        Block::Code { language, body } => {
            code_block(ui, language, body, size);
            ui.add_space(size * 0.3);
        }
        Block::Quote(inner) => {
            let shown = Frame::new().inner_margin(Margin { left: 12, right: 0, top: 2, bottom: 2 }).show(ui, |ui| {
                for block in inner {
                    block_ui(ui, block, size, muted(), false);
                }
            });
            let rect = shown.response.rect;
            ui.painter().line_segment([rect.left_top(), rect.left_bottom()], Stroke::new(3.0, accent()));
        }
        Block::List { first, items } => {
            for (index, item) in items.iter().enumerate() {
                ui.horizontal_top(|ui| {
                    marker(ui, first.map(|start| start + index as u64), item.checked, size);
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        for block in &item.blocks {
                            block_ui(ui, block, size, color, true);
                        }
                    });
                });
            }
            ui.add_space(size * 0.15);
        }
        Block::Rule => {
            ui.separator();
        }
        Block::Table { header, rows } => {
            egui::Grid::new(ui.next_auto_id()).striped(true).spacing([14.0, 5.0]).show(ui, |ui| {
                for cell in header {
                    spans_ui(ui, cell, size, strong(), true);
                }
                ui.end_row();
                for row in rows {
                    for cell in row {
                        spans_ui(ui, cell, size, color, false);
                    }
                    ui.end_row();
                }
            });
            ui.add_space(size * 0.3);
        }
    }
}

/// The bullet, number or tick box in front of a list item. Painted, because the window's fonts have no bullet glyph.
fn marker(ui: &mut Ui, number: Option<u64>, checked: Option<bool>, size: f32) {
    let width = size * 1.4;
    if let Some(done) = checked {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(width, size * 1.2), egui::Sense::hover());
        let boxed = egui::Rect::from_center_size(rect.center(), egui::vec2(size * 0.8, size * 0.8));
        ui.painter().rect_stroke(boxed, 3.0, Stroke::new(1.5, muted()), egui::StrokeKind::Inside);
        if done {
            let (a, b, c) = (
                boxed.left_center() + egui::vec2(2.0, 0.0),
                boxed.center_bottom() - egui::vec2(1.0, 3.0),
                boxed.right_top() + egui::vec2(-2.0, 3.0),
            );
            ui.painter().line_segment([a, b], Stroke::new(2.0, accent()));
            ui.painter().line_segment([b, c], Stroke::new(2.0, accent()));
        }
    } else if let Some(number) = number {
        ui.add_sized(
            [width + 4.0, size * 1.2],
            egui::Label::new(RichText::new(format!("{number}.")).font(semibold(size * 0.95)).color(accent())),
        );
    } else {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(width, size * 1.2), egui::Sense::hover());
        ui.painter().circle_filled(rect.center() + egui::vec2(-1.0, 1.0), size * 0.16, accent());
    }
}

fn format(span: &Span, size: f32, color: Color32, heading: bool) -> TextFormat {
    let font = if span.code {
        mono(size * 0.94)
    } else if span.bold || heading {
        semibold(size)
    } else {
        sans(size)
    };
    let color = if span.link.is_some() || span.code {
        accent()
    } else if span.bold {
        strong()
    } else {
        color
    };
    TextFormat {
        font_id: font,
        color,
        italics: span.italic,
        strikethrough: if span.strike { Stroke::new(1.0, color) } else { Stroke::NONE },
        underline: if span.link.is_some() { Stroke::new(1.0, color) } else { Stroke::NONE },
        ..TextFormat::default()
    }
}

fn job(spans: &[Span], size: f32, color: Color32, heading: bool, width: f32) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.wrap.max_width = width;
    for span in spans {
        job.append(&span.text, 0.0, format(span, size, color, heading));
    }
    job
}

/// Only an address a browser should open: a link in an answer is the model's text, and a click opens it.
pub fn safe_link(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    lower.starts_with("https://") || lower.starts_with("http://")
}

/// One paragraph. Without links it is one label; with links the runs are laid out one after the other so each link can
/// be clicked.
fn spans_ui(ui: &mut Ui, spans: &[Span], size: f32, color: Color32, heading: bool) {
    if spans.is_empty() {
        return;
    }
    if !spans.iter().any(|span| span.link.is_some()) {
        ui.label(job(spans, size, color, heading, ui.available_width()));
        return;
    }
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        for span in spans {
            let one = std::slice::from_ref(span);
            match span.link.as_deref() {
                Some(url) => {
                    let response = ui
                        .add(
                            egui::Label::new(job(one, size, color, heading, f32::INFINITY)).sense(egui::Sense::click()),
                        )
                        .on_hover_text(url)
                        .on_hover_cursor(egui::CursorIcon::PointingHand);
                    if response.clicked() && safe_link(url) {
                        super::settings_ui::open_path(std::path::Path::new(url.trim()));
                    }
                }
                None => {
                    ui.label(job(one, size, color, heading, f32::INFINITY));
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(spans: &[Span]) -> String {
        spans.iter().map(|span| span.text.as_str()).collect()
    }

    #[test]
    fn headings_paragraphs_and_emphasis_are_told_apart() {
        let blocks = parse("## Joins\n\nA **left** join keeps *every* row of `users`, ~~not~~ only some.");
        assert!(matches!(&blocks[0], Block::Heading(2, spans) if plain(spans) == "Joins"));
        let Block::Paragraph(spans) = &blocks[1] else { panic!("{blocks:?}") };
        assert_eq!(plain(spans), "A left join keeps every row of users, not only some.");
        assert!(spans.iter().any(|span| span.text == "left" && span.bold));
        assert!(spans.iter().any(|span| span.text == "every" && span.italic));
        assert!(spans.iter().any(|span| span.text == "users" && span.code));
        assert!(spans.iter().any(|span| span.text == "not" && span.strike));
    }

    #[test]
    fn lists_nest_and_numbers_and_ticks_are_kept() {
        let blocks = parse("1. one\n2. two\n   - inner\n\n- [x] done\n- [ ] todo");
        let Block::List { first, items } = &blocks[0] else { panic!("{blocks:?}") };
        assert_eq!((*first, items.len()), (Some(1), 2));
        assert!(items[1].blocks.iter().any(|block| matches!(block, Block::List { first: None, .. })));
        let Block::List { items: tasks, .. } = &blocks[1] else { panic!("{blocks:?}") };
        assert_eq!((tasks[0].checked, tasks[1].checked), (Some(true), Some(false)));
        assert!(matches!(&tasks[0].blocks[0], Block::Paragraph(spans) if plain(spans) == "done"));
    }

    #[test]
    fn code_quotes_rules_and_tables_are_blocks() {
        let blocks = parse("> a quote\n\n---\n\n```sql\nselect 1;\n```\n\n| a | b |\n|---|---|\n| 1 | **2** |");
        assert!(
            matches!(&blocks[0], Block::Quote(inner) if matches!(&inner[0], Block::Paragraph(spans) if plain(spans) == "a quote"))
        );
        assert_eq!(blocks[1], Block::Rule);
        assert_eq!(blocks[2], Block::Code { language: "sql".into(), body: "select 1;".into() });
        let Block::Table { header, rows } = &blocks[3] else { panic!("{blocks:?}") };
        assert_eq!((header.len(), rows.len(), rows[0].len()), (2, 1, 2));
        assert!(rows[0][1][0].bold);
    }

    #[test]
    fn links_keep_their_address_and_only_web_addresses_open() {
        let Block::Paragraph(spans) = &parse("See [the docs](https://example.com/a) now")[0] else { panic!() };
        assert_eq!(spans[1].link.as_deref(), Some("https://example.com/a"));
        assert!(safe_link("https://example.com") && safe_link(" HTTP://example.com"));
        for bad in ["file:///etc/passwd", "javascript:alert(1)", "ms-settings:", "\\\\host\\share", "calc.exe"] {
            assert!(!safe_link(bad), "{bad}");
        }
    }

    #[test]
    fn a_half_written_answer_never_breaks_and_an_open_fence_shows_its_code() {
        for partial in [
            "I use **Dock",
            "```py\ndef f(x):\n    ret",
            "- one\n- tw",
            "| a | b |\n|---",
            "> quo",
            "`unclosed",
            "**",
            "~~",
            "[link](http",
            "",
        ] {
            let blocks = parse(partial);
            let _ = format!("{blocks:?}"); // parsing never panics
        }
        assert_eq!(
            parse("```py\ndef f(x):\n    ret"),
            [Block::Code { language: "py".into(), body: "def f(x):\n    ret".into() }]
        );
        let Block::Paragraph(spans) = &parse("I use **Dock")[0] else { panic!() };
        assert_eq!(plain(spans), "I use **Dock"); // an unclosed marker stays text
    }
}
