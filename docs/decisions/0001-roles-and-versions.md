# 0001 分工、版本、谁用本库

2026-11-27 站主定（总仓库 `docs/decisions/0003`、`0006`、`0007`）。

- **为什么有本库**：每个引擎都要做「给 Agent 的工具包」和统一的 find，格式和归一化要一模一样。放进一个独立的小库，各引擎钉它的 tag，不各抄一份，也不放进 engine-pdf（别的引擎不该为了几段共用代码带上 PDF 引擎和它的 Mac 依赖）。
- **分工**：Codex 在本机实施，本机 Claude 设计、审核、commit、打 tag；push 由站主做。云端 Claude 不做本库。
- **版本**：照 `CLAUDE.md`「三」。engine-pdf v0.7、engine-web、engine-word、engine-txt 的工具包和 find 都钉 v0.2.0 起的 tag。**1.0 和 engine-pdf 1.0 同时冻结**，之后只加不改（1.x），因为每个引擎都站在它上面。
- 本库没有 App 直接接入；它的「接入」就是各引擎钉上它的 tag。
