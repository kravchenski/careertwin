//! Cutting a reply into sentences as it streams, so the interviewer can start speaking after the
//! first one instead of waiting for the whole answer.

const ABBREVIATIONS: [&str; 20] = [
    "mr", "mrs", "ms", "dr", "prof", "vs", "etc", "e.g", "i.e", "eg", "ie", "inc", "ltd", "st", "no", "approx", "fig",
    "sr", "jr", "u.s",
];
/// A sentence this long without a full stop is cut at a comma instead.
const LONG_RUN: usize = 200;
/// The first thing said in a reply is cut early, because the voice needs time to make each piece
/// and a long first sentence is a long silence: at its first comma once it has this many letters...
const FIRST_CLAUSE_MIN: usize = 16;
/// ...or, with no comma, after the first word that ends past this many.
const FIRST_RUN: usize = 48;

/// Accepts pieces of any size and hands back sentences as they complete.
#[derive(Default)]
pub struct SentenceSplitter {
    buffer: String,
    /// Set once something of this reply has been handed back.
    emitted: bool,
}

impl SentenceSplitter {
    pub fn push(&mut self, piece: &str) -> Vec<String> {
        self.buffer.push_str(piece);
        let mut out = Vec::new();
        loop {
            let early = if self.emitted { None } else { self.first_clause() };
            let Some(end) = [self.next_boundary(), early].into_iter().flatten().min() else { break };
            let sentence: String = self.buffer.drain(..end).collect();
            let sentence = clean_for_speech(&sentence);
            if !sentence.is_empty() {
                self.emitted = true;
                out.push(sentence);
            }
        }
        out
    }

    /// Where the first piece of a reply may end before its first sentence is over.
    fn first_clause(&self) -> Option<usize> {
        let chars: Vec<(usize, char)> = self.buffer.char_indices().collect();
        for (n, &(index, ch)) in chars.iter().enumerate() {
            let after = index + ch.len_utf8();
            let next_is_space = chars.get(n + 1).is_some_and(|&(_, c)| c.is_whitespace());
            if n >= FIRST_CLAUSE_MIN && matches!(ch, ',' | ';' | ':') && next_is_space {
                return Some(after);
            }
            if n >= FIRST_RUN && ch.is_whitespace() {
                return Some(after);
            }
        }
        None
    }

    /// The rest, once the reply is over.
    pub fn finish(&mut self) -> Option<String> {
        let rest = clean_for_speech(&std::mem::take(&mut self.buffer));
        (!rest.is_empty()).then_some(rest)
    }

    /// Byte index just past the first complete sentence, if there is one.
    fn next_boundary(&self) -> Option<usize> {
        let text = &self.buffer;
        let chars: Vec<(usize, char)> = text.char_indices().collect();
        for (n, &(index, ch)) in chars.iter().enumerate() {
            if !matches!(ch, '.' | '!' | '?' | '…' | '。' | '！' | '？') {
                if ch == '\n' && n + 1 < chars.len() && chars[n + 1].1 == '\n' && index > 0 {
                    return Some(index + 1); // a blank line ends a paragraph
                }
                continue;
            }
            // Take a run of terminators and closing quotes/brackets: `?!`, `."`, `.)`.
            let mut last = n;
            while last + 1 < chars.len()
                && matches!(chars[last + 1].1, '.' | '!' | '?' | '…' | '"' | '\'' | ')' | '”' | '’')
            {
                last += 1;
            }
            // The next character decides: it has to be a space (or the text must end later).
            let &(_, after) = chars.get(last + 1)?;
            if !after.is_whitespace() {
                continue; // 3.5, e.g.com, file.py
            }
            if ch == '.' && self.is_abbreviation(index) {
                continue;
            }
            let end = chars.get(last + 1).map_or(text.len(), |&(i, _)| i);
            return Some(end);
        }
        if text.chars().count() > LONG_RUN {
            return self.cut_long_run();
        }
        None
    }

    fn is_abbreviation(&self, dot_index: usize) -> bool {
        let before = &self.buffer[..dot_index];
        let word: String = before
            .chars()
            .rev()
            .take_while(|c| c.is_alphanumeric() || *c == '.')
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        let lower = word.to_lowercase();
        if ABBREVIATIONS.contains(&lower.as_str()) {
            return true;
        }
        // An initial: "J. Smith".
        word.chars().count() == 1 && word.chars().all(char::is_uppercase)
    }

    fn cut_long_run(&self) -> Option<usize> {
        let limit = self.buffer.char_indices().nth(LONG_RUN).map_or(self.buffer.len(), |(i, _)| i);
        let head = &self.buffer[..limit];
        [", ", "; ", ": ", " — ", " - "]
            .iter()
            .filter_map(|mark| head.rfind(mark).map(|i| i + mark.len()))
            .max()
            .filter(|&end| end > 40)
    }
}

/// Plain speech from a reply that may carry Markdown: nothing a voice would read out as symbols.
pub fn clean_for_speech(text: &str) -> String {
    let mut lines = Vec::new();
    let mut in_code = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            in_code = !in_code;
            continue;
        }
        if in_code || trimmed.is_empty() {
            continue;
        }
        let trimmed = trimmed.trim_start_matches('#').trim_start();
        let trimmed = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "))
            .or_else(|| trimmed.strip_prefix("• "))
            .unwrap_or(trimmed);
        lines.push(trimmed.to_owned());
    }
    let joined = lines.join(" ");
    let mut out = String::with_capacity(joined.len());
    let mut chars = joined.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '*' | '`' | '_' if is_markup(&out, chars.peek().copied()) => {}
            '[' | ']' => {}
            _ => out.push(ch),
        }
    }
    let mut collapsed = String::with_capacity(out.len());
    let mut previous_space = true;
    for ch in out.chars() {
        if ch.is_whitespace() {
            if !previous_space {
                collapsed.push(' ');
            }
            previous_space = true;
        } else {
            collapsed.push(ch);
            previous_space = false;
        }
    }
    collapsed.trim().to_owned()
}

/// `snake_case` keeps its underscores; emphasis marks and backticks go.
fn is_markup(before: &str, next: Option<char>) -> bool {
    let previous = before.chars().last();
    // an underscore between two word characters belongs to an identifier
    !(previous.is_some_and(char::is_alphanumeric) && next.is_some_and(char::is_alphanumeric)) || before.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(pieces: &[&str]) -> Vec<String> {
        let mut splitter = SentenceSplitter::default();
        let mut out = Vec::new();
        for piece in pieces {
            out.extend(splitter.push(piece));
        }
        out.extend(splitter.finish());
        out
    }

    #[test]
    fn a_long_first_sentence_is_cut_early_so_the_voice_can_start() {
        let reply = run(&[
            "Yeah, a hash map maps keys to values using a hash function that puts each key into a bucket, and then lookups are fast. Next one.",
        ]);
        assert_eq!(reply[0], "Yeah, a hash map maps keys to values using a hash");
        assert_eq!(
            reply[1], "function that puts each key into a bucket, and then lookups are fast.",
            "only the first piece is cut early"
        );
        assert_eq!(reply[2], "Next one.");
        // a comma that comes after a few words is the better place to cut
        let reply =
            run(&["Right, so a hash map stores values by key, and a lookup is quick, which is the point. Fine."]);
        assert_eq!(reply[0], "Right, so a hash map stores values by key,");
    }

    #[test]
    fn a_first_sentence_with_no_comma_is_cut_after_a_few_words_and_a_short_one_is_not() {
        let long =
            run(&["A hash map is a structure that stores pairs of keys and values and finds them again very quickly."]);
        assert!(long.len() == 2 && long[0].len() >= 48 && long[0].len() < 60, "{long:?}");
        assert_eq!(
            run(&["Hi Alex, I'm Daniel from the platform team."]),
            vec!["Hi Alex, I'm Daniel from the platform team."]
        );
    }

    #[test]
    fn the_early_cut_works_with_a_reply_that_arrives_in_small_pieces() {
        let pieces: Vec<&str> = "Sure, a hash map stores values by key, so a lookup does not need to scan. Right?"
            .split_inclusive(' ')
            .collect();
        assert_eq!(
            run(&pieces),
            vec!["Sure, a hash map stores values by key,", "so a lookup does not need to scan.", "Right?"]
        );
    }

    #[test]
    fn cuts_at_sentence_ends() {
        assert_eq!(run(&["Hello there. How are you? Fine!"]), vec!["Hello there.", "How are you?", "Fine!"]);
    }

    #[test]
    fn works_with_any_piece_size() {
        let text = "Nice to meet you. Tell me about yourself? Go ahead.";
        let expected = vec!["Nice to meet you.", "Tell me about yourself?", "Go ahead."];
        for step in [1, 2, 3, 5, 100] {
            let pieces: Vec<String> =
                text.chars().collect::<Vec<_>>().chunks(step).map(|c| c.iter().collect()).collect();
            let refs: Vec<&str> = pieces.iter().map(String::as_str).collect();
            assert_eq!(run(&refs), expected, "step {step}");
        }
    }

    #[test]
    fn the_first_sentence_is_ready_before_the_rest_arrives() {
        let mut splitter = SentenceSplitter::default();
        assert!(splitter.push("Welcome, I am Anna").is_empty());
        let first = splitter.push(" from the platform team. Let us");
        assert_eq!(first, vec!["Welcome, I am Anna from the platform team."]);
    }

    #[test]
    fn decimals_domains_and_filenames_do_not_split() {
        assert_eq!(
            run(&["Latency fell from 3.5 seconds to 0.8 seconds. Good."]),
            vec!["Latency fell from 3.5 seconds to 0.8 seconds.", "Good."]
        );
        assert_eq!(
            run(&["Open main.py and read api.example.com today."]),
            vec!["Open main.py and read api.example.com today."]
        );
    }

    #[test]
    fn abbreviations_and_initials_do_not_split() {
        assert_eq!(
            run(&["Ask Dr. Smith about it. Then e.g. Redis is fine."]),
            vec!["Ask Dr. Smith about it.", "Then e.g. Redis is fine."]
        );
        assert_eq!(run(&["Written by J. Smith and others."]), vec!["Written by J. Smith and others."]);
    }

    #[test]
    fn quotes_and_ellipses_stay_with_their_sentence() {
        assert_eq!(run(&["She said \"go.\" Then left… Okay?!"]), vec!["She said \"go.\"", "Then left…", "Okay?!"]);
    }

    #[test]
    fn an_unfinished_tail_comes_out_on_finish() {
        assert_eq!(run(&["First one. And the tail without a stop"]), vec!["First one.", "And the tail without a stop"]);
    }

    #[test]
    fn a_blank_line_ends_a_paragraph() {
        assert_eq!(run(&["Intro line\n\nSecond paragraph. End."]), vec!["Intro line", "Second paragraph.", "End."]);
    }

    #[test]
    fn a_very_long_run_is_cut_at_a_comma() {
        let long = format!(
            "{}, and then it keeps going without ever stopping for a very long time {}",
            "word ".repeat(30),
            "more ".repeat(10)
        );
        let parts = run(&[&long]);
        assert!(parts.len() >= 2, "{parts:?}");
        assert!(parts[0].len() <= LONG_RUN + 2);
    }

    #[test]
    fn markdown_is_not_read_out() {
        assert_eq!(
            clean_for_speech("**Great** answer — use `FastAPI` and _asyncio_."),
            "Great answer — use FastAPI and asyncio."
        );
        assert_eq!(clean_for_speech("# Title\n- one\n- two"), "Title one two");
        assert_eq!(clean_for_speech("see [the docs](x)"), "see the docs(x)");
    }

    #[test]
    fn identifiers_keep_their_underscores() {
        assert_eq!(clean_for_speech("call get_user_by_id now"), "call get_user_by_id now");
    }

    #[test]
    fn code_blocks_are_never_spoken() {
        assert_eq!(clean_for_speech("Try this:\n```python\nprint('x')\n```\nIt works."), "Try this: It works.");
    }

    #[test]
    fn nothing_in_nothing_out() {
        assert!(run(&[""]).is_empty());
        assert!(run(&["   \n  "]).is_empty());
    }

    #[test]
    fn russian_sentences_split_too() {
        assert_eq!(
            run(&["Привет! Расскажите о себе. Что вы делали на прошлой работе?"]),
            vec!["Привет!", "Расскажите о себе.", "Что вы делали на прошлой работе?"]
        );
    }
}
