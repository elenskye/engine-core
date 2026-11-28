//! engine-core：各引擎共用的工具清单、调用、命令行、查找、取消和花钱的规矩；feature `encoding` 另有文字编码的识别，
//! feature `clean` 另有清理开发目录的 `engine-clean`。

pub mod call;
pub mod cancel;
#[cfg(feature = "clean")]
pub mod clean;
pub mod cli;
#[cfg(feature = "encoding")]
pub mod encoding;
pub mod find;
pub mod overrides;
mod schema;
pub mod spend;
pub mod tool;

pub use cancel::{Cancel, Cancelled};

/// 本库的版本，和 `Cargo.toml` 一致。
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    #[test]
    fn version_matches_manifest() {
        assert_eq!(super::VERSION, "0.4.0");
    }
}
