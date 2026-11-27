use engine_core::{
    overrides::{Overrides, apply, to_anthropic, to_openai},
    tool::{SpecError, ToolSpec, check_all},
};
use serde_json::json;

fn specs() -> Vec<ToolSpec> {
    [
        include_str!("../examples/demo/tools/demo_echo.json"),
        include_str!("../examples/demo/tools/demo_read.json"),
        include_str!("../examples/demo/tools/demo_meter.json"),
        include_str!("../examples/demo/tools/demo_find.json"),
    ]
    .into_iter()
    .map(|source| serde_json::from_str(source).unwrap())
    .collect()
}

#[test]
fn checks_demo_and_reports_name_and_registration_errors() {
    let specs = specs();
    let names = ["demo_echo", "demo_read", "demo_meter", "demo_find"];
    check_all(&specs, &names).unwrap();
    assert_eq!(
        check_all(&specs, &names[..3]),
        Err(SpecError::MissingImplementation("demo_find".into()))
    );
    assert_eq!(
        check_all(&specs[..3], &names),
        Err(SpecError::MissingSpec("demo_find".into()))
    );
    assert_eq!(
        check_all(&specs[..1], &["demo_echo", "demo_echo"]),
        Err(SpecError::DuplicateImplementation("demo_echo".into()))
    );
    assert_eq!(
        check_all(&[specs[0].clone(), specs[0].clone()], &names[..1]),
        Err(SpecError::DuplicateName("demo_echo".into()))
    );

    let mut spec = specs[0].clone();
    for name in [
        "echo",
        "Demo_echo",
        "demo__echo",
        "demo_echo_",
        &format!("demo_{}", "x".repeat(60)),
    ] {
        spec.name = name.into();
        assert_eq!(
            check_all(std::slice::from_ref(&spec), &[name]),
            Err(SpecError::InvalidName(name.into()))
        );
    }
}

#[test]
fn rejects_bad_defaults_unsupported_schema_and_reserved_errors() {
    let specs = specs();
    let mut meter = specs[2].clone();
    meter.params["properties"]["units"]["default"] = json!(101);
    let failure = check_all(std::slice::from_ref(&meter), &["demo_meter"]).unwrap_err();
    assert!(
        failure
            .to_string()
            .contains("params.units.default 超过最大值")
    );

    meter = specs[2].clone();
    meter.result_schema["properties"]["units"]["oneOf"] = json!([]);
    let failure = check_all(&[meter], &["demo_meter"]).unwrap_err();
    assert!(
        failure
            .to_string()
            .contains("result_schema.units 不支持关键字 oneOf")
    );

    let mut echo = specs[0].clone();
    echo.params["properties"]["style"]["enum"] = json!(["plain", 1]);
    let failure = check_all(&[echo], &["demo_echo"]).unwrap_err();
    assert!(
        failure
            .to_string()
            .contains("params.style.enum 类型应为 string")
    );

    let mut echo = specs[0].clone();
    echo.errors = serde_json::from_value(json!([{
        "kind": "bad_args", "message": "参数有误。", "hint": "请改参数。", "retry": false,
    }]))
    .unwrap();
    let failure = check_all(&[echo], &["demo_echo"]).unwrap_err();
    assert!(failure.to_string().contains("与通用种类冲突"));
}

#[test]
fn validates_nested_values_and_preserves_extra_fields() {
    let mut spec = specs().remove(0);
    spec.params = json!({
        "type": "object",
        "properties": {
            "rows": {
                "type": "array", "maxItems": 1,
                "items": {"type": "object", "properties": {
                    "count": {"type": "integer", "maximum": 9_007_199_254_740_993_u64},
                    "ratio": {"type": "number", "minimum": -0.5, "maximum": 0.5, "default": 0.25},
                    "active": {"type": "boolean", "default": false},
                    "notes": {"type": "null", "default": null}
                }, "required": ["count"]}
            }
        },
        "required": ["rows"],
    });
    let original = json!({"rows":[{"count":2.0,"extra":"保留"}],"extra":{"x":true}});
    let result = spec.prepare_args(&original).unwrap();
    assert_eq!(
        result,
        json!({
            "rows":[{"count":2,"ratio":0.25,"active":false,"notes":null,"extra":"保留"}],
            "extra":{"x":true},
        })
    );
    assert!(original["rows"][0].get("ratio").is_none());
    for invalid in [
        json!(null),
        json!({}),
        json!({"rows":[{}, {}]}),
        json!({"rows":[{}]}),
        json!({"rows":[{"count":3.5}]}),
        json!({"rows":[{"count":null}]}),
        json!({"rows":[{"count":9_007_199_254_740_994_u64}]}),
        json!({"rows":[{"count":1,"ratio":-0.6}]}),
        json!({"rows":[{"count":1,"active":0}]}),
    ] {
        assert!(spec.prepare_args(&invalid).is_err(), "{invalid}");
    }
    assert!(
        spec.prepare_args(&json!({"rows":[{"count":9_007_199_254_740_993_u64}]}))
            .is_ok()
    );
}

#[test]
fn overrides_limits_and_exports_only_effective_tools() {
    let mut specs = specs();
    specs[2].params["properties"]["modes"] = json!({"type":"array","maxItems":4});
    let overrides: Overrides = serde_json::from_value(json!({
        "demo_echo": {"disabled": true},
        "demo_meter": {
            "description": "应用自定义说明",
            "defaults": {"units": 2},
            "limits": {"units": {"minimum": 1,"maximum": 2}, "modes": {"maxItems":2}},
            "confirm": {"kind":"above_usd","usd":0.03}
        }
    }))
    .unwrap();
    let effective = apply(&specs, &overrides).unwrap();
    assert_eq!(
        effective
            .iter()
            .map(|s| s.name.as_str())
            .collect::<Vec<_>>(),
        ["demo_read", "demo_meter", "demo_find"]
    );
    let tool = &effective[1];
    assert_eq!(tool.prepare_args(&json!({})).unwrap()["units"], 2);
    assert_eq!(tool.examples.len(), 1);
    assert_eq!(
        to_anthropic(&effective)[1],
        json!({
            "name":"demo_meter","description":"应用自定义说明","input_schema":tool.params,
        })
    );
    assert_eq!(
        to_openai(&effective)[1],
        json!({
            "type":"function","function":{
                "name":"demo_meter","description":"应用自定义说明","parameters":tool.params,
            },
        })
    );
    for invalid in [
        json!({"missing":{"disabled":true}}),
        json!({"demo_meter":{"limits":{"units":{"maximum":101}}}}),
        json!({"demo_meter":{"limits":{"units":{"minimum":-1}}}}),
        json!({"demo_meter":{"limits":{"modes":{"maxItems":5}}}}),
        json!({"demo_meter":{"limits":{"units":{"minimum":2}}}}),
        json!({"demo_meter":{"defaults":{"unknown":0}}}),
    ] {
        assert!(apply(&specs, &serde_json::from_value(invalid).unwrap()).is_err());
    }
    assert_eq!(specs[2].params["properties"]["units"]["maximum"], 100);
}

#[test]
fn find_tools_share_the_query_and_result_shape() {
    let find = specs().remove(3);
    check_all(std::slice::from_ref(&find), &["demo_find"]).unwrap();
    let broken = [
        ("/params/properties/query/type", json!("integer"), "query"),
        ("/params/required", json!(["file"]), "query"),
        ("/params/properties/max_hits/default", json!(10), "max_hits"),
        (
            "/params/properties/context/type",
            json!("number"),
            "context",
        ),
        (
            "/result_schema/properties/total/type",
            json!("number"),
            "total",
        ),
        (
            "/result_schema/required",
            json!(["hits", "total"]),
            "truncated",
        ),
    ];
    for (pointer, value, field) in broken {
        let mut spec = serde_json::to_value(&find).unwrap();
        *spec.pointer_mut(pointer).unwrap() = value;
        let spec: ToolSpec = serde_json::from_value(spec).unwrap();
        let failure = check_all(&[spec], &["demo_find"]).unwrap_err().to_string();
        assert!(
            failure.contains("find 工具") && failure.contains(field),
            "{pointer}: {failure}"
        );
    }
}
