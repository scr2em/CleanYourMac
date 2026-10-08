mod common;
use common::*;
use cym_core::{
    model::*,
    modules::{self, applications::leftover_id, developer::ArtifactsModule, ScanModule},
    ports::*,
    Engine, Services,
};
use std::{fs, sync::Arc};

fn scan(f: &Fixture, id: &str) -> ScanReport {
    Engine::new(services(f), builtin(f)).scan_report(
        &[id.into()],
        &f.context(),
        &ScanControl::default(),
    )
}

#[test]
fn node_finder_lists_projects_and_skips_package_manager_folders() {
    let f = Fixture::new();
    f.write("app/package.json", "{}");
    f.write("app/pnpm-lock.yaml", "lockfileVersion: 9");
    f.write(
        "app/node_modules/dependency/node_modules/nested/index.js",
        "x",
    );
    // No owning package.json: left out rather than listed blocked.
    f.write("unknown/node_modules/index.js", "x");
    // Package managers' own folders, even where they look like projects.
    f.write("Library/pnpm/global/5/package.json", "{}");
    f.write("Library/pnpm/global/5/node_modules/tool/index.js", "x");
    f.write(
        "Library/pnpm/global/5/.pnpm/marked@13.0.3/node_modules/marked/index.js",
        "x",
    );
    f.write(".npm/_npx/272617f699a83097/package.json", "{}");
    f.write(".npm/_npx/272617f699a83097/node_modules/x/index.js", "x");
    f.write(".bun/install/global/package.json", "{}");
    f.write(".bun/install/global/node_modules/x/index.js", "x");
    f.write(
        ".nvm/versions/node/v22.0.0/lib/node_modules/npm/package.json",
        "{}",
    );
    // A pnpm virtual store inside any project is pnpm's too.
    f.write(
        "store/.pnpm/refractor@5.0.0/node_modules/refractor/index.js",
        "x",
    );
    let node = modules::developer::DependenciesModule {
        home: Some(f.path()),
        ..Default::default()
    };
    let engine = Engine::new(services(&f), builtin(&f).register(Arc::new(node)));
    let report = engine.scan_report(&["node".into()], &f.context(), &ScanControl::default());
    let titles: Vec<_> = report.findings.iter().map(|f| f.title.as_str()).collect();
    assert_eq!(titles, ["app"], "{:?}", report.warnings);
    let app = &report.findings[0];
    assert_eq!(app.actions, vec![ActionKind::Trash]);
    assert_eq!(app.value("Package manager"), Some("pnpm"));
    assert!(app.blocked_reason.is_none());
}

#[test]
fn artifacts_require_project_evidence() {
    let f = Fixture::new();
    let s = services(&f);
    let module = ArtifactsModule::default();
    assert_eq!(module.matches(&s, &f.dir("build")), None);
    assert_eq!(module.matches(&s, &f.dir("target")), None);
    f.write("Cargo.toml", "[package]");
    let cargo = module.matches(&s, &f.at("target")).unwrap();
    assert_eq!(cargo.rule.ecosystem, "Rust (Cargo)");
    assert_eq!(cargo.project, f.path());
    assert_eq!(module.matches(&s, &f.dir("dist")), None);
}

#[test]
fn duplicates_preserve_an_original_skip_dependency_stores_and_revalidate() {
    let f = Fixture::new();
    let body = "equal contents ".repeat(400);
    f.write("a.txt", &body);
    f.write("b.txt", &body);
    f.write("different.txt", &"other contents ".repeat(400));
    f.write("node_modules/c.txt", &body);
    f.write("build/d.txt", &body);
    let report = scan(&f, "duplicates");
    assert_eq!(report.findings.len(), 2, "{:?}", report.findings);
    assert_eq!(
        report
            .findings
            .iter()
            .filter(|f| f.actions.is_empty())
            .count(),
        1
    );
    let candidate = report
        .findings
        .iter()
        .find(|f| !f.actions.is_empty())
        .unwrap();
    let original = candidate.value("Preserved original").unwrap();
    fs::write(original, "changed").unwrap();
    let engine = Engine::new(services(&f), builtin(&f));
    let results = engine.execute(
        &ActionRequest {
            findings: vec![candidate.clone()],
            kind: ActionKind::Trash,
            context: f.context(),
            acknowledged: vec![],
            force: false,
        },
        &ScanControl::default(),
    );
    assert_eq!(results[0].outcome, Outcome::Failed);
    assert!(fs::metadata(candidate.resource.path().unwrap()).is_ok());
}

#[test]
fn leftover_names_exclude_apple_and_shared_containers() {
    assert_eq!(
        leftover_id("com.example.Editor.plist").as_deref(),
        Some("com.example.Editor")
    );
    assert_eq!(
        leftover_id("com.example.Editor.savedState").as_deref(),
        Some("com.example.Editor")
    );
    assert_eq!(leftover_id("com.apple.finder.plist"), None);
    assert_eq!(leftover_id("group.com.example.shared"), None);
    assert_eq!(leftover_id("Unidentified User Files"), None);
}

struct ExampleModule;
impl ScanModule for ExampleModule {
    fn descriptor(&self) -> ModuleDescriptor {
        ModuleDescriptor::new("node", "Replacement", "Developer", "folder", "", true)
    }
    fn scan(
        &self,
        _: &Services,
        _: &ScanContext,
        _: &ScanControl,
        sink: &mut dyn Sink,
    ) -> Result<()> {
        sink.warning("replacement ran".into());
        Err("fixture unavailable".into())
    }
}
#[test]
fn modules_can_be_replaced_and_failures_become_warnings() {
    let registry = modules::builtin()
        .register(Arc::new(ExampleModule))
        .remove("orphans");
    let ids: Vec<_> = registry.descriptors().into_iter().map(|d| d.id).collect();
    assert_eq!(ids.iter().filter(|id| *id == "node").count(), 1);
    assert_eq!(ids[3], "node");
    assert!(!ids.contains(&"orphans".to_string()));
    let f = Fixture::new();
    let report = Engine::new(services(&f), registry).scan_report(
        &["node".into(), "missing".into()],
        &f.context(),
        &ScanControl::default(),
    );
    assert_eq!(
        report.warnings,
        [
            "Unknown module: missing",
            "Replacement: replacement ran",
            "Replacement: fixture unavailable"
        ]
    );
    assert!(
        modules::Registry::new(vec![Arc::new(ExampleModule), Arc::new(ExampleModule)]).is_err()
    );
}

#[test]
fn storage_lists_immediate_children_that_can_be_reviewed_for_trash() {
    let f = Fixture::new();
    f.write("folder/a", "abc");
    f.write("file", "abcdef");
    let report = scan(&f, "storage");
    assert_eq!(report.findings.len(), 2);
    assert!(report
        .findings
        .iter()
        .all(|f| f.actions == [ActionKind::Trash] && f.risk == Risk::Review));
    let folder = report
        .findings
        .iter()
        .find(|f| f.title == "folder")
        .unwrap();
    assert_eq!(folder.bytes, Some(3));
}

#[test]
fn large_files_respect_the_threshold() {
    let f = Fixture::new();
    f.write("small", "x");
    let big = fs::File::create(f.at("big.bin")).unwrap();
    big.set_len(100_000_000).unwrap();
    let report = scan(&f, "large");
    assert_eq!(report.findings.len(), 1);
    assert_eq!(report.findings[0].title, "big.bin");
}

#[test]
fn duplicate_stages_separate_files_that_share_only_a_prefix() {
    let f = Fixture::new();
    let prefix = "p".repeat(70_000);
    f.write("same-1", &format!("{prefix}tail"));
    f.write("same-2", &format!("{prefix}tail"));
    f.write("prefix-only", &format!("{prefix}diff"));
    fs::hard_link(f.at("same-1"), f.at("same-1-link")).unwrap();
    let report = scan(&f, "duplicates");
    let mut titles: Vec<_> = report.findings.iter().map(|f| f.title.as_str()).collect();
    titles.sort();
    // Hard links to the preserved inode are one copy; the prefix-only file is unique.
    assert_eq!(titles.len(), 2, "{titles:?}");
    assert!(!titles.contains(&"prefix-only"));
    assert!(report.findings.iter().all(|f| f.value("BLAKE3").is_some()));
}

#[test]
fn overview_scan_leaves_out_explicit_only_modules() {
    let explicit: Vec<String> = modules::builtin()
        .descriptors()
        .into_iter()
        .filter(|d| !d.in_overview)
        .map(|d| d.id)
        .collect();
    assert_eq!(explicit, ["duplicates"]);
    let json = serde_json::to_value(modules::builtin().descriptors()).unwrap();
    let duplicates = json
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["id"] == "duplicates")
        .unwrap();
    assert_eq!(duplicates["inOverview"], false);
}

#[test]
fn sparse_files_count_by_size_on_disk() {
    let f = Fixture::new();
    // A virtual disk image: 1 GB long, almost nothing written.
    fs::create_dir_all(f.at("vm")).unwrap();
    fs::File::create(f.at("vm/disk.img"))
        .unwrap()
        .set_len(1_000_000_000)
        .unwrap();
    let report = scan(&f, "storage");
    let vm = report.findings.iter().find(|f| f.title == "vm").unwrap();
    assert_eq!(vm.bytes, Some(1_000_000_000));
    assert!(vm.allocated_bytes.unwrap() < 1_000_000);
    let totals = cym_core::analytics::analytics(&report.findings);
    assert_eq!(totals.logical_bytes, 1_000_000_000);
    assert!(totals.disk_bytes < 1_000_000, "{}", totals.disk_bytes);
    assert_eq!(totals.disk_bytes, vm.allocated_bytes.unwrap());
}

#[test]
fn chosen_folders_limit_every_tool_to_them() {
    let f = Fixture::new();
    // Inside the chosen folder.
    f.write("projects/app/package.json", "{}");
    f.write("projects/app/node_modules/x/index.js", "x");
    // Fixed locations outside it: shared stores, caches, logs, toolchains and Mac leftovers.
    f.write("home/.cargo/registry/src/index/serde/lib.rs", "x");
    f.write("home/.npm/_cacache/index", "x");
    f.write("home/.cache/tool/data", "x");
    f.write("home/Library/Logs/App/log.txt", "x");
    f.write("home/Library/Caches/com.example.App/data", "x");
    f.write("home/.nvm/versions/node/v20.0.0/bin/node", "x");
    let context = ScanContext {
        roots: vec![f.at("projects")],
        limit_to_roots: true,
        ..Default::default()
    };
    let engine = Engine::new(services(&f), builtin(&f));
    let ids: Vec<String> = engine
        .registry
        .descriptors()
        .into_iter()
        .map(|d| d.id)
        .collect();
    let report = engine.scan_report(&ids, &context, &ScanControl::default());
    let outside: Vec<_> = report
        .findings
        .iter()
        .filter_map(|r| r.resource.path())
        .filter(|p| !p.starts_with(&f.at("projects")))
        .collect();
    assert!(outside.is_empty(), "{outside:?}");
    assert!(report
        .findings
        .iter()
        .any(|r| r.module_id == "node" && r.title == "app"));
    // Without the limit, the same scan also lists the fixed locations.
    let everywhere = engine.scan_report(
        &["caches".into(), "node".into(), "toolchains".into()],
        &ScanContext {
            limit_to_roots: false,
            ..context
        },
        &ScanControl::default(),
    );
    assert!(everywhere.findings.iter().any(|r| r
        .resource
        .path()
        .is_some_and(|p| p.starts_with(&f.at("home")))));
}
