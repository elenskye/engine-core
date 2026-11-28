//! 清理（feature `clean`）：在临时目录里搭一个引擎仓库，看各档列什么、怎么清。
#![cfg(feature = "clean")]

use engine_core::clean::{self, Action, Kind, Options, Tier};
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

fn write(path: &Path, bytes: usize) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, vec![b'x'; bytes]).unwrap();
}

/// 一个像样的引擎仓库：target、local 的样本和中间产物、可重装的依赖、一个 Git 收着的 dist。
fn engine(root: &Path) {
    let git = |args: &[&str]| {
        assert!(
            Command::new("git")
                .arg("-C")
                .arg(root)
                .args(args)
                .output()
                .unwrap()
                .status
                .success()
        )
    };
    fs::create_dir_all(root).unwrap();
    git(&["init", "-q", "-b", "main"]);
    fs::write(
        root.join(".gitignore"),
        "/target/\n/local/\n/web/node_modules/\n/samples/out/\n",
    )
    .unwrap();
    fs::write(root.join("Cargo.toml"), "[package]\nname = \"engine-x\"\n").unwrap();
    write(&root.join("target/debug/deps/a.rlib"), 40_000);
    write(&root.join("target/release/b"), 20_000);
    write(
        &root.join("target/aarch64-unknown-linux-gnu/debug/c"),
        10_000,
    );
    write(&root.join("local/pdf/real.pdf"), 5_000); // 真实样本：从不动
    write(&root.join("local/pdfium/lib/libpdfium.a"), 5_000); // 大文件：从不动
    write(&root.join("local/tmp/probe"), 3_000);
    write(&root.join("local/runs/v0.1.0/review.log"), 2_000);
    write(&root.join("local/runs/v0.1.0/swift-cache/m"), 1_000);
    write(&root.join("local/runs/v0.2.0/baseline.txt"), 2_000);
    write(&root.join("local/runs/odd/x"), 100);
    write(&root.join("local/check/swift-cache/m"), 1_000);
    write(&root.join("web/node_modules/pkg/index.js"), 4_000);
    write(&root.join("samples/out/a.pdf"), 1_000);
    write(&root.join("web/dist/app.js"), 1_000); // Git 收着
    git(&["add", "."]);
    git(&[
        "-c",
        "user.name=t",
        "-c",
        "user.email=t@example.com",
        "commit",
        "-qm",
        "init",
    ]);
}

fn opts(root: &Path) -> Options {
    Options {
        target_limit: 50_000,
        temp_age: Duration::from_secs(3600),
        temp_dir: None,
        trash: root.parent().unwrap().join("Trash"),
        released: Some(BTreeSet::from(["v0.1.0".to_string()])),
        busy: Some(|_| false),
    }
}

fn listed(plan: &clean::Plan) -> Vec<(String, Kind, Action)> {
    plan.items
        .iter()
        .map(|i| (clean::rel(&plan.root, &i.path), i.kind, i.action))
        .collect()
}

#[test]
fn each_tier_lists_only_what_it_owns() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("engine-x");
    engine(&root);
    let o = opts(&root);

    // 开工：target 约 70 KB，上限 50 KB → 整份
    let start = clean::plan(&root, Tier::Start, &o).unwrap();
    assert_eq!(
        listed(&start),
        [("target/".into(), Kind::Target, Action::Delete)]
    );
    let roomy = Options {
        target_limit: 10_000_000,
        ..opts(&root)
    };
    assert!(
        clean::plan(&root, Tier::Start, &roomy)
            .unwrap()
            .items
            .is_empty()
    );

    // 每轮：tmp 进废纸篓，local 下的缓存删；runs、target 不动
    let round = clean::plan(&root, Tier::Round, &o).unwrap();
    assert_eq!(
        listed(&round),
        [
            ("local/tmp/".into(), Kind::Tmp, Action::Trash),
            (
                "local/check/swift-cache/".into(),
                Kind::Cache,
                Action::Delete
            ),
            (
                "local/runs/v0.1.0/swift-cache/".into(),
                Kind::Cache,
                Action::Delete
            ),
        ]
    );

    // 发版：加已发的 runs（v0.2.0 没发、odd 不认识，留着）、target 里 debug 以外的、可重装的；
    // Git 收着的 web/dist 不动
    let release = clean::plan(&root, Tier::Release, &o).unwrap();
    let got = listed(&release);
    for want in [
        (
            "target/aarch64-unknown-linux-gnu/".to_string(),
            Kind::Target,
            Action::Delete,
        ),
        ("target/release/".into(), Kind::Target, Action::Delete),
        ("local/tmp/".into(), Kind::Tmp, Action::Trash),
        (
            "local/check/swift-cache/".into(),
            Kind::Cache,
            Action::Delete,
        ),
        ("local/runs/v0.1.0/".into(), Kind::Runs, Action::Trash),
        ("web/node_modules/".into(), Kind::Reinstall, Action::Delete),
        ("samples/out/".into(), Kind::Reinstall, Action::Delete),
    ] {
        assert!(got.contains(&want), "{want:?} 不在 {got:?}");
    }
    assert_eq!(got.len(), 7, "{got:?}");
    let notes = release.notes.join("\n");
    assert!(notes.contains("v0.2.0 还没发版"), "{notes}");
    assert!(notes.contains("local/runs/odd：不是版本号"), "{notes}");
    assert!(notes.contains("web/dist/：Git 收着"), "{notes}");

    // all：整个 target 代替里面的几块
    let all = clean::plan(&root, Tier::All, &o).unwrap();
    assert!(listed(&all).contains(&("target/".into(), Kind::Target, Action::Delete)));
    assert!(!listed(&all).iter().any(|(p, ..)| p == "target/release/"));
}

#[test]
fn apply_deletes_or_trashes_and_never_touches_samples() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("engine-x");
    engine(&root);
    let o = opts(&root);
    let plan = clean::plan(&root, Tier::All, &o).unwrap();
    let applied = clean::apply(&plan, &o);
    assert!(applied.failed.is_empty(), "{:?}", applied.failed);
    assert_eq!(applied.bytes(), plan.bytes());
    assert!(!root.join("target").exists());
    assert!(!root.join("web/node_modules").exists());
    assert!(root.join("local/pdf/real.pdf").exists());
    assert!(root.join("local/pdfium/lib/libpdfium.a").exists());
    assert!(root.join("local/runs/v0.2.0/baseline.txt").exists());
    assert!(root.join("web/dist/app.js").exists());
    // 中间产物在废纸篓里，名字带引擎目录名
    let trashed: Vec<String> = fs::read_dir(&o.trash)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert!(
        trashed.iter().any(|n| n.starts_with("engine-x-local-tmp-")),
        "{trashed:?}"
    );
    assert!(
        trashed
            .iter()
            .any(|n| n.starts_with("engine-x-local-runs-v0.1.0-")),
        "{trashed:?}"
    );
    assert!(o.trash.join(&trashed[0]).is_dir());
}

#[test]
fn busy_targets_and_young_temp_dirs_are_left_alone() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("engine-x");
    engine(&root);
    let busy = Options {
        busy: Some(|_| true),
        ..opts(&root)
    };
    let plan = clean::plan(&root, Tier::Start, &busy).unwrap();
    assert!(plan.items.is_empty());
    assert!(plan.notes[0].contains("有程序从里面跑着"));

    let temp = dir.path().join("T");
    write(&temp.join("engine-excel-numbers-1/x"), 100);
    write(&temp.join("other-tool/x"), 100);
    let young = Options {
        temp_dir: Some(temp.clone()),
        ..opts(&root)
    };
    assert!(clean::plan_temp(Tier::Round, &young).items.is_empty());
    let any_age = Options {
        temp_age: Duration::ZERO,
        temp_dir: Some(temp.clone()),
        ..opts(&root)
    };
    std::thread::sleep(Duration::from_millis(20));
    let plan = clean::plan_temp(Tier::Round, &any_age);
    assert_eq!(plan.items.len(), 1);
    assert!(plan.items[0].path.ends_with("engine-excel-numbers-1"));
    assert!(clean::plan_temp(Tier::Start, &any_age).items.is_empty());
}

#[test]
fn sizes_read_like_people_write_them() {
    assert_eq!(clean::human(512), "512 B");
    assert_eq!(clean::human(1536), "1.5 KB");
    assert_eq!(clean::human(3 * 1024 * 1024 * 1024), "3.0 GB");
}

#[test]
fn hard_links_count_once_like_du() {
    let dir = tempfile::tempdir().unwrap();
    write(&dir.path().join("a/big"), 64 * 1024);
    let one = clean::size(&dir.path().join("a"));
    fs::hard_link(dir.path().join("a/big"), dir.path().join("a/link")).unwrap();
    assert_eq!(clean::size(&dir.path().join("a")), one);
}
