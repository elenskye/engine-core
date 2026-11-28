//! `engine-clean`：清理引擎的开发目录（engine-core 的 `clean`，总仓库 `decisions/0013`）。
//!
//! ```text
//! engine-clean [--start | --round | --release | --all] [--yes] [--quiet] [<目录> …]
//! engine-clean --version
//! ```
//!
//! - 不加档，按 `--all` 列；不加 `--yes` 只列清单，不动手。`--quiet`：没有要清的就什么都不打（开工钩子用）。
//! - 目录省略时是当前目录；给的是引擎总仓库（下面有 `engine-*/`）就把每个引擎都过一遍。
//! - 各引擎用 `./scripts/clean.sh` 调它（第一次会 `cargo install` 一份）。

use engine_core::clean::{self, Options, Plan, Tier, human};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut tier = None;
    let mut yes = false;
    let mut quiet = false;
    let mut dirs = Vec::new();
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--version" | "-V" => {
                println!("engine-clean {}", engine_core::VERSION);
                return ExitCode::SUCCESS;
            }
            "--help" | "-h" => {
                println!(
                    "engine-clean [--start | --round | --release | --all] [--yes] [--quiet] [<目录> …]\n\
                     不加档按 --all 列；不加 --yes 只列清单。规矩见总仓库 GUIDE「五」「清理」。"
                );
                return ExitCode::SUCCESS;
            }
            "--yes" => yes = true,
            "--quiet" => quiet = true,
            flag if flag.starts_with("--") => match Tier::parse(flag) {
                Some(t) if tier.is_none() => tier = Some(t),
                _ => {
                    eprintln!("参数不对：{flag}（用 --help 看用法）");
                    return ExitCode::from(2);
                }
            },
            dir => dirs.push(PathBuf::from(dir)),
        }
    }
    let tier = tier.unwrap_or(Tier::All);
    if dirs.is_empty() {
        dirs.push(PathBuf::from("."));
    }
    let opts = Options::default();
    let mut plans = Vec::new();
    for dir in engines(&dirs) {
        match clean::plan(&dir, tier, &opts) {
            Ok(plan) => plans.push(plan),
            Err(e) => {
                eprintln!("{}：{e}", dir.display());
                return ExitCode::from(1);
            }
        }
    }
    plans.push(clean::plan_temp(tier, &opts));

    let total: u64 = plans.iter().map(Plan::bytes).sum();
    if quiet && plans.iter().all(|plan| plan.items.is_empty()) {
        return ExitCode::SUCCESS;
    }
    let mut failed = 0;
    let mut freed = 0;
    for plan in &plans {
        if plan.items.is_empty() && plan.notes.is_empty() {
            continue;
        }
        let name = if Some(&plan.root) == opts.temp_dir.as_ref() {
            "系统临时目录".to_owned()
        } else {
            plan.root
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
        };
        println!("{name}（{}）", human(plan.bytes()));
        print!("{plan}");
        if yes {
            let applied = clean::apply(plan, &opts);
            freed += applied.bytes();
            for (path, why) in &applied.failed {
                failed += 1;
                println!("  没清掉：{}：{why}", clean::rel(&plan.root, path));
            }
        }
    }
    if total == 0 {
        println!("没有要清的。");
    } else if yes {
        println!(
            "清掉 {}；进废纸篓的在 {}。",
            human(freed),
            opts.trash.display()
        );
    } else {
        println!("一共 {}。只列了清单，加 --yes 才动手。", human(total));
    }
    if failed > 0 {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

/// 给的是引擎总仓库就换成下面的每个 `engine-*/`。
fn engines(dirs: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for dir in dirs {
        if is_engine(dir) {
            out.push(dir.clone());
            continue;
        }
        let mut subs: Vec<PathBuf> = std::fs::read_dir(dir)
            .map(|rd| {
                rd.filter_map(|e| e.ok().map(|e| e.path()))
                    .filter(|p| {
                        p.file_name()
                            .is_some_and(|n| n.to_string_lossy().starts_with("engine-"))
                            && is_engine(p)
                    })
                    .collect()
            })
            .unwrap_or_default();
        subs.sort();
        if subs.is_empty() {
            out.push(dir.clone());
        } else {
            out.extend(subs);
        }
    }
    out
}

/// 引擎目录：有 `Cargo.toml`，或是名叫 `engine-*` 的 Git 仓库（还没写代码的）。
fn is_engine(dir: &Path) -> bool {
    let named = std::path::absolute(dir)
        .ok()
        .and_then(|d| {
            d.file_name()
                .map(|n| n.to_string_lossy().starts_with("engine-"))
        })
        .unwrap_or(false);
    dir.join("Cargo.toml").is_file() || (named && dir.join(".git").exists())
}
