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
