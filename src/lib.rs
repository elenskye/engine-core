//! engine-core：各引擎共用的工具清单、调用、命令行、查找和取消。

pub mod call;
pub mod cancel;
pub mod cli;
pub mod find;
pub mod overrides;
mod schema;
pub mod tool;

pub use cancel::{Cancel, Cancelled};

/// 本库的版本，和 `Cargo.toml` 一致。
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    #[test]
    fn version_matches_manifest() {
        assert_eq!(super::VERSION, "0.2.0");
    }
}
