# engine-core 设计稿

> v0.1.0 计划。依据站主 2026-11-27 定的引擎开发准则（`CLAUDE.md`「一」）：Agent First、给 Agent 的工具包、App 只管接线、统一的 find。本库提供这四样的格式和骨架；具体格式的工具在各引擎里。下面的接口是草案，v0.2.0 Phase 1–4 定稿，公共接口的变化由本机 Claude 审。

## 0. 谁用、怎么用

| 用的引擎 | 用什么 |
| --- | --- |
| engine-pdf（v0.7） | 工具清单、调用、命令行、`pdf_find` 的归一化和命中格式、`Cancel` |
| engine-web、engine-word、engine-txt（v0.2 起） | 同上 |
| 以后的 engine-excel、engine-ppt、engine-image | 同上 |

引用：`engine-core = { git = "https://github.com/elenskye/engine-core", tag = "v0.2.0" }`。各引擎在依赖 engine-core 的 Phase 之前，先做不依赖它的部分。

```text
engine_core
├── tool       工具清单：ToolSpec（从 tools/*.json 读）、自检、参数校验和默认值
├── call       调用：Toolkit、Ctx（取消、密钥、文件、确认、价格、进度）、Output、ToolError、Usage
├── overrides  App 按工具名覆盖；导出成 Anthropic、OpenAI 的工具格式
├── cli        命令行骨架：tools / describe / call
├── find       归一化、查询、命中：Query、Finder、Hit<L>、Found<L>
└── cancel     Cancel（可克隆、跨线程）
```

## 1. 工具清单

**一个工具一份 JSON，放在引擎仓库的 `tools/<名字>.json`。** 这一份就是唯一的定义：Rust 用 `include_str!` 编进去，App 读它接线，命令行照它列出和校验，TS（比如 engine-pdf 的 `web/`）也能直接读。

```json
{
  "name": "pdf_find",
  "title": "在 PDF 里找字",
  "description": "在一份 PDF 里找一个词或一句话，交回每处的页码和前后文。扫描页也能找（用本机识别的字）。整句找不到时会按词找，并标出来。",
  "params": {
    "type": "object",
    "properties": {
      "file": { "type": "string", "description": "PDF 的路径" },
      "query": { "type": "string", "description": "要找的词或句子" },
      "pages": { "type": "string", "description": "只在这些页里找，如 \"3\"、\"1-5\"、\"1,3-4\"；不填就找全份" },
      "max_hits": { "type": "integer", "default": 20, "minimum": 1, "maximum": 100 },
      "context": { "type": "integer", "default": 40, "minimum": 0, "maximum": 200 }
    },
    "required": ["file", "query"]
  },
  "result": "每处命中：物理页码和页标签、命中处原样的文字、前后各若干字、整句还是按词、原文还是 OCR；另给一共几处、是不是只列了前几处。",
  "errors": [
    { "kind": "locked", "message": "这份 PDF 加了密码，打不开。", "hint": "请用户给密码，或换一份文件。", "retry": false }
  ],
  "cost": { "kind": "local" },
  "confirm": { "kind": "never" },
  "effects": { "network": false, "writes": "none" },
  "examples": [
    { "args": { "file": "samples/out/mixed.pdf", "query": "Results" }, "expect": { "contains": "第 4 页" } }
  ]
}
```

| 字段 | 规矩 |
| --- | --- |
| `name` | `<引擎短名>_<动作>`，小写字母、数字、下划线，全家不重名 |
| `title` | 给人看的中文短名 |
| `description` | 给模型看的说明：什么时候用、能做什么、做不到什么、找不到时怎么办。默认中文（三家 App 的模型都用中文和站主说话），App 可以覆盖 |
| `params` | JSON Schema 的一个子集：`object`、`string`、`integer`、`number`、`boolean`、`array`、`enum`、`required`、`default`、`minimum`、`maximum`、`maxItems`、`description`。本库自己校验，不引整套 JSON Schema 库 |
| `result` | 交回什么，一两句 |
| `errors` | 这个工具可能交回的错误种类；每种一句中文、给模型的下一步建议、能不能重试。另有全家通用的种类（§2）不用再列 |
| `cost` | `free`；`local`（只花本机时间，可写 `typical_ms`）；`metered`（`service`、`unit`、`usd_per_unit`，报价方式由引擎实现） |
| `confirm` | `never`；`always`；`above_usd`（超过多少美元才问） |
| `effects` | `network`：会不会联网；`writes`：`none` 或 `new_bytes`（交回新文件的字节，**从不改原文件**） |
| `note` | 可选。`cost`、`effects` 都按最坏的情况写；随参数变的（比如 `web_fetch` 本机抓免费、走 Tavily 才计费）在这里用一句话说清什么时候才花钱、才联网 |
| `examples` | 参数 + 期望（`contains` 某段文字、`error` 某种错误、`data` 里某个字段的值）；`cargo test` 每个都跑 |

**自检**（各引擎的测试里调 `engine_core::tool::check_all`）：名字合法、不重名；`params` 在子集之内；默认值符合自己的约束；`errors` 的种类不和通用种类冲突；每个工具都有实现、每个实现都在清单里；例子都能跑通。

## 2. 调用

```rust
pub trait Toolkit {
    fn manifest(&self) -> &[ToolSpec];
    fn call(&self, name: &str, args: &serde_json::Value, ctx: &Ctx) -> Result<Output, ToolError>;
}

pub struct Ctx<'a> {
    pub cancel: &'a Cancel,
    pub keys: &'a dyn Keys,          // fn get(&self, name: &str) -> Option<Zeroizing<String>>
    pub files: &'a dyn Files,        // read(path) -> 字节；local_path(path) -> 本机路径（可选）；App 在这里管权限
    pub approve: &'a dyn Approve,    // fn ask(&self, quote: &Quote) -> bool；App 的确认框
    pub prices: &'a Prices,          // 单价，App 可以改（比如 Tavily 每点多少钱）
    pub progress: &'a dyn Fn(Progress),
}

pub struct Output {
    pub text: String,                 // 给模型的 Markdown，位置写清
    pub data: serde_json::Value,      // 结构化结果，App 和界面用
    pub attachments: Vec<Attachment>, // 页图、新文件的字节（带建议的文件名和类型）
    pub usage: Option<Usage>,         // 花了钱就有
    pub next: Option<serde_json::Value>, // 太长没交完：接着取要用的参数
}
```

- **`Files`**：`read(&self, path) -> Result<Vec<u8>, ToolError>` 必有；`local_path(&self, path) -> Option<PathBuf>` 默认 `None`。要交给系统程序或 App 的（`textutil` 读 .doc、.rtf，借 Word、PowerPoint 导出，PDFKit 按路径打开）先问 `local_path`，有就直接用，没有就把字节写进临时文件、用完删掉。App 在这两个方法里管权限：不许读的路径报 `file`。
- 调用前，按（覆盖过的）清单校验参数、补默认值，不合格就报 `bad_args`，不进实现。
- **花钱的工具**：先算 `Quote`（服务、数量、单位、预计美元），再照确认方式问 `ctx.approve`。报价是 0 的（这次没走计费的路）不问。没同意就交 `denied`，保证一个请求都没发。
- **错误** `ToolError { kind, message, hint, retry }`：`message` 是一句中文，不带密钥、路径和服务原文；`hint` 是给模型的下一步。通用种类有 `bad_args`、`denied`、`cancelled`、`unsupported`、`file`、`too_large`、`network`、`timeout`、`rate_limited`、`auth`、`quota`、`service`、`internal`。各引擎自己的 `Error` 枚举映射到这里。
- **`Usage { service, unit, units, usd, estimated }`**：成功、失败都交回；请求发出后失败照实记，`estimated = true`。
- **App 层的配置不进 `Ctx`**：User-Agent、Cookie、代理、能不能抓内网、跟不跟别的网站这类，不经模型、每次调用都一样的设置，是各引擎 `Toolkit` 的构造参数（App 建 Toolkit 时交一次）。`Ctx` 只放每次调用会变的东西。

## 3. 覆盖和导出

```rust
pub struct Overrides(HashMap<String, Override>);
pub struct Override {
    pub description: Option<String>,
    pub defaults: Option<serde_json::Map<String, Value>>,  // 改默认值
    pub limits: Option<serde_json::Map<String, Value>>,    // 改上限（只能收紧，不能放宽）
    pub confirm: Option<Confirm>,
    pub disabled: bool,
}
pub fn apply(manifest: &[ToolSpec], o: &Overrides) -> Vec<ToolSpec>;
pub fn to_anthropic(tools: &[ToolSpec]) -> Vec<Value>;   // { name, description, input_schema }
pub fn to_openai(tools: &[ToolSpec]) -> Vec<Value>;      // { type: "function", function: { name, description, parameters } }
```

App 只做三件事：读清单、套上自己的覆盖、导出给自己的模型接口；收到模型的调用就交给 `Toolkit::call`，把 `Output` 转成自己的消息和界面。上限只能收紧，避免 App 把引擎保护自己的限制（页数、大小）放开。

## 4. 命令行

每个引擎一个可执行文件（`src/bin/<引擎>.rs`），里面只有一行：`engine_core::cli::run(&MyToolkit)`。

| 命令 | 做什么 |
| --- | --- |
| `<引擎> tools` | 列出工具：名字、中文短名、费用、确认方式 |
| `<引擎> describe <工具>` | 打印这个工具的整份清单（JSON） |
| `<引擎> call <工具> '<JSON>'` | 调一个工具，结果以 JSON 打到标准输出：`text`、`data`、`usage`、`next`，附件写进 `--out` 目录 |
| `--yes` | 花钱的工具同意执行；不加就只报价、退出码 3 |
| `--json-only` | 只打 `data` |

- 文件按本机路径读（`Files` 的命令行实现，只读，不写原文件）。
- 密钥从环境变量 `ENGINE_KEY_<名字大写>` 取（比如 `ENGINE_KEY_TAVILY`），不从参数传，不打印。
- 退出码：0 成功；1 工具交回错误（错误也以 JSON 打出）；2 参数不对；3 要确认没给 `--yes`。
- 不起常驻进程、不开服务。

## 5. find

### 5.1 问

```rust
pub struct Query {
    pub text: String,        // 一个词或一句话
    pub max_hits: usize,     // 默认 20
    pub context: usize,      // 前后各留多少字，默认 40
}
```

范围（哪几页、哪张表、哪几章）由各引擎在自己的 `<x>_find` 参数里加，本库不管。各引擎 `<x>_find` 的参数照 §1 的例子：`file`、`query`、本格式的范围、`max_hits`、`context`。

### 5.2 归一化

比的是归一化后的文字，交回的是原样的文字。`normalize(text) -> Normalized { text, map }`，`map` 记着每个归一化后的字对应原文的哪一段，命中后用它取回原样的文字、前后文和位置。

| 项 | 做法 | 例 |
| --- | --- | --- |
| 连字、全角半角、兼容字符 | NFKC | `ﬀ` → `ff`，`１２３` → `123`，`ＡＢ` → `AB` |
| 行尾断开的英文单词 | 字母 + `-` + 换行 + 小写字母 → 去掉连字符和换行 | `infor-\nmation` → `information` |
| 被换行拆开的中文词 | 两个汉字之间的换行和空白去掉 | `认\n知` → `认知` |
| 空白 | 连续空白收成一个空格；汉字和汉字之间的空白去掉 | |
| 简繁 | 两边都转成简体再比 | `認知` ↔ `认知` |
| 大小写 | 小写 | `Theory` ↔ `theory` |
| 引号、破折号 | 弯引号 → 直引号；各种横线 → `-` | `“a”` ↔ `"a"`，`—` ↔ `-` |
| 重音 | 拆开后去掉附加符号 | `é` ↔ `e` |

不做容错的近似匹配（错一两个字不算命中）。

### 5.3 找

```rust
let mut f = Finder::new(&query);
for unit in units {                     // 引擎逐个交「单元」：PDF 一页、Word 一段、Excel 一格、Markdown 一段
    cancel.check()?;
    f.scan(&unit.text, unit.source, |range| locate(&unit, range));  // range 是原文里的字节区间，引擎据此给位置
}
let found: Found<Loc> = f.finish();
```

- **先整句**：归一化后的查询在归一化后的单元里原样出现。
- **整句一处都没有，再按词**：查询按空白和标点切成词（中文按标点和空格切，不引分词库）；几个词都在同一个单元里就算一处，命中范围从最前一个词到最后一个词，标 `Words`。单元由引擎定：PDF 一页、Word 一段、Markdown 一块、字幕一条；单元越短，按词命中越少，这是有意的（词散在一整本书里不算找到）。
- 一处命中交回 `Hit<L> { location, before, matched, after, how: Match, source: Source }`；`pub enum Match { Phrase, Words }`、`pub enum Source { Text, Ocr }` 都单独导出，引擎交单元时就带上 `Source`；`before`、`after` 按原文截，各 `context` 个字，不切断字。
- `Found<L> { hits, total, truncated }`：`total` 是一共几处，超过 `max_hits` 时 `truncated = true`。
- 位置类型 `L` 由各引擎定（PDF 的页码和框、Excel 的单元格……），要能序列化；`text` 里给模型的写法也由引擎定，比如「第 4 页（印 iv）」。

## 6. 取消

`Cancel` 和 engine-pdf 现在的一样：`Arc<AtomicBool>`，可克隆、跨线程；`cancel()`、`is_cancelled()`、`check() -> Result<(), Cancelled>`。各引擎逐单元检查。engine-pdf v0.7 改用本库的，再导出一遍，保持对外接口不变。

## 7. 依赖和平台

- 纯 Rust，macOS、Linux 都要过。不引 HTTP、异步运行时、Mac 框架。
- 计划用：`serde`、`serde_json`、`zeroize`、`unicode-normalization`；简繁转换表 Phase 3 选（候选：Pomelo 在用的 `ferrous-opencc`，或只带字符级对照表的更小的库），比体积、许可证和覆盖面，写进决定。
- 测试替身：`examples/demo` 是一个假引擎，三个工具：免费的、本机耗时的、花钱的，把校验、默认值、覆盖、导出、确认、命令行、find 都走一遍。

## 8. 验收

- 清单自检、参数校验、覆盖、导出：单元测试 + demo 引擎的集成测试。
- 命令行：测试里跑 demo 的 `tools`、`describe`、`call`（含要确认的、出错的），比对 JSON。
- find：归一化每一项一组正反例；engine-pdf 已有样本和真实论文取出的文字片段存成文本夹具（只存短片段，来源写清），整句、按词、简繁、断词、中文换行各有命中用例。
- Linux 编译检查。

## 9. 还没定的（Phase 里定，写进决定）

- 简繁转换表用哪一个（Phase 3）。
- 附件太大时是交字节还是交临时文件路径（Phase 2，和 engine-pdf 的页图一起看）。
- 工具清单要不要带「结果」的 JSON Schema，还是只写一两句说明（Phase 1；先只写说明）。
