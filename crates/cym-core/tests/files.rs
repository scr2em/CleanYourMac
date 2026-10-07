mod common;
use common::*;
use cym_core::{model::*, policy, ports::*, Services};
use std::{
    fs,
    os::unix::fs::symlink,
    sync::{Arc, Mutex},
};

#[test]
fn path_boundaries_and_sensitive_items() {
    assert!(policy::contains("/Projects/app/file", "/Projects/app"));
    assert!(!policy::contains(
        "/Projects/application/file",
        "/Projects/app"
    ));
    assert!(!policy::contains(
        "/Projects/app/../../secret",
        "/Projects/app"
    ));
    assert!(policy::protected("/Projects/app/.git/config"));
    assert!(policy::protected("/Projects/app/.env.local"));
    assert!(policy::protected(&(policy::home() + "/.ssh/id_ed25519")));
    assert!(policy::protected(
        &(policy::home() + "/Library/CloudStorage/provider/file")
    ));
    assert!(policy::protected(&policy::home()));
    assert!(policy::duplicate_excluded("/Projects/app/node_modules/x"));
    assert!(policy::duplicate_excluded("/Projects/app/Pods/x"));
    assert!(!policy::duplicate_excluded("/Projects/app/src/x"));
    for ignored in [
        ".angular",
        "node_modules",
        ".svelte-kit",
        "zig-out",
        "bazel-out",
        "__pycache__",
        "dist-newstyle",
        "DerivedData",
        "lib.egg-info",
    ] {
        assert!(
            policy::duplicate_excluded(&format!("/Projects/app/{ignored}/cache/file")),
            "{ignored}"
        );
    }
    assert!(policy::package("/Applications/Editor.app"));
}

#[test]
fn file_identity_rejects_changes_and_symlink_swaps() {
    let f = Fixture::new();
    let s = services(&f);
    let k = ScanControl::default();
    let file = f.write("data", "before");
    let identity = s.entry(&file).unwrap().identity;
    fs::write(&file, "after with a different size").unwrap();
    assert!(s.validate(&identity, &f.context(), &[], &k).is_err());
    fs::remove_file(&file).unwrap();
    let other = f.write("other", "fixture");
    symlink(other, &file).unwrap();
    assert!(s.validate(&identity, &f.context(), &[], &k).is_err());
}

#[test]
fn protected_descendant_blocks_removal_of_its_parent() {
    let f = Fixture::new();
    let s = services(&f);
    let folder = f.dir("reviewed-folder");
    let preserved = f.write("reviewed-folder/preserved.txt", "keep");
    let context = ScanContext {
        exclusions: vec![preserved.clone()],
        ..f.context()
    };
    assert!(context.protects(&folder));
    let identity = s.snapshot(&folder, &ScanControl::default()).unwrap();
    assert!(s
        .validate(&identity, &context, &[], &ScanControl::default())
        .is_err());
}

#[test]
fn sizing_does_not_double_count_hard_link_allocation() {
    let f = Fixture::new();
    let s = services(&f);
    let original = f.write("original", &"x".repeat(10_000));
    fs::hard_link(&original, f.at("linked")).unwrap();
    let size = s.sizer.size(&f.path(), &ScanControl::default()).unwrap();
    assert_eq!(size.logical, 20_000);
    assert_eq!(size.allocated, s.entry(&original).unwrap().allocated);
    assert_eq!(size.files, 2);
    assert!(size.complete);
}

#[test]
fn aliased_and_overlapping_roots_are_scanned_once() {
    let f = Fixture::new();
    let s = services(&f);
    let project = f.dir("project");
    let nested = f.dir("project/nested");
    let alias = f.at("alias");
    symlink(&project, &alias).unwrap();
    assert_eq!(s.roots(&[project.clone(), alias, nested]), vec![project]);
}

#[test]
fn folder_identity_detects_deep_changes_and_counts_links_without_following_them() {
    let f = Fixture::new();
    let s = services(&f);
    let k = ScanControl::default();
    let folder = f.dir("artifact");
    let nested = f.write("artifact/deep/file", "before");
    let external = f.write("external/file", &"x".repeat(50_000));
    symlink(&external, format!("{folder}/linked")).unwrap();
    let size = s.sizer.size(&folder, &k).unwrap();
    assert!(size.complete);
    assert!(size.logical < 50_000);
    let identity = s.snapshot(&folder, &k).unwrap();
    s.validate(&identity, &f.context(), &[], &k).unwrap();
    fs::write(&nested, "changed inside nested folder").unwrap();
    assert!(s.validate(&identity, &f.context(), &[], &k).is_err());
    assert!(fs::metadata(&external).is_ok());
}

#[test]
fn walk_enforces_exclusions_packages_and_symlinks() {
    let f = Fixture::new();
    let s = services(&f);
    f.write("keep/a.txt", "a");
    f.write("skip/b.txt", "b");
    f.write("Tool.app/Contents/Info.plist", "x");
    symlink(f.at("keep"), f.at("link")).unwrap();
    let context = ScanContext {
        exclusions: vec![f.at("skip")],
        ..f.context()
    };
    let mut seen = vec![];
    let mut warnings = vec![];
    s.walk(&context, &ScanControl::default(), &mut warnings, &mut |e| {
        seen.push(e.path().strip_prefix(&f.path()).unwrap().to_owned());
        Ok(true)
    })
    .unwrap();
    seen.sort();
    assert_eq!(seen, ["/Tool.app", "/keep", "/keep/a.txt"]);
}

/// A walker that records roots and delegates, proving traversal is replaceable.
struct RecordingWalker(Arc<dyn Walker>, Mutex<Vec<String>>);
impl Walker for RecordingWalker {
    fn walk(
        &self,
        root: &str,
        control: &ScanControl,
        problem: &mut dyn FnMut(String),
        visit: &mut dyn FnMut(&Entry) -> Result<Visit>,
    ) -> Result<()> {
        self.1.lock().unwrap().push(root.into());
        self.0.walk(root, control, problem, visit)
    }
}
#[test]
fn walker_can_be_replaced_without_changing_modules() {
    let f = Fixture::new();
    f.write("app/package.json", "{}");
    f.write("app/node_modules/x/index.js", "x");
    let base = services(&f);
    let walker = Arc::new(RecordingWalker(base.walker.clone(), Mutex::default()));
    let s = Services {
        walker: walker.clone(),
        ..base
    };
    let engine = cym_core::Engine::new(s, cym_core::modules::builtin());
    let report = engine.scan_report(&["node".into()], &f.context(), &ScanControl::default());
    assert_eq!(report.findings.len(), 1);
    assert_eq!(*walker.1.lock().unwrap(), vec![f.path()]);
}

#[test]
fn walk_limit_reports_partial_coverage() {
    let f = Fixture::new();
    for i in 0..5 {
        f.write(&format!("file-{i}"), "x");
    }
    let s = Services {
        walk_limit: 2,
        ..services(&f)
    };
    let mut warnings = vec![];
    s.walk(
        &f.context(),
        &ScanControl::default(),
        &mut warnings,
        &mut |_| Ok(true),
    )
    .unwrap();
    assert!(warnings.iter().any(|w| w.contains("limit")));
}

#[test]
fn cancelled_walk_stops() {
    let f = Fixture::new();
    f.write("a", "x");
    let s = services(&f);
    let k = ScanControl::default();
    k.cancel();
    assert!(s
        .walk(&f.context(), &k, &mut vec![], &mut |_| Ok(true))
        .is_err());
}
