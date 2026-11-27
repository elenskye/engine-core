use engine_core::{
    cancel::Cancel,
    find::{Finder, Found, Match, Query, Source},
};
use serde_json::json;
use std::ops::Range;

#[test]
fn later_phrase_wins_globally_and_total_exceeds_limit() {
    let query = Query {
        max_hits: 1,
        context: 1,
        ..Query::new("blue cat")
    };
    let mut finder = Finder::new(&query);
    finder.scan("cat and blue", Source::Text, |range| (1, range));
    let text = "前BLUE CAT后 Blue Cat!";
    finder.scan(text, Source::Ocr, |range| (2, range));
    finder.scan("blue cat", Source::Text, |_| panic!("已到收集上限"));
    finder.scan("cat and blue", Source::Text, |_| panic!("已有整句命中"));
    let found = finder.finish();
    assert_eq!(
        (found.hits.len(), found.total, found.truncated),
        (1, 3, true)
    );
    let hit = &found.hits[0];
    assert_eq!(hit.location.0, 2);
    assert_eq!(&text[hit.location.1.clone()], "BLUE CAT");
    assert_eq!(
        (&*hit.before, &*hit.matched, &*hit.after),
        ("前", "BLUE CAT", "后")
    );
    assert_eq!((hit.how, hit.source), (Match::Phrase, Source::Ocr));
}

#[test]
fn word_fallback_respects_unit_and_non_han_word_boundaries() {
    let mut finder = Finder::new(&Query::new("blue cat 42"));
    for (index, text) in [
        "cat, 42, blue",
        "blue cat 142",
        "blue bobcat 42",
        "blue cat",
        "42",
    ]
    .into_iter()
    .enumerate()
    {
        finder.scan(text, Source::Text, |range| (index, range));
    }
    let found = finder.finish();
    // 只有第一个单元三个词都在；每个词各算一处，按原文顺序。
    assert_eq!((found.total, found.truncated), (3, false));
    assert_eq!(matched(&found), ["cat", "42", "blue"]);
    assert!(found.hits.iter().all(|hit| hit.location.0 == 0));
    assert!(found.hits.iter().all(|hit| hit.how == Match::Words));
    assert_eq!(found.hits[2].location.1, 9..13);
}

fn matched<L>(found: &Found<L>) -> Vec<&str> {
    found.hits.iter().map(|hit| hit.matched.as_str()).collect()
}

#[test]
fn han_terms_allow_substrings_and_choose_the_shortest_range() {
    let mut finder = Finder::new(&Query::new("看 哈哈"));
    finder.scan("哈哈哈看", Source::Text, |range| range);
    let found = finder.finish();
    assert_eq!(matched(&found), ["哈哈", "看"]);
    assert_eq!(found.hits[0].location, "哈".len().."哈哈哈".len());
    assert_eq!(found.hits[0].how, Match::Words);

    let mut finder = Finder::new(&Query::new("認知、模型"));
    finder.scan("这是模型的認\n知能力。", Source::Text, |range| {
        range
    });
    assert_eq!(matched(&finder.finish()), ["模型", "認\n知"]);
}

#[test]
fn repeats_require_non_overlapping_occurrences_and_choose_the_nearest_ones() {
    let mut finder = Finder::new(&Query::new("alpha beta alpha"));
    finder.scan("alpha 与 beta", Source::Text, |_| panic!("只有一次 alpha"));
    let text = "alpha 一大段内容 alpha，beta alpha";
    finder.scan(text, Source::Ocr, |range| range);
    let found = finder.finish();
    assert_eq!(found.total, 3);
    assert_eq!(matched(&found), ["alpha", "beta", "alpha"]);
    assert_eq!(found.hits[0].location.start, text.find("alpha，").unwrap());
    assert_eq!(found.hits[0].source, Source::Ocr);

    let mut finder = Finder::new(&Query::new("看 哈哈 哈哈"));
    finder.scan("哈哈哈看", Source::Ocr, |_| {
        panic!("重叠字形不能计两次")
    });
    finder.scan("哈哈哈哈看", Source::Ocr, |range| range);
    let found = finder.finish();
    assert_eq!(matched(&found), ["哈哈", "哈哈", "看"]);
    assert_eq!(found.hits[1].location, "哈哈".len().."哈哈哈哈".len());
}

#[test]
fn shortest_uses_original_character_length_and_breaks_ties_by_order() {
    let mut finder = Finder::new(&Query::new("cat dog"));
    let first = "dog 中文 cat 这里是分隔多个候选的很长一段内容 dog abcd cat";
    let second = "dog, cat; ..................... dog! cat";
    finder.scan(first, Source::Text, |range| (1, range));
    finder.scan(second, Source::Text, |range| (2, range));
    let found = finder.finish();
    let at = |i: usize| (found.hits[i].location.0, found.hits[i].location.1.start);
    // 第一单元按原文字数「dog 中文 cat」最短；第二单元两组一样长，取靠前的。
    assert_eq!(matched(&found), ["dog", "cat", "dog", "cat"]);
    assert_eq!(at(1), (1, first.find("cat").unwrap()));
    assert_eq!(at(3), (2, second.find("cat").unwrap()));
}

#[test]
fn phrase_expansions_and_overlaps_respect_physical_occurrences() {
    let mut finder = Finder::new(&Query::new("f"));
    finder.scan("前ﬀ後", Source::Ocr, |range| range);
    let found = finder.finish();
    assert_eq!(found.total, 1);
    assert_eq!(found.hits[0].matched, "ﬀ");

    let mut finder = Finder::new(&Query::new("ff"));
    finder.scan("fﬀff", Source::Text, |range| range);
    let found = finder.finish();
    assert_eq!(found.total, 2);
    assert_eq!(
        found
            .hits
            .iter()
            .map(|hit| hit.matched.as_str())
            .collect::<Vec<_>>(),
        ["fﬀ", "ff"]
    );

    let mut finder = Finder::new(&Query::new("哈哈"));
    finder.scan("哈哈哈", Source::Text, |range| range);
    let found = finder.finish();
    assert_eq!(found.total, 1);
    assert_eq!(found.hits[0].location, 0.."哈哈".len());
}

#[test]
fn empty_query_zero_limit_and_serde_behavior() {
    // 参数名和 `<短名>_find` 一致，别的参数忽略。
    let query: Query = serde_json::from_value(json!({"query": " \t　", "file": "a.pdf"})).unwrap();
    assert_eq!((query.max_hits, query.context), (20, 40));
    let mut finder: Finder<()> = Finder::new(&query);
    finder.scan("test", Source::Text, |_| panic!("空查询"));
    assert_eq!(
        finder.finish(),
        Found {
            hits: Vec::new(),
            total: 0,
            truncated: false
        }
    );

    let mut finder: Finder<()> = Finder::new(&Query {
        max_hits: 0,
        ..Query::new("blue cat")
    });
    finder.scan("cat, blue", Source::Text, |_| panic!("只计数"));
    finder.scan("blue cat, blue cat", Source::Ocr, |_| panic!("只计数"));
    assert_eq!(
        finder.finish(),
        Found {
            hits: Vec::new(),
            total: 2,
            truncated: true
        }
    );

    let mut finder: Finder<Range<usize>> = Finder::new(&Query {
        context: 0,
        ..Query::new("cafe")
    });
    finder.scan("前Café后", Source::Ocr, |range| range);
    let found = finder.finish();
    let wire = serde_json::to_value(&found).unwrap();
    assert_eq!(wire["hits"][0]["how"], "phrase");
    assert_eq!(wire["hits"][0]["source"], "ocr");
    assert_eq!(wire["hits"][0]["before"], "");
    assert_eq!(wire["hits"][0]["after"], "");
    assert_eq!(
        serde_json::from_value::<Found<Range<usize>>>(wire).unwrap(),
        found
    );
}

#[test]
fn matches_actual_pdf_excerpts_and_returns_original_offsets() {
    let fixtures = [
        (
            include_str!("fixtures/find/mixed-page-1.txt"),
            "４２ LAMPS",
            &["42 lamps"][..],
            Match::Phrase,
        ),
        (
            include_str!("fixtures/find/attention-abstract.txt"),
            "mechanisms Transformer",
            &["Transformer", "mechanisms"],
            Match::Words,
        ),
        (
            include_str!("fixtures/find/attention-abstract.txt"),
            "Transformer, based solely",
            &["Transformer,\nbased solely"],
            Match::Phrase,
        ),
    ];
    let cancel = Cancel::new();
    for (text, query, expected, how) in fixtures {
        let mut finder = Finder::new(&Query::new(query));
        cancel.check().unwrap();
        finder.scan(text, Source::Text, |range| range);
        let found = finder.finish();
        assert_eq!(matched(&found), expected);
        for hit in &found.hits {
            assert_eq!(&text[hit.location.clone()], hit.matched);
            assert_eq!(hit.how, how);
        }
    }
}
