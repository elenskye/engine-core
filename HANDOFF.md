# HANDOFF

更新于 2026-11-28。**v0.3.0 已提交并打 tag `v0.3.0`**（本机 Claude，在 engine-excel v0.2 Phase 4 的会话里做的，等站主 push）：加 feature `encoding`，文字编码的识别从 engine-txt 搬来（`decisions/0008`，设计稿 §8，`CHANGELOG.md`）。v0.2.1 已推上 GitHub。

> 准则更新（2026-11-28）：`CLAUDE.md`「五」的「花钱的调用」换成「花钱的规矩」：一次超过 $0.5 或算不出、这个服务 24 小时合计（所有 App 一起）超过 $5 才问站主，否则直接调；一个引擎里几种付费服务分开算；引擎算、引擎记共用账本，App 从引擎取（总仓库 `docs/decisions/0011`）。「三」加了「引擎带的大文件」：App 用 `cp -c` 放一份 APFS 克隆（`0012`）。共用的那部分归本库，计划是 v0.4.0（`ROADMAP.md`）。11-27 的「改用到的引擎」（「二」末尾，`0009`）照旧。

## 本版

- `src/encoding.rs`（feature `encoding`）：`decode`、`Decoded`、`Encoding`、`DetectedBy`、`Newline`、`DecodeError`、`declared`，再导出 `encoding_rs`。规矩和测试照搬 engine-txt v0.2 的 `src/encoding.rs`；为了 `rust-version` 1.85 换了两处写法（`as_chunks`、`if let` 链），行为不变；加了一个测「给人看的写法」和 JSON 的。
- `Cargo.toml`：0.3.0，可选依赖 `encoding_rs`、`chardetng`（登记 `THIRD_PARTY.md`）；`scripts/check.sh` 加了 `--features encoding` 那一组。
- 谁在用：engine-excel v0.2 Phase 4（CSV），engine-txt v0.3.0（换掉它自己那份）。两边都先用 `[patch]` 指本机路径，等 tag 推上去再去掉（见各自的 `HANDOFF.md`）。

## 验证

- `./scripts/check.sh`：通过（fmt；不带 feature、`--features encoding` 两组 `cargo test` 和 clippy `-D warnings`；编码 10 个单元测试 + 1 个文档测试）。
- `cargo check --locked --target aarch64-unknown-linux-gnu --all-targets --features encoding`：通过。
- engine-txt 换过去以后它的全套检查、engine-excel 的 CSV 测试都用本机路径跑过（见它们的 `HANDOFF.md`）。

## 已知问题、留给以后

- 按词找只在一个单元里找；engine-excel 想要「整句按一格、按词按一行」，到时加（`ROADMAP.md` v0.x）。
- 整句是子串匹配，不要求词边界：搜 `cat` 会命中 `concatenate`。
- 「换行变空格」的查法（`over- interpreting`）找不到（`decisions/0007`）。

## 等站主

- push 提交和 tag：`git push origin main --tags`（在 `engine-core/` 里）。**要先于 engine-txt、engine-excel 推**：它们钉 `v0.3.0`。

## 下一步

各引擎照 `CHANGELOG.md` 钉 v0.2.x / v0.3.0。本库等它们提需求再出 v0.x。
