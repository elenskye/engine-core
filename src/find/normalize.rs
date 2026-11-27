use std::{ops::Range, sync::OnceLock};
use unicode_normalization::char::{
    canonical_combining_class, compose, decompose_canonical, decompose_compatible,
    is_combining_mark,
};
use unicode_script::{Script, UnicodeScript};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mapping {
    /// 归一化文字中对应 Unicode 标量的 UTF-8 起始字节。
    pub normalized_start: usize,
    /// 原文 UTF-8 字节区间。
    pub original: Range<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Normalized {
    pub text: String,
    pub map: Vec<Mapping>,
}

impl Normalized {
    /// 归一化后的非空 UTF-8 字节区间所覆盖的原文区间。
    pub fn original_range(&self, range: Range<usize>) -> Option<Range<usize>> {
        if range.is_empty()
            || !self.text.is_char_boundary(range.start)
            || !self.text.is_char_boundary(range.end)
        {
            return None;
        }
        let start = self
            .map
            .binary_search_by_key(&range.start, |item| item.normalized_start)
            .ok()?;
        let end =
            self.map[start..].partition_point(|item| item.normalized_start < range.end) + start;
        let mut source = self.map[start].original.clone();
        for item in &self.map[start + 1..end] {
            source.start = source.start.min(item.original.start);
            source.end = source.end.max(item.original.end);
        }
        Some(source)
    }

    pub(super) fn from_folded(folded: &[Scalar]) -> Self {
        let mut text = String::with_capacity(folded.len());
        let mut map = Vec::with_capacity(folded.len());
        let cjk_at =
            |index: Option<usize>| index.and_then(|i| folded.get(i)).is_some_and(Scalar::cjk);
        for (index, scalar) in folded.iter().enumerate() {
            // 中日文字和全角标点旁的空白是排版或换行留下的。查询也走这里，词间空格仍留给按词切分。
            if scalar.ch == ' ' && (cjk_at(index.checked_sub(1)) || cjk_at(Some(index + 1))) {
                continue;
            }
            map.push(Mapping {
                normalized_start: text.len(),
                original: scalar.original.clone(),
            });
            text.push(scalar.ch);
        }
        Self { text, map }
    }
}

pub fn normalize(text: &str) -> Normalized {
    Normalized::from_folded(&fold(text))
}

pub(super) struct Scalar {
    pub(super) ch: char,
    original: Range<usize>,
    /// 原文是全角或中日文标点（NFKC 之后看不出来）。
    wide: bool,
}

impl Scalar {
    fn cjk(&self) -> bool {
        self.wide
            || matches!(
                self.ch.script(),
                Script::Han | Script::Hiragana | Script::Katakana
            )
    }
}

pub(super) fn fold(text: &str) -> Vec<Scalar> {
    let composed = mapped_nfkc(text);
    let mut unaccented: Vec<Scalar> = Vec::with_capacity(composed.len());
    let mut latin_starter: Option<usize> = None;
    for scalar in composed {
        if ignorable(scalar.ch) {
            continue;
        }
        if let Some(index) = latin_starter {
            if is_combining_mark(scalar.ch)
                && canonical_combining_class(scalar.ch) != 0
                && matches!(
                    scalar.ch.script(),
                    Script::Inherited | Script::Common | Script::Latin
                )
            {
                let original = &mut unaccented[index].original;
                original.start = original.start.min(scalar.original.start);
                original.end = original.end.max(scalar.original.end);
                continue;
            }
        }
        if is_latin_letter(scalar.ch) {
            latin_starter = Some(unaccented.len());
            decompose_canonical(scalar.ch, |ch| {
                if !is_combining_mark(ch) {
                    unaccented.push(Scalar {
                        ch,
                        original: scalar.original.clone(),
                        wide: scalar.wide,
                    });
                }
            });
        } else {
            latin_starter = None;
            unaccented.push(scalar);
        }
    }

    let mut folded = Vec::with_capacity(unaccented.len());
    let mut index = 0;
    while index < unaccented.len() {
        let scalar = &unaccented[index];
        if scalar.ch.is_whitespace() {
            let end = whitespace_end(&unaccented, index);
            if !folded.is_empty() && end < unaccented.len() {
                folded.push(Scalar {
                    ch: ' ',
                    original: scalar.original.start..unaccented[end - 1].original.end,
                    wide: false,
                });
            }
            index = end;
            continue;
        }
        // 拉丁字母之间的横线去掉，中间可以有一次换行：infor-\nmation、self-\nattention、
        // self-attention 都折成一个词。软连字符后的空白也算换行，其他位置的软连字符直接去掉。
        if is_dash(scalar.ch) {
            let dashes = index
                + unaccented[index..]
                    .iter()
                    .take_while(|item| is_dash(item.ch))
                    .count();
            let gap = whitespace_end(&unaccented, dashes);
            let breaks = line_breaks(&unaccented[dashes..gap]);
            // 换行后的软连字符也看不见：t-\n\u{ad}tests
            let end = gap
                + unaccented[gap..]
                    .iter()
                    .take_while(|item| item.ch == SOFT_HYPHEN)
                    .count();
            let soft = unaccented[index..dashes]
                .iter()
                .any(|item| item.ch == SOFT_HYPHEN);
            if folded
                .last()
                .is_some_and(|prev: &Scalar| is_latin_letter(prev.ch))
                && unaccented
                    .get(end)
                    .is_some_and(|next| is_latin_letter(next.ch))
                && (dashes == gap || breaks == 1 || (soft && breaks == 0))
            {
                index = end;
                continue;
            }
            if scalar.ch == SOFT_HYPHEN {
                index += 1;
                continue;
            }
        }
        let ch = match scalar.ch {
            '\u{2018}'..='\u{201b}' => '\'',
            '\u{201c}'..='\u{201f}' => '"',
            '\u{2010}'..='\u{2015}' | '\u{2212}' => '-',
            ch if is_han(ch) => simplified(ch),
            ch => ch,
        };
        for ch in ch.to_lowercase() {
            folded.push(Scalar {
                ch,
                original: scalar.original.clone(),
                wide: scalar.wide,
            });
        }
        index += 1;
    }
    folded
}

fn mapped_nfkc(text: &str) -> Vec<Scalar> {
    let mut decomposed = Vec::with_capacity(text.len());
    let mut marks_start = 0;
    for (index, input) in text.char_indices() {
        let original = index..index + input.len_utf8();
        let wide = wide_punctuation(input);
        decompose_compatible(input, |ch| {
            if canonical_combining_class(ch) == 0 {
                decomposed[marks_start..]
                    .sort_by_key(|item: &Scalar| canonical_combining_class(item.ch));
                marks_start = decomposed.len() + 1;
            }
            decomposed.push(Scalar {
                ch,
                original: original.clone(),
                wide,
            });
        });
    }
    decomposed[marks_start..].sort_by_key(|item| canonical_combining_class(item.ch));

    // 依规范顺序重组，合并参与重组的原文位置。
    let mut result: Vec<Scalar> = Vec::with_capacity(decomposed.len());
    let mut starter: Option<usize> = None;
    let mut last_class = 0;
    for scalar in decomposed {
        let class = canonical_combining_class(scalar.ch);
        let composed = starter
            .filter(|_| last_class == 0 || last_class < class)
            .and_then(|index| compose(result[index].ch, scalar.ch).map(|ch| (index, ch)));
        if let Some((index, ch)) = composed {
            let base = &mut result[index];
            base.ch = ch;
            base.original.start = base.original.start.min(scalar.original.start);
            base.original.end = base.original.end.max(scalar.original.end);
        } else {
            if class == 0 {
                starter = Some(result.len());
            }
            last_class = class;
            result.push(scalar);
        }
    }
    result
}

fn whitespace_end(scalars: &[Scalar], start: usize) -> usize {
    start
        + scalars[start..]
            .iter()
            .take_while(|item| item.ch.is_whitespace())
            .count()
}

fn line_breaks(whitespace: &[Scalar]) -> usize {
    let mut previous = '\0';
    let mut count = 0;
    for item in whitespace {
        if matches!(item.ch, '\r' | '\n' | '\u{0085}' | '\u{2028}' | '\u{2029}')
            && (item.ch != '\n' || previous != '\r')
        {
            count += 1;
        }
        previous = item.ch;
    }
    count
}

const SOFT_HYPHEN: char = '\u{00ad}';

/// NFKC 之后的连字符、横线、减号和软连字符。
fn is_dash(ch: char) -> bool {
    matches!(ch, '-' | SOFT_HYPHEN | '\u{2010}'..='\u{2015}' | '\u{2212}')
}

/// 看不见的格式字符（Unicode Default_Ignorable_Code_Point），软连字符另算。
fn ignorable(ch: char) -> bool {
    matches!(
        ch,
        '\u{034f}'
            | '\u{061c}'
            | '\u{115f}'..='\u{1160}'
            | '\u{17b4}'..='\u{17b5}'
            | '\u{180b}'..='\u{180f}'
            | '\u{200b}'..='\u{200f}'
            | '\u{202a}'..='\u{202e}'
            | '\u{2060}'..='\u{206f}'
            | '\u{3164}'
            | '\u{fe00}'..='\u{fe0f}'
            | '\u{feff}'
            | '\u{ffa0}'
            | '\u{fff0}'..='\u{fff8}'
            | '\u{1bca0}'..='\u{1bca3}'
            | '\u{1d173}'..='\u{1d17a}'
            | '\u{e0000}'..='\u{e0fff}'
    )
}

/// 中日文标点和全角符号（不含全角字母、数字）。
fn wide_punctuation(ch: char) -> bool {
    matches!(
        ch,
        '\u{3000}'..='\u{303f}' | '\u{fe10}'..='\u{fe1f}' | '\u{fe30}'..='\u{fe4f}' | '\u{ff00}'..='\u{ffef}'
    ) && !ch.is_alphanumeric()
}

pub(super) fn is_han(ch: char) -> bool {
    ch.script() == Script::Han
}

fn is_latin_letter(ch: char) -> bool {
    ch.is_alphabetic() && ch.script() == Script::Latin
}

fn simplified(ch: char) -> char {
    static CHARACTERS: OnceLock<Vec<(char, char)>> = OnceLock::new();
    let characters = CHARACTERS.get_or_init(|| {
        let mut entries: Vec<_> = include_str!("data/TSCharacters.txt")
            .lines()
            .map(|line| {
                let mut fields = line.split_whitespace();
                let original = fields.next().unwrap().chars().next().unwrap();
                let simplified = fields.next().unwrap().chars().next().unwrap();
                (original, simplified)
            })
            .collect();
        entries.sort_unstable_by_key(|&(source, _)| source);
        entries
            .iter()
            .map(|&(source, mut target)| {
                // 上游存在逐字转换链，查询和原文应得到同一个简体字。
                while let Ok(index) = entries.binary_search_by_key(&target, |&(key, _)| key) {
                    if entries[index].1 == target {
                        break;
                    }
                    target = entries[index].1;
                }
                (source, target)
            })
            .collect()
    });
    characters
        .binary_search_by_key(&ch, |&(source, _)| source)
        .map_or(ch, |i| characters[i].1)
}

#[cfg(test)]
mod tests {
    use super::mapped_nfkc;
    use unicode_normalization::UnicodeNormalization;

    #[test]
    fn mapped_composition_agrees_with_unicode_nfkc() {
        for text in [
            "ﬃ Ａ Å",
            "a\u{0315}\u{0300}\u{0327}",
            "\u{0315}\u{0300}b",
            "a\u{034f}\u{0301}",
            "ㄱㅏ\u{11a8}",
            "ｶ\u{ff9e}",
            "א\u{05b8}\u{05b4}",
            "\u{0f73}\u{0f75}",
        ] {
            let mapped: String = mapped_nfkc(text).iter().map(|scalar| scalar.ch).collect();
            let expected: String = text.nfkc().collect();
            assert_eq!(mapped, expected, "{text:?}");
        }
    }
}
