//! 工具清单、参数校验和自检。

use crate::{
    call::COMMON_ERRORS,
    find::{DEFAULT_CONTEXT, DEFAULT_MAX_HITS},
    schema,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::{collections::BTreeSet, fmt};

fn is_common_error(kind: &str) -> bool {
    COMMON_ERRORS.iter().any(|(common, ..)| *common == kind)
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ToolSpec {
    pub name: String,
    pub title: String,
    pub description: String,
    pub params: Value,
    pub result: String,
    pub result_schema: Value,
    pub errors: Vec<ErrorSpec>,
    pub cost: Cost,
    pub confirm: Confirm,
    pub effects: Effects,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub examples: Vec<Example>,
}

impl ToolSpec {
    /// 校验参数并给缺失字段补默认值。App 可以在调工具前预览结果。
    pub fn prepare_args(&self, args: &Value) -> Result<Value, SpecError> {
        schema::check(&self.params, "params").map_err(|detail| self.invalid(detail))?;
        if self.params["type"] != "object" {
            return Err(self.invalid("params 最外层必须是 object"));
        }
        self.prepare_checked_args(args)
            .map_err(|detail| self.invalid(detail))
    }

    pub(crate) fn prepare_checked_args(&self, args: &Value) -> Result<Value, String> {
        let mut args = args.clone();
        schema::validate(&self.params, &mut args, "参数", true)?;
        Ok(args)
    }

    pub(crate) fn invalid(&self, detail: impl Into<String>) -> SpecError {
        SpecError::InvalidSpec {
            tool: self.name.clone(),
            detail: detail.into(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ErrorSpec {
    pub kind: String,
    pub message: String,
    pub hint: String,
    pub retry: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Cost {
    Free,
    Local {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        typical_ms: Option<u64>,
    },
    Metered {
        service: String,
        unit: String,
        usd_per_unit: f64,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Confirm {
    Never,
    Always,
    AboveUsd { usd: f64 },
}

impl Confirm {
    pub(crate) fn is_valid(self) -> bool {
        match self {
            Self::AboveUsd { usd } => usd.is_finite() && usd >= 0.0,
            _ => true,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Effects {
    pub network: bool,
    pub writes: Writes,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Writes {
    None,
    NewBytes,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Example {
    pub args: Value,
    pub expect: Expect,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Expect {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contains: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Map<String, Value>>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum SpecError {
    InvalidName(String),
    DuplicateName(String),
    DuplicateImplementation(String),
    MissingImplementation(String),
    MissingSpec(String),
    InvalidSpec { tool: String, detail: String },
}

impl fmt::Display for SpecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidName(name) => write!(f, "工具名不合法：{name}"),
            Self::DuplicateName(name) => write!(f, "工具清单重名：{name}"),
            Self::DuplicateImplementation(name) => write!(f, "工具实现重名：{name}"),
            Self::MissingImplementation(name) => write!(f, "工具缺少实现：{name}"),
            Self::MissingSpec(name) => write!(f, "工具实现缺少清单：{name}"),
            Self::InvalidSpec { tool, detail } => write!(f, "{tool}：{detail}"),
        }
    }
}

impl std::error::Error for SpecError {}

/// 检查清单、示例输入及实现名是否一一对应。
pub fn check_all(specs: &[ToolSpec], implementations: &[&str]) -> Result<(), SpecError> {
    let mut names = BTreeSet::new();
    for spec in specs {
        if spec.name.len() > 64 || !spec.name.contains('_') || !valid_id(&spec.name) {
            return Err(SpecError::InvalidName(spec.name.clone()));
        }
        if !names.insert(spec.name.as_str()) {
            return Err(SpecError::DuplicateName(spec.name.clone()));
        }
        check_spec(spec).map_err(|detail| spec.invalid(detail))?;
    }
    let mut registered = BTreeSet::new();
    for name in implementations {
        if !registered.insert(*name) {
            return Err(SpecError::DuplicateImplementation((*name).into()));
        }
        if !names.contains(name) {
            return Err(SpecError::MissingSpec((*name).into()));
        }
    }
    if let Some(name) = names.difference(&registered).next() {
        return Err(SpecError::MissingImplementation((*name).into()));
    }
    Ok(())
}

fn check_spec(spec: &ToolSpec) -> Result<(), String> {
    for (key, value) in [
        ("title", &spec.title),
        ("description", &spec.description),
        ("result", &spec.result),
    ] {
        if value.trim().is_empty() {
            return Err(format!("{key} 不能为空"));
        }
    }
    schema::check(&spec.params, "params")?;
    if spec.params["type"] != "object" {
        return Err("params 最外层必须是 object".into());
    }
    schema::check(&spec.result_schema, "result_schema")?;
    if !spec.confirm.is_valid() {
        return Err("确认门槛必须是有限的非负金额".into());
    }
    if let Cost::Metered {
        service,
        unit,
        usd_per_unit,
    } = &spec.cost
    {
        if service.trim().is_empty() || unit.trim().is_empty() {
            return Err("计费服务名和单位不能为空".into());
        }
        if !usd_per_unit.is_finite() || *usd_per_unit < 0.0 {
            return Err("单价必须是有限的非负金额".into());
        }
    }

    if spec.name.ends_with("_find") {
        check_find(spec)?;
    }

    let mut errors = BTreeSet::new();
    for error in &spec.errors {
        if !valid_id(&error.kind)
            || is_common_error(&error.kind)
            || !errors.insert(error.kind.as_str())
        {
            return Err(format!(
                "错误种类不合法、重复或与通用种类冲突：{}",
                error.kind
            ));
        }
        if error.message.trim().is_empty() || error.hint.trim().is_empty() {
            return Err(format!("错误 {} 缺少说明或建议", error.kind));
        }
    }

    if spec.examples.is_empty() {
        return Err("至少需要一个示例".into());
    }
    for (i, example) in spec.examples.iter().enumerate() {
        let expect = &example.expect;
        let example_label = format!("示例 {}", i + 1);
        if let Some(kind) = &expect.error {
            if expect.contains.is_some() || expect.data.is_some() {
                return Err(format!("{example_label} 的错误和成功预期不能并用"));
            }
            if !errors.contains(kind.as_str()) && !is_common_error(kind) {
                return Err(format!("{example_label} 的错误种类未声明"));
            }
        } else if expect.contains.as_deref().is_none_or(str::is_empty)
            && expect.data.as_ref().is_none_or(Map::is_empty)
        {
            return Err(format!("{example_label} 缺少可核对的预期"));
        }
        if expect.error.as_deref() != Some("bad_args") {
            spec.prepare_checked_args(&example.args)
                .map_err(|detail| format!("{example_label}：{detail}"))?;
        }
    }
    Ok(())
}

/// 各引擎的 `<短名>_find` 同一种问法、同一种答法（`find::Query`、`find::Found`）。
fn check_find(spec: &ToolSpec) -> Result<(), String> {
    let params = &spec.params["properties"];
    let required = |schema: &Value, name: &str| {
        schema["required"]
            .as_array()
            .is_some_and(|names| names.iter().any(|n| n == name))
    };
    if params["query"]["type"] != "string" || !required(&spec.params, "query") {
        return Err("find 工具要有必填的 query（string）".into());
    }
    for (name, default) in [("max_hits", DEFAULT_MAX_HITS), ("context", DEFAULT_CONTEXT)] {
        if params[name]["type"] != "integer" || params[name]["default"] != default {
            return Err(format!("find 工具的 {name} 要是 integer，默认 {default}"));
        }
    }
    let result = &spec.result_schema;
    for (name, kind) in [
        ("hits", "array"),
        ("total", "integer"),
        ("truncated", "boolean"),
    ] {
        if result["properties"][name]["type"] != kind || !required(result, name) {
            return Err(format!("find 工具的结果要有必填的 {name}（{kind}）"));
        }
    }
    Ok(())
}

fn valid_id(name: &str) -> bool {
    name.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
        && name.split('_').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        })
}
