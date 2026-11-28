# HANDOFF

更新于 2026-11-28。**v0.4.1 已提交并打 tag `v0.4.1`**（同日先发了 v0.4.0；本机 Claude，在总仓库的会话里做的；所有引擎都停着，站主让直接做、不等批；等站主 push）：花钱的规矩 `spend` 和清理程序（feature `clean`，命令行 `engine-clean`）。v0.4.1 只修 `scripts/clean.sh`：按版本和源码指纹重装 `engine-clean`（v0.4.0 只比版本号，装着的是修硬链接之前的那份，第一次列清单时算多了约三成）。v0.3.0、v0.2.1 已推上 GitHub。

> 准则更新（2026-11-28）：「五」加了「花钱的规矩」（`decisions/0011`）和「清理」（`0013`），「三」加了「引擎带的大文件」（`0012`）；这两样共用的代码就是本版。11-27 的「改用到的引擎」（「二」末尾，`0009`）照旧。

## 本版

- `src/spend.rs`：门槛（$0.5、$5、24 小时）、`Ledger` → `Book`（一个引擎的一个服务）、`Entry`、`Why`；账本 `<底>/engine-<引擎>/<服务>-ledger.jsonl`，底默认 `~/Library/Application Support`，`ENGINE_LEDGER_DIR` 改。照 engine-pdf v0.8 的 `mathpix/spend.rs` 写，按服务分开、`Entry` 记单位和用量（不只页数）。
- `Toolkit`：`with_ledger`、`ledger()`；`prepare` 先照账本判断（工具名下划线前是引擎短名，报价的服务是哪份账），`Prepared::why()`；清单和覆盖只能更严。
- 命令行：只在要确认时才要 `--yes`，回复带 `why`；附件临时目录 `engine-cli-` 开头。
- `src/clean.rs` + `src/bin/engine-clean.rs`（feature `clean`）：开工、每轮、发版、全部四档，只列 / `--yes` 动手 / `--quiet`；`scripts/clean.sh` 按需 `cargo install` 到 `~/.cargo/bin`（已装：`engine-clean 0.4.0`）。
- `scripts/check.sh` 加 `clean` 一组，开头设 `ENGINE_LEDGER_DIR`；`.claude/settings.json` 加开工钩子；`local/review` 挪到 `local/runs/v0.2.0/review`。设计稿 §9、§10，`CHANGELOG.md`「v0.4.0」。

## 验证

- `./scripts/check.sh`：通过（fmt；不带 feature、`encoding`、`clean` 三组测试和 clippy `-D warnings`）。新测试：门槛和滚动的 24 小时、几个 App 合计、引擎和服务分开、坏行跳过、`Off`；`Toolkit` 照账本问、清单只能更严；命令行小额直接执行、超了交 `why`；清理的四档各列什么、真样本和 Git 收着的不动、在跑不清、临时目录看时间、硬链接只算一次、进废纸篓的名字。
- `cargo check --target aarch64-unknown-linux-gnu --all-targets --features clean,encoding`：通过。
- 调用方：engine-pdf、engine-web、engine-txt、engine-excel 的 `./scripts/check.sh` 用本机这份都通过（见各自的 `HANDOFF.md`）。
- `engine-clean` 在真实的引擎目录上只列不删跑过：认出 `target/`、已发版本的 `local/runs/`、可重装的；engine-pdf 的 target 有检查在跑时跳过，没发版的 `local/runs/v0.8.0` 留着。

## 已知问题、留给以后

- 报价函数只能交一个数：「算不出」只在引擎自己的代码里用（`Book::approval(None)`）；工具包里报不出数的工具要报上限。
- 废纸篓是改名挪进 `~/.Trash`，Finder 的「放回原处」用不了。
- 按词找只在一个单元里找；整句是子串匹配；「换行变空格」的查法找不到（同 v0.3.0）。

## 等站主

- push 提交和 tag（`v0.4.0`、`v0.4.1`）：`git push origin main --tags`（在 `engine-core/` 里）。**最先推**：engine-pdf、engine-web、engine-txt、engine-excel 都等它，推上去后它们才能从本机路径换回 git tag。
- 第一次清理：列出的清单在总仓库会话里给站主看，点头后清。

## 下一步

各引擎 tag 推上去以后把 `engine-core = { path = "../engine-core" … }` 换回 `{ git = …, tag = "v0.4.1" … }`（engine-pdf、engine-web、engine-txt、engine-excel 的 `Cargo.toml` 里都留了注释）。本库等它们提需求再出 v0.x。
