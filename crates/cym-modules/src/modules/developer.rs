use super::{
    add_files, descriptor, flush, glob, owners, owners_closed, Candidate, LastUsed, ScanModule,
};
use crate::{git::Git, model::*, orphans, policy, ports::*, services::Services};
use std::path::Path;

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
/// Refuses to move generated output that Git tracks inside its repository.
fn untracked(s: &Services, path: &str, k: &ScanControl) -> Result<()> {
    let directory = parent(path);
    let mut ancestor = Some(Path::new(&directory));
    while let Some(folder) = ancestor {
        if s.exists(&folder.join(".git").to_string_lossy()) {
            if Git(s.commands.as_ref()).tracks(&directory, path, k)? {
                return Err("Artifact contains tracked files.".into());
            }
            return Ok(());
        }
        ancestor = folder.parent();
    }
    Ok(())
}

/// A generated folder, the project files that prove what produced it, and what rebuilding
/// costs. A folder name alone is never evidence.
///
/// A name with `/` is a nested layout (`ios/Pods`): every component must match and the
/// project folder is the one above the first component. Names and evidence may use one `*`
/// per component (`cmake-build-*`, `*.csproj`), and evidence ending in `/` must be a folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArtifactRule {
    /// Shown as the finding's "Ecosystem" detail.
    pub ecosystem: &'static str,
    /// What the folder holds, such as "build output".
    pub kind: &'static str,
    pub names: &'static [&'static str],
    /// At least one must exist in the project folder.
    pub evidence: &'static [&'static str],
    /// Each must also exist in the project folder.
    pub requires: &'static [&'static str],
    /// Each must exist in the folder that directly contains the artifact.
    pub beside: &'static [&'static str],
    /// Each must exist inside the artifact itself.
    pub inside: &'static [&'static str],
    /// The consequence of removing it, shown in the reason.
    pub rebuild: &'static str,
    /// The tool's own clean command, preferred where it exists.
    pub command: Option<&'static str>,
    /// Bundle-identifier prefixes of apps that must be closed first.
    pub apps: &'static [&'static str],
    pub risk: Risk,
    /// Left out when Git tracks it, for folders projects often commit (`vendor`).
    pub hide_tracked: bool,
}
fn rule(
    ecosystem: &'static str,
    kind: &'static str,
    names: &'static [&'static str],
    evidence: &'static [&'static str],
    rebuild: &'static str,
) -> ArtifactRule {
    ArtifactRule {
        ecosystem,
        kind,
        names,
        evidence,
        requires: &[],
        beside: &[],
        inside: &[],
        rebuild,
        command: None,
        apps: &[],
        risk: Risk::Rebuild,
        hide_tracked: false,
    }
}
impl ArtifactRule {
    pub fn requires(mut self, files: &'static [&'static str]) -> Self {
        self.requires = files;
        self
    }
    pub fn beside(mut self, files: &'static [&'static str]) -> Self {
        self.beside = files;
        self
    }
    pub fn inside(mut self, files: &'static [&'static str]) -> Self {
        self.inside = files;
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
    pub fn risk(mut self, risk: Risk) -> Self {
        self.risk = risk;
        self
    }
    pub fn hide_tracked(mut self) -> Self {
        self.hide_tracked = true;
        self
    }
}
/// A folder matched by a rule, with its project folder and the evidence that proved it.
#[derive(Debug, PartialEq, Eq)]
pub struct ArtifactMatch<'a> {
    pub rule: &'a ArtifactRule,
    pub project: String,
    /// The artifact's path relative to the project, such as `ios/Pods`.
    pub relative: String,
    pub evidence: String,
}
/// The name of an item matching `pattern` in `folder`, if one exists.
fn present(s: &Services, folder: &str, pattern: &str) -> Option<String> {
    let (pattern, folder_wanted) = match pattern.strip_suffix('/') {
        Some(p) => (p, true),
        None => (pattern, false),
    };
    let kind = |e: &Entry| {
        if folder_wanted {
            e.directory
        } else {
            e.regular
        }
    };
    if !pattern.contains('*') {
        return s
            .entry(&format!("{folder}/{pattern}"))
            .ok()
            .filter(kind)
            .map(|_| pattern.into());
    }
    let (base, last) = match pattern.rsplit_once('/') {
        Some((sub, last)) => (format!("{folder}/{sub}"), last),
        None => (folder.to_owned(), pattern),
    };
    s.children(&base, &mut vec![])
        .into_iter()
        .find(|e| kind(e) && glob(last, e.name()).is_some())
        .map(|e| e.name().to_owned())
}

const PYTHON: &[&str] = &[
    "pyproject.toml",
    "requirements.txt",
    "setup.py",
    "setup.cfg",
    "Pipfile",
    "poetry.lock",
    "uv.lock",
];
const PYTHON_TOOLS: &[&str] = &[
    "pyproject.toml",
    "setup.cfg",
    "setup.py",
    "requirements.txt",
    "tox.ini",
    "pytest.ini",
    "mypy.ini",
    "ruff.toml",
    ".ruff.toml",
    "*.py",
];
const GRADLE: &[&str] = &[
    "build.gradle",
    "build.gradle.kts",
    "settings.gradle",
    "settings.gradle.kts",
];
const APP_CONFIG: &[&str] = &["app.json", "app.config.js", "app.config.ts"];
const CAPACITOR: &[&str] = &[
    "capacitor.config.ts",
    "capacitor.config.json",
    "capacitor.config.js",
];
const PODS: &[&str] = &["Podfile", "Podfile.lock"];
const PODS_REBUILD: &str = "Restored by pod install.";

/// Generated folders identified by name and owning-project evidence. Traversal never enters
/// a matched folder, a `prune` folder or installed dependencies, which belong to the
/// Dependencies module.
pub struct ArtifactsModule {
    /// Checked in order; the first rule with evidence wins.
    pub rules: Vec<ArtifactRule>,
    /// Folder names never descended into.
    pub prune: Vec<&'static str>,
    /// Installed dependencies, which are neither listed nor searched here.
    pub dependencies: Vec<ArtifactRule>,
}
impl Default for ArtifactsModule {
    fn default() -> Self {
        Self {
            rules: vec![
                // Mobile projects with nested native folders come before generic rules. The
                // ios and android folders themselves are never candidates.
                rule(
                    "React Native",
                    "iOS build output",
                    &["ios/build"],
                    APP_CONFIG,
                    "Rebuilt by the next iOS build.",
                )
                .requires(&["package.json"])
                .beside(&["*.xcodeproj/"])
                .apps(owners::XCODE),
                rule(
                    "React Native",
                    "Android build output",
                    &["android/build", "android/app/build", "android/.gradle"],
                    APP_CONFIG,
                    "Rebuilt by the next Android build.",
                )
                .requires(&["package.json"])
                .beside(&["build.gradle*"])
                .command("./gradlew clean")
                .apps(owners::ANDROID),
                rule(
                    "Expo",
                    "local project state",
                    &[".expo"],
                    APP_CONFIG,
                    "Recreated by the next expo start.",
                ),
                rule(
                    "Capacitor",
                    "Android build output",
                    &["android/build", "android/app/build", "android/.gradle"],
                    CAPACITOR,
                    "Rebuilt by the next Android build.",
                )
                .beside(&["build.gradle*"])
                .command("./gradlew clean")
                .apps(owners::ANDROID),
                rule(
                    "Flutter / Dart",
                    "build output or tool cache",
                    &["build", ".dart_tool"],
                    &["pubspec.yaml"],
                    "Restored by flutter pub get and the next build.",
                )
                .command("flutter clean")
                .apps(owners::XCODE),
                // JavaScript frameworks.
                rule(
                    "Angular",
                    "build cache",
                    &[".angular"],
                    &["angular.json"],
                    "Rebuilt by the next ng build or ng serve.",
                )
                .command("ng cache clean"),
                rule(
                    "SvelteKit",
                    "generated output",
                    &[".svelte-kit"],
                    &["svelte.config.*"],
                    "Regenerated by the next dev server start or build.",
                ),
                rule(
                    "Next.js",
                    "build output",
                    &[".next"],
                    &["next.config.*"],
                    "Rebuilt by the next next dev or next build.",
                ),
                rule(
                    "Nuxt",
                    "generated output",
                    &[".nuxt", ".output"],
                    &["nuxt.config.*"],
                    "Regenerated by the next nuxt dev or nuxt build.",
                ),
                rule(
                    "Docusaurus",
                    "generated site",
                    &[".docusaurus", "build"],
                    &["docusaurus.config.*"],
                    "Regenerated by the next docusaurus start or build.",
                )
                .command("docusaurus clear"),
                rule(
                    "Nx",
                    "task cache",
                    &[".nx/cache"],
                    &["nx.json"],
                    "Cached task results are recomputed on the next run.",
                )
                .command("nx reset"),
                rule(
                    "Turborepo",
                    "task cache",
                    &[".turbo"],
                    &["turbo.json"],
                    "Cached task results are recomputed on the next run.",
                ),
                rule(
                    "Storybook",
                    "static build",
                    &["storybook-static"],
                    &[".storybook/"],
                    "Rebuilt by build-storybook.",
                )
                .requires(&["package.json"]),
                rule(
                    "JavaScript",
                    "generated output",
                    &[".parcel-cache", "coverage"],
                    &["package.json"],
                    "Regenerated by the project's build or test scripts.",
                ),
                // Apple platforms.
                // Each folder in .build except fetched packages, which are dependencies.
                rule(
                    "SwiftPM",
                    "build output",
                    &[".build/*"],
                    &["Package.swift"],
                    "Rebuilt by the next swift build.",
                )
                .command("swift package clean")
                .apps(owners::XCODE),
                rule(
                    "Xcode",
                    "project-local build data",
                    &["DerivedData"],
                    &["*.xcodeproj/", "*.xcworkspace/"],
                    "Rebuilt by the next Xcode build.",
                )
                .apps(owners::XCODE),
                // Rust, JVM and native toolchains.
                rule(
                    "Rust (Cargo)",
                    "build output",
                    &["target"],
                    &["Cargo.toml"],
                    "Rebuilt by the next cargo build; full rebuilds can take a while.",
                )
                .command("cargo clean"),
                rule(
                    "Maven",
                    "build output",
                    &["target"],
                    &["pom.xml"],
                    "Rebuilt by the next mvn package.",
                )
                .command("mvn clean")
                .apps(owners::ANDROID),
                rule(
                    "sbt",
                    "build output",
                    &["target", "project/target"],
                    &["build.sbt"],
                    "Rebuilt by the next sbt compile.",
                )
                .command("sbt clean")
                .apps(owners::JETBRAINS),
                rule(
                    "Gradle",
                    "build output",
                    &["build", ".gradle", ".kotlin"],
                    GRADLE,
                    "Rebuilt by the next Gradle build.",
                )
                .command("./gradlew clean")
                .apps(owners::ANDROID),
                rule(
                    "Zig",
                    "build cache or output",
                    &[".zig-cache", "zig-cache", "zig-out"],
                    &["build.zig"],
                    "Rebuilt by the next zig build.",
                ),
                rule(
                    "CMake",
                    "build tree",
                    &["build", "cmake-build-*"],
                    &["CMakeLists.txt"],
                    "Reconfigure and rebuild with CMake; cache options set in the build tree are lost.",
                )
                .inside(&["CMakeCache.txt", "CMakeFiles/"]),
                // Game engines come before .NET because Unity generates .csproj files.
                rule(
                    "Unity",
                    "generated project data",
                    &["Library", "Temp", "Obj", "obj", "Logs"],
                    &["ProjectSettings/ProjectVersion.txt"],
                    "Unity reimports assets on the next open, which can take a long time; logs are lost.",
                )
                .requires(&["Assets/"])
                .apps(owners::UNITY),
                rule(
                    "Unreal Engine",
                    "build output or derived data",
                    &["Intermediate", "DerivedDataCache", "Binaries"],
                    &["*.uproject"],
                    "Rebuilt by the next editor launch or build, which can take a long time.",
                )
                .apps(owners::UNREAL),
                rule(
                    "Unreal Engine",
                    "saved data",
                    &["Saved"],
                    &["*.uproject"],
                    "Holds autosaves, crash logs, screenshots and local editor settings; review before removal.",
                )
                .apps(owners::UNREAL)
                .risk(Risk::Review),
                rule(
                    ".NET",
                    "build output",
                    &["bin", "obj"],
                    &["*.csproj", "*.fsproj", "*.vbproj", "*.sln"],
                    "Rebuilt by the next dotnet build; packages are restored again.",
                )
                .command("dotnet clean")
                .apps(owners::JETBRAINS),
                // Python.
                rule(
                    "Python",
                    "test environments",
                    &[".tox"],
                    &["tox.ini", "pyproject.toml", "setup.cfg"],
                    "Recreated by the next tox run.",
                ),
                rule(
                    "Python",
                    "test environments",
                    &[".nox"],
                    &["noxfile.py"],
                    "Recreated by the next nox run.",
                ),
                rule(
                    "Python",
                    "tool cache",
                    &[".pytest_cache", ".mypy_cache", ".ruff_cache"],
                    PYTHON_TOOLS,
                    "Rebuilt by the next pytest, mypy or ruff run; last-failed state is lost.",
                ),
                rule(
                    "Python",
                    "bytecode cache",
                    &["__pycache__"],
                    &["*.py"],
                    "Python recompiles modules on next import.",
                ),
                rule(
                    "Python",
                    "package metadata",
                    &["*.egg-info", "src/*.egg-info"],
                    &["pyproject.toml", "setup.py", "setup.cfg"],
                    "Regenerated by the next editable install or build.",
                ),
                // Other languages and tools.
                rule(
                    "Elixir",
                    "build output",
                    &["_build", ".elixir_ls"],
                    &["mix.exs"],
                    "Rebuilt by the next mix compile.",
                )
                .requires(&["mix.lock"])
                .command("mix clean"),
                rule(
                    "Haskell (Stack)",
                    "build output",
                    &[".stack-work"],
                    &["stack.yaml"],
                    "Rebuilt by the next stack build.",
                )
                .command("stack clean --full"),
                rule(
                    "Haskell (Cabal)",
                    "build output",
                    &["dist-newstyle"],
                    &["cabal.project", "*.cabal"],
                    "Rebuilt by the next cabal build.",
                )
                .command("cabal clean"),
                rule(
                    "Elm",
                    "build cache",
                    &["elm-stuff"],
                    &["elm.json"],
                    "Rebuilt by the next elm make.",
                ),
            ],
            prune: vec!["node_modules", "site-packages"],
            dependencies: dependency_rules(),
        }
    }
}
impl ArtifactsModule {
    /// The first rule whose names and evidence match the folder at `path`.
    pub fn matches(&self, s: &Services, path: &str) -> Option<ArtifactMatch<'_>> {
        match_rules(&self.rules, s, path)
    }
}
/// The first of `rules` whose names and evidence match the folder at `path`.
pub fn match_rules<'a>(
    rules: &'a [ArtifactRule],
    s: &Services,
    path: &str,
) -> Option<ArtifactMatch<'a>> {
    let parts: Vec<&str> = path.split('/').collect();
    let last = *parts.last()?;
    let containing = parent(path);
    for rule in rules {
        for pattern in rule.names {
            let wanted: Vec<&str> = pattern.split('/').collect();
            if glob(wanted[wanted.len() - 1], last).is_none()
                || wanted.len() >= parts.len()
                || !wanted
                    .iter()
                    .rev()
                    .zip(parts.iter().rev())
                    .all(|(p, n)| glob(p, n).is_some())
            {
                continue;
            }
            let split = parts.len() - wanted.len();
            let project = parts[..split].join("/");
            let all = |folder: &str, files: &[&str]| {
                files.iter().all(|f| present(s, folder, f).is_some())
            };
            if project.is_empty()
                || !all(&project, rule.requires)
                || !all(&containing, rule.beside)
                || !all(path, rule.inside)
            {
                continue;
            }
            if let Some(evidence) = rule.evidence.iter().find_map(|f| present(s, &project, f)) {
                return Some(ArtifactMatch {
                    rule,
                    relative: parts[split..].join("/"),
                    project,
                    evidence,
                });
            }
        }
    }
    None
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
        let walked = s.walk(c, k, &mut warnings, &mut |e| {
            if self.prune.contains(&e.name()) {
                return Ok(false);
            }
            if e.directory {
                if match_rules(&self.dependencies, s, e.path()).is_some() {
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
                    candidates.push(
                        Candidate::new(e.clone(), &reason, vec![ActionKind::Trash], rule.risk)
                            .title(m.relative)
                            .details(details)
                            .last_used(LastUsed::Project { folder: project }),
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
        owners_closed(s, m.rule.apps)
            .err()
            .or_else(|| active_project_tools(s, &m.project))
    }
    fn preflight(&self, s: &Services, f: &Finding, _: ActionKind, k: &ScanControl) -> Result<()> {
        let path = f.resource.path().ok_or("Missing path")?;
        self.matches(s, path)
            .ok_or("The owning project's evidence is no longer available.")?;
        untracked(s, path, k)
    }
}

/// Installed dependencies other than `node_modules`, identified like build output: by folder
/// name and evidence from the project that installs them. More specific layouts come first.
pub fn dependency_rules() -> Vec<ArtifactRule> {
    vec![
        rule(
            "React Native",
            "installed pods",
            &["ios/Pods"],
            APP_CONFIG,
            PODS_REBUILD,
        )
        .requires(&["package.json"])
        .beside(PODS)
        .command("pod install")
        .apps(owners::XCODE),
        rule(
            "Capacitor",
            "installed pods",
            &["ios/App/Pods"],
            CAPACITOR,
            PODS_REBUILD,
        )
        .beside(PODS)
        .command("pod install")
        .apps(owners::XCODE),
        rule(
            "Flutter / Dart",
            "installed pods",
            &["ios/Pods", "macos/Pods"],
            &["pubspec.yaml"],
            PODS_REBUILD,
        )
        .beside(PODS)
        .command("pod install")
        .apps(owners::XCODE),
        rule(
            "Yarn",
            "unplugged packages",
            &[".yarn/unplugged"],
            &[".yarnrc.yml"],
            "Restored by yarn install. The committed .yarn/cache is never offered.",
        )
        .requires(&["package.json"]),
        rule(
            "CocoaPods",
            "installed pods",
            &["Pods"],
            &["Podfile"],
            PODS_REBUILD,
        )
        .requires(&["Podfile.lock"])
        .command("pod install")
        .apps(owners::XCODE),
        rule(
            "Carthage",
            "built dependencies",
            &["Carthage/Build"],
            &["Cartfile"],
            "Restored by carthage bootstrap; rebuilding can take a while.",
        )
        .requires(&["Cartfile.resolved"])
        .apps(owners::XCODE),
        rule(
            "Python",
            "virtual environment",
            &[".venv", "venv", "env"],
            PYTHON,
            "Recreate it and reinstall packages; anything installed by hand is lost.",
        )
        .inside(&["pyvenv.cfg"]),
        rule(
            "Ruby (Bundler)",
            "installed gems",
            &["vendor/bundle"],
            &["Gemfile"],
            "Restored by bundle install.",
        )
        .requires(&["Gemfile.lock"]),
        rule(
            "PHP (Composer)",
            "installed packages",
            &["vendor"],
            &["composer.json"],
            "Restored by composer install; local edits to packages are lost.",
        )
        .requires(&["composer.lock"])
        .inside(&["autoload.php"])
        .hide_tracked(),
        rule(
            "Go",
            "vendored modules",
            &["vendor"],
            &["go.mod"],
            "Restored by go mod vendor; builds use the module cache meanwhile.",
        )
        .inside(&["modules.txt"])
        .command("go mod vendor")
        .hide_tracked(),
        rule(
            "Rust (Cargo)",
            "vendored crates",
            &["vendor"],
            &["Cargo.toml"],
            "Restored by cargo vendor. Offline builds fail until it runs again, since .cargo/config.toml points them here.",
        )
        .requires(&["Cargo.lock", ".cargo/config.toml"])
        .command("cargo vendor")
        .hide_tracked(),
        rule(
            "SwiftPM",
            "fetched packages",
            &[".build/checkouts", ".build/repositories", ".build/artifacts"],
            &["Package.swift"],
            "Fetched again by the next swift build or swift package resolve.",
        )
        .command("swift package reset")
        .apps(owners::XCODE),
        rule(
            "Terraform",
            "providers and modules",
            &[".terraform"],
            &["*.tf"],
            "Restored by terraform init.",
        ),
        rule(
            "Elixir",
            "dependencies",
            &["deps"],
            &["mix.exs"],
            "Restored by mix deps.get.",
        )
        .requires(&["mix.lock"])
        .command("mix deps.clean --all"),
        rule(
            "Bower",
            "installed packages",
            &["bower_components"],
            &["bower.json"],
            "Restored by bower install.",
        ),
        rule(
            "R (renv)",
            "project library",
            &["renv/library"],
            &["renv.lock"],
            "Restored by renv::restore().",
        ),
    ]
}

/// Installed dependencies by project: `node_modules`, Python virtual environments, CocoaPods,
/// SwiftPM checkouts, Carthage, Bundler, Composer, Go and Cargo vendor folders, Elixir deps
/// and more, found without descending into them. Ecosystems that keep packages outside the
/// project (Cargo, Go, Maven, Gradle, Pub and others) are covered by their shared package
/// stores, which Caches & Logs also lists.
pub struct DependenciesModule {
    /// Dependency folders other than `node_modules`; the first rule with evidence wins.
    pub rules: Vec<ArtifactRule>,
    /// Shared package stores in the home folder, one finding each.
    pub stores: Vec<CacheLocation>,
    /// Package manager name and the lockfile that identifies it, in priority order.
    pub lockfiles: Vec<(&'static str, &'static str)>,
    /// Files beside a `node_modules` that name its project: Node's, then Deno's.
    pub manifests: Vec<(&'static str, &'static str)>,
    /// Package managers' own stores, caches and global installs, relative to the home
    /// folder. Their `node_modules` belong to the tool, not a project, so they are not
    /// searched; Caches & Logs and Toolchains & SDKs cover them.
    pub tool_folders: Vec<&'static str>,
    /// Folder names that are always a package manager's store, wherever they appear.
    pub tool_folder_names: Vec<&'static str>,
    /// The home folder `tool_folders` and `stores` are relative to; the user's when `None`.
    pub home: Option<String>,
}
impl Default for DependenciesModule {
    fn default() -> Self {
        Self {
            rules: dependency_rules(),
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
            tool_folder_names: vec![".pnpm", ".pnpm-store", "_cacache", "_npx"],
            home: None,
            lockfiles: vec![
                ("pnpm", "pnpm-lock.yaml"),
                ("Yarn", "yarn.lock"),
                ("Bun", "bun.lock"),
                ("Bun", "bun.lockb"),
                ("npm", "package-lock.json"),
                ("Deno", "deno.lock"),
            ],
            manifests: vec![
                ("Node.js", "package.json"),
                ("Deno", "deno.json"),
                ("Deno", "deno.jsonc"),
            ],
        }
    }
}
impl DependenciesModule {
    /// The rule matching a dependency folder other than `node_modules`.
    pub fn matches(&self, s: &Services, path: &str) -> Option<ArtifactMatch<'_>> {
        match_rules(&self.rules, s, path)
    }
    fn home(&self) -> String {
        self.home.clone().unwrap_or_else(policy::home)
    }
    /// The runtime and manifest of the project a `node_modules` folder belongs to.
    fn manifest(&self, s: &Services, project: &str) -> Option<(&'static str, &'static str)> {
        self.manifests
            .iter()
            .find(|(_, file)| s.is_file(&format!("{project}/{file}")))
            .copied()
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
            // project owns.
            if self.tool_folder_names.contains(&e.name()) || skipped.iter().any(|f| f == e.path()) {
                return Ok(false);
            }
            if let Some(m) = self.matches(s, e.path()) {
                matched.push((e.clone(), m.rule.hide_tracked));
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
                let (runtime, _) = self.manifest(s, &parent(e.path()))?;
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
        found.extend(self.stores(s, c));
        add_files(s, sink, "node", found, k)
    }
    fn in_use(&self, s: &Services, f: &Finding, _: ActionKind) -> Option<String> {
        let path = f.resource.path()?;
        if let Some(store) = self.store(path) {
            return owners_closed(s, store.apps).err().or_else(|| {
                let active = orphans::active_tools(s, None);
                (!active.is_empty()).then(|| {
                    format!(
                        "Developer tools are running that may be installing from this store:{}",
                        orphans::list(&active)
                    )
                })
            });
        }
        match self.matches(s, path) {
            Some(m) => owners_closed(s, m.rule.apps)
                .err()
                .or_else(|| active_project_tools(s, &m.project)),
            None => active_project_tools(s, &parent(path)),
        }
    }
    fn preflight(&self, s: &Services, f: &Finding, _: ActionKind, k: &ScanControl) -> Result<()> {
        let path = f.resource.path().ok_or("Missing path")?;
        if let Some(store) = self.store(path) {
            return match store.blocked {
                Some(reason) => Err(reason.into()),
                None => Ok(()),
            };
        }
        if name(path) == "node_modules" {
            if self.manifest(s, &parent(path)).is_none() {
                return Err("The owning package.json or deno.json is no longer available.".into());
            }
        } else {
            self.matches(s, path)
                .ok_or("The owning project's evidence is no longer available.")?;
        }
        untracked(s, path, k)
    }
}

/// Xcode records when it last opened each DerivedData folder in its `info.plist`.
fn derived_data_accessed(folder: &str) -> Option<f64> {
    let info = plist::Value::from_file(Path::new(folder).join("info.plist")).ok()?;
    let date = info.as_dictionary()?.get("LastAccessedDate")?.as_date()?;
    std::time::SystemTime::from(date)
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs_f64())
}

pub struct XcodeModule;
impl XcodeModule {
    pub fn roots() -> Vec<String> {
        ["DerivedData", "iOS DeviceSupport", "Archives"]
            .iter()
            .map(|s| format!("{}/Library/Developer/Xcode/{s}", policy::home()))
            .collect()
    }
}
impl ScanModule for XcodeModule {
    fn descriptor(&self) -> ModuleDescriptor {
        descriptor(
            "xcode",
            "Xcode Data",
            "Developer",
            "wrench.and.screwdriver",
            "Build and device-support data. Archives remain protected.",
            false,
        )
    }
    fn action_roots(&self) -> Vec<String> {
        Self::roots()
    }
    fn scan(
        &self,
        s: &Services,
        c: &ScanContext,
        k: &ScanControl,
        sink: &mut dyn Sink,
    ) -> Result<()> {
        let mut warnings = vec![];
        let mut candidates = vec![];
        for root in Self::roots() {
            if !s.is_dir(&root) || c.excludes(&root) {
                continue;
            }
            let archive = root.ends_with("/Archives");
            for e in s.children(&root, &mut warnings) {
                if !c.allows(e.path()) {
                    continue;
                }
                candidates.push(if archive {
                    Candidate::new(
                        e,
                        "Archives may contain irreplaceable release builds and dSYMs.",
                        vec![],
                        Risk::Review,
                    )
                    .acknowledge(
                        "This archive may be the only copy of a build you shipped, with the debug symbols (dSYMs) needed to read its crash reports.",
                        vec![ActionKind::Trash],
                    )
                } else {
                    let accessed = derived_data_accessed(e.path());
                    Candidate::new(
                        e,
                        "Xcode regenerates this build or device-support data when needed.",
                        vec![ActionKind::Trash],
                        Risk::Rebuild,
                    )
                    .last_used(accessed.map_or(LastUsed::Unknown, LastUsed::At))
                });
            }
        }
        flush(sink, &mut warnings);
        add_files(s, sink, "xcode", candidates, k)
    }
    fn in_use(&self, s: &Services, _: &Finding, _: ActionKind) -> Option<String> {
        let xcode = s
            .apps
            .running()
            .unwrap_or_default()
            .into_iter()
            .find(|a| a.bundle_id == "com.apple.dt.Xcode")
            .map(|a| format!("Xcode (PID {})", a.pid));
        let building: Vec<String> = orphans::active_tools(s, None)
            .into_iter()
            .filter(|t| t.starts_with("xcodebuild ") || t.starts_with("swift-frontend "))
            .chain(xcode)
            .collect();
        (!building.is_empty()).then(|| {
            format!(
                "Quit Xcode and stop builds before removing its data:{}",
                orphans::list(&building)
            )
        })
    }
}

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
    /// The home-relative folder that holds this location's matches.
    fn root(&self) -> &'static str {
        match self.path.rsplit_once('/') {
            Some((parent, last)) if last.contains('*') => parent,
            _ => self.path,
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
                cache("JavaScript", "Playwright browsers", "Library/Caches/ms-playwright")
                    .note("Browsers are downloaded again by playwright install."),
                cache("JavaScript", "Cypress binaries", "Library/Caches/Cypress")
                    .command("cypress cache clear"),
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
                // Apple platforms
                cache("Xcode", "Xcode cache", "Library/Caches/com.apple.dt.Xcode")
                    .apps(owners::XCODE),
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
                // Flutter and Dart: global tools in bin and global_packages are kept.
                cache("Flutter / Dart", "Pub hosted packages", ".pub-cache/hosted").package_store()
                    .command("dart pub cache clean"),
                cache("Flutter / Dart", "Pub Git packages", ".pub-cache/git").package_store()
                    .command("dart pub cache clean"),
                // JVM and Android: settings such as gradle.properties and settings.xml are kept.
                cache("Gradle", "Gradle caches", ".gradle/caches").package_store().apps(owners::ANDROID),
                cache("Gradle", "Gradle wrapper distributions", ".gradle/wrapper/dists")
                    .apps(owners::ANDROID),
                cache("Gradle", "Gradle daemon logs", ".gradle/daemon").apps(owners::ANDROID),
                cache("Kotlin", "Kotlin/Native toolchains", ".konan").apps(owners::ANDROID),
                cache("Maven", "Maven repository", ".m2/repository").package_store()
                    .note("Artifacts installed locally with mvn install exist nowhere else.")
                    .apps(owners::ANDROID),
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
                cache("VS Code", "VS Code cache", "Library/Application Support/Code/Cache")
                    .apps(owners::VSCODE),
                cache(
                    "VS Code",
                    "VS Code code cache",
                    "Library/Application Support/Code/Code Cache",
                )
                .apps(owners::VSCODE),
                cache(
                    "VS Code",
                    "VS Code cached data",
                    "Library/Application Support/Code/CachedData",
                )
                .apps(owners::VSCODE),
                cache(
                    "VS Code",
                    "VS Code GPU cache",
                    "Library/Application Support/Code/GPUCache",
                )
                .apps(owners::VSCODE),
                cache(
                    "VS Code",
                    "VS Code extension downloads",
                    "Library/Application Support/Code/CachedExtensionVSIXs",
                )
                .apps(owners::VSCODE),
                cache("VS Code", "VS Code logs", "Library/Application Support/Code/logs")
                    .note("Logs can help diagnose problems.")
                    .apps(owners::VSCODE),
                cache("Unity", "Unity package cache", "Library/Unity/cache").apps(owners::UNITY),
            ],
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
    /// Every folder this module reports on, so other modules can leave them to it.
    pub fn covered(&self) -> Vec<String> {
        let home = self.home();
        self.locations
            .iter()
            .map(|l| format!("{home}/{}", l.root()))
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
    let relative = path.strip_prefix(home)?.strip_prefix('/')?;
    locations.iter().find(|l| match l.path.rsplit_once('/') {
        Some((parent, last)) if last.contains('*') => relative
            .rsplit_once('/')
            .is_some_and(|(p, n)| p == parent && glob(last, n).is_some()),
        _ => relative == l.path,
    })
}
/// Existing folders for a location: the path itself, or each match of its last `*`.
fn expand(s: &Services, home: &str, location: &CacheLocation) -> Vec<(Entry, String)> {
    let path = format!("{home}/{}", location.path);
    match location.path.rsplit_once('/') {
        Some((_, last)) if last.contains('*') => {
            let root = format!("{home}/{}", location.root());
            if !s.is_dir(&root) {
                return vec![];
            }
            s.children(&root, &mut vec![])
                .into_iter()
                .filter(|e| e.directory && glob(last, e.name()).is_some())
                .map(|e| {
                    let title = format!("{} · {}", location.name, e.name());
                    (e, title)
                })
                .collect()
        }
        _ => s
            .entry(&path)
            .ok()
            .map(|e| (e, location.name.to_owned()))
            .into_iter()
            .collect(),
    }
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
            let mut reason = format!(
                "{} Close the owning tools before moving this cache to Trash.",
                location
                    .note
                    .unwrap_or("Cached downloads may be needed for offline installs.")
            );
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
        // macOS leftovers, except folders the tool caches above already cover.
        candidates.extend(
            self.mac
                .candidates(s, c, k, &self.home(), &self.covered())?,
        );
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
        // A macOS leftover only needs its app closed; developer tools are irrelevant to it.
        if let Some(result) = self.mac.preflight(s, &self.home(), path) {
            return result.err();
        }
        if let Some(reason) = self
            .location(path)
            .and_then(|l| owners_closed(s, l.apps).err())
        {
            return Some(reason);
        }
        let active = orphans::active_tools(s, None);
        (!active.is_empty()).then(|| {
            format!(
                "Developer tools are running that may be using shared caches:{}",
                orphans::list(&active)
            )
        })
    }
    fn preflight(&self, _: &Services, f: &Finding, _: ActionKind, _: &ScanControl) -> Result<()> {
        let path = f.resource.path().ok_or("Missing path")?;
        if let Some(reason) = self.location(path).and_then(|l| l.blocked) {
            return Err(reason.into());
        }
        Ok(())
    }
}
