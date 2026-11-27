# engine-core

personal-engine-studio 各引擎共用的小库（纯 Rust）：工具清单的格式、校验、覆盖和导出，按名字调用的入口，命令行骨架，find 的归一化、查询和命中类型，取消，错误和费用的约定。各引擎用它做「给 Agent 的工具包」和统一的 find。

> 现在是 **v0.1.0**：只有计划和设计稿，还没有可用的接口。下一步 v0.2.0：工具包骨架 + find。进度见 `ROADMAP.md`，交接见 `HANDOFF.md`。

## 各引擎怎么引用

钉一个 tag：

```toml
[dependencies]
engine-core = { git = "https://github.com/elenskye/engine-core", tag = "v0.2.0" }
```

升级改 tag，回退改回上一个。每版接口变化见 `CHANGELOG.md`。

## 开发

```bash
./scripts/check.sh
```

规则见 `CLAUDE.md`（Codex 读 `AGENTS.md`，是同一份），设计见 `docs/design.md`，选型见 `docs/decisions/`。
