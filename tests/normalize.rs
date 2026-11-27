use engine_core::find::normalize;

#[test]
fn folds_search_variants_without_merging_unrelated_text() {
    for (source, expected) in [
        ("oﬃce", "office"),
        ("Ｆｕｌｌ　１２３", "full 123"),
        ("infor-\r\n  mation", "information"),
        // 真实论文：复合词在连字符处换行、软连字符、换行被取字变成空格。
        (
            "self-\nattention self-attention",
            "selfattention selfattention",
        ),
        ("Woman-\nStats", "womanstats"),
        (
            "arti\u{ad}cle dis\u{ad}trib\u{ad}uted",
            "article distributed",
        ),
        // 软连字符后跟空格是词间空格（Duke 出版社的期刊，2026-11-27 engine-pdf 审核）。
        ("all\u{ad} obser\u{ad}va\u{ad}tions", "all observations"),
        (
            "between-\u{ad} and within-\u{ad}variation",
            "between- and withinvariation",
        ),
        (
            "cross-\u{ad}sectional t-\n\u{ad}tests",
            "crosssectional ttests",
        ),
        ("zero\u{200b}width\u{feff}", "zerowidth"),
        ("認\n 知", "认知"),
        // 知网论文：全角标点旁的换行、中西文之间的空格。
        ("模型，\n认知；\n李燕", "模型,认知;李燕"),
        ("因素［４－６］\n。", "因素[4-6]。"),
        ("使用 Python 编程", "使用python编程"),
        (" \tA\u{2002} \tB  ", "a b"),
        ("舊 體、薴 苧 苎", "旧体、苎苎苎"),
        ("Theory", "theory"),
        ("‘a’ and “B”", "'a' and \"b\""),
        ("left — right − 3, 1–5", "left - right - 3, 1-5"),
        ("left—right", "leftright"),
        ("Café e\u{0301}\u{0323}", "cafe e"),
    ] {
        assert_eq!(normalize(source).text, expected, "{source:?}");
    }

    for (left, right) in [
        ("oﬃce", "ofice"),
        ("１２３", "124"),
        ("infor- mation", "information"),
        ("pre-\n\nfix", "prefix"),
        ("word —\nwrap", "wordwrap"),
        ("12–\n15", "1215"),
        ("a\nb", "ab"),
        ("a  b", "ab"),
        ("Ｌａｗ　ａｎｄ", "lawand"),
        ("認知", "感知"),
        ("Theory", "Theorys"),
        ("a′", "a'"),
        ("عَلَم", "علم"),
        ("का", "क"),
        ("ά", "α"),
    ] {
        assert_ne!(normalize(left).text, normalize(right).text, "{left:?}");
    }
}

#[test]
fn maps_expansion_recomposition_and_removed_text_to_original_bytes() {
    let text = "前ﬃ E\u{0301}\u{0323}\u{2002}\t認\n知 infor-\n  mation ㄱㅏ 后 dis\u{ad}trib";
    let normal = normalize(text);
    assert_eq!(normal.text, "前ffi e认知information 가后distrib");

    for (needle, expected) in [
        ("前", "前"),
        ("ffi", "ﬃ"),
        ("i e", "ﬃ E\u{0301}\u{0323}"),
        ("e认", "E\u{0301}\u{0323}\u{2002}\t認"),
        ("认知", "認\n知"),
        ("formation", "for-\n  mation"),
        ("가", "ㄱㅏ"),
        ("가后", "ㄱㅏ 后"),
        ("str", "s\u{ad}tr"),
    ] {
        let start = normal.text.find(needle).unwrap();
        let round_trip = normal.original_range(start..start + needle.len()).unwrap();
        assert_eq!(&text[round_trip], expected, "{needle:?}");
    }

    assert_eq!(normal.map.len(), normal.text.chars().count());
    for (segment, (start, _)) in normal.map.iter().zip(normal.text.char_indices()) {
        assert_eq!(segment.normalized_start, start);
        assert!(text.is_char_boundary(segment.original.start));
        assert!(text.is_char_boundary(segment.original.end));
    }
    assert_eq!(normal.original_range(1..3), None);
    assert_eq!(normal.original_range(0..0), None);
    assert_eq!(normal.original_range(0..normal.text.len() + 1), None);
}

#[test]
fn maps_reordered_non_latin_marks_to_their_own_source() {
    let original = "א\u{05b8}\u{05b4}";
    let normal = normalize(original);
    assert_eq!(normal.text, "א\u{05b4}\u{05b8}");
    let marks = normal.original_range("א".len()..normal.text.len()).unwrap();
    assert_eq!(&original[marks], "\u{05b8}\u{05b4}");
    let start = normal.text.find('\u{05b4}').unwrap();
    let mark = normal
        .original_range(start..start + '\u{05b4}'.len_utf8())
        .unwrap();
    assert_eq!(&original[mark], "\u{05b4}");
}
