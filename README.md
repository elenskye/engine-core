# engine-core

personal-engine-studio 各引擎共用的小库（纯 Rust，macOS、Linux 都能用）：工具清单的格式、自检、覆盖和导出，按名字调用（报价、确认、结果校验），命令行骨架，统一 find 的归一化、查询和命中，取消；feature `encoding` 另有文字编码的识别（v0.3.0 起）。它自己没有工具，各引擎钉它的 tag 来做「给 Agent 的工具包」和 `<短名>_find`。

```toml
[dependencies]
engine-core = { git = "https://github.com/elenskye/engine-core", tag = "v0.3.0" }
# 要认文字编码（txt、CSV）的：
# engine-core = { git = "https://github.com/elenskye/engine-core", tag = "v0.3.0", features = ["encoding"] }
```

升级改 tag，回退改回上一个。接法见 [CHANGELOG](CHANGELOG.md#v030)，接口见 [设计稿](docs/design.md)，照着抄的样子见 [examples/demo](examples/demo/main.rs)。

```rust
use engine_core::find::{Finder, Query, Source};

let page = "前認\n知後";
let mut finder = Finder::new(&Query::new("认知"));
finder.scan(page, Source::Text, |range| range);   // range：原文里的字节区间，引擎换成页码、行号……
let hit = &finder.finish().hits[0];
assert_eq!(&page[hit.location.clone()], "認\n知");
```

## 本机检查和试用

```bash
./scripts/check.sh
cargo check --locked --target aarch64-unknown-linux-gnu --all-targets --features encoding
cargo run --example demo -- tools
cargo run --example demo -- call demo_find '{"file":"examples/demo/sample.txt","query":"認知模型"}'
cargo run --example demo -- call demo_meter '{"units":2}'
```

最后一条只报价（退出码 3）；加 `--yes` 并设 `ENGINE_KEY_DEMO_KEY=demo` 才模拟执行。规则见 [CLAUDE.md](CLAUDE.md)，现状见 [HANDOFF.md](HANDOFF.md)。
