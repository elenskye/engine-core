# 第三方依赖

引入依赖时在这里登记一行（`CLAUDE.md`「五」）。本库的依赖每个引擎都会带上，要轻。

| 名称 | 版本 | 许可证 | 用途 | 链接 |
| --- | --- | --- | --- | --- |
| `serde` | `^1.0.229` | MIT OR Apache-2.0 | 工具清单和调用结果的序列化 | [crate](https://crates.io/crates/serde) |
| `serde_json` | `^1.0.154` | MIT OR Apache-2.0 | 清单、参数和命令行的 JSON | [crate](https://crates.io/crates/serde_json) |
| `tempfile` | `^3.27.0` | MIT OR Apache-2.0 | 自动清理临时文件附件和 CLI 输出暂存 | [crate](https://crates.io/crates/tempfile) |
| `unicode-general-category` | `^1.1.0` | Apache-2.0 | Unicode 标点分类 | [crate](https://crates.io/crates/unicode-general-category) |
| `unicode-normalization` | `^0.1.26` | (MIT OR Apache-2.0) AND Unicode-3.0 | 带原文位置映射的兼容分解和重组 | [crate](https://crates.io/crates/unicode-normalization) |
| `unicode-script` | `^0.5.9` | MIT OR Apache-2.0 | 汉字和拉丁字母的脚本识别 | [crate](https://crates.io/crates/unicode-script) |
| `zeroize` | `^1.9.1` | Apache-2.0 OR MIT | 清零调用期间用到的密钥 | [crate](https://crates.io/crates/zeroize) |
| OpenCC `TSCharacters.txt` | `ver.1.1.9` | Apache-2.0 | 离线逐字简繁折叠，许可证随 `src/find/data/` 保存 | [原表](https://github.com/BYVoid/OpenCC/blob/ver.1.1.9/data/dictionary/TSCharacters.txt) |
