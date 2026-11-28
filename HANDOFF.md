# HANDOFF

更新于 2026-11-28。**v0.3.0 已提交并打 tag `v0.3.0`**（本机 Claude，在 engine-excel v0.2 Phase 4 的会话里做的，等站主 push）：加 feature `encoding`，文字编码的识别从 engine-txt 搬来（`decisions/0008`，设计稿 §8，`CHANGELOG.md`）。v0.2.1 已推上 GitHub。

> 准则（2026-11-27）：`CLAUDE.md`「二」末尾「改用到的引擎」（总仓库 `docs/decisions/0009`）。

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
