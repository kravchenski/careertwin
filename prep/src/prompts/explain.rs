//! Asking the model for a visual explanation, and reading it back.
//!
//! The brief is a complete beginner, a person with no technical background, who should understand
//! the topic after one short animation. The model gets a small, strict vocabulary of pictures
//! (see [`crate::explain::PanelKind`]) because a picture it cannot draw is worse than no picture.

use serde_json::Value;

use super::json::{extract_object, strings, text};
use super::{Lang, PromptError};
use crate::explain::{MAX_PANELS, MAX_SCENES, Panel, PanelKind, Scene, Side, Storyboard};

/// Lines in a panel, items in a row: more would not fit and could not be read.
const MAX_LINES: usize = 6;
const MAX_ITEMS: usize = 8;
const MAX_WORDS_SAID: usize = 55;

pub fn system(native: Lang, simpler: bool) -> String {
    let extra = if simpler {
        "\nThe person did not understand the first version. Make this one simpler: fewer terms, one idea per \
         scene, a more everyday comparison, and a tiny example with real numbers."
    } else {
        ""
    };
    format!(
        "You make a short animated explanation for a complete beginner: someone with no technical background who \
         should understand the topic after watching it once. It plays as floating glowing panels around a talking \
         sphere while you narrate; a scene is what is on screen while one piece of narration is said.\n\n\
         PRINCIPLES\n\
         - Start from an everyday comparison (a kitchen, a post office, a library, a road). One idea per scene. \
         Build up: repeat a panel's \"id\" in the next scene with one more item, so it grows instead of being drawn \
         again.\n\
         - Show a tiny concrete example with real values, not only definitions.\n\
         - What is drawn and what is said must agree. A cells panel numbers its cells from 0 in the order of its \
         items, and an empty string is an empty cell: if the narration says the third shelf, the third cell holds it \
         and the cells before it are empty (\"\"). Never say a number that is not on screen.\n\
         - End with a scene that sums it up in one line.\n\
         - 5 to 8 scenes. 1 to 3 panels in a scene (4 at most). Panel text is very short: at most 6 words a line, at \
         most 5 lines.\n\
         - \"say\" is the narration, one to three short sentences in {lang}, spoken aloud: plain words, no symbols, \
         no code, no lists. It says what the panels show, as a person would explain it to a friend.\n\
         - Names of terms and all code stay in English.{extra}\n\n\
         PANEL KINDS (use only these)\n\
         - text: {{\"kind\":\"text\",\"title\":str,\"lines\":[str]}}\n\
         - flow: boxes joined by arrows, left to right: {{\"kind\":\"flow\",\"title\":str,\"items\":[str],\"active\":int}}\n\
         - cells: a row of cells (an array, a queue, buckets): {{\"kind\":\"cells\",\"title\":str,\"items\":[str],\
         \"highlight\":[int],\"note\":str}}\n\
         - code: {{\"kind\":\"code\",\"title\":str,\"lines\":[str],\"highlight\":[int]}} (at most 6 short lines)\n\
         - compare: {{\"kind\":\"compare\",\"title\":str,\"left\":{{\"title\":str,\"lines\":[str]}},\"right\":{{\"title\":\
         str,\"lines\":[str]}}}}\n\
         - steps: {{\"kind\":\"steps\",\"title\":str,\"lines\":[str],\"active\":int}}\n\
         - analogy: {{\"kind\":\"analogy\",\"title\":str,\"lines\":[the everyday thing, the technical thing]}}\n\
         - layers: bars from top to bottom: {{\"kind\":\"layers\",\"title\":str,\"lines\":[str]}}\n\
         Every panel also has an \"id\": a short lowercase name that stays the same when the panel continues in the \
         next scene.\n\n\
         Reply with JSON only: {{\"title\": str, \"scenes\": [{{\"say\": str, \"panels\": [panel]}}], \"check\": str}}. \
         title is a few words in {lang}. check is one question in {lang} to ask afterwards to see whether it landed.",
        lang = native.name(),
    )
}

pub fn prompt(topic: &str, context: &str) -> String {
    let context = context.trim();
    if context.is_empty() {
        format!("TOPIC: {}\n\nMake the explanation.", topic.trim())
    } else {
        format!("TOPIC: {}\n\nWHAT THE PERSON KNOWS AND NEEDS\n{context}\n\nMake the explanation.", topic.trim())
    }
}

fn side(value: &Value) -> Side {
    Side { title: text(value, "title"), lines: strings(value, "lines", MAX_LINES) }
}

fn index_list(value: &Value, key: &str, limit: usize) -> Vec<usize> {
    value
        .get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_u64)
        .map(|n| n as usize)
        .filter(|n| *n < limit)
        .collect()
}

/// The cells of a row, in order. An empty string is an empty cell and stays one: the narration says "the
/// third shelf", and the third cell has to be the third one drawn.
fn cell_items(value: &Value) -> Vec<String> {
    let mut cells: Vec<String> = value
        .get("items")
        .and_then(Value::as_array)
        .map(|items| {
            items.iter().map(|i| i.as_str().map_or_else(String::new, |s| s.trim().to_owned())).take(MAX_ITEMS).collect()
        })
        .unwrap_or_default();
    while cells.last().is_some_and(String::is_empty) {
        cells.pop();
    }
    cells
}

fn panel(value: &Value, position: usize) -> Option<Panel> {
    let kind = PanelKind::parse(&text(value, "kind"));
    let title = text(value, "title");
    let mut panel = Panel {
        id: text(value, "id").to_lowercase(),
        kind,
        title,
        lines: strings(value, "lines", MAX_LINES),
        items: strings(value, "items", MAX_ITEMS),
        note: text(value, "note"),
        left: value.get("left").map(side).unwrap_or_default(),
        right: value.get("right").map(side).unwrap_or_default(),
        ..Panel::default()
    };
    if kind == PanelKind::Cells {
        let cells = cell_items(value);
        if cells.iter().any(|cell| !cell.is_empty()) {
            panel.items = cells;
        }
    }
    // a model that put the content under the other name still gets a picture
    if matches!(kind, PanelKind::Flow | PanelKind::Cells) && panel.items.is_empty() {
        panel.items = std::mem::take(&mut panel.lines);
    }
    if !matches!(kind, PanelKind::Flow | PanelKind::Cells) && panel.lines.is_empty() {
        panel.lines = std::mem::take(&mut panel.items);
    }
    let limit = match kind {
        PanelKind::Flow | PanelKind::Cells => panel.items.len(),
        _ => panel.lines.len(),
    };
    panel.highlight = index_list(value, "highlight", limit);
    panel.active = value.get("active").and_then(Value::as_u64).map(|n| n as usize).filter(|n| *n < limit.max(1));
    if panel.id.is_empty() {
        panel.id = if panel.title.is_empty() { format!("panel{position}") } else { panel.title.to_lowercase() };
    }
    let empty = match kind {
        PanelKind::Compare => panel.left.lines.is_empty() && panel.right.lines.is_empty(),
        PanelKind::Flow | PanelKind::Cells => panel.items.is_empty(),
        _ => panel.lines.is_empty(),
    };
    (!empty).then_some(panel)
}

/// The words of one scene, cut to a length that is pleasant to hear.
fn narration(text: &str) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() <= MAX_WORDS_SAID {
        return words.join(" ");
    }
    let mut kept = words[..MAX_WORDS_SAID].join(" ");
    if let Some(end) = kept.rfind(['.', '!', '?']) {
        kept.truncate(end + 1);
    }
    kept
}

pub fn parse(reply: &str, native: Lang) -> Result<Storyboard, PromptError> {
    let value = extract_object(reply).ok_or(PromptError::Unusable("an explanation"))?;
    let mut scenes: Vec<Scene> = Vec::new();
    for item in value.get("scenes").and_then(Value::as_array).into_iter().flatten() {
        let say = narration(&text(item, "say"));
        let panels: Vec<Panel> = item
            .get("panels")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .enumerate()
            .filter_map(|(position, p)| panel(p, position))
            .take(MAX_PANELS)
            .collect();
        if say.is_empty() {
            continue;
        }
        scenes.push(Scene { say, panels });
        if scenes.len() == MAX_SCENES {
            break;
        }
    }
    if scenes.is_empty() || scenes.iter().all(|s| s.panels.is_empty()) {
        return Err(PromptError::Unusable("an explanation"));
    }
    Ok(Storyboard { title: text(&value, "title"), scenes, check: text(&value, "check"), native })
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = r#"```json
{"title": "Как работает хеш-таблица", "check": "Куда попадут два ключа с одним номером ячейки?",
 "scenes": [
  {"say": "Представь шкаф с пронумерованными ячейками. В каждую кладёшь по вещи.",
   "panels": [{"id": "lockers", "kind": "cells", "title": "Ячейки", "items": ["0", "1", "2", "3"], "highlight": [2], "note": "номер = hash(ключ) % 4"}]},
  {"say": "Чтобы положить вещь, считаем её номер и идём сразу к нужной ячейке.",
   "panels": [
     {"id": "lockers", "kind": "cells", "title": "Ячейки", "items": ["0", "1", "2", "3", "4"], "highlight": [2]},
     {"id": "path", "kind": "flow", "title": "Положить", "items": ["ключ", "hash()", "ячейка"], "active": 1},
     {"id": "code", "kind": "code", "title": "Python", "lines": ["d = {}", "d['cat'] = 1"], "highlight": [1]}]}
 ]}
```"#;

    #[test]
    fn an_empty_cell_stays_empty_so_a_number_in_the_narration_lands_on_the_cell_it_names() {
        let reply = r#"{"scenes": [{"say": "Boris is on the third shelf.", "panels": [{"kind": "cells", "items": ["", "Anya", "", "Boris", ""], "highlight": [3]}]}]}"#;
        let board = parse(reply, Lang::En).unwrap();
        let cells = &board.scenes[0].panels[0];
        assert_eq!(cells.items, vec!["", "Anya", "", "Boris"], "blank cells stay, and trailing ones are dropped");
        assert_eq!(cells.highlight, vec![3]);
        // a row of nothing is still not a picture
        assert!(
            parse(r#"{"scenes": [{"say": "x", "panels": [{"kind": "cells", "items": ["", ""]}]}]}"#, Lang::En).is_err()
        );
    }

    #[test]
    fn the_prompt_describes_a_beginner_and_every_kind_of_picture() {
        let text = system(Lang::Ru, false);
        for must in [
            "complete beginner",
            "no technical background",
            "in Russian",
            "everyday comparison",
            "repeat a panel's \"id\"",
            "5 to 8 scenes",
            "an empty string is an empty cell",
            "Never say a number that is not on screen",
            "text:",
            "flow:",
            "cells:",
            "code:",
            "compare:",
            "steps:",
            "analogy:",
            "layers:",
            "check is one question",
        ] {
            assert!(text.contains(must), "missing: {must}");
        }
        assert!(!text.contains("did not understand"));
        assert!(system(Lang::En, true).contains("did not understand the first version"));
        assert!(
            prompt("hash map", "knows Python").contains("TOPIC: hash map") && prompt("x", "").starts_with("TOPIC: x")
        );
    }

    #[test]
    fn a_story_is_read_with_its_scenes_panels_and_question() {
        let board = parse(GOOD, Lang::Ru).unwrap();
        assert_eq!(board.title, "Как работает хеш-таблица");
        assert_eq!(board.scenes.len(), 2);
        assert_eq!(board.native, Lang::Ru);
        assert!(board.check.starts_with("Куда попадут"));
        let first = &board.scenes[0].panels[0];
        assert_eq!(
            (first.kind, first.id.as_str(), first.items.len(), first.highlight.as_slice()),
            (PanelKind::Cells, "lockers", 4, &[2][..])
        );
        assert_eq!(first.note, "номер = hash(ключ) % 4");
        let second = &board.scenes[1];
        assert_eq!(second.panels.len(), 3);
        assert_eq!(second.panels[1].active, Some(1));
        assert_eq!(second.panels[2].kind, PanelKind::Code);
        // the cells panel grows from four cells to five instead of appearing again
        assert_eq!(board.already_shown(1, &second.panels[0]), 4);
    }

    #[test]
    fn indexes_that_point_nowhere_are_dropped() {
        let reply = r#"{"scenes": [{"say": "Look.", "panels": [{"kind": "cells", "items": ["a", "b"], "highlight": [0, 5, 1], "active": 9}]}]}"#;
        let panel = &parse(reply, Lang::En).unwrap().scenes[0].panels[0];
        assert_eq!(panel.highlight, vec![0, 1]);
        assert_eq!(panel.active, None);
        assert!(!panel.id.is_empty(), "a panel without an id gets one");
    }

    #[test]
    fn content_under_the_wrong_name_still_makes_a_picture() {
        let reply = r#"{"scenes": [
          {"say": "One.", "panels": [{"kind": "flow", "title": "Path", "lines": ["a", "b", "c"]}]},
          {"say": "Two.", "panels": [{"kind": "text", "title": "Facts", "items": ["x", "y"]}]}]}"#;
        let board = parse(reply, Lang::En).unwrap();
        assert_eq!(board.scenes[0].panels[0].items, vec!["a", "b", "c"]);
        assert_eq!(board.scenes[1].panels[0].lines, vec!["x", "y"]);
    }

    #[test]
    fn empty_panels_and_scenes_without_words_are_left_out_and_the_counts_are_capped() {
        let many: Vec<String> = (0..20)
            .map(|i| format!(r#"{{"say": "Scene {i}.", "panels": [{{"kind": "text", "lines": ["a"]}}, {{"kind": "text", "lines": []}}]}}"#))
            .collect();
        let reply = format!(
            r#"{{"scenes": [{{"say": "", "panels": [{{"kind": "text", "lines": ["lost"]}}]}}, {}]}}"#,
            many.join(",")
        );
        let board = parse(&reply, Lang::En).unwrap();
        assert_eq!(board.scenes.len(), MAX_SCENES);
        assert!(board.scenes.iter().all(|s| s.panels.len() == 1 && !s.say.is_empty()));
        let five = r#"{"scenes": [{"say": "Look.", "panels": [
          {"kind":"text","lines":["1"]},{"kind":"text","lines":["2"]},{"kind":"text","lines":["3"]},{"kind":"text","lines":["4"]},{"kind":"text","lines":["5"]}]}]}"#;
        assert_eq!(parse(five, Lang::En).unwrap().scenes[0].panels.len(), MAX_PANELS);
    }

    #[test]
    fn a_story_with_nothing_to_show_is_refused() {
        assert!(parse("Sure, here is an explanation in words.", Lang::En).is_err());
        assert!(parse(r#"{"scenes": []}"#, Lang::En).is_err());
        assert!(
            parse(r#"{"scenes": [{"say": "Only words."}]}"#, Lang::En).is_err(),
            "a story without pictures is no story"
        );
    }

    #[test]
    fn a_long_narration_is_cut_at_a_sentence() {
        let long = format!("First sentence is short. {}", "word ".repeat(80));
        let said = narration(&long);
        assert_eq!(said, "First sentence is short.");
        let no_stop = "word ".repeat(80);
        assert_eq!(narration(&no_stop).split_whitespace().count(), MAX_WORDS_SAID);
        assert_eq!(narration("  Short   and\nclean. "), "Short and clean.");
    }

    #[test]
    fn a_comparison_keeps_both_sides() {
        let reply = r#"{"scenes": [{"say": "Two ways.", "panels": [{"kind": "compare", "title": "List vs set", "left": {"title": "List", "lines": ["ordered", "repeats"]}, "right": {"title": "Set", "lines": ["no order"]}}]}]}"#;
        let panel = &parse(reply, Lang::En).unwrap().scenes[0].panels[0];
        assert_eq!((panel.left.title.as_str(), panel.left.lines.len(), panel.right.lines.len()), ("List", 2, 1));
        assert_eq!(panel.count(), 3);
    }
}
