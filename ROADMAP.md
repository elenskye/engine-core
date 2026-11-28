# ROADMAP

现在：**v0.3.0 已发（tag `v0.3.0`，加 feature `encoding`）**。各引擎做工具包和 find 钉 v0.2.x 或 v0.3.0；要认文字编码的开 `encoding`。分工见 `CLAUDE.md`「二」：Codex 实施，本机 Claude 审核、commit、打 tag。

## 版本表

| 版本 | 内容 | 状态 |
| --- | --- | --- |
| v0.1.0 | 计划、设计稿、骨架 | 完成（没打 tag） |
| v0.2.0 | 工具包骨架 + find | 完成（tag `v0.2.0`） |
| v0.2.1 | 软连字符后跟空格不拼词（engine-pdf v0.7 审核时发现） | 完成（tag `v0.2.1`） |
| v0.3.0 | 文字编码的识别（feature `encoding`，从 engine-txt 搬来；engine-excel v0.2 Phase 4 提，站主 2026-11-28 定，`decisions/0008`） | 完成（tag `v0.3.0`） |
| v0.4.0 | 共用的花钱规矩（站主 2026-11-28 定，`CLAUDE.md`「五」，总仓库 `decisions/0011`）：一次超过 $0.5 或算不出、这个服务 24 小时合计超过 $5 才问；按服务分开的共用账本（JSON Lines，所有 App 共用，App 从这里取）；问的原因交给 App 和命令行。`prepare` 和命令行照门槛走（现在命令行一切正金额都要 `--yes`），`Confirm` 的覆盖只能改严。从 engine-pdf v0.8 的 `src/mathpix/spend.rs` 搬，engine-pdf、engine-web 改用它 | 大纲，开工前写 Phase 计划 |
| v0.x | 各引擎用到时补 | 随用随加。已经有人提的：按词找用比整句大的单元（engine-excel：整句按一格、按词按一行） |
| v1.0.0 | 和 engine-pdf 1.0 同时冻结；之后只加不改（1.x） | 大纲 |

## v0.2.0 工具包骨架 + find（完成）

5 个 Phase：工具清单 → 调用、覆盖、导出、命令行 → find 的归一化 → find 的查询和命中 → 收尾。Codex 实施，本机 Claude 审核时用 38 份真实论文和杂志（891 页）加 8 页扫描件实测，改了：

- **调用分两步**：`prepare`（校验、报价、要不要确认）→ `run`，App 可以先弹确认卡片再执行；命令行去掉了截报价的旁路（`decisions/0004`）。
- **错误说明只写在清单里**：`ToolError::of(种类)`，Toolkit 照清单补齐；通用种类带默认说明。
- **例子跑成测试**：`check_examples`；`LocalHost` 让各引擎的测试一行建好上下文；命令行加了 `run_with` 和自动测试（原来只手动试过）。
- **find 问法统一**：`<短名>_find` 自检查 `query`、`max_hits`、`context` 和结果字段；`Query` 能直接从工具参数反序列化。demo 加了 `demo_find` 作接法样板。
- **find 在真实论文上的问题**（`decisions/0007`）：中文标点旁的换行、带连字符的复合词在行尾断开、软连字符埋在词里，都让模型照着打的句子找不到（失败率 6–8% → 0.2%）；按词的命中会把整页当一处，改成每个词各算一处。

## 各引擎用到哪一版

| 引擎 | 用 engine-core 的哪一版 | 用在哪 |
| --- | --- | --- |
| engine-pdf | v0.2.1 起 | v0.7 工具包 + `pdf_find` |
| engine-web | v0.2.0 起 | v0.2 的工具包和 `web_find` |
| engine-word | v0.2.0 起 | v0.2 的工具包和 `word_find` |
| engine-txt | v0.2.0 起；v0.3.0 起开 `encoding` | v0.2 的工具包和 `txt_find`；v0.3 起编码识别用本库的 |
| engine-excel | v0.3.0 起（开 `encoding`） | v0.2 的 CSV 编码（Phase 4）、工具包和 `excel_find`（Phase 5） |

## 用户验收清单

要站主亲自看的，不挡开发：

- [ ] v0.2.0：在终端里跑 README「本机检查和试用」那几条，看列出的中文和结果读起来顺不顺；`demo_meter` 不加 `--yes` 时只报价。
