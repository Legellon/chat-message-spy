use crate::match_pattern::match_fns::MatchFnPtr;
use clap::builder::Str;
use clap::ValueEnum;
use fnv::FnvHashSet;
use hyper::body::Buf;
use serde::__private::de::Borrowed;
use serde::{Deserialize, Serialize};
use std::borrow::{Borrow, Cow};
use std::collections::HashSet;

mod match_fns {
    use super::MatchPattern;
    use std::borrow::Cow;

    pub(super) type MatchFnInput<'a> = Cow<'a, str>;
    pub(super) type MatchFnPtr = fn(&MatchPattern, &MatchFnInput) -> bool;

    pub(super) fn match_inclusive(p: &MatchPattern, w: &MatchFnInput) -> bool {
        p.words
            .iter()
            .map(|pattern_w| (w.as_bytes().windows(pattern_w.len()), pattern_w.as_bytes()))
            .any(|(sub_ws, pattern_w)| sub_ws.map(|w| w == pattern_w).any(|b| b))
    }

    pub(super) fn match_exclusive(p: &MatchPattern, w: &MatchFnInput) -> bool {
        p.words.iter().any(|s| s == w)
    }
}

trait MatchFnDispatcher {
    fn dispatch_match_fn(&self) -> MatchFnPtr;
}

#[derive(Serialize, Deserialize, Default, Copy, Clone, Debug, ValueEnum)]
pub enum MatchMode {
    #[default]
    Inclusive,
    Exclusive,
}

impl MatchFnDispatcher for MatchMode {
    fn dispatch_match_fn(&self) -> MatchFnPtr {
        match self {
            MatchMode::Inclusive => match_fns::match_inclusive,
            MatchMode::Exclusive => match_fns::match_exclusive,
        }
    }
}

#[derive(Debug, Clone)]
pub struct MatchPattern {
    words: Vec<String>,
    ignore_chars: String,
    max_len: usize,
    min_len: usize,
    mode: MatchMode,
    match_fn: MatchFnPtr,
}

// impl<'a, T: IntoIterator<Item = AsRef<str>>> From<T> for MatchPattern {
//     fn from(value: T) -> Self {
//         let mut p = MatchPattern::new();
//         p.extend(value);
//         p
//     }
// }
//
// impl<'a, T: IntoIterator<Item = String>> From<T> for MatchPattern {
//     fn from(value: T) -> Self {
//         let mut p = MatchPattern::new();
//         p.extend(value);
//         p
//     }
// }
//
pub struct MatchPatternBuilder {
    pattern: MatchPattern,
}

impl MatchPatternBuilder {
    pub fn new() -> Self {
        MatchPatternBuilder {
            pattern: MatchPattern::new(),
        }
    }

    pub fn exclusive(mut self) -> Self {
        self.pattern.set_mode(MatchMode::Exclusive);
        self
    }

    pub fn inclusive(mut self) -> Self {
        self.pattern.set_mode(MatchMode::Inclusive);
        self
    }

    pub fn mode(mut self, m: MatchMode) -> Self {
        self.pattern.set_mode(m);
        self
    }

    pub fn words<'a>(mut self, words: impl IntoIterator<Item = impl Into<Cow<'a, str>>>) -> Self {
        self.pattern.extend(words);
        self
    }

    pub fn build(self) -> MatchPattern {
        self.pattern
    }
}

impl MatchPattern {
    pub fn new() -> Self {
        let mode = MatchMode::default();
        MatchPattern {
            words: vec![],
            ignore_chars: String::new(),
            max_len: 0,
            min_len: 0,
            match_fn: mode.dispatch_match_fn(),
            mode,
        }
    }

    pub fn builder() -> MatchPatternBuilder {
        MatchPatternBuilder::new()
    }

    pub fn mode(&self) -> MatchMode {
        self.mode
    }

    pub fn words(&self) -> &Vec<String> {
        &self.words
    }

    pub fn set_mode(&mut self, mode: MatchMode) {
        self.match_fn = mode.dispatch_match_fn();
        self.mode = mode;
    }

    pub fn extend<'a>(&mut self, words: impl IntoIterator<Item = impl Into<Cow<'a, str>>>) {
        let words_iter = words
            .into_iter()
            .map(|s| self.format_word(s).to_string())
            .collect::<Vec<_>>();
        self.words.extend(words_iter);
        self.on_words_mut();
    }

    pub fn insert(&mut self, word: &str) {
        let word = self.format_word(word).to_string();
        self.words.push(word);
        self.on_words_mut();
    }

    pub fn remove(&mut self, word: &str) -> bool {
        if let Some(p) = self.words.iter().position(|s| s == word) {
            self.words.swap_remove(p);
            self.on_words_mut();
            true
        } else {
            false
        }
    }

    pub fn remove_position(&mut self, p: usize) -> bool {
        if p > self.words.len() {
            return false;
        }
        self.words.swap_remove(p);
        self.on_words_mut();
        true
    }

    pub fn match_str(&self, str: &str) -> bool {
        str.split(' ')
            .map(|w| self.format_word(w))
            .filter(|s| self.min_len <= s.len())
            .any(|s| (self.match_fn)(self, &s))
    }

    fn on_words_mut(&mut self) {
        self.words.shrink_to_fit();
        (self.min_len, self.max_len) = MatchPattern::get_minmax_len(&self.words);
    }

    fn get_minmax_len(words: &Vec<String>) -> (usize, usize) {
        if words.is_empty() {
            return (0, 0);
        }

        let mut words_len_it = words.iter().map(|w| w.len());

        let (mut min, mut max) = {
            let l = words_len_it.next().unwrap();
            (l, l)
        };

        for l in words_len_it {
            if min > l {
                min = l;
            } else if max < l {
                max = l;
            }
        }

        (min, max)
    }

    fn format_word<'a>(&self, w: impl Into<Cow<'a, str>>) -> Cow<'a, str> {
        let cow = w.into();

        let is_correct = cow
            .chars()
            .all(|c| c.is_lowercase() && !self.ignore_chars.contains(c));

        if is_correct {
            cow
        } else {
            Cow::Owned(
                cow.chars()
                    .filter(|c| !self.ignore_chars.contains(*c))
                    .collect::<String>()
                    .to_lowercase()
                    .into(),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mutation() {
        let mut p = MatchPattern::new();
        assert_eq!((p.min_len, p.max_len), (0, 0));

        p.insert("a");
        assert_eq!((p.min_len, p.max_len), (1, 1));

        p.insert("abc");
        assert_eq!((p.min_len, p.max_len), (1, 3));

        p.extend(["s", "as", "abcd", "abcde"]);
        assert_eq!((p.min_len, p.max_len), (1, 5));

        p.remove("a");
        assert_eq!((p.min_len, p.max_len), (1, 5));

        p.remove("s");
        assert_eq!((p.min_len, p.max_len), (2, 5));
    }

    #[test]
    fn pattern() {
        let text = "some example text to match";

        let mut p = MatchPattern::new();
        assert_eq!(p.match_str(text), false);

        p.insert("mat");
        assert_eq!(p.match_str(text), false);

        p.insert("example");
        assert_eq!(p.match_str(text), true);
        p.remove("example");

        p.set_mode(MatchMode::Inclusive);
        assert_eq!(p.match_str(text), true);
    }
}
