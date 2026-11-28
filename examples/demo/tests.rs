use super::demo;
use engine_core::{
    Cancel,
    call::{
        Approve, AttachmentContent, Ctx, Files, Keys, NamedTempFile, Prices, Quote, ToolError,
        Zeroizing,
    },
    cli::{LocalHost, run_with},
    overrides::Override,
    spend::Ledger,
    tool::{Confirm, ToolSpec},
};
use serde_json::{Value, json};
use std::{fs, io::Write};

/// App 一侧的宿主：同意确认，交一个模拟密钥。
struct Host;

impl Approve for Host {
    fn ask(&self, _: &ToolSpec, _: Option<&Quote>) -> bool {
        true
    }
}

impl Files for Host {
    fn read(&self, path: &str) -> Result<Vec<u8>, ToolError> {
        fs::read(path).map_err(|_| ToolError::of("file"))
    }
}

impl Keys for Host {
    fn get(&self, _: &str) -> Option<Zeroizing<String>> {
        Some(Zeroizing::new("demo".into()))
    }
}

#[test]
fn all_manifest_examples_run() {
    let ctx = Ctx {
        cancel: &Cancel::new(),
        keys: &Host,
        files: &Host,
        approve: &Host,
        prices: &Prices::default(),
        progress: &|_| {},
    };
    demo().check_examples(&ctx).unwrap();
}

#[test]
fn local_host_runs_free_examples_and_errors_come_from_the_manifest() {
    let demo = demo()
        .with_overrides(&serde_json::from_value(json!({"demo_meter": {"disabled": true}})).unwrap())
        .unwrap();
    let host = LocalHost { yes: true };
    let cancel = Cancel::new();
    demo.check_examples(&host.ctx(&cancel)).unwrap();

    let error = demo
        .call(
            "demo_read",
            &json!({"file": "examples/demo/missing.txt"}),
            &host.ctx(&cancel),
        )
        .unwrap_err();
    assert_eq!(
        (&*error.kind, &*error.message, error.retry),
        ("file", "文件读不了。", false)
    );
    let mut binary = NamedTempFile::new().unwrap();
    binary.write_all(&[0xff, 0xfe, 0x00]).unwrap();
    let error = demo
        .call(
            "demo_find",
            &json!({"file": binary.path().to_str().unwrap(), "query": "x"}),
            &host.ctx(&cancel),
        )
        .unwrap_err();
    assert_eq!(
        (&*error.kind, &*error.message, &*error.hint),
        ("not_text", "文件不是 UTF-8 文本。", "请换一份 UTF-8 文本。")
    );
}

#[test]
fn temporary_attachments_are_cleaned_up_when_released() {
    let host = LocalHost::default();
    let cancel = Cancel::new();
    let result = demo()
        .call(
            "demo_read",
            &json!({"file": "examples/demo/sample.txt", "attachment": "temp_file"}),
            &host.ctx(&cancel),
        )
        .unwrap();
    let AttachmentContent::TempFile(file) = &result.attachments[0].content else {
        panic!("预期临时文件附件");
    };
    let path = file.path().to_owned();
    assert_eq!(fs::read(&path).unwrap(), include_bytes!("sample.txt"));
    drop(result);
    assert!(!path.exists());
}

#[test]
fn find_returns_original_text_and_locations() {
    let host = LocalHost::default();
    let cancel = Cancel::new();
    let output = demo()
        .call(
            "demo_find",
            &json!({"file": "examples/demo/sample.txt", "query": "self-attention", "context": 5}),
            &host.ctx(&cancel),
        )
        .unwrap();
    let hit = &output.data["hits"][0];
    assert_eq!(hit["matched"], "self-\nattention");
    assert_eq!(
        (&hit["before"], &hit["after"]),
        (&json!("uses "), &json!(" in E"))
    );
    assert_eq!(hit["location"], json!({"paragraph": 2, "line": 5}));
    assert_eq!(
        (&hit["how"], &hit["source"]),
        (&json!("phrase"), &json!("text"))
    );
    assert!(output.text.starts_with("找到 1 处："));
}

/// 跑一次命令行，交回退出码、标准输出和标准错误（各一行 JSON，没有就是 Null）。
fn cli(args: &[&str]) -> (u8, Value, Value) {
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let code = run_with(&demo(), args, &mut out, &mut err);
    let parse = |bytes: &[u8]| serde_json::from_slice(bytes).unwrap_or(Value::Null);
    (code, parse(&out), parse(&err))
}

#[test]
fn cli_lists_describes_and_calls() {
    let (code, list, _) = cli(&["tools"]);
    assert_eq!(code, 0);
    assert_eq!(list.as_array().unwrap().len(), 4);
    assert_eq!(
        list[3],
        json!({"name": "demo_meter", "title": "模拟按项计费",
            "cost": {"kind": "metered", "service": "demo", "unit": "项", "usd_per_unit": 0.01},
            "confirm": {"kind": "always"}})
    );
    let (code, spec, _) = cli(&["describe", "demo_find"]);
    assert_eq!((code, &spec["name"]), (0, &json!("demo_find")));

    let (code, result, _) = cli(&["call", "demo_echo", r#"{"text":"你好","style":"upper"}"#]);
    assert_eq!(code, 0);
    assert_eq!(
        result,
        json!({"text": "你好", "data": {"text": "你好"}, "usage": null, "next": null})
    );
    let (code, data, _) = cli(&[
        "call",
        "demo_find",
        r#"{"file":"examples/demo/sample.txt","query":"認知"}"#,
        "--json-only",
    ]);
    assert_eq!((code, &data["total"]), (0, &json!(2)));
}

#[test]
fn cli_quotes_charges_and_reports_errors_with_exit_codes() {
    // 花钱的不加 --yes：只报价，退出码 3，实现没跑（没有密钥也不报 auth）。
    let (code, reply, _) = cli(&["call", "demo_meter", r#"{"units":2}"#]);
    assert_eq!(code, 3);
    assert_eq!(reply["tool"], "demo_meter");
    assert_eq!(
        reply["quote"],
        json!({"service": "demo", "unit": "项", "units": 2.0, "usd": 0.02})
    );
    // 加了 --yes 才执行；这里没设 ENGINE_KEY_DEMO_KEY，所以是 auth 错误，退出码 1。
    let (code, reply, _) = cli(&["call", "demo_meter", r#"{"units":2}"#, "--yes"]);
    assert_eq!((code, &reply["error"]["kind"]), (1, &json!("auth")));
    // 报价为零不用确认。
    let (code, reply, _) = cli(&["call", "demo_meter", r#"{"units":0}"#]);
    assert_eq!((code, &reply["data"]), (0, &json!({"units": 0})));

    for args in [
        &["call", "demo_echo", r#"{"text":1}"#][..],
        &["call", "demo_echo", "not json"],
        &["call", "missing_tool", "{}"],
        &["call", "demo_echo", "{}", "--bogus"],
        &["describe"],
    ] {
        let (code, reply, _) = cli(args);
        assert_eq!(
            (code, &reply["error"]["kind"]),
            (2, &json!("bad_args")),
            "{args:?}"
        );
    }
    let (code, reply, _) = cli(&["call", "demo_read", r#"{"file":"nope.txt"}"#]);
    assert_eq!(
        (code, &reply["error"]["message"]),
        (1, &json!("文件读不了。"))
    );
}

#[test]
fn cli_saves_attachments_without_overwriting() {
    let out = tempfile::tempdir().unwrap();
    let dir = out.path().to_str().unwrap();
    let args = |style: &str| {
        [
            "call".to_owned(),
            "demo_read".into(),
            format!(r#"{{"file":"examples/demo/sample.txt","attachment":"{style}"}}"#),
            "--out".into(),
            dir.into(),
            "--json-only".into(),
        ]
    };
    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    assert_eq!(
        run_with(&demo(), args("temp_file"), &mut stdout, &mut stderr),
        0
    );
    let meta: Value = serde_json::from_slice(&stderr).unwrap();
    let saved = meta["attachments"][0]["path"].as_str().unwrap();
    assert_eq!(fs::read(saved).unwrap(), include_bytes!("sample.txt"));
    assert_eq!(
        serde_json::from_slice::<Value>(&stdout).unwrap(),
        json!({"bytes": 119})
    );
    // 同名文件已在：不覆盖，报 file。
    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    assert_eq!(
        run_with(&demo(), args("bytes"), &mut stdout, &mut stderr),
        1
    );
    let reply: Value = serde_json::from_slice(&stdout).unwrap();
    assert_eq!(reply["error"]["kind"], "file");
}

#[test]
fn cli_runs_cheap_paid_calls_and_asks_only_by_the_rule() {
    let dir = tempfile::tempdir().unwrap();
    let never = Override {
        confirm: Some(Confirm::Never),
        ..Override::default()
    };
    let toolkit = demo()
        .with_overrides(&[("demo_meter".to_string(), never)].into())
        .unwrap()
        .with_ledger(Ledger::At(dir.path().into()));
    let run = |units: u32| {
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let args = [
            "call".to_string(),
            "demo_meter".into(),
            format!(r#"{{"units":{units}}}"#),
        ];
        let code = run_with(&toolkit, args, &mut out, &mut err);
        (code, serde_json::from_slice::<Value>(&out).unwrap())
    };
    // $0.02：不用问，直接执行（没设 ENGINE_KEY_DEMO_KEY，所以到了 auth）
    let (code, reply) = run(2);
    assert_eq!((code, &reply["error"]["kind"]), (1, &json!("auth")));
    // $0.6：超过单次，只报价、说明为什么，退出码 3
    let (code, reply) = run(60);
    assert_eq!(code, 3);
    assert!(
        reply["why"].as_str().unwrap().contains("超过 $0.5"),
        "{reply}"
    );
    assert!(reply["hint"].as_str().unwrap().contains("--yes"));
}
