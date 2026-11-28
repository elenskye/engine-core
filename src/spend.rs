//! 花钱的规矩（站主 2026-11-28 定，总仓库 `decisions/0011`），所有引擎、所有 App 一样：
//!
//! - 一次超过 [`ASK_ABOVE_USD`]（$0.5）或算不出这一次要花多少，先问用户；
//! - 这个服务最近 24 小时（所有 App 合计）已花的，加上这一次超过 [`DAY_LIMIT_USD`]（$5），也先问；
//! - 别的情况直接调用。一个引擎里几种付费服务各算各的。
//!
//! 账本是本机的 JSON Lines，一个服务一份：`<底>/engine-<引擎>/<服务>-ledger.jsonl`。底默认
//! `~/Library/Application Support`，所有 App 共用；环境变量 [`LEDGER_DIR_ENV`] 改底（测试、验收用）。
//! 真花了钱就由引擎的服务客户端记一行（[`Book::record`]），模拟的服务不记；App 要报告花了多少就读
//! [`Book::entries`]，不另记一份。

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::call::Usage;

/// 一次超过这么多先问。
pub const ASK_ABOVE_USD: f64 = 0.5;
/// 最近 24 小时（所有 App 合计）加上这一次超过这么多先问。
pub const DAY_LIMIT_USD: f64 = 5.0;
/// 合计的时间窗：往回滚动 24 小时。
pub const WINDOW: Duration = Duration::from_secs(24 * 60 * 60);
/// 改共用账本的底（代替 `~/Library/Application Support`）。
pub const LEDGER_DIR_ENV: &str = "ENGINE_LEDGER_DIR";

/// 账本放在哪。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Ledger {
    /// 所有 App 共用：`$ENGINE_LEDGER_DIR`，否则 `~/Library/Application Support`。
    #[default]
    Shared,
    /// 换一个底（App 的开发版、测试），下面的布局和共用的一样。
    At(PathBuf),
    /// 不记、不读：只按单次的规矩问。
    Off,
}

impl Ledger {
    /// 一个引擎（短名，如 `pdf`、`web`）的一个服务（如 `Mathpix`、`tavily`）的账。
    pub fn book(&self, engine: &str, service: &str) -> Book {
        let base = match self {
            Self::Shared => std::env::var_os(LEDGER_DIR_ENV)
                .filter(|dir| !dir.is_empty())
                .map(PathBuf::from)
                .or_else(|| {
                    std::env::var_os("HOME")
                        .map(|home| PathBuf::from(home).join("Library/Application Support"))
                }),
            Self::At(base) => Some(base.clone()),
            Self::Off => None,
        };
        Book {
            path: base.map(|base| {
                base.join(format!("engine-{}", slug(engine)))
                    .join(format!("{}-ledger.jsonl", slug(service)))
            }),
            service: service.to_owned(),
        }
    }
}

/// 文件名里用的写法：小写字母、数字、`-`，别的换成 `-`。
fn slug(name: &str) -> String {
    let slug: String = name
        .trim()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    if slug.is_empty() {
        "unnamed".into()
    } else {
        slug
    }
}

/// 账本的一行：一次真花了钱的调用。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    /// Unix 秒。
    pub at: u64,
    /// 哪个 App（Peck、Pith、Pomelo、命令行…）。
    pub app: String,
    pub service: String,
    pub unit: String,
    pub units: f64,
    pub usd: f64,
    /// 按估算记的（服务没报用量、上传后失败等）。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub estimated: bool,
}

impl Entry {
    /// 照一次调用的用量记一行（现在）。
    pub fn of(app: &str, usage: &Usage) -> Self {
        Self {
            at: unix(SystemTime::now()),
            app: app.to_owned(),
            service: usage.service.clone(),
            unit: usage.unit.clone(),
            units: usage.units,
            usd: usage.usd,
            estimated: usage.estimated,
        }
    }
}

/// 为什么要先问。
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Why {
    /// 算不出这一次要花多少。
    UnknownCost,
    /// 这一次超过 [`ASK_ABOVE_USD`]。
    Over { usd: f64 },
    /// 最近 24 小时已花 `spent`，加上这一次超过 [`DAY_LIMIT_USD`]。
    Day { spent: f64, usd: f64 },
}

impl std::fmt::Display for Why {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownCost => f.write_str("算不出这一次要花多少。"),
            Self::Over { usd } => write!(f, "这一次预计 ${usd:.3}，超过 ${ASK_ABOVE_USD}。"),
            Self::Day { spent, usd } => write!(
                f,
                "所有 App 最近 24 小时已花 ${spent:.3}，加上这一次 ${usd:.3} 超过 ${DAY_LIMIT_USD}。"
            ),
        }
    }
}

/// 一个服务的账。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Book {
    path: Option<PathBuf>,
    service: String,
}

impl Book {
    /// 账本文件；[`Ledger::Off`] 没有。
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn service(&self) -> &str {
        &self.service
    }

    /// 记一次（追加一整行；几个 App 同时写也不会写坏一行）。
    pub fn record(&self, entry: &Entry) -> std::io::Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let mut line = serde_json::to_vec(entry).map_err(std::io::Error::other)?;
        line.push(b'\n');
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?
            .write_all(&line)
    }

    /// 照用量记一次：花了钱（`usd > 0`）才记。
    pub fn record_usage(&self, app: &str, usage: &Usage) -> std::io::Result<()> {
        if usage.usd.is_finite() && usage.usd > 0.0 {
            self.record(&Entry::of(app, usage))
        } else {
            Ok(())
        }
    }

    /// `since` 以来的各行，按时间先后；读不了、写坏的行跳过。
    pub fn entries(&self, since: SystemTime) -> Vec<Entry> {
        let Some(text) = self
            .path
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok())
        else {
            return vec![];
        };
        let since = unix(since);
        let mut out: Vec<Entry> = text
            .lines()
            .filter_map(|line| serde_json::from_str::<Entry>(line).ok())
            .filter(|e| e.at >= since && e.usd.is_finite())
            .collect();
        out.sort_by_key(|e| e.at);
        out
    }

    /// `now` 之前 24 小时所有 App 花的。
    pub fn spent(&self, now: SystemTime) -> f64 {
        self.entries(now.checked_sub(WINDOW).unwrap_or(UNIX_EPOCH))
            .iter()
            .filter(|e| unix(now) >= e.at)
            .map(|e| e.usd.max(0.0))
            .sum()
    }

    /// 这一次（`usd`，算不出时 `None`）要不要先问用户、为什么。不花钱的不问。
    pub fn approval(&self, usd: Option<f64>) -> Option<Why> {
        self.approval_at(usd, SystemTime::now())
    }

    pub fn approval_at(&self, usd: Option<f64>, now: SystemTime) -> Option<Why> {
        let usd = match usd {
            Some(usd) if usd.is_finite() && usd >= 0.0 => usd,
            _ => return Some(Why::UnknownCost),
        };
        if usd == 0.0 {
            return None;
        }
        if usd > ASK_ABOVE_USD {
            return Some(Why::Over { usd });
        }
        let spent = self.spent(now);
        (spent + usd > DAY_LIMIT_USD).then_some(Why::Day { spent, usd })
    }
}

pub(crate) fn unix(t: SystemTime) -> u64 {
    t.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp() -> (tempfile::TempDir, Ledger) {
        let dir = tempfile::tempdir().unwrap();
        let ledger = Ledger::At(dir.path().join("nested"));
        (dir, ledger)
    }

    fn at(now: SystemTime, ago: u64, app: &str, usd: f64) -> Entry {
        Entry {
            at: unix(now) - ago,
            app: app.into(),
            service: "Mathpix".into(),
            unit: "page".into(),
            units: usd / 0.005,
            usd,
            estimated: false,
        }
    }

    #[test]
    fn one_call_above_half_a_dollar_or_unknown_asks() {
        let (_dir, ledger) = temp();
        let book = ledger.book("pdf", "Mathpix");
        assert_eq!(book.approval(Some(0.0)), None);
        assert_eq!(book.approval(Some(0.5)), None);
        assert_eq!(book.approval(Some(0.505)), Some(Why::Over { usd: 0.505 }));
        assert_eq!(book.approval(None), Some(Why::UnknownCost));
        assert_eq!(book.approval(Some(f64::NAN)), Some(Why::UnknownCost));
        assert_eq!(book.approval(Some(-1.0)), Some(Why::UnknownCost));
    }

    #[test]
    fn the_last_day_of_every_app_counts_and_rolls() {
        let (_dir, ledger) = temp();
        let book = ledger.book("pdf", "Mathpix");
        let now = UNIX_EPOCH + Duration::from_secs(10 * 24 * 3600);
        book.record(&at(now, 25 * 3600, "Peck", 3.0)).unwrap(); // 一天以前的不算
        book.record(&at(now, 3600, "Pomelo", 2.4)).unwrap();
        book.record(&at(now, 60, "Pith", 2.4)).unwrap();
        OpenOptions::new()
            .append(true)
            .open(book.path().unwrap())
            .unwrap()
            .write_all(b"not json\n")
            .unwrap();
        assert!((book.spent(now) - 4.8).abs() < 1e-9);
        assert_eq!(book.approval_at(Some(0.2), now), None);
        let Some(Why::Day { spent, usd }) = book.approval_at(Some(0.25), now) else {
            panic!("要问")
        };
        assert!((spent - 4.8).abs() < 1e-9 && usd == 0.25);
        // Pomelo 那一笔滚出窗口以后不再算
        let later = now + Duration::from_secs(24 * 3600 - 3600 + 1);
        assert_eq!(book.approval_at(Some(0.25), later), None);
        let apps: Vec<String> = book
            .entries(UNIX_EPOCH)
            .into_iter()
            .map(|e| e.app)
            .collect();
        assert_eq!(apps, ["Peck", "Pomelo", "Pith"]);
        assert!(
            Why::Day {
                spent: 4.8,
                usd: 0.25
            }
            .to_string()
            .contains("最近 24 小时已花 $4.800")
        );
    }

    #[test]
    fn services_and_engines_keep_separate_books() {
        let (dir, ledger) = temp();
        let mathpix = ledger.book("pdf", "Mathpix");
        let tavily = ledger.book("web", "tavily");
        let now = SystemTime::now();
        mathpix.record(&at(now, 0, "Pomelo", 4.9)).unwrap();
        assert_eq!(tavily.approval_at(Some(0.4), now), None);
        assert!(mathpix.approval_at(Some(0.4), now).is_some());
        assert_eq!(
            mathpix.path().unwrap(),
            dir.path().join("nested/engine-pdf/mathpix-ledger.jsonl")
        );
        assert_eq!(
            tavily.path().unwrap(),
            dir.path().join("nested/engine-web/tavily-ledger.jsonl")
        );
    }

    #[test]
    fn usage_is_recorded_only_when_it_cost_money_and_off_records_nothing() {
        let (_dir, ledger) = temp();
        let book = ledger.book("web", "tavily");
        let usage = |usd| Usage {
            service: "tavily".into(),
            unit: "credit".into(),
            units: usd / 0.008,
            usd,
            estimated: true,
        };
        book.record_usage("Peck", &usage(0.0)).unwrap();
        book.record_usage("Peck", &usage(0.016)).unwrap();
        let entries = book.entries(UNIX_EPOCH);
        assert_eq!(entries.len(), 1);
        assert!(entries[0].estimated && entries[0].unit == "credit");

        let off = Ledger::Off.book("web", "tavily");
        off.record_usage("Peck", &usage(1.0)).unwrap();
        assert!(off.path().is_none() && off.entries(UNIX_EPOCH).is_empty());
        assert_eq!(off.approval(Some(0.4)), None);
    }

    #[test]
    fn names_become_safe_file_names() {
        let book = Ledger::At("/base".into()).book("PDF", "Mathpix API/v3");
        assert_eq!(
            book.path().unwrap(),
            Path::new("/base/engine-pdf/mathpix-api-v3-ledger.jsonl")
        );
    }
}
