use crate::{
    files::{self, ScanControl},
    model::*,
    policy,
};
use std::{
    collections::{HashMap, HashSet},
    path::Path,
    sync::Arc,
};

pub trait ScanModule: Send + Sync {
    fn descriptor(&self) -> ModuleDescriptor;
    fn scan(
        &self,
        context: &ScanContext,
        control: &ScanControl,
        report: &mut ScanReport,
    ) -> Result<()>;
}
pub struct Registry {
    modules: Vec<Arc<dyn ScanModule>>,
}
impl Registry {
    pub fn new(modules: Vec<Arc<dyn ScanModule>>) -> Result<Self> {
        let mut ids = HashSet::new();
        if modules.iter().any(|m| !ids.insert(m.descriptor().id)) {
            return Err("Module IDs must be unique".into());
        }
        Ok(Self { modules })
    }
    pub fn descriptors(&self) -> Vec<ModuleDescriptor> {
        self.modules.iter().map(|m| m.descriptor()).collect()
    }
    pub fn scan(
        &self,
        id: &str,
        context: &ScanContext,
        control: &ScanControl,
    ) -> Result<ScanReport> {
        let module = self
            .modules
            .iter()
            .find(|m| m.descriptor().id == id)
            .ok_or("Unknown module")?;
        let mut report = ScanReport::default();
        if let Err(error) = module.scan(context, control, &mut report) {
            if control.check().is_err() {
                report.cancelled = true;
            } else {
                files::warn(&mut report.warnings, error);
            }
        }
        let mut seen = HashSet::new();
        report.findings.retain(|f| seen.insert(f.id.clone()));
        if report.findings.len() > 30_000 {
            report.findings.truncate(30_000);
            files::warn(
                &mut report.warnings,
                "Result limit reached; choose narrower folders".into(),
            );
        }
        Ok(report)
    }
}
struct FileModule(ModuleDescriptor);
impl ScanModule for FileModule {
    fn descriptor(&self) -> ModuleDescriptor {
        self.0.clone()
    }
    fn scan(&self, c: &ScanContext, k: &ScanControl, r: &mut ScanReport) -> Result<()> {
        match self.0.id.as_str() {
            "storage" => storage(c, k, r),
            "large" => large(c, k, r),
            "duplicates" => duplicates(c, k, r),
            "node" => node(c, k, r),
            "artifacts" => artifacts(c, k, r),
            "xcode" => xcode(c, k, r),
            "caches" => caches(c, k, r),
            "downloads" => simple_folder(
                "downloads",
                &(policy::home() + "/Downloads"),
                c,
                k,
                r,
                ActionKind::Trash,
            ),
            "trash" => simple_folder(
                "trash",
                &(policy::home() + "/.Trash"),
                c,
                k,
                r,
                ActionKind::EmptyTrash,
            ),
            _ => Err("Unknown file module".into()),
        }
    }
}
pub fn builtin() -> Registry {
    let descriptions = [
        (
            "storage",
            "Storage Explorer",
            "Storage",
            "internaldrive",
            "Explore folder sizes and find room to work.",
            true,
        ),
        (
            "large",
            "Large Files",
            "Storage",
            "doc",
            "Review files at least 100 MB, with size and date filters.",
            true,
        ),
        (
            "duplicates",
            "Exact Duplicates",
            "Storage",
            "doc.on.doc",
            "Verified matching files. Build and dependency folders are skipped.",
            true,
        ),
        (
            "node",
            "Node Dependencies",
            "Developer",
            "shippingbox",
            "Find node_modules by project without descending into dependencies.",
            true,
        ),
        (
            "artifacts",
            "Build Artifacts",
            "Developer",
            "hammer",
            "Generated outputs with evidence from their owning projects.",
            true,
        ),
        (
            "worktrees",
            "Git Worktrees",
            "Developer",
            "arrow.triangle.branch",
            "Branches, local changes, locks and orphaned registrations.",
            true,
        ),
        (
            "simulators",
            "Simulators",
            "Developer",
            "iphone",
            "Device app data and runtimes in your selected Xcode installation.",
            false,
        ),
        (
            "xcode",
            "Xcode Data",
            "Developer",
            "wrench.and.screwdriver",
            "Build and device-support data. Archives remain protected.",
            false,
        ),
        (
            "caches",
            "Caches & Logs",
            "Developer",
            "archivebox",
            "Known package caches and logs, with rebuild costs made clear.",
            false,
        ),
        (
            "applications",
            "Applications",
            "Applications",
            "app.badge",
            "Apps and precisely named related files, selected separately.",
            false,
        ),
        (
            "leftovers",
            "App Leftovers",
            "Applications",
            "app.dashed",
            "Possible leftovers with uncertain ownership made explicit.",
            false,
        ),
        (
            "downloads",
            "Downloads",
            "Storage",
            "arrow.down.circle",
            "Downloaded files and installers, ready for your review.",
            false,
        ),
        (
            "trash",
            "Trash",
            "Storage",
            "trash",
            "Review exact items before permanent removal.",
            false,
        ),
        (
            "orphans",
            "Orphan Processes",
            "Tools",
            "cpu",
            "Suspected unmanaged processes left behind by their parents.",
            false,
        ),
    ];
    let modules = descriptions
        .into_iter()
        .map(|(id, name, category, symbol, summary, roots)| {
            let descriptor = ModuleDescriptor::new(id, name, category, symbol, summary, roots);
            match id {
                "worktrees" => Arc::new(crate::git::GitModule(descriptor)) as Arc<dyn ScanModule>,
                "simulators" => Arc::new(crate::simulator::SimulatorModule(descriptor)),
                "orphans" => Arc::new(crate::process::OrphanModule(descriptor)),
                "applications" | "leftovers" => {
                    Arc::new(crate::applications::AppModule(descriptor))
                }
                _ => Arc::new(FileModule(descriptor)),
            }
        })
        .collect();
    Registry::new(modules).expect("Built-in IDs are unique")
}
fn parent(path: &str) -> String {
    Path::new(path)
        .parent()
        .map(|p| p.to_string_lossy().into())
        .unwrap_or_default()
}
fn name(path: &str) -> &str {
    Path::new(path)
        .file_name()
        .and_then(|p| p.to_str())
        .unwrap_or(path)
}
fn add_file(
    entry: &files::Entry,
    module: &str,
    title: Option<&str>,
    reason: &str,
    actions: Vec<ActionKind>,
    risk: Risk,
    k: &ScanControl,
    r: &mut ScanReport,
) {
    match files::finding(entry, module, title, reason, actions, risk, k) {
        Ok(f) => r.findings.push(f),
        Err(e) => files::warn(
            &mut r.warnings,
            format!("Cannot size {}: {e}", entry.path()),
        ),
    }
}
fn storage(c: &ScanContext, k: &ScanControl, r: &mut ScanReport) -> Result<()> {
    for root in files::roots(&c.roots) {
        if c.excludes(&root) || policy::system_excluded(&root) {
            continue;
        }
        for e in files::children(&root, &mut r.warnings) {
            k.check()?;
            if !c.allows(e.path()) || policy::system_excluded(e.path()) {
                continue;
            }
            add_file(
                &e,
                "storage",
                None,
                "Storage inventory. Inspect this item in Finder.",
                vec![],
                Risk::Review,
                k,
                r,
            );
        }
    }
    Ok(())
}
fn large(c: &ScanContext, k: &ScanControl, r: &mut ScanReport) -> Result<()> {
    files::walk(c, k, &mut r.warnings, |e| {
        if e.regular && e.bytes >= 100_000_000 {
            let mut f = Finding::new(
                "large",
                e.path(),
                e.name(),
                Resource::File {
                    file: e.identity.clone(),
                },
                "Size alone does not make a personal file disposable. Inspect its contents.",
            );
            f.bytes = Some(e.bytes);
            f.allocated_bytes = Some(e.allocated);
            f.modified_at = Some(e.modified());
            if !policy::protected(e.path()) {
                f.actions.push(ActionKind::Trash);
            }
            r.findings.push(f);
        }
        Ok(true)
    })
}
fn node(c: &ScanContext, k: &ScanControl, r: &mut ScanReport) -> Result<()> {
    let mut candidates = vec![];
    let mut context = c.clone();
    context.roots.retain(|root| {
        if name(root) == "node_modules" {
            if c.allows(root) && !policy::system_excluded(root) {
                if let Ok(e) = files::entry(root) {
                    if e.directory {
                        candidates.push(e);
                    }
                }
            }
            false
        } else {
            true
        }
    });
    files::walk(&context, k, &mut r.warnings, |e| {
        if e.directory && e.name() == "node_modules" {
            candidates.push(e.clone());
            return Ok(false);
        }
        Ok(true)
    })?;
    for e in candidates {
        k.check()?;
        let project = parent(e.path());
        let manifest = Path::new(&(project.clone() + "/package.json")).is_file();
        let manager = [
            ("pnpm", "pnpm-lock.yaml"),
            ("Yarn", "yarn.lock"),
            ("Bun", "bun.lock"),
            ("Bun", "bun.lockb"),
            ("npm", "package-lock.json"),
        ]
        .into_iter()
        .find(|(_, lock)| Path::new(&(project.clone() + "/" + lock)).is_file())
        .map(|(n, _)| n)
        .unwrap_or("Unknown");
        let mut f = files::finding(&e, "node", Some(name(&project)), "Dependencies can be reinstalled. Local patches or edits may be lost; manifests and lockfiles are preserved.", if manifest { vec![ActionKind::Trash] } else { vec![] }, Risk::Rebuild, k)?;
        f.details.extend([
            detail("Project", project),
            detail("Package manager", manager),
            detail("Artifact", "node_modules"),
        ]);
        if !manifest {
            f.blocked_reason = Some("No owning package.json; inspect manually".into());
        }
        r.findings.push(f);
    }
    Ok(())
}
pub fn artifact_rule(name: &str, parent: &str) -> Option<&'static str> {
    let exists = |file: &str| Path::new(&(parent.to_owned() + "/" + file)).is_file();
    if [".next", ".nuxt", ".turbo", ".parcel-cache", "coverage"].contains(&name)
        && exists("package.json")
    {
        return Some("JavaScript generated output");
    }
    if name == ".build" && exists("Package.swift") {
        return Some("SwiftPM build output");
    }
    if name == "target" && exists("Cargo.toml") {
        return Some("Cargo build output");
    }
    if name == "build" && (exists("build.gradle") || exists("build.gradle.kts")) {
        return Some("Gradle build output");
    }
    if ["__pycache__", ".venv", "venv"].contains(&name)
        && (exists("pyproject.toml") || exists("requirements.txt") || exists("setup.py"))
    {
        return Some("Python generated environment or bytecode");
    }
    None
}
fn artifacts(c: &ScanContext, k: &ScanControl, r: &mut ScanReport) -> Result<()> {
    let mut candidates = vec![];
    files::walk(c, k, &mut r.warnings, |e| {
        if e.name() == "node_modules" {
            return Ok(false);
        }
        if e.directory {
            if let Some(rule) = artifact_rule(e.name(), &parent(e.path())) {
                candidates.push((e.clone(), rule));
                return Ok(false);
            }
        }
        Ok(true)
    })?;
    for (e, rule) in candidates {
        k.check()?;
        add_file(
            &e,
            "artifacts",
            None,
            &format!("{rule}. Rebuilding requires the project's tools and dependencies."),
            vec![ActionKind::Trash],
            Risk::Rebuild,
            k,
            r,
        );
    }
    Ok(())
}
fn duplicates(c: &ScanContext, k: &ScanControl, r: &mut ScanReport) -> Result<()> {
    let mut sizes: HashMap<u64, Vec<files::Entry>> = HashMap::new();
    let mut context = c.clone();
    context.roots.retain(|r| !policy::duplicate_excluded(r));
    files::walk(&context, k, &mut r.warnings, |e| {
        if e.directory && policy::duplicate_excluded(e.path()) {
            return Ok(false);
        }
        if e.regular && e.bytes >= 4_096 {
            sizes.entry(e.bytes).or_default().push(e.clone());
        }
        Ok(true)
    })?;
    for entries in sizes.values().filter(|v| v.len() > 1) {
        let mut hashes: HashMap<String, Vec<files::Entry>> = HashMap::new();
        let mut inodes = HashSet::new();
        for e in entries {
            k.check()?;
            if !inodes.insert((e.identity.device, e.identity.inode)) {
                continue;
            }
            match files::hash(e.path(), k) {
                Ok(hash) => hashes.entry(hash).or_default().push(e.clone()),
                Err(e) => files::warn(&mut r.warnings, e),
            }
        }
        for (hash, group) in hashes.iter_mut().filter(|(_, v)| v.len() > 1) {
            group.sort_by(|a, b| a.path().cmp(b.path()));
            let original = group[0].path().to_owned();
            let count = group.len();
            for e in group {
                k.check()?;
                let preserved = e.path() == original;
                let mut f = Finding::new(
                    "duplicates",
                    e.path(),
                    e.name(),
                    Resource::File {
                        file: files::entry(e.path())?.identity,
                    },
                    if preserved {
                        "One verified original is preserved in this group."
                    } else {
                        "Content matches the preserved original. Both files are verified again before removal."
                    },
                );
                f.bytes = Some(e.bytes);
                f.allocated_bytes = Some(e.allocated);
                f.modified_at = Some(e.modified());
                f.details = vec![
                    detail("Preserved original", original.clone()),
                    detail("Group files", count.to_string()),
                    detail("SHA256", hash.clone()),
                ];
                if preserved {
                    f.blocked_reason = Some("Original is preserved".into());
                    f.badge = Some("Original".into());
                } else {
                    f.actions.push(ActionKind::Trash);
                    f.badge = Some("Duplicate".into());
                }
                r.findings.push(f);
            }
        }
    }
    Ok(())
}
pub fn xcode_roots() -> Vec<String> {
    ["DerivedData", "iOS DeviceSupport", "Archives"]
        .iter()
        .map(|s| format!("{}/Library/Developer/Xcode/{s}", policy::home()))
        .collect()
}
pub fn cache_locations() -> Vec<(String, String)> {
    [
        ("npm", "/.npm/_cacache"),
        ("Yarn", "/Library/Caches/Yarn"),
        ("pnpm", "/Library/pnpm/store"),
        ("pnpm cache", "/Library/Caches/pnpm"),
        ("Bun", "/.bun/install/cache"),
        ("Homebrew", "/Library/Caches/Homebrew"),
        ("pip", "/Library/Caches/pip"),
        ("uv", "/.cache/uv"),
        ("Poetry", "/Library/Caches/pypoetry"),
        ("SwiftPM", "/Library/Caches/org.swift.swiftpm"),
        ("CocoaPods", "/Library/Caches/CocoaPods"),
        ("Cargo cache", "/.cargo/registry/cache"),
        ("Go build cache", "/Library/Caches/go-build"),
        ("Gradle caches", "/.gradle/caches"),
    ]
    .iter()
    .map(|(n, p)| (n.to_string(), policy::home() + p))
    .collect()
}
pub fn known_roots(module: &str) -> Vec<String> {
    let h = policy::home();
    match module {
        "xcode" => xcode_roots(),
        "caches" => cache_locations()
            .into_iter()
            .map(|(_, p)| p)
            .chain([h.clone() + "/Library/Logs"])
            .collect(),
        "applications" | "leftovers" => [
            "/Applications".into(),
            h.clone() + "/Applications",
            h.clone() + "/Library/Caches",
            h.clone() + "/Library/Preferences",
            h.clone() + "/Library/Saved Application State",
            h + "/Library/Application Support",
        ]
        .into(),
        "downloads" => vec![h + "/Downloads"],
        "trash" => vec![h + "/.Trash"],
        _ => vec![],
    }
}
pub fn project_roots() -> Vec<String> {
    files::roots(
        &["projects", "Projects", "Code", "dev", "GitHub", "Workspace"]
            .iter()
            .map(|p| policy::home() + "/" + p)
            .filter(|p| Path::new(p).is_dir())
            .collect::<Vec<_>>(),
    )
}
fn xcode(c: &ScanContext, k: &ScanControl, r: &mut ScanReport) -> Result<()> {
    for root in xcode_roots().into_iter().filter(|p| Path::new(p).is_dir()) {
        if c.excludes(&root) {
            continue;
        }
        let archive = root.ends_with("/Archives");
        for e in files::children(&root, &mut r.warnings) {
            k.check()?;
            if !c.allows(e.path()) {
                continue;
            }
            add_file(
                &e,
                "xcode",
                None,
                if archive {
                    "Archives may contain irreplaceable release builds and dSYMs."
                } else {
                    "Xcode regenerates this build or device-support data when needed."
                },
                if archive {
                    vec![]
                } else {
                    vec![ActionKind::Trash]
                },
                if archive { Risk::Review } else { Risk::Rebuild },
                k,
                r,
            );
        }
    }
    Ok(())
}
fn caches(c: &ScanContext, k: &ScanControl, r: &mut ScanReport) -> Result<()> {
    for (title, path) in cache_locations() {
        k.check()?;
        if !Path::new(&path).exists() || !c.allows(&path) {
            continue;
        }
        match files::entry(&path) { Ok(e) => add_file(&e, "caches", Some(&title), "Shared package cache. Clearing it can require downloads and affect offline builds.", vec![ActionKind::Trash], Risk::Rebuild, k, r), Err(e) => files::warn(&mut r.warnings,e) }
    }
    let logs = policy::home() + "/Library/Logs";
    if Path::new(&logs).is_dir() && !c.excludes(&logs) {
        for e in files::children(&logs, &mut r.warnings) {
            k.check()?;
            if c.allows(e.path()) {
                add_file(
                    &e,
                    "caches",
                    Some(&format!("Logs · {}", e.name())),
                    "Logs can help diagnose problems. Inspect before removal.",
                    vec![ActionKind::Trash],
                    Risk::Review,
                    k,
                    r,
                );
            }
        }
    }
    Ok(())
}
fn simple_folder(
    module: &str,
    folder: &str,
    c: &ScanContext,
    k: &ScanControl,
    r: &mut ScanReport,
    action: ActionKind,
) -> Result<()> {
    if c.excludes(folder) {
        return Ok(());
    }
    for e in files::children(folder, &mut r.warnings) {
        k.check()?;
        if !c.allows(e.path()) {
            continue;
        }
        add_file(
            &e,
            module,
            None,
            if module == "trash" {
                "Already in Trash. Permanent removal cannot be undone."
            } else {
                "Downloaded file or folder. Review whether it is still needed."
            },
            vec![action],
            if module == "trash" {
                Risk::Permanent
            } else {
                Risk::Review
            },
            k,
            r,
        );
    }
    Ok(())
}
