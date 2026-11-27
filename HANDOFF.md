# HANDOFF

更新于 2026-11-27。**v0.2.1 已提交并打 tag `v0.2.1`**（本机，等站主 push）：engine-pdf v0.7 审核时在真实期刊上发现软连字符后跟空格被拼成一个词，修了归一化（`CHANGELOG.md`、`decisions/0007`）。v0.2.0 已推上 GitHub。各引擎照 `CHANGELOG.md`「各引擎怎么接」钉 v0.2.x。

> 准则更新（2026-11-27）：`CLAUDE.md`「二」末尾加了「改用到的引擎」（总仓库 `docs/decisions/0009`）。用到别的引擎就能直接改它、照它的规矩提交和打 tag，动之前读它的 `CLAUDE.md`；改它已有的接口先问站主。

## 本版

Codex 实施 Phase 1–5，本机 Claude 审核后改了调用流程、错误、例子、find 的归一化和按词命中，详见 `ROADMAP.md`「v0.2.0」和 `decisions/0004`、`0007`。接口以 `docs/design.md` 为准。

## 验证

- v0.2.1：`./scripts/check.sh` 通过（fmt、测试、Clippy `-D warnings`）；`cargo check --locked --target aarch64-unknown-linux-gnu --all-targets` 通过。engine-pdf 用本机路径接 v0.2.1 跑了它的全套检查和真实论文查找（见 engine-pdf `HANDOFF.md`）。
- v0.2.0：`./scripts/check.sh`：通过（fmt、31 项测试、Clippy `-D warnings`）。
- `cargo check --locked --target aarch64-unknown-linux-gnu --all-targets`：通过。Linux 上原生跑测试没做。
- 命令行实跑 demo：`tools`、`describe`、`call` 免费工具出结果；`demo_meter` 不加 `--yes` 只报价、退出码 3；加 `--yes` 和模拟密钥执行并带用量；参数错退出码 2。同样的流程在 `examples/demo/tests.rs` 里自动测。
- 真实文件核对（`local/review/`，不进 Git，用法见那里的 README）：engine-pdf 读出 38 份真实论文和杂志（Zotero 副本 28 份 + engine-pdf `local/pdf` 10 份，891 页，268 万字）和 8 页扫描件（本机 OCR）。从原文随机截 2,413 个片段：原样去找全部在原处整句命中；「模型照着打」的写法失败 5 处（0.2%），都是核对程序把全角数字当成中文造成的假失败；扫描件 51 个片段 0 失败。891 页归一化共约 0.1 秒（release）。
- 没做：模型 API 实调（本库不联网）；各引擎的实际接入。

## 已知问题、留给以后

- 按词找只在一个单元里找；engine-excel 想要「整句按一格、按词按一行」，到时加（`ROADMAP.md` v0.x）。
- 整句是子串匹配，不要求词边界（和阅读器的查找一样）：搜 `cat` 会命中 `concatenate`。各引擎若要整词，到时再加选项。
- 「换行变空格」的查法（`over- interpreting`、`Socia\u{ad} bleness`）找不到：连字符或软连字符后跟空格（不是换行）不拼词，免得把 `pre- and post-`、`all\u{ad} the` 拼错。

## 等站主

- push 提交和 tag：`git push origin main --tags`（在 `engine-core/` 里）。**要先于 engine-pdf 推**：engine-pdf v0.7.0 钉 `v0.2.1`。

## 下一步

各引擎在自己的仓库钉 `v0.2.0` 做工具包和 find：engine-pdf v0.7 Phase 2、engine-web Phase 4、engine-word / engine-txt / engine-excel Phase 5。本库等它们提需求再出 v0.x。
