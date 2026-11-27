//! engine-core：各引擎共用的工具清单格式、调用入口、命令行骨架、find 和取消（v0.1.0 只有计划，见 `docs/design.md`）。

/// 本库的版本，和 `Cargo.toml` 一致。
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    #[test]
    fn version_matches_manifest() {
        assert_eq!(super::VERSION, "0.1.0");
    }
}
