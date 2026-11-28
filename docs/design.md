# engine-core 设计稿

依据引擎开发准则（`CLAUDE.md`「一」）。本库只给格式和骨架，具体工具在各引擎里。v0.2.0 的接口就是本文；各引擎升级要改什么见 `CHANGELOG.md`。

```text
engine_core
├── tool       工具清单 ToolSpec（读 tools/*.json）、自检 check_all、参数校验和默认值
├── call       Toolkit（prepare / run / call / check_examples）、Ctx、Output、ToolError、Quote、Usage
├── overrides  App 按工具名覆盖；导出成 Anthropic、OpenAI 的工具格式
├── cli        命令行骨架 run / run_with；LocalHost（本机文件、环境变量密钥）
├── find       normalize、Query、Finder、Hit<L>、Found<L>
├── cancel     Cancel、Cancelled（根上也导出）
└── encoding   feature `encoding`：decode、Decoded、Encoding、DetectedBy、Newline、DecodeError、declared（v0.3.0）
```

引用：`engine-core = { git = "https://github.com/elenskye/engine-core", tag = "v0.2.0" }`。示范引擎 `examples/demo`（四个工具：免费、本机、查找、计费）把全流程走一遍，接新引擎照它抄。

## 1. 工具清单

一个工具一份 JSON，放在引擎的 `tools/<名字>.json`，是唯一的定义：Rust 用 `include_str!` 编进去，App、命令行、TS 都读它。例子见 `examples/demo/tools/demo_find.json`。

| 字段 | 规矩 |
| --- | --- |
| `name` | `<引擎短名>_<动作>`：小写字母开头，小写字母、数字，下划线分隔，最多 64 字符，全家不重名 |
| `title` | 给人看的中文短名 |
| `description` | 给模型看：什么时候用、能做什么、做不到什么、找不到时怎么办。默认中文，App 可覆盖 |
| `params` | JSON Schema 子集，最外层 `object`。类型：`object`、`string`、`integer`、`number`、`boolean`、`array`、`null`；关键字：`properties`、`items`、`enum`、`required`、`default`、`minimum`、`maximum`、`maxItems`、`description` |
| `result` | 交回什么，一两句 |
| `result_schema` | 必填，同一个子集，描述 `Output.data` |
| `errors` | 本工具特有的错误种类：`kind`、`message`（一句中文）、`hint`（给模型的下一步）、`retry`。通用种类（§2）不用列，也不能重名 |
| `cost` | `free`；`local`（可写 `typical_ms`）；`metered`（`service`、`unit`、`usd_per_unit`） |
| `confirm` | `never`；`always`；`above_usd`（超过多少美元才问） |
| `effects` | `network`；`writes`：`none` 或 `new_bytes`（交回新文件的字节，从不改原文件） |
| `note` | 可选。`cost`、`effects` 按最坏情况写，随参数变的在这里说清 |
| `examples` | 参数 + 期望：`contains`（`text` 里有）、`data`（顶层字段的值）、`error`（种类）。`check_examples` 逐个跑 |

**自检** `check_all(清单, 实现名)`（`Toolkit::new` 会调）：名字合法、不重名；每个工具都有实现、每个实现都有清单；Schema 在子集内，默认值、枚举值符合自己的约束；错误种类不和通用种类冲突；每个例子有可核对的期望，非 `bad_args` 例子的参数能过校验。`<短名>_find` 另查问法和答法一致：必填 `query`（string）、`max_hits`（integer，默认 20）、`context`（integer，默认 40）；结果必填 `hits`（array）、`total`（integer）、`truncated`（boolean）。

**参数校验**按 `properties` 递归，只给缺的字段补默认值；清单外的字段原样保留；显式的 `null` 照字段类型校验；`2.0` 这样的整数值转成 `2`。

## 2. 调用

```rust
let toolkit = Toolkit::new(state, manifest, [
    ("demo_read", Handler::new(read)),                  // 免费、本机
    ("demo_meter", Handler::metered(meter, count)),     // 计费：count 本地数单位
])?.with_overrides(&app_overrides)?;

toolkit.call(name, &args, &ctx)?;                       // 一次调完，要确认就问 ctx.approve
let p = toolkit.prepare(name, &args, &ctx)?;            // 或者分两步：先报价
if p.needs_confirm() { /* App 弹确认，拿 p.quote() */ }
toolkit.run(p, &ctx)?;                                  // 用户同意后执行同一份报价
```

- **`Ctx`**（每次调用会变的）：`cancel`、`keys`（`get(名字) -> Option<Zeroizing<String>>`）、`files`（`read(路径) -> 字节`；`local_path(路径)` 默认 `None`，要交给系统程序的先问它，App 在这里管权限，不许读就报 `file`）、`approve`（`ask(工具, 报价) -> bool`）、`prices`（App 改过的单价）、`progress`。User-Agent、代理这类每次都一样的设置放各引擎 `Toolkit` 的状态里，不进 `Ctx`。
- **`prepare`**：查取消 → 按生效的清单校验参数、补默认值（不合格报 `bad_args`）→ 计费工具调本地 `count` 数单位，单价取 `Prices` 里同服务同单位的，否则用清单的 → 定要不要确认：报价为零不问；`always` 每次问（免费工具问时报价为 `None`）；`above_usd` 超过才问。交回 `Prepared`（工具名、补过默认值的参数、报价、要不要确认；字段只读）。
- **`run`**：查取消 → 执行 → 按 `result_schema` 校验 `data`，不合格报 `internal` 并保留用量。确认由调用方负责。
- **错误** `ToolError { kind, message, hint, retry, usage }`。通用种类：`bad_args`、`denied`、`cancelled`、`unsupported`、`file`、`too_large`、`network`、`timeout`、`rate_limited`、`auth`、`quota`、`service`、`internal`，各带默认的说明、建议和能否重试。实现只交种类时用 `ToolError::of(种类)`，经 Toolkit 交出时照这个工具清单的 `errors`（或通用种类）补齐——说明只写在清单里一处；要具体的说明就用 `ToolError::new`。清单和通用种类里都没有的种类按 `internal` 的说明补。消息不带密钥、路径和服务原文。
- **用量** `Usage { service, unit, units, usd, estimated }`：花了钱就填。成功随 `Output.usage`，失败随 `ToolError::with_usage`；请求发出后失败按估算记。`Quote::usage(estimated)` 照报价生成。
- **结果** `Output { text, data, attachments, usage, next }`：`text` 是给模型的 Markdown（位置写清）；`data` 给 App 和界面；附件是 `Attachment { filename, mime_type, content }`，内容为 `Bytes` 或 `TempFile`（所有权随结果转移，释放即删）；`next` 是没交完时接着取的参数。

## 3. 覆盖和导出

```rust
pub type Overrides = BTreeMap<String, Override>;   // 按工具名
pub struct Override { description, defaults, limits, confirm, disabled }
```

`Toolkit::with_overrides` 应用后，清单、导出、调用用的是同一份。`limits` 按顶层参数名和关键字写，如 `{"max_hits":{"maximum":50}}`，只能收紧（提高 `minimum`、降低 `maximum` / `maxItems`、新增这几项）。拼错工具或参数名、放宽、默认值越界，整份拒绝。覆盖后只保留参数效果不变的例子。

`to_anthropic(manifest)` → `{name, description, input_schema}`；`to_openai(manifest)` → Chat Completions 的 `{type: "function", function: {name, description, parameters}}`。

## 4. 命令行

引擎的 `src/bin/<引擎>.rs`：`fn main() -> ExitCode { engine_core::cli::run(&toolkit()) }`。`run_with(toolkit, 参数, 标准输出, 标准错误)` 同样的流程，测试用。

| 命令 | 做什么 |
| --- | --- |
| `<引擎> tools` | 列出工具：名字、中文短名、费用、确认方式 |
| `<引擎> describe <工具>` | 打印整份清单 |
| `<引擎> call <工具> '<JSON>'` | 打印 `{text, data, usage, next}`，有附件另加 `attachments`（路径、类型） |
| `--yes` | 同意执行。凡是正金额的报价、或要确认的，不加就只打 `{tool, quote, hint}`、退出码 3，不执行 |
| `--out <目录>` | 附件存到这里，不覆盖同名文件；省略时存进新建的系统临时目录，留给调用者 |
| `--json-only` | 标准输出只打 `data`，附件路径和用量打到标准错误 |

文件按本机路径只读，密钥从环境变量 `ENGINE_KEY_<名字大写>` 取（`LocalHost`；各引擎测试也可以用它：`LocalHost { yes: true }.ctx(&cancel)`）。退出码：0 成功；1 工具出错；2 参数或命令不对；3 要确认没给 `--yes`。错误以 `{error}` JSON 打出。不起常驻进程。

## 5. find

### 5.1 问

`Query { text, max_hits, context }`，默认 20 处、前后各 40 字（`DEFAULT_MAX_HITS`、`DEFAULT_CONTEXT`）。JSON 里 `text` 叫 `query`，和 `<短名>_find` 的参数同名，引擎可以把参数直接反序列化成 `Query`（其他参数忽略）。范围（哪几页、哪张表）由各引擎在自己的参数里加。

### 5.2 归一化

比的是归一化后的文字，交回的是原样的文字。`normalize(text) -> Normalized { text, map }`：`map` 按归一化后的每个字给出原文的 UTF-8 字节区间；`original_range(归一化区间)` 换回原文区间，中间被去掉的字（断词符、换行）也包在里面。查询和原文走同一套。

| 项 | 做法 | 例 |
| --- | --- | --- |
| 连字、全角半角、兼容字符 | NFKC | `ﬀ` → `ff`，`１２３` → `123`，康熙部首 → 常用字 |
| 看不见的字符 | 软连字符、零宽字符、BOM 等（Unicode Default_Ignorable）去掉 | `arti\u{ad}cle` → `article` |
| 英文的横线和断词 | 拉丁字母之间的连字符、横线去掉，中间可有一次换行；后面只有空格、没有换行的不拼（`all\u{ad} the`、`pre- and post-`） | `infor-\nmation` → `information`；`self-\nattention`、`self-attention` → `selfattention` |
| 中日文旁的空白 | 汉字、假名、全角或中文标点任一侧的空白去掉 | `认\n知` → `认知`；`模型，\n认知` → `模型,认知`；`使用 Python` → `使用python` |
| 其余空白 | 连续空白收成一个空格，去掉首尾 | |
| 简繁 | 逐字按 OpenCC 表折到稳定的简体 | `認知` ↔ `认知` |
| 大小写 | 小写 | `Theory` ↔ `theory` |
| 引号、横线 | 弯引号 → 直引号；其余横线、减号 → `-` | `“a”` ↔ `"a"`，`1–5` ↔ `1-5` |
| 重音 | 只去拉丁字母的重音 | `é` ↔ `e` |

不做容错的近似匹配。全角字母、数字之间的空格照旧保留。

### 5.3 找

```rust
let query: Query = serde_json::from_value(args.clone())?;
let mut finder = Finder::new(&query);
for unit in units {                    // PDF 一页、Word 一段、Excel 一格……
    ctx.cancel.check()?;
    finder.scan(&unit.text, unit.source, |range| locate(&unit, range));  // range：单元原文里的字节区间
}
let found: Found<Loc> = finder.finish();   // serde_json::to_value(&found) 就是 find 工具的 data
```

- **先整句**：归一化文字里的子串（不要求词边界，和阅读器的查找一样），不重叠，按单元和原文顺序计数；连字展开后指向同一原文的只算一处。
- **所有单元都没有整句命中时按词找**：查询按空白和标点切词（`self-attention` 归一化后是一个词）。英文和数字按整词，汉字可以是子串；重复的词要有对应次数的不重叠出现。一个单元里所有词都在，才算找到：取覆盖它们的最短原文范围（按原文字数，一样长取靠前的），其中每个词各算一处，按原文顺序交回。
- `Hit<L> { location, before, matched, after, how, source }`：`matched` 是原文原样（`text[range]`），前后文按原文的字截取，不切断字；`how` 是 `phrase` / `words`；`source` 是 `text` / `ocr`。
- `Found<L> { hits, total, truncated }`：`total` 是所选方式的全部命中数，`hits` 最多 `max_hits` 处，超过时 `truncated` 为真；`max_hits = 0` 只计数；空查询没有命中。
- 位置类型 `L` 由引擎定，要能序列化；`text` 里给模型的写法也由引擎定（如「第 4 页（印 iv）」）。按词的命中，同一单元的几处是挨着的，引擎可以按单元合并着写。

## 6. 取消

`Cancel`：`Arc<AtomicBool>`，可克隆、跨线程；`cancel()`、`is_cancelled()`、`check() -> Result<(), Cancelled>`。`ToolError` 实现了 `From<Cancelled>`。各引擎逐单元检查，engine-pdf、engine-txt 再导出它，对外名字不变。

## 7. 依赖、平台、验收

- 纯 Rust，macOS、Linux 都要编译和测试（`cargo check --target aarch64-unknown-linux-gnu --all-targets`）。不引 HTTP、异步运行时、Mac 框架。依赖见 `THIRD_PARTY.md`，简繁表见 `decisions/0006`。
- 自动测：清单自检、参数、覆盖、导出、调用流程（`tests/`）；demo 的例子和命令行（`examples/demo/tests.rs`，`run_with` 比对 JSON）；归一化正反例和映射；find 的整句、按词、简繁、断词、中文换行、全角，含 engine-pdf 样本和真实论文的短片段（`tests/fixtures/find/`）。
- 真实文件批量核对（不进 Git）：用 engine-pdf 取 38 份真实论文和杂志、8 页扫描件（OCR）的每页文字，从原文随机截片段，按原样和「模型照着打」的写法去找，看能否在原处整句命中。结果记在 `HANDOFF.md`。

## 8. 文字编码（feature `encoding`，v0.3.0）

engine-txt 的 txt、Markdown、字幕和 engine-excel 的 CSV 都要从字节认编码，一份放在这里（`decisions/0008`）。要用的引擎开 `features = ["encoding"]`，不用的不带 `encoding_rs`、`chardetng`。

```rust
use engine_core::encoding::{decode, Decoded, DecodeError};

let d: Decoded = decode(&bytes, None)?;        // 或 Some("gbk")：WHATWG 的编码名，另认 HZ-GB-2312
d.text;                                        // 统一成 \n、去掉开头的 BOM，别的一个字不改
d.encoding;                                    // Encoding { name, by: DetectedBy, bom, replaced }；.label() 给人看
d.newline;                                     // Option<Newline>：原文用的换行
engine_core::encoding::declared(&bytes);       // XML 开头、HTML meta 声明的编码（EPUB 正文用）
```

- **顺序**：指定 → BOM → 没有 BOM 的 UTF-16（看换行、空格、英文、常见中文标点这些「锚点」）→ 二进制（前 16 KB 里 NUL 超过 1%）→ 合法 UTF-8（7 位、带 ESC 的认 ISO-2022-JP，带 `~{` 的认 HZ）→ XML 声明的 → chardetng 猜（喂前 1 MB）。
- **出错**：`DecodeError::NotText`（二进制）、`UnknownEncoding`（自己认的解出来替换符超过 1% 或含 NUL）、`BadEncodingName`（指定的名字不认识）。指定的编码解不出的字照样交回，`replaced` 记着几个。各引擎映射成自己的错误种类。
- `Encoding`、`DetectedBy`、`Newline` 的 JSON 和 engine-txt v0.2 的一样（engine-txt 改成再导出这几个类型）。
- 测试：各种编码、没有 BOM 的 UTF-16 两种字节序、GBK / Big5 / Windows-1252 / Shift_JIS 不被当成 UTF-16、二进制、XML 声明、ISO-2022-JP、HZ、换行、给人看的写法（`src/encoding.rs`）；engine-txt 的编码测试照样过。
