//! 清理引擎的开发目录（feature `clean`，命令行 `engine-clean`；站主 2026-11-28 定，总仓库 `decisions/0013`）。
//!
//! 能清的只有这几类，别的一概不碰：
//!
//! | 类 | 在哪 | 怎么清 |
//! | --- | --- | --- |
//! | 编译缓存 | `target/` | 直接删（能重建）；有程序从里面跑着就不清 |
//! | 这一轮的临时产物 | `local/tmp/` | 进废纸篓 |
//! | 缓存 | `local/` 下的 `swift-cache`、`__pycache__`、`.pytest_cache`、`.build` | 直接删 |
//! | 已发版本的中间产物 | `local/runs/v<版本>/`，且 Git 里有 tag `v<版本>` | 进废纸篓 |
//! | 可重装的依赖和输出 | `node_modules`、`web/dist`、`samples/out`、`output/playwright`、`.playwright-cli` | 直接删 |
//! | 引擎留下的临时目录 | 系统临时目录里 `engine-` 开头、一天没动过的 | 直接删 |
//!
//! 引擎目录里的东西都要 Git 不收（`git check-ignore`）才清。`local/` 里除了 `tmp/`、`runs/` 和缓存，
//! 一律不动：真实样本副本、引擎带的大文件（如 `local/pdfium/`）放在那里。花钱的账本不在引擎目录里，也不碰。
//!
//! 什么时候清分三档（[`Tier`]）：开工时、每轮 commit 之后、发版打 tag 之后；另有一档 `All`，第一次清理用。

use std::collections::BTreeSet;
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime};

/// 什么时候清。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tier {
    /// 开工时：`target/` 超过上限（默认 3 GB）就整份清。
    Start,
    /// 每轮 commit 之后：`local/tmp/`、`local/` 下的缓存、引擎留下超过一天的临时目录。
    Round,
    /// 发版打 tag 之后：每轮的那些，加已发版本的 `local/runs/`、`target/` 里 `debug` 以外的（release、交叉编译），
    /// 和可重装的依赖和输出。
    Release,
    /// 能清的全清（第一次清理用）：发版那一档，加整个 `target/`。
    All,
}

impl Tier {
    pub fn parse(flag: &str) -> Option<Self> {
        match flag {
            "--start" => Some(Self::Start),
            "--round" => Some(Self::Round),
            "--release" => Some(Self::Release),
            "--all" => Some(Self::All),
            _ => None,
        }
    }
}

/// 清的是哪一类。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    Target,
    Tmp,
    Cache,
    Runs,
    Reinstall,
    TempLeftover,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Target => "编译缓存",
            Self::Tmp => "这一轮的临时产物",
            Self::Cache => "缓存",
            Self::Runs => "已发版本的中间产物",
            Self::Reinstall => "可重装的依赖和输出",
            Self::TempLeftover => "引擎留下的临时目录",
        }
    }
}

/// 怎么清。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// 直接删（能重建的）。
    Delete,
    /// 移到废纸篓（中间产物，删错了还能找回来）。
    Trash,
}

/// 清单上的一项。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    pub path: PathBuf,
    pub kind: Kind,
    pub action: Action,
    /// 占用的磁盘（字节；APFS 克隆共用的块也算在内）。
    pub bytes: u64,
}

/// 一个目录的清单。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Plan {
    pub root: PathBuf,
    pub items: Vec<Item>,
    /// 没清的和为什么（有程序在跑、名字不认识、Git 收着……）。
    pub notes: Vec<String>,
}

impl Plan {
    pub fn bytes(&self) -> u64 {
        self.items.iter().map(|item| item.bytes).sum()
    }
}

/// 清理的设置。
#[derive(Clone, Debug)]
pub struct Options {
    /// 开工时 `target/` 超过这么多才清。默认 3 GB，环境变量 `ENGINE_TARGET_LIMIT_GB` 可改。
    pub target_limit: u64,
    /// 系统临时目录里引擎留下的目录，多久没动过算没人要了。默认一天。
    pub temp_age: Duration,
    /// 系统临时目录；`None` 不扫。
    pub temp_dir: Option<PathBuf>,
    /// 废纸篓。默认 `~/.Trash`。
    pub trash: PathBuf,
    /// 已发的版本（`v0.7.0` 这样）；`None` 时问 Git 的 tag。测试用。
    pub released: Option<BTreeSet<String>>,
    /// 有没有程序从这个目录里跑着；`None` 时看 `ps`。测试用。
    pub busy: Option<fn(&Path) -> bool>,
}

impl Default for Options {
    fn default() -> Self {
        let gb = std::env::var("ENGINE_TARGET_LIMIT_GB")
            .ok()
            .and_then(|v| v.trim().parse::<u64>().ok())
            .unwrap_or(3);
        Self {
            target_limit: gb * 1024 * 1024 * 1024,
            temp_age: Duration::from_secs(24 * 60 * 60),
            temp_dir: Some(std::env::temp_dir()),
            trash: std::env::var_os("HOME")
                .map(|home| PathBuf::from(home).join(".Trash"))
                .unwrap_or_else(|| PathBuf::from(".Trash")),
            released: None,
            busy: None,
        }
    }
}

const CACHE_NAMES: &[&str] = &["swift-cache", "__pycache__", ".pytest_cache", ".build"];
const REINSTALL: &[&str] = &[
    "node_modules",
    "web/node_modules",
    "web/dist",
    "samples/out",
    "output/playwright",
    ".playwright-cli",
];

/// 照这一档列出一个引擎目录能清的（不动手）。
pub fn plan(root: &Path, tier: Tier, opts: &Options) -> io::Result<Plan> {
    let root = std::path::absolute(root)?;
    let mut plan = Plan {
        root: root.clone(),
        ..Plan::default()
    };
    let git = root.join(".git").exists();
    if !git {
        plan.notes
            .push("不是 Git 仓库：分不清哪些 Git 不收，只看编译缓存".into());
    }
    let ignored = |path: &Path| -> bool {
        git && Command::new("git")
            .arg("-C")
            .arg(&root)
            .args(["check-ignore", "-q", "--"])
            .arg(path)
            .status()
            .is_ok_and(|s| s.success())
    };
    let add = |plan: &mut Plan, path: PathBuf, kind: Kind, action: Action, bytes: Option<u64>| {
        if kind != Kind::Target && !ignored(&path) {
            plan.notes
                .push(format!("{}：Git 收着，不清", rel(&root, &path)));
            return;
        }
        let bytes = bytes.unwrap_or_else(|| size(&path));
        if bytes > 0 || path.is_dir() {
            plan.items.push(Item {
                path,
                kind,
                action,
                bytes,
            });
        }
    };

    // 编译缓存
    let target = root.join("target");
    if target.is_dir() {
        let busy = opts
            .busy
            .map_or_else(|| running_from(&target), |f| f(&target));
        let total = matches!(tier, Tier::Start | Tier::All).then(|| size(&target));
        let whole = match tier {
            Tier::Start => total.unwrap_or_default() > opts.target_limit,
            Tier::All => true,
            _ => false,
        };
        if busy && (whole || tier == Tier::Release) {
            plan.notes
                .push("target/：有程序从里面跑着，这次不清".into());
        } else if whole {
            add(
                &mut plan,
                target.clone(),
                Kind::Target,
                Action::Delete,
                total,
            );
        } else if tier == Tier::Release {
            for entry in children(&target) {
                let name = file_name(&entry);
                if entry.is_dir() && !matches!(name.as_str(), "debug" | "tmp") {
                    add(&mut plan, entry, Kind::Target, Action::Delete, None);
                }
            }
        } else if tier == Tier::Start {
            plan.notes.push(format!(
                "target/：{}，没超过 {}",
                human(total.unwrap_or_default()),
                human(opts.target_limit)
            ));
        }
    }
    if tier == Tier::Start {
        return Ok(plan);
    }

    // local/：tmp、缓存、已发版本的 runs
    let local = root.join("local");
    let tmp = local.join("tmp");
    if tmp.is_dir() && !children(&tmp).is_empty() {
        add(&mut plan, tmp.clone(), Kind::Tmp, Action::Trash, None);
    }
    let runs = local.join("runs");
    let mut caches = Vec::new();
    find_caches(&local, &[tmp.as_path()], &mut caches);
    for cache in caches {
        add(&mut plan, cache, Kind::Cache, Action::Delete, None);
    }
    if matches!(tier, Tier::Release | Tier::All) {
        if runs.is_dir() {
            let released = match &opts.released {
                Some(set) => set.clone(),
                None => tags(&root),
            };
            for entry in children(&runs) {
                let name = file_name(&entry);
                let version = if name.starts_with('v') {
                    name.clone()
                } else {
                    format!("v{name}")
                };
                if !is_version(&version) {
                    plan.notes.push(format!(
                        "local/runs/{name}：不是版本号，不动（中间产物放 local/runs/v<版本>/）"
                    ));
                } else if released.contains(&version) {
                    add(&mut plan, entry, Kind::Runs, Action::Trash, None);
                } else {
                    plan.notes
                        .push(format!("local/runs/{name}：{version} 还没发版，留着"));
                }
            }
        }
        for name in REINSTALL {
            let path = root.join(name);
            if path.exists() {
                add(&mut plan, path, Kind::Reinstall, Action::Delete, None);
            }
        }
    }
    // 缓存若在已列的目录里面，算在外面那一项里
    let outer: Vec<PathBuf> = plan
        .items
        .iter()
        .filter(|i| i.kind != Kind::Cache)
        .map(|i| i.path.clone())
        .collect();
    plan.items.retain(|i| {
        i.kind != Kind::Cache || !outer.iter().any(|o| i.path.starts_with(o) && *o != i.path)
    });
    Ok(plan)
}

/// 系统临时目录里引擎留下、超过 `temp_age` 没动过的（每档除了开工都清）。
pub fn plan_temp(tier: Tier, opts: &Options) -> Plan {
    let mut plan = Plan::default();
    let Some(dir) = &opts.temp_dir else {
        return plan;
    };
    plan.root.clone_from(dir);
    if tier == Tier::Start {
        return plan;
    }
    let now = SystemTime::now();
    for entry in children(dir) {
        if !file_name(&entry).starts_with("engine-") {
            continue;
        }
        let old = std::fs::symlink_metadata(&entry)
            .and_then(|m| m.modified())
            .is_ok_and(|t| now.duration_since(t).unwrap_or_default() > opts.temp_age);
        if old {
            let bytes = size(&entry);
            plan.items.push(Item {
                path: entry,
                kind: Kind::TempLeftover,
                action: Action::Delete,
                bytes,
            });
        }
    }
    plan
}

/// 清掉的和没清掉的。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Applied {
    pub done: Vec<Item>,
    pub failed: Vec<(PathBuf, String)>,
}

impl Applied {
    pub fn bytes(&self) -> u64 {
        self.done.iter().map(|item| item.bytes).sum()
    }
}

/// 照清单动手：删的直接删，进废纸篓的挪到 `opts.trash`（名字前加引擎目录名和时间，免得撞名）。
pub fn apply(plan: &Plan, opts: &Options) -> Applied {
    let mut applied = Applied::default();
    let stamp = crate::spend::unix(SystemTime::now());
    for item in &plan.items {
        let result = match item.action {
            Action::Delete => {
                if item.path.is_dir() {
                    std::fs::remove_dir_all(&item.path)
                } else {
                    std::fs::remove_file(&item.path)
                }
            }
            Action::Trash => trash(&plan.root, &item.path, &opts.trash, stamp),
        };
        match result {
            Ok(()) => applied.done.push(item.clone()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => applied.failed.push((item.path.clone(), e.to_string())),
        }
    }
    applied
}

fn trash(root: &Path, path: &Path, bin: &Path, stamp: u64) -> io::Result<()> {
    std::fs::create_dir_all(bin)?;
    let base = format!(
        "{}-{}-{stamp}",
        file_name(root),
        rel(root, path).trim_end_matches('/').replace('/', "-")
    );
    let mut to = bin.join(&base);
    let mut n = 1;
    while to.exists() {
        n += 1;
        to = bin.join(format!("{base}-{n}"));
    }
    std::fs::rename(path, to)
}

/// 照「12.3 GB」这样写。
pub fn human(bytes: u64) -> String {
    let units = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < units.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", units[unit])
    }
}

impl fmt::Display for Plan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for item in &self.items {
            let how = match item.action {
                Action::Delete => "删",
                Action::Trash => "进废纸篓",
            };
            writeln!(
                f,
                "  {:>9}  {how:<4}  {}（{}）",
                human(item.bytes),
                rel(&self.root, &item.path),
                item.kind.label()
            )?;
        }
        for note in &self.notes {
            writeln!(f, "  不清：{note}")?;
        }
        Ok(())
    }
}

/// `root` 下的相对写法，目录带 `/`。
pub fn rel(root: &Path, path: &Path) -> String {
    let shown = path
        .strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string();
    if path.is_dir() {
        format!("{shown}/")
    } else {
        shown
    }
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn children(dir: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|rd| rd.filter_map(|e| e.ok().map(|e| e.path())).collect())
        .unwrap_or_default();
    out.sort();
    out
}

fn find_caches(dir: &Path, skip: &[&Path], out: &mut Vec<PathBuf>) {
    for entry in children(dir) {
        let is_dir = std::fs::symlink_metadata(&entry).is_ok_and(|m| m.is_dir());
        if !is_dir || skip.contains(&entry.as_path()) {
            continue;
        }
        if CACHE_NAMES.contains(&file_name(&entry).as_str()) {
            out.push(entry);
        } else {
            find_caches(&entry, skip, out);
        }
    }
}

fn is_version(name: &str) -> bool {
    let Some(rest) = name.strip_prefix('v') else {
        return false;
    };
    let parts: Vec<&str> = rest.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
}

fn tags(root: &Path) -> BTreeSet<String> {
    Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["tag", "--list", "v*"])
        .output()
        .map(|out| {
            String::from_utf8_lossy(&out.stdout)
                .lines()
                .map(|l| l.trim().to_owned())
                .filter(|l| !l.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

/// 有没有程序从这个目录里跑着（看 `ps` 的命令行）。看不了就当有，宁可不清。
fn running_from(dir: &Path) -> bool {
    let needle = format!("{}/", dir.display());
    match Command::new("ps").args(["-axo", "command="]).output() {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout)
            .lines()
            .any(|line| line.contains(&needle)),
        _ => true,
    }
}

/// 占用的磁盘（不跟符号链接；硬链接只算一次，和 `du` 一样）。
pub fn size(path: &Path) -> u64 {
    let mut seen = std::collections::HashSet::new();
    size_into(path, &mut seen)
}

fn size_into(path: &Path, seen: &mut std::collections::HashSet<(u64, u64)>) -> u64 {
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return 0;
    };
    if let Some(id) = hard_link(&meta) {
        if !seen.insert(id) {
            return 0;
        }
    }
    let own = allocated(&meta);
    if meta.is_dir() {
        own + children(path)
            .iter()
            .map(|c| size_into(c, seen))
            .sum::<u64>()
    } else {
        own
    }
}

#[cfg(unix)]
fn hard_link(meta: &std::fs::Metadata) -> Option<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    (meta.is_file() && meta.nlink() > 1).then(|| (meta.dev(), meta.ino()))
}

#[cfg(not(unix))]
fn hard_link(_: &std::fs::Metadata) -> Option<(u64, u64)> {
    None
}

#[cfg(unix)]
fn allocated(meta: &std::fs::Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    meta.blocks() * 512
}

#[cfg(not(unix))]
fn allocated(meta: &std::fs::Metadata) -> u64 {
    meta.len()
}
