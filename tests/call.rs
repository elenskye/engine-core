use engine_core::{
    call::{
        Approve, Ctx, Files, Handler, Keys, Output, Prices, Quote, ToolError, Toolkit, Usage,
        Zeroizing,
    },
    cancel::Cancel,
    tool::{Confirm, ErrorSpec, ToolSpec},
};
use serde_json::{Value, json};
use std::cell::{Cell, RefCell};

#[derive(Default)]
struct Host {
    accept: bool,
    asked: RefCell<Vec<Option<Quote>>>,
}

impl Approve for Host {
    fn ask(&self, _: &ToolSpec, quote: Option<&Quote>) -> bool {
        self.asked.borrow_mut().push(quote.cloned());
        self.accept
    }
}

impl Files for Host {
    fn read(&self, _: &str) -> Result<Vec<u8>, ToolError> {
        unreachable!()
    }
}

impl Keys for Host {
    fn get(&self, _: &str) -> Option<Zeroizing<String>> {
        unreachable!()
    }
}

#[derive(Default)]
struct Counts {
    quotes: Cell<u32>,
    runs: Cell<u32>,
}

fn meter(confirm: Confirm, counts: &Counts) -> Toolkit<&Counts> {
    let mut spec: ToolSpec =
        serde_json::from_str(include_str!("../examples/demo/tools/demo_meter.json")).unwrap();
    spec.confirm = confirm;
    Toolkit::new(
        counts,
        vec![spec],
        [(
            "demo_meter",
            Handler::<&Counts> {
                quote: Some(|state, args, _, _| {
                    state.quotes.set(state.quotes.get() + 1);
                    Ok(args["units"].as_u64().unwrap() as f64)
                }),
                run: |state, args, _, quote| {
                    state.runs.set(state.runs.get() + 1);
                    let quote = quote.unwrap();
                    let usage = Some(Usage {
                        service: quote.service.clone(),
                        unit: quote.unit.clone(),
                        units: quote.units,
                        usd: quote.usd,
                        estimated: true,
                    });
                    let data = if args["fail"] == true {
                        json!({})
                    } else {
                        json!({"units":args["units"]})
                    };
                    Ok(Output {
                        data,
                        usage,
                        ..Output::default()
                    })
                },
            },
        )],
    )
    .unwrap()
}

#[test]
fn validation_cancellation_and_denial_prevent_execution() {
    let counts = Counts::default();
    let toolkit = meter(Confirm::Always, &counts);
    let cancel = Cancel::new();
    let host = Host::default();
    let ctx = Ctx {
        cancel: &cancel,
        keys: &host,
        files: &host,
        approve: &host,
        prices: &Prices::default(),
        progress: &|_| {},
    };

    let error = toolkit
        .call("demo_meter", &json!({"units":101}), &ctx)
        .unwrap_err();
    assert_eq!(error.kind, "bad_args");
    assert_eq!((counts.quotes.get(), counts.runs.get()), (0, 0));
    let error = toolkit.call("demo_meter", &json!({}), &ctx).unwrap_err();
    assert_eq!(error.kind, "denied");
    assert!(error.usage.is_none());
    assert_eq!((counts.quotes.get(), counts.runs.get()), (1, 0));
    assert_eq!(host.asked.borrow()[0].as_ref().unwrap().usd, 0.01);
    let second_handle = cancel.clone();
    std::thread::spawn(move || second_handle.cancel())
        .join()
        .unwrap();
    let error = toolkit.call("demo_meter", &json!({}), &ctx).unwrap_err();
    assert_eq!(error.kind, "cancelled");
    assert_eq!((counts.quotes.get(), counts.runs.get()), (1, 0));
}

#[test]
fn quotes_use_host_prices_and_bad_output_retains_usage() {
    let counts = Counts::default();
    let toolkit = meter(Confirm::AboveUsd { usd: 0.25 }, &counts);
    let host = Host {
        accept: true,
        ..Host::default()
    };
    let mut prices = Prices::default();
    prices.set("demo", "项", 0.25);
    let ctx = Ctx {
        cancel: &Cancel::new(),
        keys: &host,
        files: &host,
        approve: &host,
        prices: &prices,
        progress: &|_| {},
    };

    for (args, amount) in [(json!({"units":0}), 0.0), (json!({}), 0.25)] {
        let usage = toolkit
            .call("demo_meter", &args, &ctx)
            .unwrap()
            .usage
            .unwrap();
        assert_eq!(usage.usd, amount);
    }
    assert!(host.asked.borrow().is_empty());
    let error = toolkit
        .call("demo_meter", &json!({"units":2,"fail":true}), &ctx)
        .unwrap_err();
    assert_eq!(error.kind, "internal");
    assert_eq!(error.usage.unwrap().usd, 0.5);
    assert_eq!(host.asked.borrow()[0].as_ref().unwrap().usd, 0.5);
    assert_eq!((counts.quotes.get(), counts.runs.get()), (3, 3));
}

#[test]
fn overrides_drive_the_same_manifest_and_dispatch() {
    let mut echo: ToolSpec =
        serde_json::from_str(include_str!("../examples/demo/tools/demo_echo.json")).unwrap();
    echo.confirm = Confirm::Always;
    let host = Host::default();
    let ctx = Ctx {
        cancel: &Cancel::new(),
        keys: &host,
        files: &host,
        approve: &host,
        prices: &Prices::default(),
        progress: &|_| {},
    };
    let toolkit = Toolkit::new(
        (),
        vec![echo.clone()],
        [(
            "demo_echo",
            Handler {
                quote: None,
                run: |_, _: &Value, _, _| unreachable!(),
            },
        )],
    )
    .unwrap();
    assert_eq!(
        toolkit
            .call("demo_echo", &json!({"text":"测试"}), &ctx)
            .unwrap_err()
            .kind,
        "denied"
    );
    assert_eq!(&*host.asked.borrow(), &[None]);

    let toolkit = toolkit
        .with_overrides(&serde_json::from_value(json!({"demo_echo":{"disabled":true}})).unwrap())
        .unwrap();
    assert!(toolkit.manifest().is_empty());
    assert_eq!(
        toolkit
            .call("demo_echo", &json!({"text":"测试"}), &ctx)
            .unwrap_err()
            .kind,
        "bad_args"
    );
    assert_eq!(&*host.asked.borrow(), &[None]);
}

#[test]
fn prepare_quotes_without_running_and_run_skips_the_question() {
    let counts = Counts::default();
    let toolkit = meter(Confirm::AboveUsd { usd: 0.015 }, &counts);
    let host = Host::default();
    let ctx = Ctx {
        cancel: &Cancel::new(),
        keys: &host,
        files: &host,
        approve: &host,
        prices: &Prices::default(),
        progress: &|_| {},
    };

    let cheap = toolkit.prepare("demo_meter", &json!({}), &ctx).unwrap();
    assert_eq!(cheap.args(), &json!({"units": 1, "fail": false}));
    assert_eq!(cheap.quote().unwrap().usd, 0.01);
    assert!(!cheap.needs_confirm());
    let costly = toolkit
        .prepare("demo_meter", &json!({"units": 2}), &ctx)
        .unwrap();
    assert!(costly.needs_confirm());
    assert_eq!((counts.quotes.get(), counts.runs.get()), (2, 0));

    // 要确认的没 approve 就 run：denied，实现不跑（不同意就一个请求都不发）。
    assert_eq!(
        toolkit.run(costly.clone(), &ctx).unwrap_err().kind,
        "denied"
    );
    assert_eq!(counts.runs.get(), 0);
    // App 自己弹了确认、用户同意后 approve，执行同一份报价，不再问 ctx.approve。
    let usage = toolkit.run(costly.approve(), &ctx).unwrap().usage.unwrap();
    assert_eq!((usage.units, usage.usd), (2.0, 0.02));
    assert!(host.asked.borrow().is_empty());
    assert_eq!((counts.quotes.get(), counts.runs.get()), (2, 1));
}

#[test]
fn errors_given_only_a_kind_are_filled_from_the_manifest() {
    let mut spec: ToolSpec =
        serde_json::from_str(include_str!("../examples/demo/tools/demo_echo.json")).unwrap();
    spec.errors = vec![ErrorSpec {
        kind: "locked".into(),
        message: "文件加了密码。".into(),
        hint: "请用户给密码。".into(),
        retry: false,
    }];
    let toolkit = Toolkit::new(
        (),
        vec![spec],
        [(
            "demo_echo",
            Handler::new(|_, args: &Value, _, _| {
                Err(match args["text"].as_str().unwrap() {
                    "locked" => ToolError::of("locked"),
                    "timeout" => ToolError::of("timeout"),
                    "unknown" => ToolError::of("unknown"),
                    _ => ToolError::new("file", "读不了这一份。", "换一份。", false),
                })
            }),
        )],
    )
    .unwrap();
    let host = Host::default();
    let ctx = Ctx {
        cancel: &Cancel::new(),
        keys: &host,
        files: &host,
        approve: &host,
        prices: &Prices::default(),
        progress: &|_| {},
    };
    let error = |text: &str| {
        let e = toolkit
            .call("demo_echo", &json!({"text": text}), &ctx)
            .unwrap_err();
        (e.kind, e.message, e.hint, e.retry)
    };
    let owned = |kind: &str, message: &str, hint: &str, retry| {
        (kind.into(), message.into(), hint.into(), retry)
    };
    assert_eq!(
        error("locked"),
        owned("locked", "文件加了密码。", "请用户给密码。", false)
    );
    assert_eq!(
        error("timeout"),
        owned("timeout", "请求超时。", "稍后重试，或缩小范围。", true)
    );
    assert_eq!(
        error("unknown"),
        owned(
            "unknown",
            "引擎内部出错。",
            "请换个做法，或告诉用户。",
            false
        )
    );
    assert_eq!(
        error("other"),
        owned("file", "读不了这一份。", "换一份。", false)
    );
}

#[test]
fn check_examples_reports_each_unmet_expectation() {
    let mut spec: ToolSpec =
        serde_json::from_str(include_str!("../examples/demo/tools/demo_echo.json")).unwrap();
    spec.examples[0].expect.contains = Some("Bye".into());
    spec.examples[1].expect.data = Some(serde_json::from_value(json!({"text": "HI"})).unwrap());
    spec.examples[2].args = json!({"text": "x"});
    spec.examples[2].expect.error = Some("timeout".into());
    let toolkit = Toolkit::new(
        (),
        vec![spec],
        [(
            "demo_echo",
            Handler::new(|_, args: &Value, _, _| {
                let text = args["text"].as_str().unwrap().to_uppercase();
                Ok(Output {
                    data: json!({ "text": text }),
                    text,
                    ..Output::default()
                })
            }),
        )],
    )
    .unwrap();
    let host = Host::default();
    let ctx = Ctx {
        cancel: &Cancel::new(),
        keys: &host,
        files: &host,
        approve: &host,
        prices: &Prices::default(),
        progress: &|_| {},
    };
    let report = toolkit.check_examples(&ctx).unwrap_err();
    let lines: Vec<_> = report.lines().collect();
    assert_eq!(lines.len(), 3, "{report}");
    assert!(lines[0].starts_with("demo_echo 示例 1：text 里没有「Bye」"));
    assert!(lines[1].starts_with("demo_echo 示例 2：data.text 预期 \"HI\""));
    assert_eq!(lines[2], "demo_echo 示例 3：预期错误 timeout，实际成功");
}

#[test]
fn the_spend_rule_decides_and_confirm_can_only_be_stricter() {
    use engine_core::spend::{Entry, Ledger, Why};
    let dir = tempfile::tempdir().unwrap();
    let ledger = Ledger::At(dir.path().into());
    let counts = Counts::default();
    // 清单写「不问」也照规矩：$0.6 超过单次，问
    let toolkit = meter(Confirm::Never, &counts).with_ledger(ledger.clone());
    let host = Host::default();
    let ctx = Ctx {
        cancel: &Cancel::new(),
        keys: &host,
        files: &host,
        approve: &host,
        prices: &Prices::default(),
        progress: &|_| {},
    };
    let small = toolkit
        .prepare("demo_meter", &json!({"units": 30}), &ctx)
        .unwrap();
    assert!(!small.needs_confirm() && small.why().is_none());
    let big = toolkit
        .prepare("demo_meter", &json!({"units": 60}), &ctx)
        .unwrap();
    assert!(big.needs_confirm());
    assert!(matches!(big.why(), Some(Why::Over { .. })));

    // 同一个引擎、同一个服务最近 24 小时快到 $5：小的也问；账本在 engine-demo/demo-ledger.jsonl
    let book = ledger.book("demo", "demo");
    assert_eq!(
        book.path().unwrap(),
        dir.path().join("engine-demo/demo-ledger.jsonl")
    );
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    book.record(&Entry {
        at: now,
        app: "Pomelo".into(),
        service: "demo".into(),
        unit: "项".into(),
        units: 490.0,
        usd: 4.9,
        estimated: false,
    })
    .unwrap();
    let small = toolkit
        .prepare("demo_meter", &json!({"units": 30}), &ctx)
        .unwrap();
    assert!(matches!(small.why(), Some(Why::Day { .. })));
    assert_eq!(
        toolkit
            .call("demo_meter", &json!({"units": 30}), &ctx)
            .unwrap_err()
            .kind,
        "denied"
    );
    // 不花钱的不问；Off 只按单次
    let free = toolkit
        .prepare("demo_meter", &json!({"units": 0}), &ctx)
        .unwrap();
    assert!(!free.needs_confirm());
    let off = meter(Confirm::Never, &counts).with_ledger(Ledger::Off);
    assert!(
        !off.prepare("demo_meter", &json!({"units": 30}), &ctx)
            .unwrap()
            .needs_confirm()
    );
    // 清单更严的照清单：AboveUsd 0.015 时 $0.3 要问，但不是规矩要问的
    let strict = meter(Confirm::AboveUsd { usd: 0.015 }, &counts).with_ledger(Ledger::Off);
    let p = strict
        .prepare("demo_meter", &json!({"units": 30}), &ctx)
        .unwrap();
    assert!(p.needs_confirm() && p.why().is_none());
}
