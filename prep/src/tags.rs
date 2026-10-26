//! Control tags in the tutor's words.
//!
//! The tutor can end a message with a tag such as `[[native: 3]]` or `[[show: how a queue works]]`.
//! The call reads them and acts, and they are removed before anything is said aloud or shown, even
//! when a tag arrives in pieces while the reply is still streaming.

/// A tag longer than this is not a tag, but something that happens to start with two brackets.
const MAX_TAG_CHARS: usize = 300;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tag {
    /// The term was explained in your own language; the score is 0 to 3.
    Native(u8),
    /// The term was said in English; the score is 0 to 3, and the next term is up.
    English(u8),
    /// The person asked to practise the other language: stage two begins.
    Practice,
    /// One thing worth keeping.
    Note(String),
    /// Show a picture or an animation about this.
    Show(String),
    /// The lesson is over: say goodbye, and the call closes.
    End,
}

impl Tag {
    /// The tag inside `[[` and `]]`; `None` for a name that is not one of ours.
    pub fn parse(inner: &str) -> Option<Self> {
        let (name, value) = match inner.split_once(':') {
            Some((name, value)) => (name.trim(), value.trim()),
            None => (inner.trim(), ""),
        };
        let score = || value.split_whitespace().next()?.parse::<u8>().ok().map(|n| n.min(3));
        match name.to_lowercase().as_str() {
            "native" => score().map(Self::Native),
            "english" => score().map(Self::English),
            "note" if !value.is_empty() => Some(Self::Note(value.to_owned())),
            "show" if !value.is_empty() => Some(Self::Show(value.to_owned())),
            "practice" => Some(Self::Practice),
            "end" => Some(Self::End),
            _ => None,
        }
    }
}

/// Takes pieces of a streaming reply, gives back the text without tags, and keeps the tags.
#[derive(Debug, Default)]
pub struct TagFilter {
    /// Text held back because it may be the start of a tag: `[`, or `[[` and what follows it.
    held: String,
    inside: bool,
    tags: Vec<Tag>,
}

impl TagFilter {
    /// The part of `piece` that is words. A lone `[` is held until the next character shows
    /// whether it opens a tag.
    pub fn push(&mut self, piece: &str) -> String {
        let mut out = String::new();
        for ch in piece.chars() {
            if self.inside {
                self.held.push(ch);
                if self.held.ends_with("]]") {
                    let inner = &self.held[2..self.held.len() - 2];
                    if let Some(tag) = Tag::parse(inner) {
                        self.tags.push(tag);
                    }
                    self.held.clear();
                    self.inside = false;
                } else if self.held.len() > MAX_TAG_CHARS {
                    out.push_str(&self.held);
                    self.held.clear();
                    self.inside = false;
                }
                continue;
            }
            if self.held == "[" {
                if ch == '[' {
                    self.held.push('[');
                    self.inside = true;
                    continue;
                }
                out.push('[');
                self.held.clear();
            }
            if ch == '[' {
                self.held.push('[');
            } else {
                out.push(ch);
            }
        }
        out
    }

    /// What is left at the end of the reply: a lone bracket is text; an unfinished tag is dropped.
    pub fn finish(&mut self) -> String {
        let rest = if self.inside { String::new() } else { std::mem::take(&mut self.held) };
        self.held.clear();
        self.inside = false;
        rest
    }

    /// The tags read since the last call.
    pub fn take(&mut self) -> Vec<Tag> {
        std::mem::take(&mut self.tags)
    }
}

/// `text` with every complete tag removed, for a reply that is already whole.
pub fn strip(text: &str) -> (String, Vec<Tag>) {
    let mut filter = TagFilter::default();
    let mut clean = filter.push(text);
    clean.push_str(&filter.finish());
    (clean, filter.take())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_request_for_the_other_language_is_a_tag_of_its_own() {
        assert_eq!(Tag::parse("practice"), Some(Tag::Practice));
        assert_eq!(strip("Ладно, переходим. What is it? [[practice]]").1, vec![Tag::Practice]);
    }

    #[test]
    fn a_tag_at_the_end_is_read_and_removed() {
        let (clean, tags) = strip("Nice, that's it. [[native: 3]]");
        assert_eq!(clean.trim(), "Nice, that's it.");
        assert_eq!(tags, vec![Tag::Native(3)]);
    }

    #[test]
    fn every_kind_of_tag_is_understood() {
        let (_, tags) =
            strip("[[native: 2]][[english:1]][[note: Encapsulation hides state.]][[show: how a queue works]][[end]]");
        assert_eq!(
            tags,
            vec![
                Tag::Native(2),
                Tag::English(1),
                Tag::Note("Encapsulation hides state.".into()),
                Tag::Show("how a queue works".into()),
                Tag::End
            ]
        );
    }

    #[test]
    fn scores_are_capped_and_names_ignore_case() {
        assert_eq!(Tag::parse("NATIVE: 9"), Some(Tag::Native(3)));
        assert_eq!(Tag::parse(" English : 2 out of 3"), Some(Tag::English(2)));
        assert_eq!(Tag::parse("end"), Some(Tag::End));
    }

    #[test]
    fn nonsense_tags_are_dropped_not_spoken() {
        let (clean, tags) = strip("Okay. [[dance: now]] Next. [[native: lots]] [[note:]]");
        assert!(tags.is_empty(), "{tags:?}");
        assert_eq!(clean.split_whitespace().collect::<Vec<_>>().join(" "), "Okay. Next.");
    }

    #[test]
    fn a_tag_split_across_pieces_never_leaks_into_the_words() {
        let mut filter = TagFilter::default();
        let mut spoken = String::new();
        for piece in ["Good. ", "[", "[na", "tive: ", "3]", "]", " Next one?"] {
            spoken.push_str(&filter.push(piece));
        }
        spoken.push_str(&filter.finish());
        assert_eq!(spoken, "Good.  Next one?");
        assert_eq!(filter.take(), vec![Tag::Native(3)]);
    }

    #[test]
    fn single_brackets_in_ordinary_text_pass_through() {
        let (clean, tags) = strip("Use arr[i] and the [1] reference, then [x].");
        assert_eq!(clean, "Use arr[i] and the [1] reference, then [x].");
        assert!(tags.is_empty());
    }

    #[test]
    fn a_lone_bracket_at_the_end_is_text_and_an_unfinished_tag_is_dropped() {
        let (clean, _) = strip("Look at this [");
        assert_eq!(clean, "Look at this [");
        let (clean, tags) = strip("Done. [[native: 3");
        assert_eq!(clean, "Done. ");
        assert!(tags.is_empty());
    }

    #[test]
    fn something_that_only_looks_like_a_tag_is_given_back_when_it_goes_on_too_long() {
        let long = format!("[[{}", "word ".repeat(100));
        let (clean, tags) = strip(&long);
        assert!(clean.starts_with("[[word word"), "{}", &clean[..20]);
        assert!(tags.is_empty());
    }

    #[test]
    fn three_brackets_in_a_row_still_find_the_tag() {
        let (clean, tags) = strip("A [[[note: x]] B");
        assert!(tags.is_empty(), "{tags:?}");
        assert!(clean.contains('A') && clean.contains('B'));
    }

    #[test]
    fn the_filter_can_be_used_again_after_taking() {
        let mut filter = TagFilter::default();
        filter.push("[[end]]");
        assert_eq!(filter.take(), vec![Tag::End]);
        assert!(filter.take().is_empty());
        filter.push("[[note: a]]");
        assert_eq!(filter.take(), vec![Tag::Note("a".into())]);
    }
}
