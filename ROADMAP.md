# ROADMAP

现在：**v0.1.0 计划**（这一版只有计划和骨架，没有可用的接口）。下一步 v0.2.0：工具包骨架 + find，各引擎做工具包和 find 都钉它的 tag。**v0.2.0 的 Phase 计划已写，等站主看过再开工。**

分工见 `CLAUDE.md`「二」：Codex 实施，本机 Claude 审核、commit、打 tag。

## 版本表

| 版本 | 内容 | 状态 |
| --- | --- | --- |
| v0.1.0 | 计划、设计稿、骨架 | 完成（没打 tag） |
| v0.2.0 | 工具包骨架 + find | 计划已写，等批 |
| v0.x | 各引擎用到时补（比如 engine-web 要的「接着取」、附件的交法） | 随用随加 |
| v1.0.0 | 和 engine-pdf 1.0 同时冻结；之后只加不改（1.x） | 大纲 |

## v0.2.0 工具包骨架 + find（计划）

设计见 `docs/design.md`。Codex 照下表一个 Phase 一个 Phase 做，每个 Phase 做完写 `HANDOFF.md`，本机 Claude 审过再进下一个。

| Phase | 做什么 | 做完的样子 |
| --- | --- | --- |
| 1 工具清单 | `ToolSpec` 等类型（serde，读 `tools/*.json`）；自检 `check_all`；JSON Schema 子集的校验和补默认值；`examples/demo` 假引擎的三个工具清单 | demo 的清单过自检；故意写错的清单（名字、默认值越界、重名、少实现）各报对的错 |
| 2 调用、覆盖、导出、命令行 | `Toolkit`、`Ctx`、`Output`、`ToolError`、`Usage`、`Quote`；花钱工具的报价和确认；`Overrides`（上限只能收紧）；导出成 Anthropic、OpenAI 格式；`cli::run`（`tools`、`describe`、`call`、`--yes`、`--out`、`--json-only`，密钥从环境变量）；`Cancel` | demo 的命令行全流程测试过：免费的直接出结果、花钱的不加 `--yes` 只报价且没调实现、出错的交 JSON 错误和对的退出码；导出的格式和两家文档的例子对得上 |
| 3 find 的归一化 | `normalize`（带回原文的位置映射）；九项折叠；选定简繁转换表（写进决定，登记 `THIRD_PARTY.md`） | 每一项一组正反例过；映射回原文的位置在多字节字、连字、断词处都对 |
| 4 find 的查询和命中 | `Query`、`Finder`、`Hit<L>`、`Found<L>`、`Source`；整句 → 按词；前后文按原文截、不切断字；上限和 `truncated`；文本夹具（engine-pdf 样本和真实论文取出的短片段） | 整句、按词、简繁、断词、中文换行、全角数字各有命中用例；超过上限时 `total` 对、`truncated` 为真 |
| 5 收尾 | README、`CHANGELOG.md`、`HANDOFF.md`；Linux 编译检查；给各引擎的升级说明 | `./scripts/check.sh` 过；打 tag `v0.2.0` |

## 各引擎用到哪一版

| 引擎 | 用 engine-core 的哪一版 | 用在哪 |
| --- | --- | --- |
| engine-pdf | v0.2.0 起 | v0.7 工具包 + `pdf_find` |
| engine-web | v0.2.0 起 | v0.2 的工具包和 `web_find` 那一 Phase |
| engine-word | v0.2.0 起 | v0.2 的工具包和 `word_find` 那一 Phase |
| engine-txt | v0.2.0 起 | v0.2 的工具包和 `txt_find` 那一 Phase |

## 用户验收清单

要站主亲自看的，不挡开发：

- [ ] v0.2.0：在终端里跑 demo 的 `tools`、`describe`、`call`，看列出的中文和结果读起来顺不顺；花钱的那个工具不加 `--yes` 时只报价。
