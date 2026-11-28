//! 工具注册、调用上下文、报价确认和结果。

use crate::{
    cancel::{Cancel, Cancelled},
    overrides::{self, Overrides},
    schema,
    spend::{Ledger, Why},
    tool::{Confirm, Cost, Expect, SpecError, ToolSpec, check_all},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, fmt, path::PathBuf};
pub use tempfile::NamedTempFile;
pub use zeroize::Zeroizing;

/// 全家通用的错误种类：种类、说明、给模型的建议、能否重试。各工具清单不用再列。
pub(crate) const COMMON_ERRORS: &[(&str, &str, &str, bool)] = &[
    (
        "bad_args",
        "工具参数不合规。",
        "请照工具说明改参数后重试。",
        false,
    ),
    (
        "denied",
        "用户没有确认执行。",
        "请先问用户，或换不花钱的做法。",
        false,
    ),
    ("cancelled", "工具调用已取消。", "需要时可重新调用。", true),
    ("unsupported", "这项操作不受支持。", "请换一种做法。", false),
    (
        "file",
        "文件读不了。",
        "请检查路径和权限，或换一份文件。",
        false,
    ),
    ("too_large", "内容太大。", "请缩小范围后重试。", false),
    ("network", "网络连接失败。", "稍后重试。", true),
    ("timeout", "请求超时。", "稍后重试，或缩小范围。", true),
    ("rate_limited", "请求太频繁。", "稍等片刻再试。", true),
    (
        "auth",
        "密钥缺失或无效。",
        "请用户在设置里检查密钥。",
        false,
    ),
    (
        "quota",
        "额度已用完。",
        "请用户检查服务额度或费用上限。",
        false,
    ),
    ("service", "服务出错。", "稍后重试。", true),
    (
        "internal",
        "引擎内部出错。",
        "请换个做法，或告诉用户。",
        false,
    ),
];

pub struct Toolkit<S> {
    state: S,
    manifest: Vec<ToolSpec>,
    handlers: BTreeMap<String, Handler<S>>,
    ledger: Ledger,
}

impl<S> Toolkit<S> {
    pub fn new<'a>(
        state: S,
        manifest: Vec<ToolSpec>,
        handlers: impl IntoIterator<Item = (&'a str, Handler<S>)>,
    ) -> Result<Self, SpecError> {
        let handlers = handlers.into_iter().collect::<Vec<_>>();
        check_all(
            &manifest,
            &handlers.iter().map(|(name, _)| *name).collect::<Vec<_>>(),
        )?;
        let handlers: BTreeMap<_, _> = handlers
            .into_iter()
            .map(|(name, handler)| (name.to_owned(), handler))
            .collect();
        for spec in &manifest {
            if matches!(spec.cost, Cost::Metered { .. }) != handlers[&spec.name].quote.is_some() {
                return Err(spec.invalid("计费工具必须注册本地报价函数，其他工具不注册"));
            }
        }
        Ok(Self {
            state,
            manifest,
            handlers,
            ledger: Ledger::Shared,
        })
    }

    /// 花钱的规矩查哪份账本（`spend`）：默认所有 App 共用的；App 的开发版、测试换成 `At` 或 `Off`。
    /// 引擎记账用的是自己状态里的那份，两边应当一样。
    pub fn with_ledger(mut self, ledger: Ledger) -> Self {
        self.ledger = ledger;
        self
    }

    pub fn ledger(&self) -> &Ledger {
        &self.ledger
    }

    /// 在交给 App 或 CLI 之前应用覆盖。
    pub fn with_overrides(mut self, overrides: &Overrides) -> Result<Self, SpecError> {
        self.manifest = overrides::apply(&self.manifest, overrides)?;
        Ok(self)
    }

    pub fn manifest(&self) -> &[ToolSpec] {
        &self.manifest
    }

    /// 一次调完：准备 → 要确认就问 `ctx.approve` → 执行。
    pub fn call(&self, name: &str, args: &Value, ctx: &Ctx<'_>) -> Result<Output, ToolError> {
        let prepared = self.prepare(name, args, ctx)?;
        if prepared.confirm {
            let spec = self.spec(&prepared.tool)?;
            if !ctx.approve.ask(spec, prepared.quote.as_ref()) {
                return Err(ToolError::common("denied"));
            }
        }
        self.run(prepared.approve(), ctx)
    }

    /// 第一步：校验参数、补默认值、本地报价，判断要不要用户确认。不执行工具。
    /// App 可以先拿报价弹确认，用户同意后再 `run`。
    pub fn prepare(&self, name: &str, args: &Value, ctx: &Ctx<'_>) -> Result<Prepared, ToolError> {
        ctx.cancel.check()?;
        let spec = self.spec(name)?;
        let args = spec
            .prepare_checked_args(args)
            .map_err(|detail| ToolError::new("bad_args", "工具参数不合规。", detail, false))?;
        let quote = match (&spec.cost, self.handlers[name].quote) {
            (
                Cost::Metered {
                    service,
                    unit,
                    usd_per_unit,
                },
                Some(count_units),
            ) => {
                let units = count_units(&self.state, &args, ctx.files, ctx.cancel)
                    .map_err(|error| fill(spec, error))?;
                let price = ctx.prices.get(service, unit).unwrap_or(*usd_per_unit);
                let usd = units * price;
                if !units.is_finite()
                    || units < 0.0
                    || !price.is_finite()
                    || price < 0.0
                    || !usd.is_finite()
                {
                    return Err(ToolError::new(
                        "internal",
                        "计费报价无效。",
                        "请检查报价和单价设置。",
                        false,
                    ));
                }
                Some(Quote {
                    service: service.clone(),
                    unit: unit.clone(),
                    units,
                    usd,
                })
            }
            _ => None,
        };
        // 花钱的规矩（spend）：引擎按共用账本定；清单和 App 的确认方式只能在这之上更严。
        let why = quote.as_ref().and_then(|quote| {
            let engine = spec.name.split_once('_').map_or("", |(engine, _)| engine);
            self.ledger
                .book(engine, &quote.service)
                .approval(Some(quote.usd))
        });
        let confirm = why.is_some()
            || match (spec.confirm, &quote) {
                (_, Some(quote)) if quote.usd == 0.0 => false,
                (Confirm::Always, _) => true,
                (Confirm::AboveUsd { usd }, Some(quote)) => quote.usd > usd,
                _ => false,
            };
        ctx.cancel.check()?;
        Ok(Prepared {
            tool: spec.name.clone(),
            args,
            quote,
            confirm,
            why,
            approved: false,
        })
    }

    /// 第二步：执行准备好的调用。要确认的（`needs_confirm`）得先 `approve()`（用户同意了），否则报 `denied`、
    /// 什么都不做：不同意就一个请求都不发，由引擎保证。`call` 问过 `ctx.approve` 会自己 `approve`。
    pub fn run(&self, prepared: Prepared, ctx: &Ctx<'_>) -> Result<Output, ToolError> {
        if prepared.confirm && !prepared.approved {
            return Err(ToolError::common("denied"));
        }
        ctx.cancel.check()?;
        let spec = self.spec(&prepared.tool)?;
        let run = self.handlers[&prepared.tool].run;
        let mut output = run(&self.state, &prepared.args, ctx, prepared.quote.as_ref())
            .map_err(|error| fill(spec, error))?;
        if let Err(detail) = schema::validate(&spec.result_schema, &mut output.data, "结果", false)
        {
            let mut error = ToolError::new("internal", "工具结果不符合清单。", detail, false);
            error.usage = output.usage.map(Box::new);
            return Err(error);
        }
        Ok(output)
    }

    /// 跑清单里的每个例子，各引擎在测试里调。`ctx` 的确认应一律同意；没过的逐条列出。
    pub fn check_examples(&self, ctx: &Ctx<'_>) -> Result<(), String> {
        let failures: Vec<_> = self
            .manifest
            .iter()
            .flat_map(|spec| {
                spec.examples.iter().enumerate().filter_map(|(i, example)| {
                    let result = self.call(&spec.name, &example.args, ctx);
                    unmet(&example.expect, result)
                        .map(|problem| format!("{} 示例 {}：{problem}", spec.name, i + 1))
                })
            })
            .collect();
        if failures.is_empty() {
            Ok(())
        } else {
            Err(failures.join("\n"))
        }
    }

    fn spec(&self, name: &str) -> Result<&ToolSpec, ToolError> {
        self.manifest
            .iter()
            .find(|spec| spec.name == name)
            .ok_or_else(|| ToolError::new("bad_args", "工具不可用。", "请从工具清单选择。", false))
    }
}

/// 准备好的一次调用：参数已校验并补了默认值，报价已算好。执行时用的就是这份。
#[derive(Clone, Debug, PartialEq)]
pub struct Prepared {
    tool: String,
    args: Value,
    quote: Option<Quote>,
    confirm: bool,
    why: Option<Why>,
    approved: bool,
}

impl Prepared {
    pub fn tool(&self) -> &str {
        &self.tool
    }

    /// 补过默认值的参数。
    pub fn args(&self) -> &Value {
        &self.args
    }

    /// 计费工具的报价；其他工具为 `None`。
    pub fn quote(&self) -> Option<&Quote> {
        self.quote.as_ref()
    }

    /// 这一次要不要先问用户：花钱的规矩要问，或工具的确认方式（App 的覆盖）要问。
    pub fn needs_confirm(&self) -> bool {
        self.confirm
    }

    /// 花钱的规矩为什么要问（超过单次、算不出、超过 24 小时合计）；规矩不用问时为 `None`。
    /// 问用户时把它和报价一起给用户看。
    pub fn why(&self) -> Option<&Why> {
        self.why.as_ref()
    }

    /// 用户同意了：App 弹过自己的卡片、用户点了「是」以后调，再交给 `Toolkit::run`。
    pub fn approve(mut self) -> Self {
        self.approved = true;
        self
    }
}

/// 例子没过的原因。
fn unmet(expect: &Expect, result: Result<Output, ToolError>) -> Option<String> {
    match (&expect.error, result) {
        (Some(kind), Err(error)) if error.kind == *kind => None,
        (Some(kind), Err(error)) => Some(format!(
            "预期错误 {kind}，实际 {}：{}",
            error.kind, error.message
        )),
        (Some(kind), Ok(_)) => Some(format!("预期错误 {kind}，实际成功")),
        (None, Err(error)) => Some(format!(
            "实际出错 {}：{}（{}）",
            error.kind, error.message, error.hint
        )),
        (None, Ok(output)) => {
            if let Some(text) = expect.contains.as_deref() {
                if !output.text.contains(text) {
                    return Some(format!("text 里没有「{text}」"));
                }
            }
            expect.data.iter().flatten().find_map(|(key, expected)| {
                let actual = output.data.get(key);
                (!actual.is_some_and(|actual| schema::equal(actual, expected)))
                    .then(|| format!("data.{key} 预期 {expected}，实际 {actual:?}"))
            })
        }
    }
}

/// 只给了种类的错误，照工具清单或通用种类补上说明、建议和能否重试。
fn fill(spec: &ToolSpec, mut error: ToolError) -> ToolError {
    if !error.message.is_empty() {
        return error;
    }
    let (message, hint, retry) = spec
        .errors
        .iter()
        .find(|declared| declared.kind == error.kind)
        .map(|declared| {
            (
                declared.message.as_str(),
                declared.hint.as_str(),
                declared.retry,
            )
        })
        .or_else(|| common(&error.kind))
        .unwrap_or_else(|| common("internal").unwrap());
    error.message = message.into();
    error.hint = hint.into();
    error.retry = retry;
    error
}

fn common(kind: &str) -> Option<(&'static str, &'static str, bool)> {
    COMMON_ERRORS
        .iter()
        .find(|(common, ..)| *common == kind)
        .map(|&(_, message, hint, retry)| (message, hint, retry))
}

type Run<S> = fn(&S, &Value, &Ctx<'_>, Option<&Quote>) -> Result<Output, ToolError>;
type CountUnits<S> = fn(&S, &Value, &dyn Files, &Cancel) -> Result<f64, ToolError>;

pub struct Handler<S> {
    pub run: Run<S>,
    /// 只做本地核算，计费工具必填。返回本次预计使用量。
    pub quote: Option<CountUnits<S>>,
}

impl<S> Handler<S> {
    /// 免费或只花本机时间的工具。
    pub fn new(run: Run<S>) -> Self {
        Self { run, quote: None }
    }

    /// 计费工具：`quote` 在本地数出这次要用多少单位，Toolkit 照单价算钱。
    pub fn metered(run: Run<S>, quote: CountUnits<S>) -> Self {
        Self {
            run,
            quote: Some(quote),
        }
    }
}

pub struct Ctx<'a> {
    pub cancel: &'a Cancel,
    pub keys: &'a dyn Keys,
    pub files: &'a dyn Files,
    pub approve: &'a dyn Approve,
    pub prices: &'a Prices,
    pub progress: &'a dyn Fn(Progress),
}

pub trait Keys {
    fn get(&self, name: &str) -> Option<Zeroizing<String>>;
}

pub trait Files {
    fn read(&self, path: &str) -> Result<Vec<u8>, ToolError>;

    fn local_path(&self, _path: &str) -> Option<PathBuf> {
        None
    }
}

pub trait Approve {
    /// 普通免费、本机工具设置了每次确认时，quote 为 None。
    fn ask(&self, tool: &ToolSpec, quote: Option<&Quote>) -> bool;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Progress {
    pub completed: u64,
    pub total: Option<u64>,
    pub message: String,
}

/// App 改过的单价，按服务和单位查；没改的用清单里的。
#[derive(Clone, Debug, Default)]
pub struct Prices(BTreeMap<String, BTreeMap<String, f64>>);

impl Prices {
    pub const fn new() -> Self {
        Self(BTreeMap::new())
    }

    pub fn set(&mut self, service: impl Into<String>, unit: impl Into<String>, usd_per_unit: f64) {
        self.0
            .entry(service.into())
            .or_default()
            .insert(unit.into(), usd_per_unit);
    }

    pub fn get(&self, service: &str, unit: &str) -> Option<f64> {
        self.0.get(service)?.get(unit).copied()
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct Quote {
    pub service: String,
    pub unit: String,
    pub units: f64,
    pub usd: f64,
}

impl Quote {
    /// 照报价记用量。按实际计量的服务改 `units`、`usd`，并把 `estimated` 设为 false。
    pub fn usage(&self, estimated: bool) -> Usage {
        Usage {
            service: self.service.clone(),
            unit: self.unit.clone(),
            units: self.units,
            usd: self.usd,
            estimated,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct Usage {
    pub service: String,
    pub unit: String,
    pub units: f64,
    pub usd: f64,
    pub estimated: bool,
}

#[derive(Debug, Default)]
pub struct Output {
    pub text: String,
    pub data: Value,
    pub attachments: Vec<Attachment>,
    pub usage: Option<Usage>,
    pub next: Option<Value>,
}

#[derive(Debug)]
pub struct Attachment {
    pub filename: String,
    pub mime_type: String,
    pub content: AttachmentContent,
}

#[derive(Debug)]
pub enum AttachmentContent {
    Bytes(Vec<u8>),
    TempFile(NamedTempFile),
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct ToolError {
    pub kind: String,
    pub message: String,
    pub hint: String,
    pub retry: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<Box<Usage>>,
}

impl ToolError {
    pub fn new(
        kind: impl Into<String>,
        message: impl Into<String>,
        hint: impl Into<String>,
        retry: bool,
    ) -> Self {
        Self {
            kind: kind.into(),
            message: message.into(),
            hint: hint.into(),
            retry,
            usage: None,
        }
    }

    /// 只给种类。经 Toolkit 交出时，照这个工具清单的 `errors`（或通用种类）补上说明、建议和能否重试。
    pub fn of(kind: impl Into<String>) -> Self {
        Self::new(kind, "", "", false)
    }

    /// 附上这次已经产生的用量（请求发出后失败也要记）。
    pub fn with_usage(mut self, usage: Option<Usage>) -> Self {
        self.usage = usage.map(Box::new);
        self
    }

    pub(crate) fn common(kind: &str) -> Self {
        let (message, hint, retry) = common(kind).expect("通用错误种类");
        Self::new(kind, message, hint, retry)
    }
}

impl From<Cancelled> for ToolError {
    fn from(_: Cancelled) -> Self {
        Self::common("cancelled")
    }
}

impl fmt::Display for ToolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ToolError {}
