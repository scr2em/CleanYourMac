mod common;
use common::*;
use cym_core::{analytics::analytics, model::*, modules, ports::*, Engine};
use std::fs;

fn engine(f: &Fixture) -> Engine {
    Engine::new(services(f), modules::builtin())
}
fn trash(f: &Fixture, finding: &Finding, context: ScanContext) -> ActionResult {
    engine(f)
        .execute(
            &ActionRequest {
                findings: vec![finding.clone()],
                kind: ActionKind::Trash,
                context,
            },
            &ScanControl::default(),
        )
        .remove(0)
}

#[test]
fn current_exclusions_and_out_of_scope_items_block_actions() {
    let f = Fixture::new();
    let s = services(&f);
    let path = f.write("reviewed-file", "fixture");
    let finding = file_finding("fixture", "large", s.entry(&path).unwrap().identity);
    let excluded = trash(
        &f,
        &finding,
        ScanContext {
            exclusions: vec![path.clone()],
            ..f.context()
        },
    );
    assert_eq!(excluded.outcome, Outcome::Failed);
    let outside = trash(
        &f,
        &finding,
        ScanContext {
            roots: vec![f.at("elsewhere")],
            ..Default::default()
        },
    );
    assert_eq!(outside.outcome, Outcome::Failed);
    let mut blocked = finding.clone();
    blocked.blocked_reason = Some("inspect".into());
    assert_eq!(trash(&f, &blocked, f.context()).outcome, Outcome::Failed);
    assert!(fs::metadata(&path).is_ok());
}

#[test]
fn reviewed_trash_and_restore_use_exact_resources() {
    let f = Fixture::new();
    let e = engine(&f);
    let path = f.write("personal.txt", "fixture");
    let finding = file_finding("file", "large", e.services.entry(&path).unwrap().identity);
    let result = e
        .execute(
            &ActionRequest {
                findings: vec![finding],
                kind: ActionKind::Trash,
                context: f.context(),
            },
            &ScanControl::default(),
        )
        .remove(0);
    assert_eq!(result.outcome, Outcome::Applied, "{}", result.message);
    assert!(fs::metadata(&path).is_err());
    assert!(result.trash_identity.is_some());
    assert_eq!(e.history().len(), 1);
    e.restore(&result).unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), "fixture");
    assert!(e.restore(&result).is_err());
}

#[test]
fn restoring_a_changed_trash_folder_is_rejected() {
    let f = Fixture::new();
    let e = engine(&f);
    let folder = f.dir("personal");
    f.write("personal/deep/file", "fixture");
    let identity = e.services.snapshot(&folder, &ScanControl::default()).unwrap();
    let finding = file_finding("folder", "large", identity);
    let result = e
        .execute(
            &ActionRequest {
                findings: vec![finding],
                kind: ActionKind::Trash,
                context: f.context(),
            },
            &ScanControl::default(),
        )
        .remove(0);
    let trashed = result.trash_path.clone().unwrap();
    fs::write(format!("{trashed}/deep/file"), "changed").unwrap();
    assert!(e.restore(&result).is_err());
    assert!(fs::metadata(&folder).is_err());
}

#[test]
fn changed_folder_is_not_moved() {
    let f = Fixture::new();
    let e = engine(&f);
    let folder = f.dir("project/node_modules");
    f.write("project/package.json", "{}");
    f.write("project/node_modules/a/index.js", "x");
    let report = e.scan_report(&["node".into()], &f.context(), &ScanControl::default());
    f.write("project/node_modules/a/late.js", "late");
    let result = e
        .execute(
            &ActionRequest {
                findings: report.findings,
                kind: ActionKind::Trash,
                context: f.context(),
            },
            &ScanControl::default(),
        )
        .remove(0);
    assert_eq!(result.outcome, Outcome::Failed);
    assert!(fs::metadata(&folder).is_ok());
}

#[test]
fn overlapping_selections_are_normalized_and_process_instances_stay_distinct() {
    let file = |id: &str, path: &str| {
        file_finding(
            id,
            "test",
            FileIdentity {
                path: path.into(),
                device: 1,
                inode: 1,
                modified_seconds: 0,
                modified_nanos: 0,
                tree_signature: None,
            },
        )
    };
    let parent = file("parent", "/projects/node_modules");
    let child = file("child", "/projects/node_modules/package/file");
    let peer = file("peer", "/projects/node_modules-other");
    let ids: Vec<_> = cym_core::policy::normalized_selection(&[parent.clone(), parent, child, peer])
        .into_iter()
        .map(|f| f.id)
        .collect();
    assert_eq!(ids, ["parent", "peer"]);
    let process = |id: &str, micro: u64| {
        Finding::new(
            "orphans",
            id,
            "node",
            Resource::Process {
                process: ProcessIdentity {
                    pid: 42,
                    uid: 501,
                    started_seconds: 100,
                    started_microseconds: micro,
                    executable: "/opt/homebrew/bin/node".into(),
                },
            },
            "fixture",
        )
    };
    assert_eq!(
        cym_core::policy::normalized_selection(&[process("a", 1), process("b", 2)]).len(),
        2
    );
}

#[test]
fn analytics_count_each_path_once_and_keep_memory_separate() {
    let file = |module: &str, path: &str, bytes: u64, actionable: bool| {
        let mut f = file_finding(
            &format!("{module}:{path}"),
            module,
            FileIdentity {
                path: path.into(),
                device: 1,
                inode: 1,
                modified_seconds: 0,
                modified_nanos: 0,
                tree_signature: None,
            },
        );
        f.module_id = module.into();
        f.bytes = Some(bytes);
        if !actionable {
            f.actions.clear();
        }
        f
    };
    let mut process = Finding::new(
        "orphans",
        "p",
        "node",
        Resource::Process {
            process: ProcessIdentity {
                pid: 1,
                uid: 1,
                started_seconds: 0,
                started_microseconds: 0,
                executable: "/bin/node".into(),
            },
        },
        "",
    );
    process.memory_bytes = Some(500);
    let rows = vec![
        file("storage", "/p", 1_000, false),
        file("node", "/p/app/node_modules", 600, true),
        file("artifacts", "/p/app/node_modules/.cache", 100, true),
        file("large", "/p-other/big", 50, true),
        process,
    ];
    let a = analytics(&rows);
    assert_eq!(a.disk_bytes, 1_050);
    assert_eq!(a.reclaimable_bytes, 650);
    assert_eq!(a.process_count, 1);
    assert_eq!(a.process_memory_bytes, 500);
    assert_eq!(a.modules.iter().find(|m| m.module_id == "node").unwrap().bytes, 600);
}
