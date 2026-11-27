//! App 级工具覆盖，以及模型 API 的工具导出。

use crate::{
    schema,
    tool::{Confirm, SpecError, ToolSpec},
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;

pub type Overrides = BTreeMap<String, Override>;

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Override {
    pub description: Option<String>,
    pub defaults: Option<Map<String, Value>>,
    pub limits: Option<Map<String, Value>>,
    pub confirm: Option<Confirm>,
    #[serde(default)]
    pub disabled: bool,
}

pub fn apply(manifest: &[ToolSpec], overrides: &Overrides) -> Result<Vec<ToolSpec>, SpecError> {
    let mut effective = manifest.to_vec();
    for (name, change) in overrides {
        let Some(index) = effective.iter().position(|spec| spec.name == *name) else {
            return Err(SpecError::InvalidSpec {
                tool: name.clone(),
                detail: "覆盖指定了未知工具".into(),
            });
        };
        let spec = &mut effective[index];
        let base = manifest
            .iter()
            .find(|base| base.name == *name)
            .expect("覆盖源自原清单");
        schema::check(&base.params, "params").map_err(|detail| base.invalid(detail))?;
        apply_one(spec, change).map_err(|detail| spec.invalid(detail))?;
        spec.examples = base
            .examples
            .iter()
            .filter(|example| {
                match (
                    base.prepare_checked_args(&example.args),
                    spec.prepare_checked_args(&example.args),
                ) {
                    (Ok(before), Ok(after)) => schema::equal(&before, &after),
                    (Err(before), Err(after)) => before == after,
                    _ => false,
                }
            })
            .cloned()
            .collect();
        if change.disabled {
            effective.remove(index);
        }
    }
    Ok(effective)
}

fn apply_one(spec: &mut ToolSpec, change: &Override) -> Result<(), String> {
    if let Some(description) = &change.description {
        if description.trim().is_empty() {
            return Err("覆盖的工具说明不能为空".into());
        }
        spec.description.clone_from(description);
    }
    if let Some(confirm) = change.confirm {
        if !confirm.is_valid() {
            return Err("确认门槛必须是有限的非负金额".into());
        }
        spec.confirm = confirm;
    }
    for (name, limits) in change.limits.iter().flatten() {
        let property = spec
            .params
            .get_mut("properties")
            .and_then(|properties| properties.get_mut(name))
            .ok_or_else(|| format!("覆盖指定了未知参数 {name}"))?;
        let limits = limits
            .as_object()
            .ok_or_else(|| format!("{name} 的 limits 必须是对象"))?;
        for (key, value) in limits {
            let new = value
                .as_number()
                .ok_or_else(|| format!("{name}.{key} 必须是数字"))?;
            let cmp = property[key]
                .as_number()
                .map(|old| schema::compare(new, old));
            let relaxed = match key.as_str() {
                "minimum" => cmp.is_some_and(|ord| ord.is_lt()),
                "maximum" | "maxItems" => cmp.is_some_and(|ord| ord.is_gt()),
                _ => return Err(format!("{name}.{key} 不支持覆盖")),
            };
            if relaxed {
                return Err(format!("{name}.{key} 只能收紧"));
            }
            property[key] = value.clone();
        }
    }
    for (name, default) in change.defaults.iter().flatten() {
        let property = spec
            .params
            .get_mut("properties")
            .and_then(|properties| properties.get_mut(name))
            .ok_or_else(|| format!("覆盖指定了未知参数 {name}"))?;
        property["default"] = default.clone();
    }
    schema::check(&spec.params, "params")
}

pub fn to_anthropic(tools: &[ToolSpec]) -> Vec<Value> {
    tools
        .iter()
        .map(|spec| {
            json!({
                "name": spec.name,
                "description": spec.description,
                "input_schema": spec.params,
            })
        })
        .collect()
}

pub fn to_openai(tools: &[ToolSpec]) -> Vec<Value> {
    tools
        .iter()
        .map(|spec| {
            json!({
                "type": "function",
                "function": {
                    "name": spec.name,
                    "description": spec.description,
                    "parameters": spec.params,
                },
            })
        })
        .collect()
}
