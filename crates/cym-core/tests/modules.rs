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
    Engine::new(services(f), modules::builtin()).scan_report(
        &[id.into()],
        &f.context(),
        &ScanControl::default(),
    )
}

#[test]
fn node_finder_groups_nested_dependencies_and_requires_an_owner() {
    let f = Fixture::new();
    f.write("app/package.json", "{}");
    f.write("app/pnpm-lock.yaml", "lockfileVersion: 9");
    f.write(
        "app/node_modules/dependency/node_modules/nested/index.js",
        "x",
    );
    f.write("unknown/node_modules/index.js", "x");
    let report = scan(&f, "node");
    assert_eq!(report.findings.len(), 2, "{:?}", report.warnings);
    let app = report.findings.iter().find(|f| f.title == "app").unwrap();
    assert_eq!(app.actions, vec![ActionKind::Trash]);
    assert_eq!(app.value("Package manager"), Some("pnpm"));
    let unknown = report
        .findings
        .iter()
        .find(|f| f.title == "unknown")
        .unwrap();
    assert!(unknown.actions.is_empty());
    assert!(unknown.blocked_reason.is_some());
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
    let engine = Engine::new(services(&f), modules::builtin());
    let results = engine.execute(
        &ActionRequest {
            findings: vec![candidate.clone()],
            kind: ActionKind::Trash,
            context: f.context(),
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
fn storage_lists_immediate_children_as_inventory() {
    let f = Fixture::new();
    f.write("folder/a", "abc");
    f.write("file", "abcdef");
    let report = scan(&f, "storage");
    assert_eq!(report.findings.len(), 2);
    assert!(report.findings.iter().all(|f| f.actions.is_empty()));
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
