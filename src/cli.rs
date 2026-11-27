//! 引擎命令行入口。标准输出以 JSON 交回结果、报价或错误。

use crate::{
    call::{
        Approve, Attachment, AttachmentContent, Ctx, Files, Keys, Prices, Quote, ToolError,
        Toolkit, Zeroizing,
    },
    cancel::Cancel,
    tool::ToolSpec,
};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    env,
    ffi::{OsStr, OsString},
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::ExitCode,
};

/// 引擎的 `main` 直接交回它：`fn main() -> ExitCode { cli::run(&toolkit()) }`。
pub fn run<S>(toolkit: &Toolkit<S>) -> ExitCode {
    let code = run_with(
        toolkit,
        env::args_os().skip(1),
        &mut io::stdout().lock(),
        &mut io::stderr().lock(),
    );
    ExitCode::from(code)
}

/// 同 `run`，命令行参数（不含程序名）和输出由调用方交，交回退出码。测试用。
pub fn run_with<S>(
    toolkit: &Toolkit<S>,
    args: impl IntoIterator<Item = impl Into<OsString>>,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> u8 {
    let args = args.into_iter().map(Into::into).collect::<Vec<_>>();
    let (code, value, metadata) = match execute(toolkit, &args) {
        Ok(result) => result,
        Err(error) => (
            if error.kind == "bad_args" { 2 } else { 1 },
            json!({"error": error}),
            None,
        ),
    };
    let write_json = |writer: &mut dyn Write, value: &Value| -> io::Result<()> {
        serde_json::to_writer(&mut *writer, value)?;
        writeln!(writer)
    };
    if write_json(stdout, &value).is_err()
        || metadata.is_some_and(|metadata| write_json(stderr, &metadata).is_err())
    {
        return 1;
    }
    code
}

type Reply = (u8, Value, Option<Value>);

fn execute<S>(toolkit: &Toolkit<S>, args: &[OsString]) -> Result<Reply, ToolError> {
    let bad_cli = || {
        ToolError::new(
            "bad_args",
            "命令行参数不正确。",
            "请使用 tools、describe <工具>，或 call <工具> '<JSON>' [--yes] [--out <目录>] [--json-only]。",
            false,
        )
    };
    match (
        args.first().and_then(|s| s.to_str()),
        args.get(1..).unwrap_or_default(),
    ) {
        (Some("tools"), []) => {
            let list = toolkit.manifest().iter().map(|tool| json!({
                "name": tool.name, "title": tool.title, "cost": tool.cost, "confirm": tool.confirm,
            })).collect::<Vec<_>>();
            Ok((0, json!(list), None))
        }
        (Some("describe"), [name]) => {
            let tool = toolkit
                .manifest()
                .iter()
                .find(|tool| OsStr::new(&tool.name) == name)
                .ok_or_else(|| {
                    ToolError::new(
                        "bad_args",
                        "工具不可用。",
                        "请用 tools 查看工具清单。",
                        false,
                    )
                })?;
            Ok((0, json!(tool), None))
        }
        (Some("call"), [name, raw, flags @ ..]) => {
            let name = name.to_str().ok_or_else(bad_cli)?;
            let params: Value =
                serde_json::from_str(raw.to_str().ok_or_else(bad_cli)?).map_err(|_| {
                    ToolError::new(
                        "bad_args",
                        "参数不是有效的 JSON。",
                        "请提供一个 JSON 对象。",
                        false,
                    )
                })?;
            let mut yes = false;
            let mut json_only = false;
            let mut out = None;
            let mut flags = flags.iter();
            while let Some(flag) = flags.next() {
                match flag.to_str() {
                    Some("--yes") if !yes => yes = true,
                    Some("--json-only") if !json_only => json_only = true,
                    Some("--out") if out.is_none() => {
                        out = Some(Path::new(
                            flags.next().filter(|s| !s.is_empty()).ok_or_else(bad_cli)?,
                        ));
                    }
                    _ => return Err(bad_cli()),
                }
            }
            let host = LocalHost { yes };
            let cancel = Cancel::new();
            let ctx = host.ctx(&cancel);
            let prepared = toolkit.prepare(name, &params, &ctx)?;
            // 命令行对一切正金额都要 --yes，不管清单的确认门槛。
            let charged = prepared.quote().is_some_and(|quote| quote.usd > 0.0);
            if (prepared.needs_confirm() || charged) && !yes {
                return Ok((
                    3,
                    json!({
                        "tool": name, "quote": prepared.quote(),
                        "hint": "要确认：加 --yes 才会执行。",
                    }),
                    None,
                ));
            }
            let output = toolkit.run(prepared, &ctx)?;

            let attachments = if output.attachments.is_empty() {
                Vec::new()
            } else {
                write_attachments(&output.attachments, out).map_err(|_| {
                    ToolError::new(
                        "file",
                        "无法保存工具附件。",
                        "请检查输出目录、文件名、可用空间或文件名冲突。",
                        false,
                    )
                    .with_usage(output.usage.clone())
                })?
            };
            if json_only {
                let metadata = (!attachments.is_empty() || output.usage.is_some())
                    .then(|| json!({"attachments": attachments, "usage": output.usage}));
                return Ok((0, output.data, metadata));
            }
            let mut result = json!({
                "text": output.text, "data": output.data, "usage": output.usage, "next": output.next,
            });
            if !attachments.is_empty() {
                result["attachments"] = json!(attachments);
            }
            Ok((0, result, None))
        }
        _ => Err(bad_cli()),
    }
}

/// 本机宿主：命令行用，各引擎的测试也可以用。文件按本机路径只读；密钥从环境变量
/// `ENGINE_KEY_<名字大写>` 取；`yes` 为真时一律同意确认。
#[derive(Clone, Copy, Debug, Default)]
pub struct LocalHost {
    pub yes: bool,
}

impl LocalHost {
    /// 用清单单价、不报进度的调用上下文。
    pub fn ctx<'a>(&'a self, cancel: &'a Cancel) -> Ctx<'a> {
        static PRICES: Prices = Prices::new();
        Ctx {
            cancel,
            keys: self,
            files: self,
            approve: self,
            prices: &PRICES,
            progress: &|_| {},
        }
    }
}

impl Approve for LocalHost {
    fn ask(&self, _: &ToolSpec, _: Option<&Quote>) -> bool {
        self.yes
    }
}

impl Files for LocalHost {
    fn read(&self, path: &str) -> Result<Vec<u8>, ToolError> {
        fs::read(path).map_err(|_| ToolError::of("file"))
    }

    fn local_path(&self, path: &str) -> Option<PathBuf> {
        Some(PathBuf::from(path))
    }
}

impl Keys for LocalHost {
    fn get(&self, name: &str) -> Option<Zeroizing<String>> {
        env::var(format!("ENGINE_KEY_{}", name.to_ascii_uppercase()))
            .ok()
            .map(Zeroizing::new)
    }
}

fn write_attachments(attachments: &[Attachment], out: Option<&Path>) -> io::Result<Vec<Value>> {
    let mut names = BTreeSet::new();
    for attachment in attachments {
        let name = &attachment.filename;
        if name.contains(['/', '\\'])
            || Path::new(name).file_name() != Some(OsStr::new(name))
            || !names.insert(name)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "附件文件名无效或重复",
            ));
        }
    }

    let temporary = out.is_none().then(tempfile::tempdir).transpose()?;
    let directory = match (&temporary, out) {
        (Some(temporary), _) => temporary.path().to_owned(),
        (_, Some(out)) => {
            let path = std::path::absolute(out)?;
            fs::create_dir_all(&path)?;
            path
        }
        _ => unreachable!(),
    };
    let mut staged = Vec::new();
    for attachment in attachments {
        let mut file = tempfile::NamedTempFile::new_in(&directory)?;
        match &attachment.content {
            AttachmentContent::Bytes(bytes) => file.write_all(bytes)?,
            AttachmentContent::TempFile(source) => {
                io::copy(&mut source.reopen()?, &mut file)?;
            }
        }
        staged.push((
            file,
            directory.join(&attachment.filename),
            &attachment.mime_type,
        ));
    }
    let mut saved = Vec::new();
    for (file, path, mime_type) in staged {
        if let Err(error) = file.persist_noclobber(&path) {
            for (path, _) in saved {
                let _ = fs::remove_file(path);
            }
            return Err(error.error);
        }
        saved.push((path, mime_type));
    }
    if let Some(temporary) = temporary {
        let _ = temporary.keep();
    }
    Ok(saved
        .into_iter()
        .map(|(path, mime_type)| {
            json!({
                "path": path.to_string_lossy(), "mime_type": mime_type,
            })
        })
        .collect())
}
