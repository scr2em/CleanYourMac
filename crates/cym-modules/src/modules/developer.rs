use super::project_folders::{self, package_tree};
pub(crate) use super::project_folders::{app_home_folder, in_app_home_folder};
use super::{
    add_files, descriptor, flush, glob, owners, owners_closed, Candidate, LastUsed, ScanModule,
};
use crate::{git::Git, model::*, orphans, policy, ports::*, services::Services};
use std::path::Path;

pub use super::project_folders::{
    build_output_rules, dependency_rules, match_rules, ArtifactMatch, ArtifactRule,
};

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
fn active_project_tools(s: &Services, project: &str) -> Option<String> {
    let active = orphans::active_tools(s, Some(project));
    (!active.is_empty()).then(|| {
        format!(
            "Tools are running in this project:{}",
            orphans::list(&active)
        )
    })
}
/// The first shipped-build output inside a build folder: an Xcode archive, debug symbols,
/// an app package for a store or for sideloading, or an Android shrinker mapping. Searched
/// breadth-first, eight levels deep, so a huge build folder cannot stall the scan; one with
/// more entries than the search reads gives `Err`, since what it did not see may be one.
pub(crate) fn shipped_outputs(
    s: &Services,
    folder: &str,
) -> std::result::Result<Option<String>, ()> {
    let mut queue = std::collections::VecDeque::from([(folder.to_owned(), 0)]);
    let mut seen = 0;
    while let Some((dir, depth)) = queue.pop_front() {
        for e in s.children(&dir, &mut vec![]) {
            seen += 1;
            if seen > 500_000 {
                return Err(());
            }
            let name = e.name().to_ascii_lowercase();
            if [".xcarchive", ".dsym", ".ipa", ".aab", ".apk"]
                .iter()
                .any(|x| name.ends_with(x))
                || (name == "mapping" && dir.ends_with("/outputs"))
            {
                return Ok(Some(e.name().to_owned()));
            }
            // Shipped outputs sit near the top (`outputs/bundle/release/app.aab`,
            // `Build/Products/Release/App.app.dSYM`); deeper levels are packages' own files.
            if e.directory && !e.symlink && depth < 8 {
                queue.push_back((e.path().to_owned(), depth + 1));
            }
        }
    }
    Ok(None)
}

const APP_DATA: &str = "Inside an app's or tool's own folder, such as an editor extension; removing it would break that app or extension.";

/// Refuses to move an item that Git tracks inside its repository. Inside a checkout of any
/// other version-control system it refuses too: only Git is asked, because another tool
/// could run commands the repository configures, so whether it tracks the item is unknown.
pub(crate) fn untracked(s: &Services, path: &str, k: &ScanControl) -> Result<()> {
    let directory = parent(path);
    let mut ancestor = Some(Path::new(&directory));
    while let Some(folder) = ancestor {
        match super::checkout(s, &folder.to_string_lossy()) {
            Some("Git") => {
                if Git(s.commands.as_ref()).tracks(&directory, path, k)? {
                    return Err("Git tracks this item in its repository.".into());
                }
                return Ok(());
            }
            Some(system) => {
                return Err(format!(
                    "This item is in a {system} checkout. CleanYourMac checks only Git, so it cannot confirm {system} does not track it."
                ))
            }
            None => {}
        }
        ancestor = folder.parent();
    }
    Ok(())
}

/// Build output, identified by folder name and owning-project evidence (see `project`).
/// Traversal never enters a matched folder, a package tree or installed dependencies, which
/// belong to the Dependencies module.
#[derive(Default)]
pub struct ArtifactsModule {
    /// The home folder whose app and tool folders are never searched; `None` uses the
    /// current user's.
    pub home: Option<String>,
}
impl ArtifactsModule {
    fn home(&self) -> String {
        self.home.clone().unwrap_or_else(policy::home)
    }
    /// The first rule whose names and evidence match the folder at `path`.
    pub fn matches(&self, s: &Services, path: &str) -> Option<ArtifactMatch<'static>> {
        project_folders::rules().build_output(s, path)
    }
}
impl ScanModule for ArtifactsModule {
    fn descriptor(&self) -> ModuleDescriptor {
        descriptor(
            "artifacts",
            "Build Artifacts",
            "Developer",
            "hammer",
            "Generated outputs with evidence from their owning projects.",
            true,
        )
    }
    fn scan(
        &self,
        s: &Services,
        c: &ScanContext,
        k: &ScanControl,
        sink: &mut dyn Sink,
    ) -> Result<()> {
        let mut candidates = vec![];
        let mut warnings = vec![];
        let home = self.home();
        let walked = s.walk(c, k, &mut warnings, &mut |e| {
            if package_tree(e.name()) || (e.directory && app_home_folder(e.path(), &home)) {
                return Ok(false);
            }
            if e.directory {
                if project_folders::rules().dependency(s, e.path()).is_some() {
                    return Ok(false);
                }
                if let Some(m) = self.matches(s, e.path()) {
                    let rule = m.rule;
                    let mut reason = format!("{} {}. {}", rule.ecosystem, rule.kind, rule.rebuild);
                    let project = m.project.clone();
                    let mut details = vec![
                        detail("Ecosystem", rule.ecosystem),
                        detail("Project", m.project),
                        detail("Evidence", m.evidence),
                    ];
                    if let Some(command) = rule.command {
                        reason += &format!(" Official command: `{command}`.");
                        details.push(detail("Official command", command));
                    }
                    let candidate = match shipped_outputs(s, e.path()) {
                        // A release archive, crash symbols or a shrinker mapping cannot be
                        // rebuilt for a build already shipped.
                        Ok(Some(found)) => Candidate::new(
                            e.clone(),
                            &format!("{reason} It holds {found}, which a rebuild cannot recreate for a build you already shipped."),
                            vec![],
                            Risk::Review,
                        )
                        .acknowledge(
                            &format!("This build output holds {found}: a shipped build or the symbols needed to read its crash reports."),
                            vec![ActionKind::Trash],
                        ),
                        // Too large or deep to check fully: what was not seen may be one.
                        Err(()) => Candidate::new(
                            e.clone(),
                            &format!("{reason} It is too large to check fully for a shipped build (an archive, debug symbols or an app package)."),
                            vec![],
                            Risk::Review,
                        )
                        .acknowledge(
                            "This build output is too large to check for a shipped build or the symbols needed to read its crash reports.",
                            vec![ActionKind::Trash],
                        ),
                        Ok(None) => Candidate::new(e.clone(), &reason, vec![ActionKind::Trash], rule.risk),
                    };
                    candidates.push(
                        candidate
                            .title(m.relative)
                            .details(details)
                            .last_used(LastUsed::Project { folder: project })
                            .blocked(in_app_home_folder(e.path(), &home).then_some(APP_DATA)),
                    );
                    return Ok(false);
                }
            }
            Ok(true)
        });
        flush(sink, &mut warnings);
        walked?;
        add_files(s, sink, "artifacts", candidates, k)
    }
    fn in_use(&self, s: &Services, f: &Finding, _: ActionKind) -> Option<String> {
        let m = self.matches(s, f.resource.path()?)?;
        active_project_tools(s, &m.project)
    }
    fn preflight(&self, s: &Services, f: &Finding, _: ActionKind, k: &ScanControl) -> Result<()> {
        let path = f.resource.path().ok_or("Missing path")?;
        if in_app_home_folder(path, &self.home()) {
            return Err(APP_DATA.into());
        }
        let m = self
            .matches(s, path)
            .ok_or("The owning project's evidence is no longer available.")?;
        owners_closed(s, m.rule.apps)?;
        untracked(s, path, k)
    }
}

/// Installed dependencies by project: `node_modules`, Python virtual environments, CocoaPods,
/// SwiftPM checkouts, Carthage, Bundler, Composer, Go and Cargo vendor folders, Elixir deps
/// and more, found without descending into them. Ecosystems that keep packages outside the
/// project (Cargo, Go, Maven, Gradle, Pub and others) are covered by their shared package
/// stores, which Caches & Logs also lists.
pub struct DependenciesModule {
    /// Shared package stores in the home folder, one finding each.
    pub stores: Vec<CacheLocation>,
    /// Package manager name and the lockfile that identifies it, in priority order.
    pub lockfiles: Vec<(&'static str, &'static str)>,
    /// Package managers' own stores, caches and global installs, relative to the home
    /// folder. Their `node_modules` belong to the tool, not a project, so they are not
    /// searched; Caches & Logs and Toolchains & SDKs cover them.
    pub tool_folders: Vec<&'static str>,
    /// The home folder `tool_folders` and `stores` are relative to; the user's when `None`.
    pub home: Option<String>,
}
impl Default for DependenciesModule {
    fn default() -> Self {
        Self {
            stores: CachesModule::default()
                .locations
                .into_iter()
                .filter(|l| l.package_store)
                .collect(),
            tool_folders: vec![
                ".npm",
                ".pnpm-store",
                ".bun",
                ".yarn",
                ".nvm",
                ".fnm",
                ".volta",
                ".asdf",
                ".cache/pnpm",
                ".cache/yarn",
                ".cache/node",
                ".local/share/pnpm",
                ".local/share/fnm",
                ".local/share/mise",
                "Library/pnpm",
                "Library/Caches/pnpm",
                "Library/Caches/Yarn",
                "Library/Caches/node-gyp",
                "Library/Application Support/fnm",
            ],
            home: None,
            lockfiles: vec![
                ("pnpm", "pnpm-lock.yaml"),
                ("Yarn", "yarn.lock"),
                ("Bun", "bun.lock"),
                ("Bun", "bun.lockb"),
                ("npm", "package-lock.json"),
                ("Deno", "deno.lock"),
            ],
        }
    }
}
impl DependenciesModule {
    /// The rule matching a dependency folder other than `node_modules`.
    pub fn matches(&self, s: &Services, path: &str) -> Option<ArtifactMatch<'static>> {
        project_folders::rules().dependency(s, path)
    }
    fn home(&self) -> String {
        self.home.clone().unwrap_or_else(policy::home)
    }
    fn node_modules(&self, s: &Services, e: Entry, runtime: &'static str) -> Candidate {
        let project = parent(e.path());
        let manager = self
            .lockfiles
            .iter()
            .find(|(_, lock)| s.is_file(&format!("{project}/{lock}")))
            .map(|(n, _)| *n)
            .unwrap_or("Unknown");
        Candidate::new(
            e,
            "Project dependencies can be reinstalled, but local patches or edits may be lost. Manifests and lockfiles are preserved.",
            vec![ActionKind::Trash],
            Risk::Rebuild,
        )
        .title(name(&project))
        .last_used(LastUsed::Project {
            folder: project.clone(),
        })
        .details(vec![
            detail("Project", project),
            detail("Ecosystem", runtime),
            detail("Package manager", manager),
            detail("Artifact", "node_modules"),
        ])
    }
    fn matched(e: Entry, m: ArtifactMatch<'_>) -> Candidate {
        let rule = m.rule;
        let mut reason = format!("{} {}. {}", rule.ecosystem, rule.kind, rule.rebuild);
        let mut details = vec![
            detail("Project", m.project.clone()),
            detail("Ecosystem", rule.ecosystem),
            detail("Evidence", m.evidence),
            detail("Artifact", m.relative),
        ];
        if let Some(command) = rule.command {
            reason += &format!(" Official command: `{command}`.");
            details.push(detail("Official command", command));
        }
        Candidate::new(e, &reason, vec![ActionKind::Trash], rule.risk)
            .title(name(&m.project))
            .details(details)
            .last_used(LastUsed::Project { folder: m.project })
    }
    /// One finding per existing shared package store.
    fn stores(&self, s: &Services, c: &ScanContext) -> Vec<Candidate> {
        let home = self.home();
        let mut out = vec![];
        for store in &self.stores {
            let mut reason = format!(
                "Shared by every {} project on this Mac; projects download what they need again. {}",
                store.ecosystem,
                store.note.unwrap_or("Offline installs need a connection afterwards.")
            );
            let mut details = vec![
                detail("Ecosystem", store.ecosystem),
                detail("Scope", "Shared by all projects"),
            ];
            if let Some(command) = store.command {
                reason += &format!(" Official command: `{command}`.");
                details.push(detail("Official command", command));
            }
            for (e, title) in expand(s, &home, store) {
                if c.allows(e.path()) {
                    out.push(
                        Candidate::new(e, &reason, vec![ActionKind::Trash], store.risk)
                            .title(title)
                            .details(details.clone())
                            .blocked(store.blocked),
                    );
                }
            }
        }
        out
    }
    fn store(&self, path: &str) -> Option<&CacheLocation> {
        find_location(&self.home(), &self.stores, path)
    }
}
impl ScanModule for DependenciesModule {
    fn descriptor(&self) -> ModuleDescriptor {
        // The ID predates other ecosystems and is kept so saved settings and history still match.
        descriptor(
            "node",
            "Dependencies",
            "Developer",
            "shippingbox",
            "Installed packages by project, such as node_modules, Python environments, Pods and vendor folders, plus the shared package stores projects install from.",
            true,
        )
    }
    fn action_roots(&self) -> Vec<String> {
        let home = self.home();
        self.stores
            .iter()
            .map(|l| format!("{home}/{}", l.root()))
            .collect()
    }
    fn scan(
        &self,
        s: &Services,
        c: &ScanContext,
        k: &ScanControl,
        sink: &mut dyn Sink,
    ) -> Result<()> {
        let mut candidates = vec![];
        let mut context = c.clone();
        // A chosen root that is itself a node_modules folder is a candidate, not a place to search.
        context.roots.retain(|root| {
            if name(root) != "node_modules" {
                return true;
            }
            if c.allows(root) && !policy::system_excluded(root) {
                if let Some(e) = s.entry(root).ok().filter(|e| e.directory) {
                    candidates.push(e);
                }
            }
            false
        });
        let home = self.home();
        // Package managers' folders and shared stores are never searched: what they hold
        // belongs to the tool, not a project.
        let skipped: Vec<String> = self
            .tool_folders
            .iter()
            .copied()
            .chain(self.stores.iter().map(|l| l.root()))
            .map(|f| format!("{home}/{f}"))
            .collect();
        let mut matched = vec![];
        let mut warnings = vec![];
        let walked = s.walk(&context, k, &mut warnings, &mut |e| {
            if !e.directory {
                return Ok(true);
            }
            if e.name() == "node_modules" {
                candidates.push(e.clone());
                return Ok(false);
            }
            // pnpm's virtual store, npm's cache and npx folders hold node_modules that no
            // project owns, and neither do apps' and tools' own folders.
            if package_tree(e.name())
                || skipped.iter().any(|f| f == e.path())
                || app_home_folder(e.path(), &home)
            {
                return Ok(false);
            }
            if let Some(m) = self.matches(s, e.path()) {
                matched.push((e.clone(), m.rule.hide_tracked));
                return Ok(false);
            }
            // Build output (`.next`, `target`, …) belongs to Build Artifacts; packages it holds,
            // such as `.next/standalone/node_modules`, are part of the build.
            if project_folders::rules().build_output(s, e.path()).is_some() {
                return Ok(false);
            }
            Ok(true)
        });
        flush(sink, &mut warnings);
        walked?;
        // A node_modules with no package.json or deno.json beside it has no project to
        // rebuild it from; it is left out rather than listed as an item nobody can act on.
        let mut found: Vec<Candidate> = candidates
            .into_iter()
            .filter_map(|e| {
                let runtime = project_folders::rules().node_runtime(s, &parent(e.path()))?;
                Some(self.node_modules(s, e, runtime))
            })
            .collect();
        for (e, hide_tracked) in matched {
            // Committed vendor folders are part of the project's source.
            if hide_tracked && untracked(s, e.path(), k).is_err() {
                continue;
            }
            if let Some(m) = self.matches(s, e.path()) {
                found.push(Self::matched(e, m));
            }
        }
        let found: Vec<Candidate> = found
            .into_iter()
            .map(|candidate| {
                let blocked = in_app_home_folder(candidate.entry.path(), &home);
                candidate.blocked(blocked.then_some(APP_DATA))
            })
            .chain(self.stores(s, c))
            .collect();
        add_files(s, sink, "node", found, k)
    }
    fn in_use(&self, s: &Services, f: &Finding, _: ActionKind) -> Option<String> {
        let path = f.resource.path()?;
        if self.store(path).is_some() {
            let active = orphans::active_tools(s, None);
            return (!active.is_empty()).then(|| {
                format!(
                    "Developer tools are running that may be installing from this store:{}",
                    orphans::list(&active)
                )
            });
        }
        match self.matches(s, path) {
            Some(m) => active_project_tools(s, &m.project),
            None => active_project_tools(s, &parent(path)),
        }
    }
    fn preflight(&self, s: &Services, f: &Finding, _: ActionKind, k: &ScanControl) -> Result<()> {
        let path = f.resource.path().ok_or("Missing path")?;
        if let Some(store) = self.store(path) {
            if let Some(reason) = store.blocked {
                return Err(reason.into());
            }
            return owners_closed(s, store.apps);
        }
        if in_app_home_folder(path, &self.home()) {
            return Err(APP_DATA.into());
        }
        if name(path) == "node_modules" {
            if project_folders::rules()
                .node_runtime(s, &parent(path))
                .is_none()
            {
                return Err("The owning package.json or deno.json is no longer available.".into());
            }
        } else {
            let m = self
                .matches(s, path)
                .ok_or("The owning project's evidence is no longer available.")?;
            owners_closed(s, m.rule.apps)?;
        }
        untracked(s, path, k)
    }
}

/// Xcode records when it last opened each DerivedData folder in its `info.plist`.
/// A known per-user cache.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CacheLocation {
    /// Shown as the finding's "Ecosystem" detail.
    pub ecosystem: &'static str,
    pub name: &'static str,
    /// Relative to the home folder. A `*` in the last component lists one finding per match.
    pub path: &'static str,
    /// What removing it costs, when more than a re-download.
    pub note: Option<&'static str>,
    /// The tool's own clean command, preferred where it exists.
    pub command: Option<&'static str>,
    /// Bundle-identifier prefixes of apps that must be closed first.
    pub apps: &'static [&'static str],
    /// Shown instead of offering Trash, when Trash cannot work for this cache.
    pub blocked: Option<&'static str>,
    pub risk: Risk,
    /// Packages that every project of the ecosystem installs from, also listed by the
    /// Dependencies module.
    pub package_store: bool,
    /// Only its app uses it, so running command-line developer tools do not hold it.
    pub app_cache: bool,
}
fn cache(ecosystem: &'static str, name: &'static str, path: &'static str) -> CacheLocation {
    CacheLocation {
        ecosystem,
        name,
        path,
        note: None,
        command: None,
        apps: &[],
        blocked: None,
        risk: Risk::Rebuild,
        package_store: false,
        app_cache: false,
    }
}
impl CacheLocation {
    pub fn note(mut self, note: &'static str) -> Self {
        self.note = Some(note);
        self
    }
    pub fn command(mut self, command: &'static str) -> Self {
        self.command = Some(command);
        self
    }
    pub fn apps(mut self, apps: &'static [&'static str]) -> Self {
        self.apps = apps;
        self
    }
    pub fn blocked(mut self, reason: &'static str) -> Self {
        self.blocked = Some(reason);
        self
    }
    pub fn risk(mut self, risk: Risk) -> Self {
        self.risk = risk;
        self
    }
    pub fn package_store(mut self) -> Self {
        self.package_store = true;
        self
    }
    pub fn app_cache(mut self) -> Self {
        self.app_cache = true;
        self
    }
    /// The home-relative folder that holds this location's matches: the components before
    /// the first `*`.
    fn root(&self) -> &'static str {
        let path: &'static str = self.path;
        match path.find('*') {
            Some(star) => path[..star]
                .rsplit_once('/')
                .map_or("", |(parent, _)| parent),
            None => path,
        }
    }
}

pub struct CachesModule {
    /// The home folder the locations are relative to; `None` uses the current user's.
    pub home: Option<String>,
    pub locations: Vec<CacheLocation>,
    /// The shared command-line cache folder (`~/.cache`), listed as one item.
    pub tool_cache: Option<&'static str>,
    /// macOS leftovers: app caches, saved window state, Mail downloads, device updates and
    /// backups.
    pub mac: super::junk::MacJunk,
}
impl Default for CachesModule {
    fn default() -> Self {
        const HARD_LINKS: &str =
            "Environments hard-link these packages, so less space may be freed than shown.";
        Self {
            home: None,
            tool_cache: Some(".cache"),
            mac: Default::default(),
            locations: vec![
                // JavaScript and TypeScript
                cache("npm", "npm cache", ".npm/_cacache").package_store()
                    .command("npm cache clean --force"),
                cache("Yarn", "Yarn cache", "Library/Caches/Yarn").package_store()
                    .command("yarn cache clean"),
                cache("Yarn", "Yarn Berry cache", ".yarn/berry/cache").package_store()
                    .command("yarn cache clean --mirror"),
                cache("pnpm", "pnpm store", "Library/pnpm/store").package_store()
                    .note("Projects hard-link files from this store and need pnpm install again; pruning keeps what projects still use.")
                    .command("pnpm store prune"),
                cache("pnpm", "pnpm cache", "Library/Caches/pnpm"),
                cache("Bun", "Bun cache", ".bun/install/cache").package_store().command("bun pm cache rm"),
                cache("Deno", "Deno cache", "Library/Caches/deno").package_store().command("deno clean"),
                cache("JavaScript", "node-gyp headers", "Library/Caches/node-gyp"),
                cache("JavaScript", "Electron downloads", "Library/Caches/electron"),
                cache(
                    "JavaScript",
                    "electron-builder cache",
                    "Library/Caches/electron-builder",
                ),
                // Large downloads are Review, so they never join the one-click cache fix.
                cache("JavaScript", "Playwright browsers", "Library/Caches/ms-playwright")
                    .note("Browsers are downloaded again by playwright install, often several hundred MB each.")
                    .risk(Risk::Review),
                cache("JavaScript", "Cypress binaries", "Library/Caches/Cypress")
                    .note("Cypress downloads its app again, several hundred MB, the next time a project runs it.")
                    .command("cypress cache clear")
                    .risk(Risk::Review),
                cache("JavaScript", "Puppeteer browsers", ".cache/puppeteer")
                    .note("Puppeteer downloads its browser again, several hundred MB, when a project installs it.")
                    .risk(Risk::Review),
                // Python and machine learning
                cache("Python", "pip cache", "Library/Caches/pip").package_store().command("pip cache purge"),
                cache("Python", "Poetry cache", "Library/Caches/pypoetry/cache").package_store(),
                cache("Python", "Poetry artifacts", "Library/Caches/pypoetry/artifacts").package_store(),
                cache("Python", "Miniconda packages", "miniconda3/pkgs").package_store()
                    .note(HARD_LINKS)
                    .command("conda clean --all"),
                cache("Python", "Anaconda packages", "anaconda3/pkgs").package_store()
                    .note(HARD_LINKS)
                    .command("conda clean --all"),
                cache("Python", "Miniforge packages", "miniforge3/pkgs").package_store()
                    .note(HARD_LINKS)
                    .command("conda clean --all"),
                cache("Python", "uv cache", ".cache/uv").package_store()
                    .note("Environments link these packages, so less space may be freed than shown. Environments made with uv's symlink link mode stop working.")
                    .command("uv cache prune")
                    .risk(Risk::Review),
                cache("Python", "pre-commit environments", ".cache/pre-commit")
                    .note("pre-commit installs its hook environments again on the next commit.")
                    .command("pre-commit clean"),
                // Apple platforms
                // Apple's caches are left to review, as everywhere else.
                cache("Xcode", "Xcode cache", "Library/Caches/com.apple.dt.Xcode")
                    .apps(owners::XCODE)
                    .risk(Risk::Review),
                cache("SwiftPM", "SwiftPM cache", "Library/Caches/org.swift.swiftpm").package_store()
                    .command("swift package purge-cache")
                    .apps(owners::XCODE),
                cache("CocoaPods", "CocoaPods cache", "Library/Caches/CocoaPods").package_store()
                    .command("pod cache clean --all")
                    .apps(owners::XCODE),
                cache(
                    "Carthage",
                    "Carthage cache",
                    "Library/Caches/org.carthage.CarthageKit",
                )
                .package_store()
                .apps(owners::XCODE),
                cache("CocoaPods", "CocoaPods spec repo", ".cocoapods/repos/*")
                    .note("A Git copy of a Podspec repository. Projects that use the CDN (the default since CocoaPods 1.8) do not need the old master repo; a private repo comes back only with pod repo add and its URL.")
                    .command("pod repo remove <name>")
                    .apps(owners::XCODE)
                    .risk(Risk::Review),
                // Flutter and Dart: global tools in bin and global_packages are kept.
                cache("Flutter / Dart", "Pub hosted packages", ".pub-cache/hosted").package_store()
                    .command("dart pub cache clean"),
                cache("Flutter / Dart", "Pub Git packages", ".pub-cache/git").package_store()
                    .command("dart pub cache clean"),
                // JVM and Android: settings such as gradle.properties and settings.xml are kept.
                cache("Gradle", "Gradle caches", ".gradle/caches").package_store().apps(owners::ANDROID),
                cache("Gradle", "Gradle wrapper distributions", ".gradle/wrapper/dists")
                    .apps(owners::ANDROID),
                // Logs are never made again, so they are left to review.
                cache("Gradle", "Gradle daemon logs", ".gradle/daemon")
                    .note("Logs of past Gradle daemons; they can help diagnose a failed build.")
                    .apps(owners::ANDROID)
                    .risk(Risk::Review),
                cache("Kotlin", "Kotlin/Native toolchains", ".konan")
                    .note("Kotlin/Native downloads its toolchains again, about 1 GB, on the next build.")
                    .apps(owners::ANDROID)
                    .risk(Risk::Review),
                cache("Gradle", "Gradle JDKs", ".gradle/jdks")
                    .note("Java runtimes Gradle downloaded for toolchains; it downloads them again, several hundred MB each, when a build needs one.")
                    .apps(owners::ANDROID)
                    .risk(Risk::Review),
                cache("Maven", "Maven repository", ".m2/repository").package_store()
                    .note("Artifacts installed locally with mvn install exist nowhere else.")
                    .apps(owners::ANDROID)
                    .risk(Risk::Review),
                cache("Ivy", "Ivy cache", ".ivy2/cache").package_store(),
                cache("Scala", "Coursier cache", "Library/Caches/Coursier/v1").package_store(),
                cache("Android", "Android Studio caches", "Library/Caches/Google/AndroidStudio*")
                    .note("Caches of an installed Android Studio version are rebuilt with its indexes.")
                    .apps(owners::ANDROID)
                    .risk(Risk::Review),
                cache("Android", "Android Studio logs", "Library/Logs/Google/AndroidStudio*")
                    .note("Logs can help diagnose problems.")
                    .apps(owners::ANDROID)
                    .risk(Risk::Review),
                // .NET
                cache(".NET", "NuGet packages", ".nuget/packages").package_store()
                    .command("dotnet nuget locals all --clear")
                    .apps(owners::JETBRAINS),
                cache(".NET", "NuGet HTTP cache", ".local/share/NuGet/v3-cache")
                    .command("dotnet nuget locals http-cache --clear"),
                // Rust: binaries, config.toml and credentials stay in place.
                cache("Rust (Cargo)", "Cargo registry cache", ".cargo/registry/cache").package_store(),
                cache("Rust (Cargo)", "Cargo registry sources", ".cargo/registry/src").package_store(),
                cache("Rust (Cargo)", "Cargo registry index", ".cargo/registry/index"),
                cache("Rust (Cargo)", "Cargo Git database", ".cargo/git/db").package_store(),
                cache("Rust (Cargo)", "Cargo Git checkouts", ".cargo/git/checkouts").package_store(),
                cache("Rust (Cargo)", "sccache", "Library/Caches/Mozilla.sccache"),
                // C and C++
                cache("C / C++", "ccache", "Library/Caches/ccache").command("ccache --clear"),
                cache("C / C++", "ccache", ".ccache").command("ccache --clear"),
                cache("Bazel", "Bazel output", "Library/Caches/bazel")
                    .note("Build outputs and downloads of every Bazel workspace; the next build starts from scratch.")
                    .command("bazel clean --expunge")
                    .blocked("Bazel makes its outputs read-only, so Trash cannot remove them. Run `bazel clean --expunge` in each workspace."),
                // Go
                cache("Go", "Go module cache", "go/pkg/mod").package_store()
                    .command("go clean -modcache")
                    .blocked("Go makes module files read-only, so Trash cannot remove them. Use `go clean -modcache`."),
                cache("Go", "Go build cache", "Library/Caches/go-build").command("go clean -cache"),
                // PHP, Ruby and Elixir
                cache("PHP (Composer)", "Composer cache", "Library/Caches/composer").package_store()
                    .command("composer clear-cache"),
                cache("Ruby (Bundler)", "Bundler cache", ".bundle/cache").package_store(),
                cache("Elixir", "Hex packages", ".hex/packages").package_store(),
                cache("Elixir", "Mix.install cache", "Library/Caches/mix/installs"),
                // Tools and editors
                cache("Homebrew", "Homebrew downloads", "Library/Caches/Homebrew")
                    .command("brew cleanup --prune=all"),
                cache("JetBrains", "JetBrains IDE caches", "Library/Caches/JetBrains/*")
                    .note("IDEs rebuild indexes on the next open, and Local History (the IDE's file history) stored here is lost.")
                    .apps(owners::JETBRAINS)
                    .risk(Risk::Review),
                cache("Unity", "Unity package cache", "Library/Unity/cache").apps(owners::UNITY),
                // Virtual machines and clusters: images the tools download again. The
                // machines themselves are listed by Containers & VMs.
                cache("Tart", "Tart image cache", ".tart/cache")
                    .note("Images Tart pulled, often tens of GB; virtual machines cloned from them keep working.")
                    .command("tart prune")
                    .risk(Risk::Review),
                cache("Vagrant", "Vagrant box", ".vagrant.d/boxes/*")
                    .note("Machines already made from the box keep working; a new machine downloads it again.")
                    .command("vagrant box prune")
                    .risk(Risk::Review),
                cache("Kubernetes", "minikube downloads", ".minikube/cache")
                    .note("Images and Kubernetes binaries minikube downloads again, several GB; clusters stay.")
                    .risk(Risk::Review),
                // Chrome's own cache, named so the folders beside it in Caches/Google are not
                // taken for app caches.
                cache("Google Chrome", "Chrome cache", "Library/Caches/Google/Chrome")
                    .apps(owners::CHROME)
                    .app_cache(),
                // Browsers: per-profile caches in Application Support. History, cookies,
                // passwords, extensions and Local Storage stay.
                cache("Google Chrome", "Chrome script cache", "Library/Application Support/Google/Chrome/*/Code Cache")
                    .apps(owners::CHROME)
                    .app_cache(),
                cache("Google Chrome", "Chrome GPU cache", "Library/Application Support/Google/Chrome/*/GPUCache")
                    .apps(owners::CHROME)
                    .app_cache(),
                cache("Google Chrome", "Chrome offline web cache", "Library/Application Support/Google/Chrome/*/Service Worker/CacheStorage")
                    .note("Sites keep working offline only after you visit them again.")
                    .apps(owners::CHROME)
                    .app_cache(),
                cache("Google Chrome", "Chrome on-device AI model", "Library/Application Support/Google/Chrome/OptGuideOnDeviceModel")
                    .note("Chrome downloads the model, often several GB, again while its on-device AI features are on. Turn them off in Chrome's settings to keep it away.")
                    .apps(owners::CHROME)
                    .app_cache()
                    .risk(Risk::Review),
                cache("Microsoft Edge", "Edge script cache", "Library/Application Support/Microsoft Edge/*/Code Cache")
                    .apps(owners::EDGE)
                    .app_cache(),
                cache("Microsoft Edge", "Edge GPU cache", "Library/Application Support/Microsoft Edge/*/GPUCache")
                    .apps(owners::EDGE)
                    .app_cache(),
                cache("Microsoft Edge", "Edge offline web cache", "Library/Application Support/Microsoft Edge/*/Service Worker/CacheStorage")
                    .note("Sites keep working offline only after you visit them again.")
                    .apps(owners::EDGE)
                    .app_cache(),
                cache("Brave", "Brave script cache", "Library/Application Support/BraveSoftware/Brave-Browser/*/Code Cache")
                    .apps(owners::BRAVE)
                    .app_cache(),
                cache("Brave", "Brave GPU cache", "Library/Application Support/BraveSoftware/Brave-Browser/*/GPUCache")
                    .apps(owners::BRAVE)
                    .app_cache(),
                cache("Brave", "Brave offline web cache", "Library/Application Support/BraveSoftware/Brave-Browser/*/Service Worker/CacheStorage")
                    .note("Sites keep working offline only after you visit them again.")
                    .apps(owners::BRAVE)
                    .app_cache(),
                // Games and media apps
                cache("Steam", "Steam shader cache", "Library/Application Support/Steam/steamapps/shadercache")
                    .note("Games compile their shaders again and may stutter briefly the first time.")
                    .apps(owners::STEAM)
                    .app_cache(),
                cache("Steam", "Steam web cache", "Library/Application Support/Steam/appcache/httpcache")
                    .apps(owners::STEAM)
                    .app_cache(),
                cache("Steam", "Steam unfinished downloads", "Library/Application Support/Steam/steamapps/downloading")
                    .note("Downloads and updates in progress; Steam starts them again.")
                    .apps(owners::STEAM)
                    .app_cache()
                    .risk(Risk::Review),
                cache("Adobe", "Adobe media cache", "Library/Application Support/Adobe/Common/Media Cache Files")
                    .note("Premiere Pro and After Effects make it again when they open a project; the first open is slower.")
                    .apps(owners::ADOBE)
                    .app_cache(),
                cache("Adobe", "Adobe media cache database", "Library/Application Support/Adobe/Common/Media Cache")
                    .note("Premiere Pro and After Effects make it again when they open a project; the first open is slower.")
                    .apps(owners::ADOBE)
                    .app_cache(),
                // Shown so their size is known, but managed by their apps: removing the folder
                // can lose changes that are not uploaded yet, or leave broken messages.
                cache("iCloud", "iCloud Drive sync cache", "Library/Caches/CloudKit")
                    .note("macOS keeps iCloud Drive transfers here, including changes not uploaded yet.")
                    .risk(Risk::Review)
                    .app_cache()
                    .blocked("macOS manages this iCloud cache. Removing it can lose changes not uploaded yet, and it grows back. To free space, turn on Optimize Mac Storage in iCloud Drive settings."),
                cache("Google Drive", "Google Drive offline files and cache", "Library/Application Support/Google/DriveFS")
                    .note("Google Drive keeps files made available offline, its file cache and changes waiting to upload here.")
                    .risk(Risk::Review)
                    .app_cache()
                    .blocked("Google Drive manages this folder, and it can hold changes not uploaded yet. To free space, make folders online-only in Google Drive, or use its Clear cache setting."),
                cache("macOS", "Messages attachments", "Library/Messages/Attachments")
                    .note("Photos, videos and files from your conversations.")
                    .risk(Risk::Review)
                    .app_cache()
                    .blocked("Removing attachments here leaves broken messages. Delete them in Messages, or in System Settings › General › Storage › Messages."),
            ]
            .into_iter()
            // VS Code's Electron caches, logs and crash reports, named as for every editor.
            .chain(
                super::electron_folders!("Library/Application Support/Code/", "VS Code · ").map(
                    |(path, name, note, cache_folder)| {
                        let location = cache("VS Code", name, path).note(note).apps(owners::VSCODE);
                        // Logs and crash reports are never made again.
                        if cache_folder {
                            location
                        } else {
                            location.risk(Risk::Review)
                        }
                    },
                ),
            )
            .collect(),
        }
    }
}
impl CachesModule {
    fn home(&self) -> String {
        self.home.clone().unwrap_or_else(policy::home)
    }
    /// `~/.cache` as one reviewable item. Command-line tools (uv, Puppeteer, Hugging Face,
    /// PyTorch, pre-commit and more) share it; some keep large downloads or a login there.
    fn tool_cache_candidate(&self, s: &Services, c: &ScanContext) -> Option<Candidate> {
        let path = format!("{}/{}", self.home(), self.tool_cache?);
        let entry = s.entry(&path).ok().filter(|e| e.directory)?;
        if !c.allows(&path) {
            return None;
        }
        let mut names: Vec<String> = s
            .children(&path, &mut vec![])
            .into_iter()
            .map(|e| e.name().to_owned())
            .collect();
        names.sort_by_key(|n| n.to_lowercase());
        let shown = names
            .iter()
            .take(12)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        let contents = match names.len().saturating_sub(12) {
            0 => shown,
            more => format!("{shown} and {more} more"),
        };
        let mut reason = "Caches of command-line tools, which download or rebuild what they need. Some keep large downloads here, such as machine-learning models.".to_owned();
        let mut details = vec![
            detail("Ecosystem", "Command-line tools"),
            detail("Contains", contents),
        ];
        if s.is_file(&format!("{path}/huggingface/token")) {
            reason +=
                " It also holds your Hugging Face login token; you will need to sign in again.";
            details.push(detail("Sign-in", "Hugging Face token"));
        }
        Some(
            Candidate::new(entry, &reason, vec![ActionKind::Trash], Risk::Review)
                .title("Command-line tool caches (~/.cache)".to_owned())
                .details(details),
        )
    }
    fn logs(&self) -> String {
        self.home() + "/Library/Logs"
    }
    /// Every folder this module reports on, so other modules can leave them to it. A
    /// pattern covers only the folders it matches: `Library/Caches/Google/AndroidStudio*`
    /// leaves Chrome's `Library/Caches/Google/Chrome` to the macOS leftovers.
    pub fn covered(&self, s: &Services) -> Vec<String> {
        let home = self.home();
        self.locations
            .iter()
            .flat_map(|l| {
                if l.path.contains('*') {
                    expand(s, &home, l)
                        .into_iter()
                        .map(|(e, _)| e.identity.path)
                        .collect()
                } else {
                    vec![format!("{home}/{}", l.path)]
                }
            })
            .chain([self.logs()])
            .chain(self.tool_cache.map(|c| format!("{home}/{c}")))
            .collect()
    }
    /// The location an existing path belongs to.
    fn location(&self, path: &str) -> Option<&CacheLocation> {
        find_location(&self.home(), &self.locations, path)
    }
}
/// The location of `locations` an existing path belongs to.
fn find_location<'a>(
    home: &str,
    locations: &'a [CacheLocation],
    path: &str,
) -> Option<&'a CacheLocation> {
    let relative: Vec<&str> = policy::home_relative(path, home)?.split('/').collect();
    locations.iter().find(|l| {
        let parts: Vec<&str> = l.path.split('/').collect();
        parts.len() == relative.len()
            && parts
                .iter()
                .zip(&relative)
                .all(|(pattern, name)| glob(pattern, name).is_some())
    })
}
/// Existing folders for a location: the path itself, or each match of its `*` components,
/// titled with the names they matched (`JetBrains IDE caches · PyCharm2024.2`).
fn expand(s: &Services, home: &str, location: &CacheLocation) -> Vec<(Entry, String)> {
    let mut current: Vec<(String, Vec<String>)> = vec![(home.to_owned(), vec![])];
    for part in location.path.split('/') {
        let mut next = vec![];
        for (folder, names) in current {
            if !part.contains('*') {
                next.push((format!("{folder}/{part}"), names));
                continue;
            }
            if !s.is_dir(&folder) {
                continue;
            }
            for e in s.children(&folder, &mut vec![]) {
                if e.directory && !e.symlink && glob(part, e.name()).is_some() {
                    let mut names = names.clone();
                    names.push(e.name().to_owned());
                    next.push((e.identity.path, names));
                }
            }
        }
        current = next;
    }
    current
        .into_iter()
        .filter_map(|(path, names)| {
            let e = s.entry(&path).ok()?;
            let title = match names.is_empty() {
                true => location.name.to_owned(),
                false => format!("{} · {}", location.name, names.join(" · ")),
            };
            Some((e, title))
        })
        .collect()
}
impl ScanModule for CachesModule {
    fn descriptor(&self) -> ModuleDescriptor {
        descriptor(
            "caches",
            "Caches & Logs",
            "Developer",
            "archivebox",
            "Developer and app caches, logs and other Mac leftovers, with rebuild costs made clear.",
            false,
        )
    }
    fn action_roots(&self) -> Vec<String> {
        let home = self.home();
        self.locations
            .iter()
            .map(|l| format!("{home}/{}", l.root()))
            .chain([format!("{home}/Library/Caches"), self.logs()])
            .chain(self.tool_cache.map(|c| format!("{home}/{c}")))
            .chain(self.mac.action_roots(&home))
            .collect()
    }
    fn scan(
        &self,
        s: &Services,
        c: &ScanContext,
        k: &ScanControl,
        sink: &mut dyn Sink,
    ) -> Result<()> {
        let mut candidates = vec![];
        for location in &self.locations {
            k.check()?;
            let note = location
                .note
                .unwrap_or("Cached downloads may be needed for offline installs.");
            let mut reason = match location.blocked {
                Some(_) => note.to_owned(),
                None => format!("{note} Close the owning tools before moving this cache to Trash."),
            };
            let mut details = vec![detail("Ecosystem", location.ecosystem)];
            if let Some(command) = location.command {
                reason += &format!(" Official command: `{command}`.");
                details.push(detail("Official command", command));
            }
            for (e, title) in expand(s, &self.home(), location) {
                if c.allows(e.path()) {
                    candidates.push(
                        Candidate::new(e, &reason, vec![ActionKind::Trash], location.risk)
                            .title(title)
                            .details(details.clone())
                            .blocked(location.blocked),
                    );
                }
            }
        }
        if let Some(candidate) = self.tool_cache_candidate(s, c) {
            candidates.push(candidate);
        }
        // macOS leftovers, except folders the tool caches above cover or another tool lists
        // with checks of its own (a leftover's app check would never see `limactl` run).
        let home = self.home();
        let covered: Vec<String> = self
            .covered(s)
            .into_iter()
            .chain(
                super::listed_elsewhere()
                    .into_iter()
                    .map(|f| format!("{home}/{f}")),
            )
            .collect();
        candidates.extend(self.mac.candidates(s, c, k, &home, &covered)?);
        let logs = self.logs();
        if s.is_dir(&logs) && !c.excludes(&logs) {
            let mut warnings = vec![];
            for e in s.children(&logs, &mut warnings) {
                if c.allows(e.path()) {
                    let title = format!("Logs · {}", e.name());
                    candidates.push(
                        Candidate::new(
                            e,
                            "Logs can help diagnose problems. Review before removal.",
                            vec![ActionKind::Trash],
                            Risk::Review,
                        )
                        .title(title),
                    );
                }
            }
            flush(sink, &mut warnings);
        }
        add_files(s, sink, "caches", candidates, k)
    }
    fn in_use(&self, s: &Services, f: &Finding, _: ActionKind) -> Option<String> {
        let path = f.resource.path()?;
        // Owning apps are checked in `preflight`. A macOS leftover or an app's own cache
        // waits only for its app, not for developer tools.
        match self.location(path) {
            None if self.mac.lists(&self.home(), path) => return None,
            Some(l) if l.app_cache => return None,
            _ => {}
        }
        let active = orphans::active_tools(s, None);
        (!active.is_empty()).then(|| {
            format!(
                "Developer tools are running that may be using shared caches:{}",
                orphans::list(&active)
            )
        })
    }
    fn preflight(&self, s: &Services, f: &Finding, _: ActionKind, _: &ScanControl) -> Result<()> {
        let path = f.resource.path().ok_or("Missing path")?;
        // The scan lists a macOS leftover only where no cache above claims the folder, so
        // the leftover's app check applies only then.
        match self.location(path) {
            Some(location) => {
                if let Some(reason) = location.blocked {
                    return Err(reason.into());
                }
                owners_closed(s, location.apps)
            }
            None => self.mac.preflight(s, &self.home(), path).unwrap_or(Ok(())),
        }
    }
}
