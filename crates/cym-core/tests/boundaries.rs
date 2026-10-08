//! Scope boundaries, checked for every built-in tool at once. A fixture holds the same kinds
//! of clutter inside the chosen folder, inside an excluded folder within it, outside it, and
//! behind a symbolic link that points outside. Every finding of every tool must then respect
//! the context it was scanned with, and every action must respect the context it runs with.
mod common;
use common::*;
use cym_core::{model::*, policy, ports::*, Engine};
use std::{fs, os::unix::fs::symlink};

/// A Node project, a Rust build folder, a 150 MB file and a duplicate pair under `base`.
fn clutter(f: &Fixture, base: &str) {
    f.write(&format!("{base}/web/package.json"), "{}");
    f.write(&format!("{base}/web/node_modules/dep/index.js"), "x");
    f.write(&format!("{base}/crate/Cargo.toml"), "[package]");
    f.write(&format!("{base}/crate/target/debug/app"), "x");
    let big = f.write(&format!("{base}/media/video.mov"), "");
    fs::File::options()
        .write(true)
        .open(big)
        .unwrap()
        .set_len(150_000_000)
        .unwrap();
    let copy = "duplicate contents ".repeat(400);
    f.write(&format!("{base}/docs/a/copy.txt"), &copy);
    f.write(&format!("{base}/docs/b/copy.txt"), &copy);
}

struct Scene {
    f: Fixture,
    engine: Engine,
    ids: Vec<String>,
}
fn scene() -> Scene {
    let f = Fixture::new();
    clutter(&f, "chosen");
    clutter(&f, "chosen/private");
    clutter(&f, "elsewhere");
    symlink(f.at("elsewhere"), f.at("chosen/link")).unwrap();
    // Fixed locations in the fixture home: shared stores, caches, logs and a toolchain.
    f.write("home/.cargo/registry/src/index/serde/lib.rs", "x");
    f.write("home/.npm/_cacache/index", "x");
    f.write("home/.cache/huggingface/hub/model.bin", "x");
    f.write("home/.cache/pip/http/x", "x");
    f.write("home/Library/Logs/App/log.txt", "x");
    f.write("home/Library/Caches/com.example.App/data", "x");
    f.write("home/.nvm/versions/node/v20.0.0/bin/node", "x");
    let engine = Engine::new(services(&f), builtin(&f));
    let ids = engine
        .registry
        .descriptors()
        .into_iter()
        .map(|d| d.id)
        .collect();
    Scene { f, engine, ids }
}

/// Everything a finding must satisfy under the context it was scanned with.
fn assert_within(scene: &Scene, c: &ScanContext) -> Vec<Finding> {
    let report = scene
        .engine
        .scan_report(&scene.ids, c, &ScanControl::default());
    let uses_roots: Vec<String> = scene
        .engine
        .registry
        .descriptors()
        .into_iter()
        .filter(|d| d.uses_roots)
        .map(|d| d.id)
        .collect();
    let under_root = |p: &str| c.roots.iter().any(|r| policy::contains(p, r));
    for r in &report.findings {
        let Some(p) = r.resource.path() else { continue };
        let what = format!("{} {p}", r.module_id);
        assert!(!c.excludes(p), "excluded: {what}");
        assert!(!policy::system_excluded(p), "system: {what}");
        assert!(
            !p.contains("/elsewhere/") && !p.contains("/link/"),
            "outside the chosen folder or through a link: {what}"
        );
        // Shared package stores are fixed locations, listed by a roots-based tool.
        let shared = r.value("Scope") == Some("Shared by all projects");
        if c.limit_to_roots || (uses_roots.contains(&r.module_id) && !shared) {
            assert!(under_root(p), "outside the roots: {what}");
        }
        // An actionable folder never holds an excluded path, or acting on it would move it.
        if !r.actions.is_empty() && r.blocked_reason.is_none() {
            for excluded in &c.exclusions {
                assert!(
                    !policy::contains(excluded, p) || excluded == p,
                    "actionable {what} contains excluded {excluded}"
                );
            }
        }
    }
    report.findings
}

#[test]
fn every_tool_stays_inside_roots_and_out_of_exclusions() {
    let s = scene();
    let base = ScanContext {
        roots: vec![s.f.at("chosen")],
        exclusions: vec![s.f.at("chosen/private")],
        ..Default::default()
    };
    // Roots-based tools only; fixed locations still listed.
    let found = assert_within(&s, &base);
    let count = |id: &str| found.iter().filter(|r| r.module_id == id).count();
    assert!(found
        .iter()
        .any(|r| r.module_id == "node" && r.title == "web"));
    assert_eq!(count("artifacts"), 1);
    assert_eq!(count("large"), 1);
    assert_eq!(count("duplicates"), 2);
    assert!(count("caches") > 0);
    // Limited to the roots, fixed locations outside them disappear.
    let limited = ScanContext {
        limit_to_roots: true,
        ..base.clone()
    };
    let found = assert_within(&s, &limited);
    assert!(found.iter().all(|r| r.module_id != "caches"));
    // Excluding fixed locations removes them, and anything inside them.
    let excluded = ScanContext {
        roots: vec![s.f.at("chosen")],
        exclusions: vec![
            s.f.at("home/.cargo"),
            s.f.at("home/.cache/huggingface"),
            s.f.at("home/Library/Logs/App"),
            s.f.at("home/.nvm"),
        ],
        ..Default::default()
    };
    assert_within(&s, &excluded);
    // A chosen folder that is itself a link scans what it points to, and nothing else;
    // links inside a chosen folder are never followed (checked above).
    let through_link = ScanContext {
        roots: vec![s.f.at("chosen/link")],
        limit_to_roots: true,
        ..Default::default()
    };
    let report = s
        .engine
        .scan_report(&s.ids, &through_link, &ScanControl::default());
    assert!(!report.findings.is_empty());
    for r in &report.findings {
        let p = r.resource.path().unwrap_or_default();
        assert!(p.starts_with(&s.f.at("elsewhere")), "{} {p}", r.module_id);
    }
}

#[test]
fn exclusions_match_however_they_are_spelled() {
    let s = scene();
    // Different letter case, and through the link to the excluded folder.
    for exclusion in [s.f.at("CHOSEN/Private"), s.f.at("chosen/link")] {
        let roots = vec![s.f.at("chosen"), s.f.at("elsewhere")];
        let c = ScanContext {
            roots,
            exclusions: vec![exclusion.clone(), s.f.at("chosen/private")],
            ..Default::default()
        };
        let excluded = if exclusion.ends_with("link") {
            s.f.at("elsewhere")
        } else {
            s.f.at("chosen/private")
        };
        let c = ScanContext {
            exclusions: vec![exclusion],
            ..c
        };
        let report = s.engine.scan_report(&s.ids, &c, &ScanControl::default());
        for r in &report.findings {
            let p = r.resource.path().unwrap_or_default();
            assert!(!policy::contains(p, &excluded), "{} {p}", r.module_id);
        }
    }
}

#[test]
fn actions_recheck_the_current_scope() {
    let s = scene();
    let scanned = ScanContext {
        roots: vec![s.f.at("chosen")],
        ..Default::default()
    };
    let findings: Vec<Finding> = assert_within(&s, &scanned)
        .into_iter()
        .filter(|r| {
            r.actions.contains(&ActionKind::Trash)
                && r.blocked_reason.is_none()
                && r.resource.path().is_some()
        })
        .collect();
    assert!(!findings.is_empty());
    let attempt = |f: &Finding, c: ScanContext| {
        s.engine
            .execute(
                &ActionRequest {
                    findings: vec![f.clone()],
                    kind: ActionKind::Trash,
                    context: c,
                    acknowledged: vec![],
                    force: true,
                },
                &ScanControl::default(),
            )
            .remove(0)
    };
    for f in &findings {
        let path = f.resource.path().unwrap().to_owned();
        let parent = std::path::Path::new(&path)
            .parent()
            .unwrap()
            .to_string_lossy()
            .to_string();
        // Excluded since the scan, or with an excluded ancestor or descendant.
        for exclusion in [path.clone(), parent, format!("{path}/inner")] {
            let result = attempt(
                f,
                ScanContext {
                    exclusions: vec![exclusion.clone()],
                    ..scanned.clone()
                },
            );
            if exclusion.ends_with("/inner") && !fs::metadata(&path).is_ok_and(|m| m.is_dir()) {
                continue;
            }
            assert_eq!(
                result.outcome,
                Outcome::Failed,
                "{} {path} acted on with {exclusion} excluded",
                f.module_id
            );
        }
        // Outside the chosen folders when the scope is limited to them.
        let result = attempt(
            f,
            ScanContext {
                roots: vec![s.f.at("elsewhere")],
                limit_to_roots: true,
                ..scanned.clone()
            },
        );
        assert_eq!(
            result.outcome,
            Outcome::Failed,
            "{} {path} acted on outside the roots",
            f.module_id
        );
        assert!(fs::symlink_metadata(&path).is_ok(), "{path} was moved");
    }
}

#[test]
fn changing_scope_forgets_rows_outside_it_and_stale_scans_cannot_write() {
    let s = scene();
    let k = ScanControl::default();
    let whole = ScanContext {
        roots: vec![s.f.at("chosen"), s.f.at("elsewhere")],
        ..Default::default()
    };
    s.engine
        .scan_to_store(&["large".into()], &whole, &k, 1, &|_| {});
    assert_eq!(s.engine.results.len(), 3);
    // Now only the chosen folder, with its private folder excluded.
    let narrow = ScanContext {
        roots: vec![s.f.at("chosen")],
        exclusions: vec![s.f.at("chosen/private")],
        limit_to_roots: true,
        ..Default::default()
    };
    let removed = s.engine.retain_in_scope(&narrow);
    assert_eq!(removed.len(), 2, "{removed:?}");
    assert_eq!(s.engine.results.len(), 1);
    // A scan that started before its module was cleared cannot add rows afterwards.
    let epoch = s.engine.results.epoch("large");
    s.engine.results.clear_modules(&["large".into()]);
    let stale = s.engine.scan_report(&["large".into()], &whole, &k).findings;
    assert!(!s.engine.results.insert_at("large", epoch, stale.clone()));
    assert_eq!(s.engine.results.len(), 0);
    let current = s.engine.results.epoch("large");
    assert!(s.engine.results.insert_at("large", current, stale));
    // Selected IDs that are gone are reported, not silently dropped.
    let results = s
        .engine
        .execute_ids(&["large:/nowhere".into()], ActionKind::Trash, &whole, &k);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].outcome, Outcome::Failed);
}

#[test]
fn a_duplicate_stays_when_its_original_is_in_the_same_batch() {
    let s = scene();
    let c = ScanContext {
        roots: vec![s.f.at("chosen")],
        exclusions: vec![s.f.at("chosen/private")],
        ..Default::default()
    };
    let report = s
        .engine
        .scan_report(&["duplicates".into()], &c, &ScanControl::default());
    let copy = report
        .findings
        .iter()
        .find(|r| r.blocked_reason.is_none())
        .unwrap()
        .clone();
    let original = copy.value("Preserved original").unwrap().to_owned();
    // The folder holding the original goes in the same batch.
    let folder = std::path::Path::new(&original).parent().unwrap();
    let identity = s
        .engine
        .services
        .entry(folder.to_str().unwrap())
        .unwrap()
        .identity;
    let holder = file_finding("holder", "large", identity);
    let results = s.engine.execute(
        &ActionRequest {
            findings: vec![copy.clone(), holder],
            kind: ActionKind::Trash,
            context: c,
            acknowledged: vec![],
            force: true,
        },
        &ScanControl::default(),
    );
    let mine = results
        .iter()
        .find(|r| r.finding_id.as_deref() == Some(copy.id.as_str()))
        .unwrap();
    assert_eq!(mine.outcome, Outcome::Failed, "{}", mine.message);
    assert!(fs::metadata(copy.resource.path().unwrap()).is_ok());
}

#[test]
fn restore_only_returns_trash_items_to_unprotected_places() {
    let s = scene();
    let path = s.f.write("chosen/notes.txt", "x");
    let mut row = cym_core::model::ActionResult {
        id: "r".into(),
        date: 0.0,
        title: "notes".into(),
        original_path: Some(s.f.at("chosen/restored.txt")),
        action: ActionKind::Trash,
        outcome: Outcome::Applied,
        message: String::new(),
        trash_path: Some(path.clone()),
        finding_id: None,
        trash_identity: Some(s.engine.services.entry(&path).unwrap().identity),
        journal_warning: None,
        overridable: false,
    };
    // Not in a Trash folder.
    assert!(s.engine.restore(&row).is_err());
    // Into a protected location.
    row.original_path = Some(policy::home());
    assert!(s.engine.restore(&row).is_err());
    assert!(fs::metadata(&path).is_ok());
    // The parent of a protected folder is protected too.
    let home = policy::home();
    let parent = std::path::Path::new(&home).parent().unwrap();
    assert!(policy::protected(parent.to_str().unwrap()));
}
