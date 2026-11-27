# HANDOFF

更新：2026-11-27 · v0.1.0 · 本机 Claude 建仓库、写计划

## 现在在哪

只有计划和骨架：`CLAUDE.md`（开发准则照抄总仓库 `GUIDE.md`）、`docs/design.md`（工具清单格式、调用、覆盖和导出、命令行、find、取消）、`ROADMAP.md`（v0.2.0 的 Phase 表）、两份决定。`src/lib.rs` 只有版本常量。

## 测试

- `./scripts/check.sh`：通过（`cargo fmt --check`、`cargo test`、`cargo clippy --all-targets -D warnings`；只有版本测试）。

## 等你

1. GitHub 上建 `elenskye/engine-core`，照对话里给的命令加远端、push。
2. 看 `ROADMAP.md` 的 v0.2.0 计划，说开工。

## 下一轮

站主说开工后，让 Codex 做 v0.2.0 Phase 1：「读 engine-core 的 AGENTS.md、HANDOFF.md、ROADMAP.md『v0.2.0』、docs/design.md §1；做 Phase 1（工具清单类型、自检、参数校验和默认值、demo 假引擎的清单），做完写 HANDOFF」。做完回到本机 Claude 审核。engine-web、engine-word、engine-txt 的工具包和 find 都等本库 v0.2.0 的 tag，越早发越好。
