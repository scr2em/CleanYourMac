//! Repeatable performance benchmark for the core.
//!
//! cargo run --release --bin cym-bench [-- --rows N] [--runs N] [--fixture DIR]
//!
//! Generates a deterministic developer home folder once (JavaScript and Next.js projects,
//! Rust targets, Python environments, source trees, duplicate sets and large sparse files),
//! then times each file module's scan with a warm cache and the result store's operations on
//! synthetic rows. Prints medians as a Markdown table.
use cym_core::{
    model::*,
    modules,
    ports::ScanControl,
    results::{synthetic, Query, ResultStore, SortKey},
    Engine, Services,
};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

const FIXTURE_VERSION: &str = "1";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let option = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let rows: usize = option("--rows").map_or(1_000_000, |v| v.parse().expect("--rows N"));
    let runs: usize = option("--runs").map_or(5, |v| v.parse().expect("--runs N"));
    let fixture = option("--fixture").map_or_else(
        // Not under `target/` or `/tmp`: scans skip build folders and system locations.
        || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.bench-fixture"),
        PathBuf::from,
    );

    let entries = ensure_fixture(&fixture);
    println!(
        "Fixture: {} entries in {} · {} threads · {runs} runs, median\n",
        entries,
        fixture.display(),
        std::thread::available_parallelism().map_or(1, |n| n.get())
    );
    println!("| Benchmark | Median | Detail |");
    println!("| --- | ---: | --- |");

    // Scans: one warm-up, then the median of `runs`.
    let engine = Engine::new(Services::native(), modules::builtin());
    let context = ScanContext {
        roots: vec![fixture.to_string_lossy().into()],
        ..Default::default()
    };
    let control = ScanControl::default();
    let walked = || {
        let mut count = 0usize;
        engine
            .services
            .walk(&context, &control, &mut vec![], &mut |_| {
                count += 1;
                Ok(true)
            })
            .unwrap();
        count
    };
    let count = walked();
    report(
        "Walk (scope checks only)",
        median(runs, || timed(walked).1),
        &format!("{count} entries visited"),
    );
    for (module, label) in [
        ("large", "Scan · Large Files"),
        ("duplicates", "Scan · Exact Duplicates"),
        ("node", "Scan · Node Dependencies"),
        ("artifacts", "Scan · Build Artifacts"),
        ("storage", "Scan · Storage Explorer"),
    ] {
        let scan = || engine.scan_report(&[module.into()], &context, &control);
        let findings = scan().findings.len();
        report(
            label,
            median(runs, || timed(scan).1),
            &format!("{findings} findings"),
        );
    }

    // Result store on synthetic rows.
    let data = synthetic(rows);
    let label = |what: &str| format!("Store · {what} ({})", human(rows));
    let fresh = || {
        let store = ResultStore::default();
        store.insert(data.clone());
        store
    };
    let store_runs = runs.min(3);
    report(
        &label("insert"),
        median(store_runs, || {
            let rows = data.clone();
            let store = ResultStore::default();
            timed(|| store.insert(rows)).1
        }),
        "",
    );
    let by_size = Query::default();
    report(
        &label("first query, sorted by size"),
        median(store_runs, || {
            let store = fresh();
            timed(|| store.query(&by_size)).1
        }),
        "new generation each run",
    );
    let store = fresh();
    store.query(&by_size);
    report(
        &label("repeat query"),
        median(runs, || timed(|| store.query(&by_size)).1),
        "same sort and filters",
    );
    let by_name = Query {
        sort: SortKey::Name,
        ascending: true,
        ..Default::default()
    };
    report(
        &label("first query, sorted by name"),
        median(store_runs, || {
            let store = fresh();
            timed(|| store.query(&by_name)).1
        }),
        "",
    );
    let search = Query {
        search: "item-00012".into(),
        ..Default::default()
    };
    let (snapshot, _) = timed(|| store.query(&search));
    report(
        &label("search"),
        median(runs, || timed(|| store.query(&search)).1),
        &format!("{} matches", snapshot.len()),
    );
    let filtered = Query {
        min_bytes: 1_000_000_000,
        ..Default::default()
    };
    let (snapshot, _) = timed(|| store.query(&filtered));
    report(
        &label("size filter"),
        median(runs, || timed(|| store.query(&filtered)).1),
        &format!("{} rows ≥ 1 GB", snapshot.len()),
    );
    let all = store.query(&by_size);
    report(
        &label("page of 200"),
        median(runs, || timed(|| all.page(rows / 2, 200)).1),
        "from the middle",
    );
    let (ids, _) = timed(|| all.eligible_ids(0, usize::MAX));
    report(
        &label("select all (eligible IDs)"),
        median(runs, || timed(|| all.eligible_ids(0, usize::MAX)).1),
        &format!("{} IDs", ids.len()),
    );
    // Last, because a run that exceeds its time budget keeps a thread busy.
    let store = Arc::new(store);
    for count in [1_000, 10_000, 100_000] {
        let chosen: Arc<Vec<String>> = Arc::new(ids.iter().take(count).cloned().collect());
        let name = label(&format!(
            "selection summary, {} selected",
            human_count(count)
        ));
        let (store, chosen) = (store.clone(), chosen.clone());
        match bounded(Duration::from_secs(60), move || {
            median(runs.min(3), || timed(|| store.selection(&chosen, 0)).1)
        }) {
            Some(time) => report(&name, time, "totals and shared actions for review"),
            None => {
                println!("| {name} | > 60 s | did not finish within the budget |");
                std::process::exit(0);
            }
        }
    }
}

/// Runs `work` on its own thread and gives up waiting after `limit`.
fn bounded(limit: Duration, work: impl FnOnce() -> Duration + Send + 'static) -> Option<Duration> {
    let (send, receive) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = send.send(work());
    });
    receive.recv_timeout(limit).ok()
}
fn human_count(n: usize) -> String {
    if n >= 1_000 && n.is_multiple_of(1_000) {
        format!("{}k", n / 1_000)
    } else {
        n.to_string()
    }
}
fn timed<T>(work: impl FnOnce() -> T) -> (T, Duration) {
    let start = Instant::now();
    let value = work();
    (value, start.elapsed())
}
fn median(runs: usize, mut sample: impl FnMut() -> Duration) -> Duration {
    let mut times: Vec<Duration> = (0..runs.max(1)).map(|_| sample()).collect();
    times.sort();
    times[times.len() / 2]
}
fn report(name: &str, time: Duration, detail: &str) {
    let ms = time.as_secs_f64() * 1000.0;
    let shown = if ms >= 100.0 {
        format!("{ms:.0} ms")
    } else if ms >= 1.0 {
        format!("{ms:.1} ms")
    } else {
        format!("{:.0} µs", ms * 1000.0)
    };
    println!("| {name} | {shown} | {detail} |");
}
fn human(n: usize) -> String {
    if n >= 1_000_000 && n.is_multiple_of(1_000_000) {
        format!("{}M rows", n / 1_000_000)
    } else {
        format!("{n} rows")
    }
}

/// Builds the fixture unless a complete one of this version exists. Returns its entry count.
fn ensure_fixture(root: &Path) -> usize {
    let marker = root.join(".cym-bench");
    if let Ok(text) = fs::read_to_string(&marker) {
        if let Some(count) = text.strip_prefix(&format!("{FIXTURE_VERSION}:")) {
            if let Ok(count) = count.trim().parse() {
                return count;
            }
        }
    }
    let _ = fs::remove_dir_all(root);
    eprintln!("Generating fixture in {} …", root.display());
    let mut count = 0usize;
    let mut file = |path: PathBuf, contents: &[u8]| {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, contents).unwrap();
        count += 1;
    };
    let mut seed = 0x2545_F491_4F6C_DD1Du64;
    let mut random = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    // JavaScript projects with installed dependencies; every other one is a Next.js app.
    for p in 0..200 {
        let project = root.join(format!("web/app-{p:03}"));
        file(project.join("package.json"), b"{}");
        file(
            project.join(if p % 3 == 0 {
                "pnpm-lock.yaml"
            } else {
                "package-lock.json"
            }),
            b"lock",
        );
        for d in 0..25 {
            let package = project.join(format!("node_modules/dep-{d:02}"));
            file(package.join("package.json"), b"{}");
            for f in 0..10 {
                file(package.join(format!("lib/sub-{}/file-{f}.js", f % 3)), b"x");
            }
        }
        if p % 2 == 0 {
            file(project.join("next.config.js"), b"module.exports = {}");
            for f in 0..50 {
                file(project.join(format!(".next/cache/chunk-{f}.js")), b"x");
            }
        }
        for f in 0..30 {
            file(
                project.join(format!("src/components/c-{}/view-{f}.tsx", f % 5)),
                b"x",
            );
        }
    }
    // Rust crates with build output.
    for p in 0..100 {
        let project = root.join(format!("rust/crate-{p:03}"));
        file(project.join("Cargo.toml"), b"[package]");
        file(project.join("src/main.rs"), b"fn main() {}");
        for f in 0..100 {
            file(
                project.join(format!("target/debug/deps/lib-{f}.rlib")),
                b"x",
            );
        }
    }
    // Python projects with virtual environments.
    for p in 0..100 {
        let project = root.join(format!("python/svc-{p:03}"));
        file(project.join("pyproject.toml"), b"[project]");
        file(project.join(".venv/pyvenv.cfg"), b"home = /usr/bin");
        for f in 0..100 {
            file(
                project.join(format!(
                    ".venv/lib/python3.12/site-packages/pkg-{}/m-{f}.py",
                    f % 10
                )),
                b"x",
            );
        }
    }
    // Source trees, documents and duplicate sets: 500 contents copied four times, plus
    // 3,000 files of the same size with different contents.
    for p in 0..300 {
        for f in 0..100 {
            file(
                root.join(format!(
                    "documents/project-{p:03}/notes/n-{}/note-{f}.md",
                    f % 4
                )),
                b"note",
            );
        }
    }
    for set in 0..500 {
        let contents: Vec<u8> = (0..8192).map(|i| (random() as usize + i) as u8).collect();
        for copy in 0..4 {
            file(
                root.join(format!("documents/copies-{copy}/set-{set:03}.bin")),
                &contents,
            );
        }
    }
    for f in 0..3_000 {
        let contents: Vec<u8> = (0..8192).map(|_| random() as u8).collect();
        file(
            root.join(format!("documents/unique/u-{f:04}.bin")),
            &contents,
        );
    }
    // Large sparse files of distinct sizes.
    for f in 0..5u64 {
        let path = root.join(format!("media/video-{f}.mov"));
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::File::create(&path)
            .unwrap()
            .set_len(150_000_000 + f * 1_000_000)
            .unwrap();
        count += 1;
    }
    fs::write(&marker, format!("{FIXTURE_VERSION}:{count}")).unwrap();
    count
}
