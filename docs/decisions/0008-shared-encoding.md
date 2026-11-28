# 0008 文字编码的识别收进本库

2026-11-28 站主定（engine-excel v0.2 Phase 4 开工时提问）：engine-excel 读 CSV 要认编码，engine-txt v0.2 已经有一套测过的（620 行），收进本库共用，不复制、也不让格式引擎互相依赖。站主同时同意 engine-excel 的会话改 engine-txt，让它在自己的版本里换过去。

- **放哪**：`src/encoding.rs`，feature `encoding`（可选依赖 `encoding_rs`、`chardetng`），不开 feature 的引擎不带这两个库（本库「依赖要轻」）。
- **搬了什么**：engine-txt `src/encoding.rs` 的全部规矩和测试原样搬来，类型 `Encoding`、`DetectedBy`、`Newline`、`Decoded` 一起搬（JSON 不变），错误改成本库自己的 `DecodeError`（三种，各引擎映射）。为了本库的 `rust-version` 1.85，两处写法换了（`as_chunks` → `chunks_exact`，`if let` 链拆开），行为不变。`declared`（XML、HTML 声明的编码）也公开，engine-txt 读 EPUB 正文用；交回的是 `encoding_rs` 的编码，所以模块再导出 `encoding_rs`，调用方用同一份。
- **版本**：加功能，v0.3.0。engine-excel v0.2 Phase 4 起钉它，engine-txt 在 v0.3.0 里换过去（它的 `src/encoding.rs` 只留映射错误的一层）。
- **没做的**：CSV 的分隔符、按扩展名猜格式这些不是编码的事，留在各引擎。
