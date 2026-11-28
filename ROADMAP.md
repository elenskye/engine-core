# ROADMAP

现在：**v0.4.0 已发（tag `v0.4.0`，2026-11-28，等站主 push）**：花钱的规矩 `spend`、清理程序（feature `clean`）。engine-pdf、engine-web、engine-txt、engine-excel 都改钉了它（tag 推上去之前指本机路径）；要认文字编码的开 `encoding`。分工见 `CLAUDE.md`「二」：Codex 实施，本机 Claude 审核、commit、打 tag。

## 版本表

| 版本 | 内容 | 状态 |
| --- | --- | --- |
| v0.1.0 | 计划、设计稿、骨架 | 完成（没打 tag） |
| v0.2.0 | 工具包骨架 + find | 完成（tag `v0.2.0`） |
| v0.2.1 | 软连字符后跟空格不拼词（engine-pdf v0.7 审核时发现） | 完成（tag `v0.2.1`） |
| v0.3.0 | 文字编码的识别（feature `encoding`，从 engine-txt 搬来；engine-excel v0.2 Phase 4 提，站主 2026-11-28 定，`decisions/0008`） | 完成（tag `v0.3.0`） |
| v0.4.0 | 共用的花钱规矩（`spend`）+ 清理程序（feature `clean`，`engine-clean`）（站主 2026-11-28 定，总仓库 `decisions/0011`、`0013`） | 完成（tag `v0.4.0`） |
| v0.x | 各引擎用到时补 | 随用随加。已经有人提的：按词找用比整句大的单元（engine-excel：整句按一格、按词按一行） |
| v1.0.0 | 和 engine-pdf 1.0 同时冻结；之后只加不改（1.x） | 大纲 |

## v0.4.0 花钱的规矩 + 清理程序（完成）

站主 2026-11-28 定规矩（总仓库 `decisions/0011` 花钱、`0013` 清理），并让本机 Claude 直接做、不等批。所有引擎这时都停着，调用方一起改。

| Phase | 内容 |
| --- | --- |
| 1 花钱 | `spend`：门槛（一次 $0.5、24 小时 $5）、按引擎和服务分开的共用账本（`Ledger` 在哪、`Book` 一个服务的账、`Entry` 一行、`Why` 为什么问）。`Toolkit` 照账本定要不要问（`with_ledger`、`Prepared::why`），`Confirm` 只能更严；命令行只在要问时才要 `--yes`，交回原因；命令行的附件临时目录带 `engine-cli-` 前缀 |
| 2 清理 | feature `clean` 的 `clean` 模块和 `engine-clean` 命令：分开工、每轮、发版三档，加一档 `all`；默认只列清单，`--yes` 才动手；编译缓存和可重装的直接删，中间产物进废纸篓；只碰 Git 不收的；在跑的程序不清 |
| 3 调用方 | engine-pdf（改用 `spend`，钉本版）、engine-web（Tavily 记账、照门槛问）、engine-txt / engine-excel（钉本版）；各引擎加 `scripts/clean.sh` 和开工钩子，`local/` 的中间产物挪进 `local/runs/v<版本>/` |
| 4 收尾 | 各引擎 `check.sh`、文档、`CHANGELOG.md`、打 tag `v0.4.0`；第一次清理先列清单给站主看 |

四个 Phase 在同一个会话里做完（本机 Claude，2026-11-28），见 `HANDOFF.md`。

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
| engine-pdf | v0.4.0（v0.8.0 起；之前 v0.2.1） | 工具包、`pdf_find`、Mathpix 的花钱规矩和账本 |
| engine-web | v0.4.0（v0.4.0 起；之前 v0.2.0） | 工具包、`web_find`、Tavily 的花钱规矩和账本 |
| engine-word | 还没接（v0.2 Phase 5 接） | 工具包和 `word_find` |
| engine-txt | v0.4.0（开 `encoding`） | 工具包、`txt_find`、编码识别 |
| engine-excel | v0.4.0（开 `encoding`） | CSV 编码；Phase 5 的工具包和 `excel_find` |
| 所有引擎 | `scripts/clean.sh` 调本机 engine-core 的 `engine-clean`（不经依赖） | 清理 |

## 用户验收清单

要站主亲自看的，不挡开发：

- [ ] v0.2.0：在终端里跑 README「本机检查和试用」那几条，看列出的中文和结果读起来顺不顺；`demo_meter` 不加 `--yes` 时只报价。
