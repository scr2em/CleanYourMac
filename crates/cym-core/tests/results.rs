mod common;
use common::*;
use cym_core::{
    model::*,
    modules,
    ports::ScanControl,
    results::{natural, synthetic, Query, ResultStore, SortKey},
    Engine,
};
use std::{cmp::Ordering, fs, sync::Mutex, time::Instant};

#[test]
fn store_queries_filter_sort_and_page_snapshots() {
    let store = ResultStore::default();
    store.insert(synthetic(10_000));
    assert_eq!(store.len(), 10_000);
    let all = store.query(&Query::default());
    assert_eq!(all.len(), 10_000);
    let page = all.page(0, 50);
    assert_eq!(page.len(), 50);
    assert!(page.windows(2).all(|w| w[0].bytes >= w[1].bytes));
    let ascending = store.query(&Query {
        ascending: true,
        ..Default::default()
    });
    let first = ascending.page(0, 2);
    assert!(first[0].bytes <= first[1].bytes);

    let searched = store.query(&Query {
        search: "ITEM-000012".into(),
        ..Default::default()
    });
    assert_eq!(searched.len(), 10);
    let node = store.query(&Query {
        module: Some("node".into()),
        min_bytes: 1_000_000_000,
        ..Default::default()
    });
    assert!(node
        .page(0, 10_000)
        .iter()
        .all(|r| r.module_id == "node" && r.bytes.unwrap() >= 1_000_000_000));
    assert_eq!(node.summary.findings, node.len());
    assert!(node.summary.modules.is_empty());
    assert_eq!(all.summary.modules.len(), 5);
    assert_eq!(all.largest.len(), 5);
    assert_eq!(all.largest[0].id, all.page(0, 1)[0].id);
    // Range selection takes only the rows an action can apply to, in snapshot order.
    let window = all.page(100, 200);
    let expected: Vec<String> = window
        .iter()
        .filter(|r| r.eligible)
        .map(|r| r.id.clone())
        .collect();
    assert!(expected.len() < window.len());
    assert_eq!(all.eligible_ids(100, 200), expected);
    assert_eq!(all.eligible, all.eligible_ids(0, usize::MAX).len());
    assert!(all.eligible < all.len());

    // A snapshot stays consistent while the store changes.
    let ids: Vec<String> = all.page(0, 3).into_iter().map(|r| r.id).collect();
    assert_eq!(store.remove(&ids), 3);
    assert_eq!(all.page(0, 3)[0].id, ids[0]);
    assert_eq!(store.query(&Query::default()).len(), 9_997);
    assert!(store.get(&ids[0]).is_none());
    assert_eq!(store.snapshot(all.id).map(|s| s.len()), Some(10_000));
}

#[test]
fn natural_order_compares_numbers_and_ignores_case() {
    assert_eq!(natural("file2", "file10"), Ordering::Less);
    assert_eq!(natural("Alpha", "alpha"), Ordering::Equal);
    assert_eq!(natural("b", "A"), Ordering::Greater);
    assert_eq!(natural("v007", "v7"), Ordering::Equal);
}

#[test]
fn selection_summarizes_shared_actions_and_normalized_bytes() {
    let store = ResultStore::default();
    let mut parent = file_finding("parent", "node", identity("/p/node_modules"));
    parent.bytes = Some(100);
    let mut child = file_finding("child", "node", identity("/p/node_modules/x"));
    child.bytes = Some(40);
    let mut blocked = file_finding("blocked", "large", identity("/q"));
    blocked.bytes = Some(7);
    blocked.blocked_reason = Some("inspect".into());
    store.insert(vec![parent, child, blocked]);
    let s = store.selection(&["parent".into(), "child".into(), "child".into()], 10);
    assert_eq!((s.count, s.bytes), (2, 100));
    assert_eq!(s.actions, vec![ActionKind::Trash]);
    let s = store.selection(&["parent".into(), "blocked".into(), "missing".into()], 1);
    assert_eq!(s.count, 2);
    assert!(s.actions.is_empty());
    assert_eq!(s.preview.len(), 1);
}
fn identity(path: &str) -> FileIdentity {
    FileIdentity {
        path: path.into(),
        device: 1,
        inode: 1,
        modified_seconds: 0,
        modified_nanos: 0,
        tree_signature: None,
    }
}

#[test]
fn stored_scans_replace_module_rows_and_act_by_id() {
    let f = Fixture::new();
    f.write("app/package.json", "{}");
    f.write("app/node_modules/x/index.js", "x");
    let engine = Engine::new(services(&f), modules::builtin());
    engine.results.insert(synthetic(10));
    let events = Mutex::new(vec![]);
    let k = ScanControl::default();
    engine.scan_to_store(&["node".into()], &f.context(), &k, 3, &|e| {
        events.lock().unwrap().push(e)
    });
    let events = events.into_inner().unwrap();
    assert!(events
        .iter()
        .all(|e| !matches!(e, ScanEvent::Finding { .. })));
    assert!(events
        .iter()
        .any(|e| matches!(e, ScanEvent::Stored { module_id, .. } if module_id == "node")));
    let node = engine.results.query(&Query {
        module: Some("node".into()),
        ..Default::default()
    });
    // The node module's synthetic rows were replaced by the real scan.
    assert_eq!(node.len(), 1);
    let id = node.page(0, 1)[0].id.clone();
    let results = engine.execute_ids(
        std::slice::from_ref(&id),
        ActionKind::Trash,
        &f.context(),
        &k,
    );
    assert_eq!(
        results[0].outcome,
        Outcome::Applied,
        "{}",
        results[0].message
    );
    assert!(engine.results.get(&id).is_none());
    assert!(fs::metadata(f.at("app/node_modules")).is_err());
}

/// One million rows: insert, query, search, sort by name and page within interactive budgets.
/// Run with `cargo test --release --test results -- --ignored`.
#[test]
#[ignore]
fn one_million_rows_stay_interactive() {
    let store = ResultStore::default();
    let rows = synthetic(1_000_000);
    let started = Instant::now();
    store.insert(rows);
    let insert = started.elapsed();
    let timed = |q: Query| {
        let started = Instant::now();
        let snapshot = store.query(&q);
        (snapshot, started.elapsed())
    };
    let (all, by_size) = timed(Query::default());
    let (_, by_name) = timed(Query {
        sort: SortKey::Name,
        ascending: true,
        ..Default::default()
    });
    let (found, search) = timed(Query {
        search: "item-09".into(),
        ..Default::default()
    });
    let started = Instant::now();
    let page = all.page(999_800, 200);
    let paging = started.elapsed();
    println!(
        "insert {insert:?} size {by_size:?} name {by_name:?} search {search:?} page {paging:?}"
    );
    assert_eq!(all.len(), 1_000_000);
    assert_eq!(found.len(), 100_000);
    assert_eq!(page.len(), 200);
    assert!(insert.as_secs_f64() < 5.0);
    assert!(by_size.as_secs_f64() < 3.0);
    assert!(by_name.as_secs_f64() < 5.0);
    assert!(search.as_secs_f64() < 3.0);
    assert!(paging.as_millis() < 50);
}

#[test]
fn last_used_sorts_dated_rows_first_in_either_direction() {
    let store = ResultStore::default();
    let mut rows = synthetic(4);
    rows[0].last_used_at = Some(300.0);
    rows[1].last_used_at = None;
    rows[2].last_used_at = Some(100.0);
    rows[3].last_used_at = Some(200.0);
    let ids: Vec<String> = rows.iter().map(|f| f.id.clone()).collect();
    store.insert(rows);
    let order = |ascending| {
        store
            .query(&Query {
                sort: SortKey::LastUsed,
                ascending,
                ..Default::default()
            })
            .page(0, 4)
            .into_iter()
            .map(|r| r.id)
            .collect::<Vec<_>>()
    };
    let pick = |i: [usize; 4]| i.map(|i| ids[i].clone()).to_vec();
    assert_eq!(order(true), pick([2, 3, 0, 1]));
    assert_eq!(order(false), pick([0, 3, 2, 1]));
}

#[test]
fn simulator_timestamps_parse_as_utc() {
    use cym_core::simulator::parse_timestamp;
    assert_eq!(parse_timestamp("1970-01-01T00:00:00Z"), Some(0.0));
    assert_eq!(
        parse_timestamp("2025-01-15T09:41:12Z"),
        Some(1_736_934_072.0)
    );
    assert_eq!(
        parse_timestamp("2024-02-29T12:00:00.5Z"),
        Some(1_709_208_000.5)
    );
    assert_eq!(parse_timestamp("not a date"), None);
}

#[test]
fn node_findings_report_project_activity_as_last_used() {
    let f = Fixture::new();
    f.write("app/package.json", "{}");
    f.write("app/node_modules/x/index.js", "x");
    let report = Engine::new(services(&f), modules::builtin()).scan_report(
        &["node".into()],
        &f.context(),
        &ScanControl::default(),
    );
    let row = &report.findings[0];
    let manifest = fs::metadata(f.at("app/package.json")).unwrap();
    let expected = manifest
        .modified()
        .unwrap()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs_f64();
    assert!((row.last_used_at.unwrap() - expected).abs() < 1.0);
    assert_eq!(
        row.value("Last used from"),
        Some("Newest change to project files")
    );
}

/// The comparator the store sorted with before keys were precomputed, as the reference.
fn reference_order(a: &Finding, b: &Finding, sort: SortKey, ascending: bool) -> Ordering {
    if sort == SortKey::LastUsed {
        let order = match (a.last_used_at, b.last_used_at) {
            (Some(x), Some(y)) => {
                let order = x.total_cmp(&y);
                if ascending {
                    order
                } else {
                    order.reverse()
                }
            }
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => Ordering::Equal,
        };
        return order.then_with(|| a.id.cmp(&b.id));
    }
    let order = match sort {
        SortKey::LastUsed => Ordering::Equal,
        SortKey::Size => a
            .bytes
            .or(a.memory_bytes)
            .unwrap_or(0)
            .cmp(&b.bytes.or(b.memory_bytes).unwrap_or(0)),
        SortKey::Cpu => a
            .cpu_percent
            .unwrap_or(0.)
            .total_cmp(&b.cpu_percent.unwrap_or(0.)),
        SortKey::Name => natural(&a.title, &b.title),
    }
    .then_with(|| a.id.cmp(&b.id));
    if ascending {
        order
    } else {
        order.reverse()
    }
}

#[test]
fn keyed_sorting_matches_the_reference_comparator() {
    let mut rows = synthetic(20_000);
    for (i, f) in rows.iter_mut().enumerate() {
        // Ties, missing values, processes, negative zero and numbered names.
        f.bytes = (i % 7 != 0).then_some((i % 50) as u64 * 1_000);
        f.memory_bytes = (i % 7 == 0 && i % 2 == 0).then_some((i % 30) as u64);
        f.cpu_percent = (i % 3 == 0).then_some(if i % 9 == 0 {
            -0.0
        } else {
            (i % 40) as f64 / 3.0
        });
        f.last_used_at = (i % 4 != 0).then_some((i % 25) as f64 * 1e6);
        f.title = format!("item {}", i % 300);
    }
    let store = ResultStore::default();
    store.insert(rows.clone());
    for sort in [
        SortKey::Size,
        SortKey::Name,
        SortKey::Cpu,
        SortKey::LastUsed,
    ] {
        for ascending in [true, false] {
            let snapshot = store.query(&Query {
                sort,
                ascending,
                ..Default::default()
            });
            let got: Vec<String> = snapshot
                .page(0, rows.len())
                .into_iter()
                .map(|r| r.id)
                .collect();
            let mut expected = rows.clone();
            expected.sort_by(|a, b| reference_order(a, b, sort, ascending));
            let expected: Vec<String> = expected.into_iter().map(|f| f.id).collect();
            assert_eq!(got, expected, "{sort:?} ascending={ascending}");
        }
    }
}

#[test]
fn fast_analytics_matches_the_reference_totals() {
    use cym_core::analytics::{analytics_of, analytics_reference};
    let mut seed = 0xDEAD_BEEF_CAFE_F00Du64;
    let mut next = move |n: u64| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed % n
    };
    let modules = [
        "large",
        "node",
        "artifacts",
        "duplicates",
        "simulators",
        "orphans",
        "worktrees",
    ];
    let names = ["a", "b", "node_modules", "x y", "é", "target", "deep"];
    let mut rows = vec![];
    for i in 0..30_000u64 {
        let mut path = String::new();
        for _ in 0..1 + next(5) {
            path.push('/');
            path.push_str(names[next(names.len() as u64) as usize]);
        }
        if next(50) == 0 {
            path = "/".into();
        }
        let module = modules[next(modules.len() as u64) as usize];
        let file = FileIdentity {
            path: path.clone(),
            device: 1,
            inode: i,
            modified_seconds: 0,
            modified_nanos: 0,
            tree_signature: None,
        };
        let resource = match module {
            "orphans" => Resource::Process {
                process: ProcessIdentity {
                    pid: i as i32,
                    uid: 501,
                    started_seconds: 0,
                    started_microseconds: 0,
                    executable: path.clone(),
                },
            },
            "worktrees" => Resource::Worktree {
                file,
                repository: "/repo".into(),
                head: "abc".into(),
            },
            "simulators" => Resource::Simulator {
                id: format!("{i}"),
                state: "Shutdown".into(),
            },
            _ => Resource::File { file },
        };
        let mut f = Finding::new(module, &format!("{i}"), "row", resource, "test");
        f.bytes = (next(10) != 0).then(|| next(1_000_000));
        f.allocated_bytes = (next(3) == 0).then(|| next(1_000_000));
        f.memory_bytes = (module == "orphans").then(|| next(1_000));
        if next(3) != 0 {
            f.actions = vec![ActionKind::Trash];
        }
        if next(7) == 0 {
            f.blocked_reason = Some("blocked".into());
        }
        if module == "simulators" && next(2) == 0 {
            f.details = vec![Detail {
                label: "Data path".into(),
                value: path,
            }];
        }
        rows.push(f);
    }
    // Rows at `/` contain everything else, so also compare a set without them.
    let without_root: Vec<Finding> = rows
        .iter()
        .filter(|f| f.resource.path() != Some("/"))
        .cloned()
        .collect();
    for (rows, per_module) in [
        (&rows, true),
        (&rows, false),
        (&without_root, true),
        (&without_root, false),
    ] {
        let fast = analytics_of(rows.iter(), per_module);
        let reference = analytics_reference(rows.iter(), per_module);
        assert_eq!(
            serde_json::to_value(&fast).unwrap(),
            serde_json::to_value(&reference).unwrap(),
            "per_module={per_module}"
        );
        assert!(fast.disk_bytes > 0 && fast.reclaimable_bytes > 0 && fast.process_count > 0);
    }
}

#[test]
fn recommendations_offer_conservative_one_click_fixes() {
    let now = 2_000_000_000.0;
    let day = 86_400.0;
    let row = |module: &str, key: &str, bytes: u64, risk: Risk, last_used: Option<f64>| {
        let mut f = Finding::new(
            module,
            key,
            key,
            Resource::File {
                file: FileIdentity {
                    path: key.into(),
                    device: 1,
                    inode: 1,
                    modified_seconds: 0,
                    modified_nanos: 0,
                    tree_signature: None,
                },
            },
            "test",
        );
        f.bytes = Some(bytes);
        f.risk = risk;
        f.actions = vec![if module == "trash" {
            ActionKind::EmptyTrash
        } else {
            ActionKind::Trash
        }];
        f.last_used_at = last_used;
        f.modified_at = Some(now);
        f.brand = (module == "node").then(|| "pnpm".into());
        f
    };
    let mut blocked = row(
        "node",
        "/p/blocked/node_modules",
        9_000_000_000,
        Risk::Rebuild,
        None,
    );
    blocked.blocked_reason = Some("No owner".into());
    // No last-use date: the modification time decides.
    let mut never = row(
        "node",
        "/p/never/node_modules",
        1_000_000_000,
        Risk::Rebuild,
        None,
    );
    never.modified_at = Some(now - 200.0 * day);
    let rows = vec![
        row(
            "node",
            "/p/old/node_modules",
            3_000_000_000,
            Risk::Rebuild,
            Some(now - 90.0 * day),
        ),
        never,
        // Used last week: not recommended.
        row(
            "node",
            "/p/active/node_modules",
            5_000_000_000,
            Risk::Rebuild,
            Some(now - 7.0 * day),
        ),
        blocked,
        row("caches", "/c/pip", 2_000_000_000, Risk::Rebuild, None),
        // A cache that needs review is not a quick fix.
        row("caches", "/c/models", 8_000_000_000, Risk::Review, None),
        row("caches", "/c/tiny", 10, Risk::Rebuild, None),
        // Nested inside /c/pip: counted once.
        row("caches", "/c/pip/http", 500_000_000, Risk::Rebuild, None),
        row("large", "/v/movie.mov", 7_000_000_000, Risk::Review, None),
        row("trash", "/t/old.zip", 100, Risk::Permanent, None),
    ];
    let store = ResultStore::default();
    store.insert(rows);
    let fixes = store.recommendations(now);
    let summary: Vec<(&str, usize, u64)> = fixes
        .iter()
        .map(|r| (r.id.as_str(), r.count, r.bytes))
        .collect();
    assert_eq!(
        summary,
        [
            ("idle-dependencies", 2, 4_000_000_000),
            ("rebuildable-caches", 3, 2_000_000_010),
            ("empty-trash", 1, 100),
        ]
    );
    let deps = &fixes[0];
    assert_eq!(deps.brand.as_deref(), Some("pnpm"));
    assert_eq!(deps.action, ActionKind::Trash);
    assert!(deps.ids.contains(&"node:/p/old/node_modules".to_owned()));
    assert_eq!(fixes[2].action, ActionKind::EmptyTrash);
    assert_eq!(fixes[2].risk, Risk::Permanent);
}
