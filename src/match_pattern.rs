use std::collections::HashSet;

mod match_fns {
    use super::MatchPattern;

    pub(super) fn match_inclusive(p: &MatchPattern, w: &str) -> bool {
        p.words
            .iter()
            .map(|pattern_w| (w.as_bytes().windows(pattern_w.len()), pattern_w.as_bytes()))
            .any(|(sub_ws, pattern_w)| sub_ws.map(|w| w == pattern_w).any(|b| b))
    }

    pub(super) fn match_exclusive(p: &MatchPattern, w: &str) -> bool {
        p.words.contains(w)
    }
}

type MatchFnPtr = fn(&MatchPattern, &str) -> bool;

trait MatchFnDispatcher {
    fn dispatch_match_fn(&self) -> MatchFnPtr;
}

#[derive(Default, Copy, Clone)]
pub enum MatchMode {
    Inclusive,
    #[default]
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

pub struct MatchPattern {
    words: HashSet<String>,
    max_len: usize,
    min_len: usize,
    mode: MatchMode,
    match_fn: MatchFnPtr,
}

impl<'a, T> From<T> for MatchPattern
where
    T: IntoIterator<Item = &'a str>,
{
    fn from(value: T) -> Self {
        let words: HashSet<_> = value.into_iter().map(MatchPattern::format_word).collect();
        let mode = MatchMode::default();
        let (min, max) = MatchPattern::get_minmax_len(&words);
        MatchPattern {
            words,
            max_len: max,
            min_len: min,
            match_fn: mode.dispatch_match_fn(),
            mode,
        }
    }
}

impl<'a> MatchPattern {
    pub fn new() -> Self {
        let mode = MatchMode::default();
        MatchPattern {
            words: HashSet::new(),
            max_len: 0,
            min_len: 0,
            match_fn: mode.dispatch_match_fn(),
            mode,
        }
    }

    pub fn mode(&self) -> MatchMode {
        self.mode
    }

    pub fn words(&self) -> &HashSet<String> {
        &self.words
    }

    pub fn set_mode(&mut self, mode: MatchMode) {
        self.match_fn = mode.dispatch_match_fn();
        self.mode = mode;
    }

    pub fn extend(&mut self, words: impl IntoIterator<Item = &'a str>) {
        let words = words.into_iter().map(MatchPattern::format_word);
        self.words.extend(words);
        self.on_words_mut();
    }

    pub fn insert(&mut self, word: &str) {
        let word = MatchPattern::format_word(word);
        self.words.insert(word);
        self.on_words_mut();
    }

    pub fn remove(&mut self, word: &str) -> bool {
        if self.words.remove(word) {
            self.on_words_mut();
            return true;
        }
        false
    }

    pub fn match_str(&self, str: &str) -> bool {
        str.split(' ')
            .map(MatchPattern::format_word)
            .filter(|s| self.min_len <= s.len())
            .any(|s| (self.match_fn)(self, &s))
    }

    fn on_words_mut(&mut self) {
        self.words.shrink_to_fit();
        (self.min_len, self.max_len) = MatchPattern::get_minmax_len(&self.words);
    }

    fn get_minmax_len(words: &HashSet<String>) -> (usize, usize) {
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

    fn format_word(word: &str) -> String {
        word.to_lowercase()
            .chars()
            .filter(|c| c.is_alphanumeric())
            .collect()
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

        p.insert("abc;;;;");
        assert_eq!((p.min_len, p.max_len), (1, 3));

        p.extend(["s", "as", "abcd", "ab,,,,,,cde"]);
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

    #[test]
    fn format() {
        let text = "so.....me te,xt t,.,3./o filter";

        let p = MatchPattern::from(["so.,.me"]);
        assert_eq!(p.match_str(text), true);
    }
}
