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
    let engine = cym_core::Engine::new(s, builtin(&f));
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

#[test]
fn prefetching_walker_visits_in_stack_order_and_honors_skip_and_stop() {
    use cym_core::adapters::fs::{PrefetchWalker, StackWalker, StdFileSystem};
    let f = Fixture::new();
    for a in 0..6 {
        for b in 0..5 {
            f.write(&format!("d{a}/e{b}/file.txt"), "x");
            f.write(&format!("d{a}/skip/e{b}/file.txt"), "x");
        }
    }
    let files: Arc<dyn FileSystem> = Arc::new(StdFileSystem);
    let pool = Arc::new(
        rayon::ThreadPoolBuilder::new()
            .num_threads(2)
            .build()
            .unwrap(),
    );
    let walk = |walker: &dyn Walker, stop_after: usize| {
        let mut seen = vec![];
        walker
            .walk(
                &f.path(),
                &ScanControl::default(),
                &mut |e| panic!("{e}"),
                &mut |e| {
                    seen.push(e.path().to_owned());
                    Ok(if seen.len() == stop_after {
                        Visit::Stop
                    } else if e.name() == "skip" {
                        Visit::Skip
                    } else {
                        Visit::Descend
                    })
                },
            )
            .unwrap();
        seen
    };
    let mut prefetch = PrefetchWalker::new(files.clone(), Some(pool));
    prefetch.window = 3;
    // Always read ahead, so the prefetching path is the one compared.
    prefetch.slow_listing = std::time::Duration::ZERO;
    let expected = walk(&StackWalker(files.clone()), usize::MAX);
    let seen = walk(&prefetch, usize::MAX);
    assert_eq!(seen, expected);
    assert!(!seen.iter().any(|p| p.contains("/skip/")));
    assert_eq!(walk(&prefetch, 7), expected[..7]);
}

#[test]
fn bulk_listing_matches_lstat_for_every_kind_of_entry() {
    use cym_core::adapters::{bulk::BulkFileSystem, fs::StdFileSystem};
    let f = Fixture::new();
    f.write("plain.txt", "hello");
    f.write("empty", "");
    f.write("é ü 名前.txt", "unicode");
    f.write("nested/inner.txt", "x");
    fs::hard_link(f.at("plain.txt"), f.at("linked.txt")).unwrap();
    symlink(f.at("plain.txt"), f.at("link")).unwrap();
    symlink(f.at("missing"), f.at("dangling")).unwrap();
    let fifo = std::ffi::CString::new(f.at("pipe")).unwrap();
    assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
    for i in 0..3000 {
        f.write(&format!("many/{i:04}-{}", "n".repeat(i % 200)), "x");
    }
    for folder in [f.path(), f.at("many"), format!("{}/", f.path())] {
        let list = |fs: &dyn FileSystem| {
            let mut entries: Vec<_> = fs
                .children(&folder)
                .unwrap()
                .into_iter()
                .map(|e| e.unwrap())
                .map(|e| {
                    let file = e.regular || e.symlink;
                    (
                        e.identity.path.clone(),
                        (e.identity.device, e.identity.inode),
                        (e.identity.modified_seconds, e.identity.modified_nanos),
                        (e.directory, e.regular, e.symlink, e.dataless),
                        file.then_some((e.bytes, e.links)),
                    )
                })
                .collect();
            entries.sort();
            entries
        };
        assert_eq!(list(&BulkFileSystem), list(&StdFileSystem), "{folder}");
    }
    assert!(BulkFileSystem.children(&f.at("absent")).is_err());
}

/// The path rules as they were before the allocation-free rewrite, kept as the reference.
mod reference {
    use cym_core::policy::{canonical, home, DUPLICATE_IGNORES, DUPLICATE_IGNORE_SUFFIXES};
    use std::path::Path;
    pub fn contains(path: &str, root: &str) -> bool {
        Path::new(&canonical(path)).starts_with(canonical(root))
    }
    pub fn system_excluded(path: &str) -> bool {
        [
            "/System", "/Library", "/bin", "/sbin", "/usr", "/dev", "/Network", "/private", "/var",
            "/etc", "/tmp",
        ]
        .iter()
        .any(|r| contains(path, r))
            || sensitive(path)
    }
    fn sensitive(path: &str) -> bool {
        let h = home();
        [
            "/.ssh",
            "/.aws",
            "/.gnupg",
            "/Library/Keychains",
            "/Library/Mobile Documents",
            "/Library/CloudStorage",
        ]
        .iter()
        .any(|r| contains(path, &(h.clone() + r)))
            || Path::new(path).components().any(|c| {
                let s = c.as_os_str().to_string_lossy();
                s == ".git" || s == ".env" || s.starts_with(".env.")
            })
    }
    pub fn duplicate_excluded(path: &str) -> bool {
        system_excluded(path)
            || Path::new(path).components().any(|c| {
                let name = c.as_os_str().to_string_lossy().to_lowercase();
                DUPLICATE_IGNORES.contains(&name.as_str())
                    || DUPLICATE_IGNORE_SUFFIXES
                        .iter()
                        .any(|suffix| name.ends_with(&suffix.to_lowercase()))
            })
    }
}

#[test]
fn fast_path_rules_match_the_reference_rules() {
    let home = policy::home();
    let segments = [
        "Users",
        "dev",
        "usr",
        "usrx",
        "Library",
        "Keychains",
        "Mobile Documents",
        ".git",
        ".gitignore",
        ".env",
        ".env.local",
        ".envrc",
        "node_modules",
        "Node_Modules",
        "x.egg-info",
        "App.dSYM",
        "b.XCARCHIVE",
        "projects",
        ".",
        "..",
        "é",
        "名前",
        "target",
        "Target",
        "CloudStorage",
        ".ssh",
        ".sshx",
        "tmp",
        "private",
        "a b",
    ];
    let mut seed = 0x9E37_79B9_7F4A_7C15u64;
    let mut next = move |n: usize| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed % n as u64) as usize
    };
    let mut paths = vec![
        "/".to_owned(),
        home.clone(),
        "~".into(),
        "~/".into(),
        "".into(),
    ];
    for _ in 0..1_500 {
        let mut path = match next(6) {
            0 => String::new(),
            1 => "~".into(),
            2 => home.clone(),
            _ => "/".into(),
        };
        for i in 0..1 + next(5) {
            if i > 0 || !(path.is_empty() || path.ends_with('/')) {
                path.push_str(if next(8) == 0 { "//" } else { "/" });
            }
            path.push_str(segments[next(segments.len())]);
        }
        if next(6) == 0 {
            path.push('/');
        }
        paths.push(path);
    }
    let roots: Vec<String> = paths.iter().step_by(15).cloned().collect();
    // Real subfolders only: excluding `/` or the working folder would exclude everything.
    let mut context = ScanContext {
        exclusions: paths[5..]
            .iter()
            .filter(|p| p.len() > home.len() + 4)
            .step_by(60)
            .cloned()
            .collect(),
        ..Default::default()
    };
    // Ensure exclusion coverage independently of the machine's home-path length.
    let excluded_root = format!("{home}/Projects/policy-fixture-excluded");
    context.exclusions.push(excluded_root.clone());
    for index in 0..32 {
        paths.push(format!("{excluded_root}/file-{index}.txt"));
        paths.push(format!(
            "{home}/Projects/policy-fixture-visible/file-{index}.txt"
        ));
    }
    let scope = policy::Scope::new(&context);
    // Every rule must give both answers somewhere, or the comparison proves nothing.
    let mut outcomes = [[0usize; 2]; 4];
    for path in &paths {
        outcomes[0][usize::from(
            roots[1..]
                .iter()
                .filter(|r| r.len() > 2)
                .any(|r| policy::contains(path, r) && path != r),
        )] += 1;
        outcomes[1][usize::from(policy::system_excluded(path))] += 1;
        outcomes[2][usize::from(scope.excludes(path))] += 1;
        outcomes[3][usize::from(policy::duplicate_excluded(path))] += 1;
        for root in &roots {
            assert_eq!(
                policy::contains(path, root),
                reference::contains(path, root),
                "contains({path:?}, {root:?})"
            );
        }
        let system = reference::system_excluded(path);
        assert_eq!(policy::system_excluded(path), system, "system {path:?}");
        assert_eq!(scope.system_excluded(path), system, "scope system {path:?}");
        let excluded = context
            .exclusions
            .iter()
            .any(|r| reference::contains(path, r));
        assert_eq!(context.excludes(path), excluded, "excludes {path:?}");
        assert_eq!(scope.excludes(path), excluded, "scope excludes {path:?}");
        assert_eq!(
            policy::duplicate_excluded(path),
            reference::duplicate_excluded(path),
            "duplicate {path:?}"
        );
    }
    assert!(
        outcomes.iter().all(|o| o[0] > 20 && o[1] > 20),
        "{outcomes:?}"
    );
}
