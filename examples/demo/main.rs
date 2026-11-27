use engine_core::{
    call::{
        Attachment, AttachmentContent, Ctx, Handler, NamedTempFile, Output, Progress, Quote,
        ToolError, Toolkit,
    },
    cli,
    find::{Finder, Found, Match, Query, Source},
};
use serde_json::{Value, json};
use std::{io::Write, process::ExitCode};

fn main() -> ExitCode {
    cli::run(&demo())
}

#[cfg(test)]
mod tests;

fn demo() -> Toolkit<()> {
    let manifest = [
        include_str!("tools/demo_echo.json"),
        include_str!("tools/demo_read.json"),
        include_str!("tools/demo_find.json"),
        include_str!("tools/demo_meter.json"),
    ]
    .into_iter()
    .map(|source| serde_json::from_str(source).expect("demo 清单 JSON"))
    .collect();
    Toolkit::new(
        (),
        manifest,
        [
            ("demo_echo", Handler::new(echo)),
            ("demo_read", Handler::new(read)),
            ("demo_find", Handler::new(find)),
            (
                "demo_meter",
                Handler::metered(meter, |_, args, _, _| Ok(args["units"].as_f64().unwrap())),
            ),
        ],
    )
    .expect("demo 清单和实现匹配")
}

fn echo(_: &(), args: &Value, _: &Ctx<'_>, _: Option<&Quote>) -> Result<Output, ToolError> {
    let text = args["text"].as_str().unwrap();
    let text = if args["style"] == "upper" {
        text.to_ascii_uppercase()
    } else {
        text.to_owned()
    };
    Ok(Output {
        data: json!({"text": text}),
        text,
        ..Output::default()
    })
}

fn read(_: &(), args: &Value, ctx: &Ctx<'_>, _: Option<&Quote>) -> Result<Output, ToolError> {
    let text = read_text(args, ctx)?;
    let content = match args["attachment"].as_str().unwrap() {
        "none" => None,
        "temp_file" => {
            let map_error =
                |_| ToolError::new("file", "无法暂存附件。", "请检查系统临时目录。", false);
            let mut temp = NamedTempFile::new().map_err(map_error)?;
            temp.write_all(text.as_bytes()).map_err(map_error)?;
            Some(AttachmentContent::TempFile(temp))
        }
        _ => Some(AttachmentContent::Bytes(text.clone().into_bytes())),
    };
    (ctx.progress)(Progress {
        completed: 1,
        total: Some(1),
        message: "已读取文本。".into(),
    });
    Ok(Output {
        data: json!({"bytes": text.len()}),
        text,
        attachments: content
            .map(|content| Attachment {
                filename: "sample.txt".into(),
                mime_type: "text/plain".into(),
                content,
            })
            .into_iter()
            .collect(),
        ..Output::default()
    })
}

/// 读一份 UTF-8 文本。读不到、不是文本都只交种类，说明和建议来自清单。
fn read_text(args: &Value, ctx: &Ctx<'_>) -> Result<String, ToolError> {
    let bytes = ctx.files.read(args["file"].as_str().unwrap())?;
    ctx.cancel.check()?;
    String::from_utf8(bytes).map_err(|_| ToolError::of("not_text"))
}

/// find 的接法：参数直接反序列化成 `Query`，逐段交给 `Finder`；`locate` 收到段内的字节区间，
/// 这里换成段号和行号（PDF 换成页码和字框，Excel 换成单元格……）。
fn find(_: &(), args: &Value, ctx: &Ctx<'_>, _: Option<&Quote>) -> Result<Output, ToolError> {
    let text = read_text(args, ctx)?;
    let query: Query = serde_json::from_value(args.clone()).expect("参数已照清单校验");
    let mut finder = Finder::new(&query);
    let mut line = 1;
    for (index, paragraph) in text.split("\n\n").enumerate() {
        ctx.cancel.check()?;
        finder.scan(paragraph, Source::Text, |range| {
            json!({
                "paragraph": index + 1,
                "line": line + paragraph[..range.start].matches('\n').count(),
            })
        });
        line += paragraph.matches('\n').count() + 2;
    }
    let found: Found<Value> = finder.finish();
    let mut markdown = match (found.total, found.hits.first().map(|hit| hit.how)) {
        (0, _) => "没有找到。".to_owned(),
        (total, Some(Match::Words)) => format!("整句没有找到，按词找到 {total} 处："),
        (total, _) => format!("找到 {total} 处："),
    };
    let flat = |text: &str| text.replace('\n', " ");
    for hit in &found.hits {
        let at = &hit.location;
        markdown += &format!(
            "\n- 第 {} 段第 {} 行：{}**{}**{}",
            at["paragraph"],
            at["line"],
            flat(&hit.before),
            flat(&hit.matched),
            flat(&hit.after)
        );
    }
    if found.truncated {
        markdown += &format!("\n\n只列了前 {} 处。", found.hits.len());
    }
    Ok(Output {
        text: markdown,
        data: serde_json::to_value(&found).unwrap(),
        ..Output::default()
    })
}

fn meter(_: &(), args: &Value, ctx: &Ctx<'_>, quote: Option<&Quote>) -> Result<Output, ToolError> {
    let quote = quote.expect("计费工具有报价");
    let usage = if quote.usd > 0.0 {
        let _key = ctx
            .keys
            .get("demo_key")
            .ok_or_else(|| ToolError::of("auth"))?;
        Some(quote.usage(true))
    } else {
        None
    };
    if args["fail"] == true {
        return Err(ToolError::of("service").with_usage(usage));
    }
    Ok(Output {
        text: format!("处理了 {} 项。", quote.units),
        data: json!({"units": args["units"]}),
        usage,
        ..Output::default()
    })
}
