# CHANGELOG

公共接口变化和各引擎的接入说明。发版以 Git tag 为准。

## v0.2.0

首个可接入的版本：工具包骨架 + 统一 find。接口细节见 `docs/design.md`，照着抄的样子见 `examples/demo`。

### 公共接口

- `tool`：`ToolSpec` 读 `tools/*.json`；`check_all` 自检清单和实现名，`<短名>_find` 另查问法和答法一致；`prepare_args` 预览校验和默认值。
- `call`：`Toolkit::new(状态, 清单, [(名字, Handler::new / Handler::metered)])`；`prepare` → `Prepared`（报价、要不要确认）→ `run`，或 `call` 一次调完；`check_examples` 跑清单里的例子。`Ctx` 交取消、密钥、文件、确认、单价、进度。`ToolError::of(种类)` 只给种类，说明照清单补；`with_usage` 附用量。`Quote::usage` 照报价记用量。
- `overrides`：`Overrides` 按工具名改说明、默认值、上限（只能收紧）、确认方式或关掉；`to_anthropic`、`to_openai` 导出生效的清单。
- `cli`：`run` / `run_with`，命令 `tools`、`describe`、`call`（`--yes`、`--out`、`--json-only`），退出码 0–3；`LocalHost` 本机宿主，测试也能用。
- `find`：`normalize`（带原文映射）；`Query`（JSON 字段同 find 工具参数）、`Finder`、`Hit<L>`、`Found<L>`、`Match`、`Source`；整句优先，没有时按词，按词时每个词各算一处。
- `Cancel`、`Cancelled`（根上导出）。

### 各引擎怎么接

钉 `v0.2.0`，更新各自的 `Cargo.lock`。一个工具一份 JSON 加一个注册名；错误用 `ToolError::of`，说明写在清单里；测试里调 `toolkit.check_examples(&LocalHost { yes: true }.ctx(&cancel))`（要密钥的计费工具自己实现 `Keys`）；命令行 `src/bin/<引擎>.rs` 交回 `cli::run`。

| 引擎 | 接入节点 | 要做的 |
| --- | --- | --- |
| engine-pdf | v0.7.0 Phase 2 起 | 再导出 `Cancel`，`Cancelled` 映射到现有 `Error::Cancelled`；Mathpix 走计费工具（`prepare` 给报价，App 弹确认后 `run`）。Phase 4 的 `pdf_find` 逐物理页扫描，`locate` 把字节区间换成页码、页标签和字框，扫描页标 `Source::Ocr` |
| engine-web | v0.2.0 Phase 4 | `web_extract`、`web_find`；正文块逐块交给 `Finder`，位置是标题路径和第几段，`under` 范围由网页侧限定 |
| engine-word | v0.2.0 Phase 5 | `word_read`、`word_outline`、`word_find`；逐段扫描，位置带正文、表格格子、脚注等 |
| engine-txt | v0.2.0 Phase 5 | `src/cancel.rs` 的占位换成 `pub use engine_core::{Cancel, Cancelled, find::Source}`；`txt_find` 从字节区间收窄到行、段或字幕时间 |
| engine-excel | v0.2.0 Phase 5 | `excel_find` 每格一个单元，位置是表名、单元格和表头 |

## v0.1.0（未打 tag）

- 初始化仓库：开发准则、设计稿、v0.2.0 的 Phase 计划。
- 还没有可用的接口（只有 `VERSION`），别的引擎不要引用这一版。
