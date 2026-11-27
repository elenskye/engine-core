use serde_json::{Number, Value};
use std::cmp::Ordering;
use std::collections::BTreeSet;

pub(crate) fn check(schema: &Value, path: &str) -> Result<(), String> {
    let kind = schema["type"]
        .as_str()
        .ok_or_else(|| format!("{path} 缺少有效的 type"))?;
    let applicable = match kind {
        "object" => &["properties", "required"][..],
        "array" => &["items", "maxItems"],
        "integer" | "number" => &["minimum", "maximum"],
        "string" | "boolean" | "null" => &[],
        _ => return Err(format!("{path} 的 type 不受支持")),
    };
    for key in schema.as_object().unwrap().keys() {
        if !["type", "description", "default", "enum"].contains(&key.as_str())
            && !applicable.contains(&key.as_str())
        {
            return Err(format!("{path} 不支持关键字 {key}"));
        }
    }
    if schema.get("description").is_some_and(|v| !v.is_string()) {
        return Err(format!("{path}.description 必须是字符串"));
    }
    if let Some(properties) = schema.get("properties") {
        for (name, child) in properties
            .as_object()
            .ok_or_else(|| format!("{path}.properties 必须是对象"))?
        {
            check(child, &format!("{path}.{name}"))?;
        }
    }
    if let Some(required) = schema.get("required") {
        let mut seen = BTreeSet::new();
        for name in required
            .as_array()
            .ok_or_else(|| format!("{path}.required 必须是数组"))?
        {
            let name = name
                .as_str()
                .ok_or_else(|| format!("{path}.required 必须列出参数名"))?;
            if schema["properties"].get(name).is_none() || !seen.insert(name) {
                return Err(format!("{path}.required 含未声明或重复的字段 {name}"));
            }
        }
    }
    if let Some(items) = schema.get("items") {
        check(items, &format!("{path}.items"))?;
    }
    if schema.get("maxItems").is_some_and(|max| !max.is_u64()) {
        return Err(format!("{path}.maxItems 必须是非负整数"));
    }
    for key in ["minimum", "maximum"] {
        if schema.get(key).is_some_and(|value| !value.is_number()) {
            return Err(format!("{path}.{key} 必须是数字"));
        }
    }
    if let (Some(min), Some(max)) = (schema["minimum"].as_number(), schema["maximum"].as_number()) {
        if compare(min, max).is_gt() {
            return Err(format!("{path} 的最小值超过最大值"));
        }
    }
    if let Some(values) = schema.get("enum") {
        let values = values
            .as_array()
            .filter(|values| !values.is_empty())
            .ok_or_else(|| format!("{path}.enum 必须是非空数组"))?;
        for (i, value) in values.iter().enumerate() {
            if values[..i].iter().any(|other| equal(value, other)) {
                return Err(format!("{path}.enum 含重复的值"));
            }
            validate(schema, &mut value.clone(), &format!("{path}.enum"), false)?;
        }
    }
    if let Some(default) = schema.get("default") {
        validate(
            schema,
            &mut default.clone(),
            &format!("{path}.default"),
            false,
        )?;
    }
    Ok(())
}

// schema 已由 check 校验，validate 只检查传入数据。
pub(crate) fn validate(
    schema: &Value,
    value: &mut Value,
    path: &str,
    fill_defaults: bool,
) -> Result<(), String> {
    let kind = schema["type"].as_str().expect("schema 已自检");
    let type_matches = match kind {
        "object" => value.is_object(),
        "array" => value.is_array(),
        "string" => value.is_string(),
        "boolean" => value.is_boolean(),
        "integer" => value
            .as_number()
            .is_some_and(|n| integer(n).is_some() || n.as_f64().is_some_and(|f| f.fract() == 0.0)),
        "number" => value.is_number(),
        "null" => value.is_null(),
        _ => unreachable!("schema 已自检"),
    };
    if !type_matches {
        return Err(format!("{path} 类型应为 {kind}"));
    }
    if fill_defaults && kind == "integer" {
        let number = value.as_number().unwrap();
        if let Some(decimal) = number.as_f64().filter(|_| integer(number).is_none()) {
            let whole = decimal as i128;
            *value = if let Ok(signed) = i64::try_from(whole) {
                signed.into()
            } else if let Ok(unsigned) = u64::try_from(whole) {
                unsigned.into()
            } else {
                return Err(format!("{path} 超过可表示的整数范围"));
            };
        }
    }
    if let Some(object) = value.as_object_mut() {
        if let Some(properties) = schema["properties"].as_object() {
            for (name, child) in properties {
                if fill_defaults && !object.contains_key(name) {
                    if let Some(default) = child.get("default") {
                        object.insert(name.clone(), default.clone());
                    }
                }
                if let Some(value) = object.get_mut(name) {
                    validate(child, value, &format!("{path}.{name}"), fill_defaults)?;
                }
            }
        }
        if let Some(required) = schema["required"].as_array() {
            for name in required.iter().filter_map(Value::as_str) {
                if !object.contains_key(name) {
                    return Err(format!("{path} 缺少必填字段 {name}"));
                }
            }
        }
    }
    if let Some(array) = value.as_array_mut() {
        if schema["maxItems"]
            .as_u64()
            .is_some_and(|max| array.len() as u64 > max)
        {
            return Err(format!("{path} 的数组长度超过上限"));
        }
        if let Some(items) = schema.get("items") {
            for (i, value) in array.iter_mut().enumerate() {
                validate(items, value, &format!("{path}[{i}]"), fill_defaults)?;
            }
        }
    }
    if let Some(number) = value.as_number() {
        if schema["minimum"]
            .as_number()
            .is_some_and(|min| compare(number, min).is_lt())
        {
            return Err(format!("{path} 低于最小值"));
        }
        if schema["maximum"]
            .as_number()
            .is_some_and(|max| compare(number, max).is_gt())
        {
            return Err(format!("{path} 超过最大值"));
        }
    }
    if let Some(values) = schema["enum"].as_array() {
        if !values.iter().any(|allowed| equal(allowed, value)) {
            return Err(format!("{path} 不是允许的枚举值"));
        }
    }
    Ok(())
}

fn integer(number: &Number) -> Option<i128> {
    number
        .as_i64()
        .map(i128::from)
        .or_else(|| number.as_u64().map(i128::from))
}

pub(crate) fn compare(left: &Number, right: &Number) -> Ordering {
    // 整数保持精确。混合比较先比整数部分，再比小数，避免大整数被舍入。
    let compare_float = |int: i128, float: f64| {
        int.cmp(&(float as i128))
            .then_with(|| 0.0_f64.partial_cmp(&float.fract()).unwrap())
    };
    match (integer(left), integer(right)) {
        (Some(l), Some(r)) => l.cmp(&r),
        (Some(l), None) => compare_float(l, right.as_f64().unwrap()),
        (None, Some(r)) => compare_float(r, left.as_f64().unwrap()).reverse(),
        (None, None) => left
            .as_f64()
            .unwrap()
            .partial_cmp(&right.as_f64().unwrap())
            .unwrap(),
    }
}

pub(crate) fn equal(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Number(left), Value::Number(right)) => compare(left, right).is_eq(),
        (Value::Array(left), Value::Array(right)) => {
            left.len() == right.len() && left.iter().zip(right).all(|(l, r)| equal(l, r))
        }
        (Value::Object(left), Value::Object(right)) => {
            left.len() == right.len()
                && left
                    .iter()
                    .all(|(key, value)| right.get(key).is_some_and(|r| equal(value, r)))
        }
        _ => left == right,
    }
}
