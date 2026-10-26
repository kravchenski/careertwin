//! Comparing what you said with what you meant to say, word by word.
//!
//! This is how the "say it again" drills tell you which words went missing. It compares what a
//! speech recogniser heard with the target text, so it shows words the recogniser could not make
//! out; it does not score pronunciation.

const MAX_WORDS: usize = 400;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    /// In the target and heard.
    Match,
    /// In the target but not heard.
    Missed,
    /// Heard but not in the target.
    Extra,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WordMark {
    pub word: String,
    pub mark: Mark,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Comparison {
    pub marks: Vec<WordMark>,
    /// The share of target words that were heard, 0.0 to 1.0.
    pub accuracy: f32,
}

impl Comparison {
    pub fn missed(&self) -> Vec<&str> {
        self.marks.iter().filter(|m| m.mark == Mark::Missed).map(|m| m.word.as_str()).collect()
    }
}

/// Lower case, no punctuation; apostrophes inside a word stay (`don't`).
pub fn normalize(word: &str) -> String {
    word.chars()
        .map(|c| if c == '’' || c == '‘' { '\'' } else { c })
        .filter(|c| c.is_alphanumeric() || *c == '\'')
        .collect::<String>()
        .trim_matches('\'')
        .to_lowercase()
}

fn words(text: &str) -> Vec<(String, String)> {
    text.split_whitespace()
        .map(|raw| (raw.to_owned(), normalize(raw)))
        .filter(|(_, norm)| !norm.is_empty())
        .take(MAX_WORDS)
        .collect()
}

pub fn compare(target: &str, heard: &str) -> Comparison {
    let target = words(target);
    let heard = words(heard);
    let (n, m) = (target.len(), heard.len());
    // longest common subsequence of the normalised words
    let mut table = vec![vec![0u16; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            table[i][j] =
                if target[i].1 == heard[j].1 { table[i + 1][j + 1] + 1 } else { table[i + 1][j].max(table[i][j + 1]) };
        }
    }
    let mut marks = Vec::with_capacity(n + m);
    let (mut i, mut j, mut matched) = (0, 0, 0usize);
    while i < n || j < m {
        if i < n && j < m && target[i].1 == heard[j].1 {
            marks.push(WordMark { word: target[i].0.clone(), mark: Mark::Match });
            matched += 1;
            i += 1;
            j += 1;
        } else if j < m && (i == n || table[i][j + 1] >= table[i + 1][j]) {
            marks.push(WordMark { word: heard[j].0.clone(), mark: Mark::Extra });
            j += 1;
        } else {
            marks.push(WordMark { word: target[i].0.clone(), mark: Mark::Missed });
            i += 1;
        }
    }
    let accuracy = if n == 0 { 0.0 } else { matched as f32 / n as f32 };
    Comparison { marks, accuracy }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary(c: &Comparison) -> Vec<(String, Mark)> {
        c.marks.iter().map(|m| (m.word.clone(), m.mark)).collect()
    }

    #[test]
    fn identical_text_is_all_matches() {
        let c = compare("I worked on the order API.", "i worked on the order api");
        assert!(c.marks.iter().all(|m| m.mark == Mark::Match));
        assert_eq!(c.accuracy, 1.0);
    }

    #[test]
    fn a_missing_word_is_marked_missed() {
        let c = compare("I traced the slow query", "I traced the query");
        assert_eq!(c.missed(), vec!["slow"]);
        assert!((c.accuracy - 0.8).abs() < 0.001);
    }

    #[test]
    fn an_added_word_is_extra_and_does_not_hurt_accuracy() {
        let c = compare("I added an index", "I added um an index");
        assert_eq!(c.marks.iter().filter(|m| m.mark == Mark::Extra).count(), 1);
        assert_eq!(c.accuracy, 1.0);
    }

    #[test]
    fn a_swapped_word_is_one_miss_and_one_extra() {
        let c = compare("the cache was stale", "the cash was stale");
        assert_eq!(c.missed(), vec!["cache"]);
        assert!(summary(&c).contains(&("cash".to_string(), Mark::Extra)));
    }

    #[test]
    fn punctuation_case_and_curly_apostrophes_do_not_matter() {
        let c = compare("It’s fine, isn't it?", "its fine isnt it");
        assert!(c.accuracy < 1.0, "its != it's is a real difference");
        let c = compare("Don’t panic.", "don't panic");
        assert_eq!(c.accuracy, 1.0);
    }

    #[test]
    fn nothing_heard_misses_everything() {
        let c = compare("one two three", "");
        assert_eq!(c.accuracy, 0.0);
        assert_eq!(c.missed(), vec!["one", "two", "three"]);
    }

    #[test]
    fn an_empty_target_has_zero_accuracy_and_no_panic() {
        assert_eq!(compare("", "anything").accuracy, 0.0);
        assert_eq!(compare("", "").marks.len(), 0);
    }

    #[test]
    fn russian_words_compare_too() {
        assert_eq!(compare("Привет, мир", "привет мир").accuracy, 1.0);
    }

    #[test]
    fn a_huge_text_is_capped_not_quadratic_forever() {
        let big = "word ".repeat(5000);
        let started = std::time::Instant::now();
        let c = compare(&big, &big);
        assert!(c.marks.len() <= MAX_WORDS * 2);
        assert!(started.elapsed() < std::time::Duration::from_secs(2));
    }

    #[test]
    fn order_is_kept_in_the_marks() {
        let c = compare("a b c", "a x c");
        let order: Vec<&str> = c.marks.iter().map(|m| m.word.as_str()).collect();
        assert_eq!(order.first(), Some(&"a"));
        assert_eq!(order.last(), Some(&"c"));
    }
}
