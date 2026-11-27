//! 跨格式的全文查找，返回原文及引擎提供的位置。

mod normalize;

pub use normalize::{Mapping, Normalized, normalize};

use normalize::{fold, is_han};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    ops::Range,
};
use unicode_general_category::{GeneralCategory, get_general_category};
use unicode_normalization::char::is_combining_mark;

/// 默认最多交回几处。
pub const DEFAULT_MAX_HITS: usize = 20;
/// 默认前后各留多少字。
pub const DEFAULT_CONTEXT: usize = 40;

/// 一次查找。JSON 字段和 `<短名>_find` 的参数同名，引擎可以直接从参数反序列化（其他参数忽略）。
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Query {
    #[serde(rename = "query")]
    pub text: String,
    #[serde(default = "default_max_hits")]
    pub max_hits: usize,
    #[serde(default = "default_context")]
    pub context: usize,
}

impl Query {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            max_hits: DEFAULT_MAX_HITS,
            context: DEFAULT_CONTEXT,
        }
    }
}

const fn default_max_hits() -> usize {
    DEFAULT_MAX_HITS
}

const fn default_context() -> usize {
    DEFAULT_CONTEXT
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Match {
    Phrase,
    Words,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Text,
    Ocr,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Hit<L> {
    pub location: L,
    pub before: String,
    pub matched: String,
    pub after: String,
    pub how: Match,
    pub source: Source,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Found<L> {
    pub hits: Vec<Hit<L>>,
    pub total: usize,
    pub truncated: bool,
}

/// 每次扫描一个由引擎定义的单元，最后按全局命中方式返回结果。
pub struct Finder<L> {
    phrase: String,
    words: BTreeMap<String, usize>,
    max_hits: usize,
    context: usize,
    phrase_hits: Vec<Hit<L>>,
    phrase_total: usize,
    word_hits: Vec<Hit<L>>,
    word_total: usize,
}

impl<L> Finder<L> {
    pub fn new(query: &Query) -> Self {
        let folded = fold(&query.text);
        let phrase = Normalized::from_folded(&folded).text;
        let separated: String = folded.iter().map(|scalar| scalar.ch).collect();
        let mut words = BTreeMap::new();
        for word in separated
            .split(is_separator)
            .filter(|word| !word.is_empty())
        {
            *words.entry(word.to_owned()).or_insert(0) += 1;
        }
        Self {
            phrase,
            words,
            max_hits: query.max_hits,
            context: query.context,
            phrase_hits: Vec::new(),
            phrase_total: 0,
            word_hits: Vec::new(),
            word_total: 0,
        }
    }

    /// locate 收到当前单元原文中的 UTF-8 字节区间，可在引擎中映射到页框、行号等。
    pub fn scan(&mut self, text: &str, source: Source, mut locate: impl FnMut(Range<usize>) -> L) {
        if self.phrase.is_empty() {
            return;
        }
        let normalized = normalize(text);
        let mut previous_end = 0;
        let mut next = 0;
        while let Some(offset) = normalized.text[next..].find(&self.phrase) {
            let start = next + offset;
            let end = start + self.phrase.len();
            let range = normalized.original_range(start..end).unwrap();
            if range.start < previous_end {
                next = start + normalized.text[start..].chars().next().unwrap().len_utf8();
                continue;
            }
            next = end;
            previous_end = range.end;
            self.phrase_total += 1;
            if self.phrase_total == 1 {
                self.word_hits.clear();
            }
            if self.phrase_hits.len() < self.max_hits {
                let location = locate(range.clone());
                self.phrase_hits.push(hit(
                    text,
                    range,
                    self.context,
                    location,
                    Match::Phrase,
                    source,
                ));
            }
        }
        if self.phrase_total == 0 && !self.words.is_empty() {
            // 每个词各算一处：取本单元里离得最近的那一组，各自带位置和前后文。
            for range in nearest_words(text, &normalized, &self.words) {
                self.word_total += 1;
                if self.word_hits.len() < self.max_hits {
                    let location = locate(range.clone());
                    self.word_hits.push(hit(
                        text,
                        range,
                        self.context,
                        location,
                        Match::Words,
                        source,
                    ));
                }
            }
        }
    }

    pub fn finish(self) -> Found<L> {
        let (hits, total) = if self.phrase_total > 0 {
            (self.phrase_hits, self.phrase_total)
        } else {
            (self.word_hits, self.word_total)
        };
        Found {
            truncated: total > hits.len(),
            hits,
            total,
        }
    }
}

fn hit<L>(
    text: &str,
    range: Range<usize>,
    context: usize,
    location: L,
    how: Match,
    source: Source,
) -> Hit<L> {
    let before = text[..range.start]
        .char_indices()
        .rev()
        .take(context)
        .last()
        .map_or(range.start, |(offset, _)| offset);
    let after = text[range.end..]
        .char_indices()
        .nth(context)
        .map_or(text.len(), |(offset, _)| range.end + offset);
    Hit {
        location,
        before: text[before..range.start].to_owned(),
        matched: text[range.clone()].to_owned(),
        after: text[range.end..after].to_owned(),
        how,
        source,
    }
}

/// 所有词（重复的词按次数）都在本单元时，交回覆盖它们的最短原文范围里每个词的位置，按原文顺序。
fn nearest_words(
    original: &str,
    normalized: &Normalized,
    words: &BTreeMap<String, usize>,
) -> Vec<Range<usize>> {
    let occurrences: Vec<_> = words
        .keys()
        .map(|word| word_occurrences(normalized, word))
        .collect();
    let Some(window) = shortest_window(original, &occurrences, words) else {
        return Vec::new();
    };
    let mut chosen = Vec::new();
    for (found, &required) in occurrences.iter().zip(words.values()) {
        // 已按结束位置排好，从左往右取不重叠的，窗口里一定够数。
        let mut end = window.start;
        let inside = found.iter().filter(|range| {
            let take = range.start >= end && range.end <= window.end;
            if take {
                end = range.end;
            }
            take
        });
        chosen.extend(inside.take(required).cloned());
    }
    chosen.sort_unstable_by_key(|range| (range.start, range.end));
    chosen.dedup();
    chosen
}

fn shortest_window(
    original: &str,
    occurrences: &[Vec<Range<usize>>],
    words: &BTreeMap<String, usize>,
) -> Option<Range<usize>> {
    let mut windows = Vec::new();
    for (index, (occurrences, &required)) in occurrences.iter().zip(words.values()).enumerate() {
        if occurrences.len() < required {
            return None;
        }
        // 对每个结束位置保留满足本词次数的最晚起点。
        let mut starts: Vec<_> = occurrences.iter().map(|range| Some(range.start)).collect();
        for _ in 1..required {
            let prefix: Vec<_> = starts
                .iter()
                .scan(None, |best, &start| {
                    *best = (*best).max(start);
                    Some(*best)
                })
                .collect();
            for (start, range) in starts.iter_mut().zip(occurrences) {
                let prior = occurrences.partition_point(|prior| prior.end <= range.start);
                *start = prior.checked_sub(1).and_then(|i| prefix[i]);
            }
            if !starts.iter().any(Option::is_some) {
                return None;
            }
        }
        windows.extend(
            starts
                .into_iter()
                .zip(occurrences)
                .filter_map(|(start, range)| Some((index, start?..range.end))),
        );
    }

    windows.sort_unstable_by_key(|(_, range)| range.end);
    let char_starts: Vec<_> = original
        .char_indices()
        .map(|(start, _)| start)
        .chain(std::iter::once(original.len()))
        .collect();
    let mut best = None;
    let mut latest_starts = vec![None; words.len()];
    let mut starts_in_order = BTreeSet::new();
    for (index, range) in windows {
        // 扫到当前右端时，每个词都用其已出现的最晚起点。
        if let Some(previous) = latest_starts[index] {
            if previous >= range.start {
                continue;
            }
            starts_in_order.remove(&(previous, index));
        }
        latest_starts[index] = Some(range.start);
        starts_in_order.insert((range.start, index));
        if starts_in_order.len() == words.len() {
            let start = starts_in_order.first().unwrap().0;
            let start_char = char_starts.binary_search(&start).unwrap();
            let end_char = char_starts.binary_search(&range.end).unwrap();
            let candidate = (end_char - start_char, start, range.end);
            if best.is_none_or(|best| candidate < best) {
                best = Some(candidate);
            }
        }
    }
    best.map(|(_, start, end)| start..end)
}

fn word_occurrences(normalized: &Normalized, word: &str) -> Vec<Range<usize>> {
    let text = &normalized.text;
    let left_han = word.chars().next().is_some_and(is_han);
    let right_han = word.chars().next_back().is_some_and(is_han);
    let mut occurrences = Vec::new();
    let mut next = 0;
    while let Some(offset) = text[next..].find(word) {
        let start = next + offset;
        let end = start + word.len();
        next = start + text[start..].chars().next().unwrap().len_utf8();
        if (left_han || !text[..start].chars().next_back().is_some_and(word_neighbor))
            && (right_han || !text[end..].chars().next().is_some_and(word_neighbor))
        {
            occurrences.push(normalized.original_range(start..end).unwrap());
        }
    }
    occurrences.sort_unstable_by_key(|range| (range.end, range.start));
    occurrences.dedup();
    occurrences
}

fn word_neighbor(ch: char) -> bool {
    !is_han(ch) && (ch.is_alphanumeric() || is_combining_mark(ch))
}

fn is_separator(ch: char) -> bool {
    ch.is_whitespace()
        || matches!(
            get_general_category(ch),
            GeneralCategory::ConnectorPunctuation
                | GeneralCategory::DashPunctuation
                | GeneralCategory::OpenPunctuation
                | GeneralCategory::ClosePunctuation
                | GeneralCategory::InitialPunctuation
                | GeneralCategory::FinalPunctuation
                | GeneralCategory::OtherPunctuation
        )
}
