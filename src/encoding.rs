//! 文字编码的识别和解码（feature `encoding`；`docs/design.md` §8，`decisions/0008`）。
//!
//! 从 engine-txt v0.2 搬来，engine-txt 的 txt、Markdown、字幕和 engine-excel 的 CSV 共用。顺序：指定 → BOM →
//! 没有 BOM 的 UTF-16 → 二进制判定 → 合法 UTF-8（含 7 位的 ISO-2022-JP、HZ）→ XML 声明的编码 → chardetng 猜。
//! 解完统一换行、去掉开头的 BOM，别的一个字不改。
//!
//! ```
//! use engine_core::encoding::{DetectedBy, decode};
//!
//! let gbk = encoding_rs::GBK.encode("二〇二四年示例市统计，城市，金额").0;
//! let d = decode(&gbk, None).unwrap();
//! assert_eq!(d.text, "二〇二四年示例市统计，城市，金额");
//! assert_eq!((d.encoding.name.as_str(), d.encoding.by), ("GBK", DetectedBy::Guess));
//! ```

use chardetng::{EncodingDetector, Iso2022JpDetection, Utf8Detection};
use encoding_rs::{Encoding as Enc, GBK, ISO_2022_JP, REPLACEMENT, UTF_8, UTF_16BE, UTF_16LE};
use serde::Serialize;
use std::fmt;

/// 认编码用的库，调用方要拿 [`declared`] 交回的编码去解时用同一份。
pub use encoding_rs;

/// 编码和它是怎么认出来的。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Encoding {
    /// encoding_rs 的名字：`UTF-8`、`UTF-16LE`、`UTF-16BE`、`GBK`（按 GB18030 解）、`Big5`、`Shift_JIS`……
    pub name: String,
    pub by: DetectedBy,
    /// 开头有没有 BOM（读出来的文字里已经去掉）。
    pub bom: bool,
    /// 解不出来、换成替换符（U+FFFD）的字数；自己认的编码超过 1% 会报 `UnknownEncoding`，指定的照样交回。
    pub replaced: usize,
}

impl Encoding {
    /// 是不是猜的（没有 BOM、不是合法 UTF-8、文件里也没声明）。
    pub fn guessed(&self) -> bool {
        matches!(self.by, DetectedBy::Pattern | DetectedBy::Guess)
    }

    /// 给人看的写法：「UTF-8」「GBK（猜的）」「Big5（指定）」。
    pub fn label(&self) -> String {
        let how = match self.by {
            DetectedBy::Given => "（指定）",
            DetectedBy::Bom => "（有 BOM）",
            DetectedBy::Valid => "",
            DetectedBy::Declared => "（文件里声明的）",
            DetectedBy::Pattern | DetectedBy::Guess => "（猜的）",
        };
        format!("{}{how}", self.name)
    }
}

/// 编码是怎么认出来的（模块说明里的顺序）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum DetectedBy {
    /// 调用方指定。
    Given,
    /// BOM。
    Bom,
    /// 整份是合法的 UTF-8（或 7 位的 ISO-2022-JP）。
    Valid,
    /// 文件里声明的（XML 开头的 `encoding="…"`）。
    Declared,
    /// 按字节规律认的：没有 BOM 的 UTF-16、HZ。
    Pattern,
    /// chardetng 猜的。
    Guess,
}

/// 原文用的换行。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Newline {
    Lf,
    Crlf,
    Cr,
    Mixed,
}

/// 解不了的原因。各引擎映射成自己的错误种类（engine-txt：`not_text`、`unknown_encoding`、`bad_args`）。
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum DecodeError {
    /// 二进制，不是文字。
    NotText,
    /// 自己认的编码解出来替换符太多或含 NUL。
    UnknownEncoding,
    /// 指定的编码名不认识或不支持。
    BadEncodingName(String),
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DecodeError::NotText => f.write_str("这不是文字文件。"),
            DecodeError::UnknownEncoding => f.write_str("认不出这份文件的编码。"),
            DecodeError::BadEncodingName(name) => {
                let name: String = name.chars().take(40).collect();
                write!(f, "不认识或不支持编码「{name}」。")
            }
        }
    }
}

impl std::error::Error for DecodeError {}

type Error = DecodeError;

/// 看前多少字节判断二进制、UTF-16。
const SNIFF: usize = 16 * 1024;
/// 交给 chardetng 猜的最多字节。
const GUESS_FEED: usize = 1024 * 1024;

/// 解出来的文字。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decoded {
    /// 统一成 `\n`、去掉开头 BOM 的文字。
    pub text: String,
    pub encoding: Encoding,
    /// 原文用的换行；没有换行是 `None`。
    pub newline: Option<Newline>,
}

/// 把字节解成文字。`encoding` 是 WHATWG 的编码名（`gbk`、`big5`、`utf-16le`……），`None` 就自己认。
///
/// 自己认的编码解出来替换符超过 1% 或含 NUL，报 `UnknownEncoding`；指定的照样交回，`replaced` 记着解不出的字数。
pub fn decode(bytes: &[u8], encoding: Option<&str>) -> Result<Decoded, Error> {
    let (raw, encoding) = detect_and_decode(bytes, encoding)?;
    let (text, newline) = unify_newlines(&raw);
    Ok(Decoded {
        text,
        encoding,
        newline,
    })
}

fn detect_and_decode(bytes: &[u8], given: Option<&str>) -> Result<(String, Encoding), Error> {
    // 1. 指定
    if let Some(label) = given.map(str::trim).filter(|l| !l.is_empty()) {
        if is_hz_label(label) {
            return Ok(finish(
                hz(bytes, false).unwrap_or_default(),
                HZ,
                DetectedBy::Given,
                false,
            ));
        }
        let enc = Enc::for_label(label.as_bytes())
            .filter(|e| *e != REPLACEMENT)
            .ok_or_else(|| Error::BadEncodingName(label.to_string()))?;
        if !is_utf16(enc) && binary(bytes) {
            return Err(Error::NotText);
        }
        let (text, bom) = match Enc::for_bom(bytes) {
            Some((b, len)) if b == enc => (enc.decode_without_bom_handling(&bytes[len..]).0, true),
            _ => (enc.decode_without_bom_handling(bytes).0, false),
        };
        return Ok(finish(
            text.into_owned(),
            enc.name(),
            DetectedBy::Given,
            bom,
        ));
    }
    // 2. BOM
    if let Some((enc, len)) = Enc::for_bom(bytes) {
        let text = enc
            .decode_without_bom_handling(&bytes[len..])
            .0
            .into_owned();
        return Ok(finish(text, enc.name(), DetectedBy::Bom, true));
    }
    // 3. 没有 BOM 的 UTF-16（要在二进制判定和 UTF-8 之前：ASCII 多的 UTF-16 满是 0 字节，也是合法的 UTF-8）
    if let Some(enc) = utf16_without_bom(bytes) {
        let text = enc.decode_without_bom_handling(bytes).0.into_owned();
        let e = finish(text, enc.name(), DetectedBy::Pattern, false);
        if acceptable(&e.0, &e.1) {
            return Ok(e);
        }
    }
    // 4. 二进制
    if binary(bytes) {
        return Err(Error::NotText);
    }
    // 5. 合法 UTF-8；只有 7 位字节、带 ESC 的可能是 ISO-2022-JP
    if let Ok(s) = std::str::from_utf8(bytes) {
        if s.contains('\u{1b}') && iso_2022_jp(bytes) {
            let text = ISO_2022_JP
                .decode_without_bom_handling(bytes)
                .0
                .into_owned();
            return Ok(finish(text, ISO_2022_JP.name(), DetectedBy::Valid, false));
        }
        // 只有 7 位字节、带 `~{` 的可能是 HZ
        if bytes.is_ascii() && s.contains("~{") {
            if let Some(text) = hz(bytes, true) {
                return Ok(finish(text, HZ, DetectedBy::Pattern, false));
            }
        }
        return Ok(finish(
            s.to_string(),
            UTF_8.name(),
            DetectedBy::Valid,
            false,
        ));
    }
    // 6. XML 开头声明的编码
    if let Some(enc) = xml_declared(bytes) {
        let text = enc.decode_without_bom_handling(bytes).0.into_owned();
        let e = finish(text, enc.name(), DetectedBy::Declared, false);
        if acceptable(&e.0, &e.1) {
            return Ok(e);
        }
    }
    // 7. 猜
    let enc = guess(bytes);
    let text = enc.decode_without_bom_handling(bytes).0.into_owned();
    let e = finish(text, enc.name(), DetectedBy::Guess, false);
    if acceptable(&e.0, &e.1) {
        Ok(e)
    } else {
        Err(Error::UnknownEncoding)
    }
}

fn finish(text: String, name: &str, by: DetectedBy, bom: bool) -> (String, Encoding) {
    let replaced = text.chars().filter(|c| *c == '\u{fffd}').count();
    (
        text,
        Encoding {
            name: name.to_string(),
            by,
            bom,
            replaced,
        },
    )
}

/// HZ 的名字（encoding_rs 不支持，本引擎自己解）。
const HZ: &str = "HZ-GB-2312";

fn is_hz_label(label: &str) -> bool {
    ["hz-gb-2312", "hz", "hz-gb2312"]
        .iter()
        .any(|l| label.eq_ignore_ascii_case(l))
}

/// HZ（RFC 1843）：7 位字节，`~{` 进 GB2312、`~}` 回 ASCII，`~~` 是 `~`，行尾的 `~` 是续行。
///
/// `strict` 时格式不对（有 8 位字节、GB 段里的字节不在 0x21–0x7E、解不出、一个 GB 段都没有）交 `None`；
/// 不严格时（调用方指定了 HZ）解不出的换成替换符。
fn hz(bytes: &[u8], strict: bool) -> Option<String> {
    let mut out = String::with_capacity(bytes.len());
    let mut run: Vec<u8> = vec![];
    let mut gb = false;
    let mut runs = 0;
    let flush = |run: &mut Vec<u8>, out: &mut String| -> bool {
        if run.len() % 2 == 1 && strict {
            return false;
        }
        let ok = match GBK.decode_without_bom_handling_and_without_replacement(run) {
            Some(s) => {
                out.push_str(&s);
                true
            }
            None => {
                out.push_str(&GBK.decode_without_bom_handling(run).0);
                !strict
            }
        };
        run.clear();
        ok
    };
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b >= 0x80 {
            if strict {
                return None;
            }
            out.push('\u{fffd}');
            i += 1;
            continue;
        }
        if gb {
            // `~}` 结束 GB 段；换行前没关上的，严格时不算 HZ，不严格时也收
            if (b == b'\n' || b == b'\r') && strict {
                return None;
            }
            if (b == b'~' && bytes.get(i + 1) == Some(&b'}')) || b == b'\n' || b == b'\r' {
                if !flush(&mut run, &mut out) {
                    return None;
                }
                gb = false;
                if b == b'~' {
                    i += 2;
                } else {
                    out.push(b as char);
                    i += 1;
                }
                continue;
            }
            if !(0x21..=0x7E).contains(&b) && strict {
                return None;
            }
            run.push(b | 0x80);
            i += 1;
            continue;
        }
        if b == b'~' {
            match bytes.get(i + 1) {
                Some(b'{') => {
                    gb = true;
                    runs += 1;
                    i += 2;
                    continue;
                }
                Some(b'~') => {
                    out.push('~');
                    i += 2;
                    continue;
                }
                Some(b'\n') => {
                    i += 2;
                    continue;
                }
                _ => {}
            }
        }
        out.push(b as char);
        i += 1;
    }
    if gb && (strict || !flush(&mut run, &mut out)) {
        return None;
    }
    (runs > 0 || !strict).then_some(out)
}

/// 自己认出来的编码能不能用：替换符不超过 1%，没有 NUL。
fn acceptable(text: &str, enc: &Encoding) -> bool {
    let chars = text.chars().count().max(1);
    enc.replaced * 100 <= chars && !text.contains('\0')
}

fn is_utf16(enc: &'static Enc) -> bool {
    enc == UTF_16LE || enc == UTF_16BE
}

/// 前 16 KB 里 NUL 超过 1% 就是二进制。
fn binary(bytes: &[u8]) -> bool {
    let head = &bytes[..bytes.len().min(SNIFF)];
    let nul = head.iter().filter(|b| **b == 0).count();
    nul * 100 > head.len()
}

/// 没有 BOM 的 UTF-16：按两种字节序各读一遍，看「锚点」多不多。
///
/// 锚点是高字节为 0 的 ASCII（换行、空格、英文）和最常见的中文标点（，。、：？！；）。
/// GBK、Big5、Shift_JIS、Latin 的文字按 UTF-16 读时，这些几乎拼不出来（要 0、0x01、0x02、0xFF 这样的字节），
/// 而它们会变成一堆「像汉字」的码位，所以只看「像不像汉字」分不出来。
fn utf16_without_bom(bytes: &[u8]) -> Option<&'static Enc> {
    if bytes.len() < 4 {
        return None;
    }
    let head = &bytes[..bytes.len().min(SNIFF) & !1];
    let le = utf16_score(head, true);
    let be = utf16_score(head, false);
    match (le.passes(), be.passes()) {
        (true, false) => Some(UTF_16LE),
        (false, true) => Some(UTF_16BE),
        (true, true) if le.anchors != be.anchors => Some(if le.anchors > be.anchors {
            UTF_16LE
        } else {
            UTF_16BE
        }),
        _ => None,
    }
}

#[derive(Debug, Default)]
struct Utf16Score {
    units: usize,
    good: usize,
    anchors: usize,
}

impl Utf16Score {
    fn passes(&self) -> bool {
        self.units >= 2
            && self.good * 10 >= self.units * 9
            && self.anchors * 50 >= self.units
            && self.anchors >= 1
    }
}

fn utf16_score(head: &[u8], le: bool) -> Utf16Score {
    let units: Vec<u16> = head
        .chunks_exact(2)
        .map(|c| {
            if le {
                u16::from_le_bytes([c[0], c[1]])
            } else {
                u16::from_be_bytes([c[0], c[1]])
            }
        })
        .collect();
    let mut s = Utf16Score {
        units: units.len(),
        ..Utf16Score::default()
    };
    let mut i = 0;
    while i < units.len() {
        let u = units[i];
        if (0xD800..0xDC00).contains(&u)
            && units
                .get(i + 1)
                .is_some_and(|n| (0xDC00..0xE000).contains(n))
        {
            s.good += 2; // 合法的代理对（CJK 扩展 B 等）
            i += 2;
            continue;
        }
        if utf16_good(u) {
            s.good += 1;
        }
        if utf16_anchor(u) {
            s.anchors += 1;
        }
        i += 1;
    }
    s
}

/// 文字里常见的码位：ASCII、Latin-1、希腊、西里尔、标点、中日韩、全角。
fn utf16_good(u: u16) -> bool {
    matches!(u,
        0x09 | 0x0A | 0x0D | 0x20..=0x7E
        | 0xA0..=0x024F
        | 0x0370..=0x04FF
        | 0x2010..=0x205E
        | 0x3000..=0x30FF
        | 0x3400..=0x4DBF
        | 0x4E00..=0x9FFF
        | 0xAC00..=0xD7A3
        | 0xFF00..=0xFFEF)
}

/// 锚点：高字节为 0 的 ASCII 和最常见的中文标点。
fn utf16_anchor(u: u16) -> bool {
    matches!(
        u,
        0x09 | 0x0A | 0x0D | 0x20
            ..=0x7E | 0x3001 | 0x3002 | 0xFF01 | 0xFF0C | 0xFF1A | 0xFF1B | 0xFF1F
    )
}

/// 7 位字节、带 ESC 序列的文字，chardetng 认成 ISO-2022-JP 才算。
fn iso_2022_jp(bytes: &[u8]) -> bool {
    let mut det = EncodingDetector::new(Iso2022JpDetection::Allow);
    let feed = &bytes[..bytes.len().min(GUESS_FEED)];
    det.feed(feed, feed.len() == bytes.len());
    det.guess(None, Utf8Detection::Allow) == ISO_2022_JP
}

/// 文件里声明的编码：XML 开头的 `encoding="…"`，或 HTML 的 `<meta charset="…">` / `content="text/html; charset=…"`（前 1 KB）。
/// UTF-8、UTF-16 不算（调用方已经认过）。engine-txt 读 EPUB 里不是 UTF-8 的正文用它。
pub fn declared(bytes: &[u8]) -> Option<&'static Enc> {
    xml_declared(bytes).or_else(|| {
        let head = String::from_utf8_lossy(&bytes[..bytes.len().min(1024)]).to_ascii_lowercase();
        let at = head.find("charset")?;
        let rest = head[at + "charset".len()..]
            .trim_start()
            .strip_prefix('=')?
            .trim_start()
            .trim_start_matches(['"', '\'']);
        let label: String = rest
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | ':' | '.'))
            .collect();
        Enc::for_label(label.as_bytes())
            .filter(|e| *e != REPLACEMENT && *e != UTF_8 && !is_utf16(e))
    })
}

/// `<?xml version="1.0" encoding="gb2312"?>` 里的编码。
fn xml_declared(bytes: &[u8]) -> Option<&'static Enc> {
    let head = &bytes[..bytes.len().min(512)];
    let start = head.iter().position(|b| !b.is_ascii_whitespace())?;
    let head = &head[start..];
    if !head.starts_with(b"<?xml") {
        return None;
    }
    let end = head.windows(2).position(|w| w == b"?>")?;
    let decl = String::from_utf8_lossy(&head[..end]).to_ascii_lowercase();
    let at = decl.find("encoding")?;
    let rest = decl[at + "encoding".len()..]
        .trim_start()
        .strip_prefix('=')?
        .trim_start();
    let quote = rest.chars().next().filter(|c| *c == '"' || *c == '\'')?;
    let label = rest[1..].split(quote).next()?;
    Enc::for_label(label.trim().as_bytes())
        .filter(|e| *e != REPLACEMENT && *e != UTF_8 && !is_utf16(e))
}

fn guess(bytes: &[u8]) -> &'static Enc {
    let mut det = EncodingDetector::new(Iso2022JpDetection::Deny);
    let feed = &bytes[..bytes.len().min(GUESS_FEED)];
    det.feed(feed, feed.len() == bytes.len());
    det.guess(None, Utf8Detection::Deny)
}

/// `\r\n`、`\r` 统一成 `\n`，去掉开头的 BOM；记下原文用的换行。
fn unify_newlines(s: &str) -> (String, Option<Newline>) {
    let s = s.strip_prefix('\u{feff}').unwrap_or(s);
    let (mut lf, mut crlf, mut cr) = (0usize, 0usize, 0usize);
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\r' if chars.peek() == Some(&'\n') => {
                chars.next();
                crlf += 1;
                out.push('\n');
            }
            '\r' => {
                cr += 1;
                out.push('\n');
            }
            '\n' => {
                lf += 1;
                out.push('\n');
            }
            _ => out.push(c),
        }
    }
    let newline = match (lf > 0, crlf > 0, cr > 0) {
        (false, false, false) => None,
        (true, false, false) => Some(Newline::Lf),
        (false, true, false) => Some(Newline::Crlf),
        (false, false, true) => Some(Newline::Cr),
        _ => Some(Newline::Mixed),
    };
    (out, newline)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utf16(s: &str, le: bool) -> Vec<u8> {
        s.encode_utf16()
            .flat_map(|u| if le { u.to_le_bytes() } else { u.to_be_bytes() })
            .collect()
    }

    #[test]
    fn utf8_and_ascii() {
        let d = decode("你好\nworld\n".as_bytes(), None).unwrap();
        assert_eq!(d.text, "你好\nworld\n");
        assert_eq!(
            (d.encoding.name.as_str(), d.encoding.by),
            ("UTF-8", DetectedBy::Valid)
        );
        assert_eq!(d.newline, Some(Newline::Lf));
        let empty = decode(b"", None).unwrap();
        assert_eq!((empty.text.as_str(), empty.newline), ("", None));
    }

    #[test]
    fn given_encoding_wins_and_bad_names_are_reported() {
        let gbk = encoding_rs::GBK.encode("中文").0.into_owned();
        let d = decode(&gbk, Some(" GBK ")).unwrap();
        assert_eq!(
            (d.text.as_str(), d.encoding.by),
            ("中文", DetectedBy::Given)
        );
        // 指定错了也照样交回，记下替换符。
        let d = decode("中文".as_bytes(), Some("big5")).unwrap();
        assert!(d.encoding.replaced > 0 || d.text != "中文");
        assert_eq!(
            decode(b"x", Some("klingon")),
            Err(Error::BadEncodingName("klingon".into()))
        );
        assert_eq!(
            decode(b"x", Some("iso-2022-kr")),
            Err(Error::BadEncodingName("iso-2022-kr".into()))
        );
        // 指定的编码和 BOM 一致时去掉 BOM。
        let d = decode(b"\xEF\xBB\xBFabc", Some("utf-8")).unwrap();
        assert_eq!((d.text.as_str(), d.encoding.bom), ("abc", true));
    }

    #[test]
    fn utf16_without_bom_both_kinds() {
        for le in [true, false] {
            // 纯中文、几乎没有 0 字节
            let d = decode(
                &utf16(
                    "我没有埋怨，磋砣的只是一些时间。今觀俗士之論也，以族舉德。",
                    le,
                ),
                None,
            )
            .unwrap();
            assert_eq!(d.encoding.by, DetectedBy::Pattern);
            assert_eq!(d.encoding.name, if le { "UTF-16LE" } else { "UTF-16BE" });
            assert_eq!(
                d.text,
                "我没有埋怨，磋砣的只是一些时间。今觀俗士之論也，以族舉德。"
            );
            // 英文、满是 0 字节
            let d = decode(&utf16("Plain English text.\nSecond line.\n", le), None).unwrap();
            assert_eq!(d.encoding.name, if le { "UTF-16LE" } else { "UTF-16BE" });
        }
    }

    #[test]
    fn legacy_text_is_not_taken_for_utf16() {
        for (enc, s) in [
            (
                encoding_rs::GBK,
                "镇上的人说，今年的雨来得早。桥下有人在洗衣服。",
            ),
            (
                encoding_rs::BIG5,
                "山城的雨總是下個不停，石階上長滿了青苔。",
            ),
            (
                encoding_rs::WINDOWS_1252,
                "Le café de la place, à midi. Prix : 1000 € ou 2000 €.",
            ),
            (
                encoding_rs::SHIFT_JIS,
                "朝早く、港には小さな漁船が並んでいた。",
            ),
        ] {
            let bytes = enc.encode(s).0.into_owned();
            assert_eq!(
                utf16_without_bom(&bytes),
                None,
                "{} 被认成了 UTF-16",
                enc.name()
            );
        }
    }

    #[test]
    fn binary_is_not_text() {
        let mut b = b"\x89PNG\r\n\x1a\n".to_vec();
        b.extend(std::iter::repeat_n(0u8, 100));
        assert_eq!(decode(&b, None), Err(Error::NotText));
    }

    #[test]
    fn xml_declaration() {
        let s = "<?xml version='1.0' encoding='GB2312'?><a>镇上的人</a>";
        let d = decode(&encoding_rs::GBK.encode(s).0, None).unwrap();
        assert_eq!(
            (d.encoding.name.as_str(), d.encoding.by),
            ("GBK", DetectedBy::Declared)
        );
        assert_eq!(d.text, s);
    }

    #[test]
    fn iso_2022_jp_is_recognised() {
        let s = "朝早く、港には小さな漁船が並んでいた。";
        let bytes = ISO_2022_JP.encode(s).0.into_owned();
        assert!(std::str::from_utf8(&bytes).is_ok());
        let d = decode(&bytes, None).unwrap();
        assert_eq!(
            (d.encoding.name.as_str(), d.text.as_str()),
            ("ISO-2022-JP", s)
        );
    }

    #[test]
    fn hz_is_recognised() {
        // 「中文」的 GB2312 是 D6D0 CEC4，HZ 里去掉高位：~{VPND~}
        let d = decode(b"HZ: ~{VPND~} and ~~ tilde.\n", None).unwrap();
        assert_eq!(
            (d.encoding.name.as_str(), d.encoding.by),
            ("HZ-GB-2312", DetectedBy::Pattern)
        );
        assert_eq!(d.text, "HZ: 中文 and ~ tilde.\n");
        // 续行
        assert_eq!(decode(b"~{VP~}~\n~{ND~}", None).unwrap().text, "中文");
        // 不成样子的 `~{` 不当 HZ
        for plain in [&b"a~{b}"[..], b"x ~{ \xff", b"~{V~}"] {
            if let Ok(d) = decode(plain, None) {
                assert_ne!(d.encoding.name, "HZ-GB-2312", "{plain:?}");
            }
        }
        // 指定 HZ
        let d = decode(b"~{VPND~}", Some("HZ-GB-2312")).unwrap();
        assert_eq!(
            (d.text.as_str(), d.encoding.by),
            ("中文", DetectedBy::Given)
        );
    }

    #[test]
    fn labels_and_errors_for_people() {
        let d = decode(&encoding_rs::BIG5.encode("山城的雨總是下個不停").0, None).unwrap();
        assert!(d.encoding.guessed());
        assert_eq!(d.encoding.label(), "Big5（猜的）");
        assert_eq!(decode(b"abc", None).unwrap().encoding.label(), "UTF-8");
        assert_eq!(
            DecodeError::BadEncodingName("x".repeat(100)).to_string(),
            format!("不认识或不支持编码「{}」。", "x".repeat(40))
        );
        assert_eq!(
            serde_json::to_value(&d.encoding).unwrap(),
            serde_json::json!({"name": "Big5", "by": "guess", "bom": false, "replaced": 0})
        );
    }

    #[test]
    fn newlines_are_unified_and_recorded() {
        assert_eq!(
            unify_newlines("a\r\nb\r\n"),
            ("a\nb\n".into(), Some(Newline::Crlf))
        );
        assert_eq!(unify_newlines("a\rb"), ("a\nb".into(), Some(Newline::Cr)));
        assert_eq!(
            unify_newlines("a\r\nb\nc\r"),
            ("a\nb\nc\n".into(), Some(Newline::Mixed))
        );
        assert_eq!(unify_newlines("\u{feff}abc"), ("abc".into(), None));
    }
}
