//! What a folder inside a project is: build output, installed packages, or the project's own
//! files. Build Artifacts, Dependencies, Large Files, Exact Duplicates, Git Worktrees and the
//! "last used" date of a project all ask here, so they agree on which folders a build or a
//! package manager recreates. A folder name alone is never evidence, except for the package
//! trees below, which hold nothing else wherever they appear.
use super::{glob, owners};
use crate::{model::*, policy, ports::*, services::Services};
use std::sync::OnceLock;

/// Folders that only ever hold installed packages: `node_modules`, Python's `site-packages`,
/// pnpm's virtual store and content store, npm's cache and npx folders. Matched in any case.
pub const PACKAGE_TREES: &[&str] = &[
    "node_modules",
    "site-packages",
    ".pnpm",
    ".pnpm-store",
    "_cacache",
    "_npx",
];
/// Files beside a `node_modules` that name its project, with the runtime they belong to.
pub const NODE_MANIFESTS: &[(&str, &str)] = &[
    ("Node.js", "package.json"),
    ("Deno", "deno.json"),
    ("Deno", "deno.jsonc"),
];

/// Whether one path component names a package tree.
pub fn package_tree(name: &str) -> bool {
    PACKAGE_TREES.iter().any(|t| t.eq_ignore_ascii_case(name))
}
/// Whether a project's own files, not what a build or package manager made, include an
/// immediate entry named `name`: generated folders by their usual names, and package trees,
/// are left out. Inside a known project a name is enough evidence; elsewhere it is not.
pub fn generated_name(name: &str) -> bool {
    package_tree(name) || policy::generated_name(name) || policy::generated_suffix(name)
}

fn parent(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(p, _)| p)
}
fn last(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// The rule tables, built once. Every module reads the same copy.
pub struct ProjectFolders {
    pub build_output: Vec<ArtifactRule>,
    pub dependencies: Vec<ArtifactRule>,
    /// Names, or `*` patterns, of the files and folders that make a folder a project: every
    /// rule's evidence and required files, and the `node_modules` manifests.
    markers: Vec<(&'static str, bool)>,
}
/// The shared rule tables.
pub fn rules() -> &'static ProjectFolders {
    static RULES: OnceLock<ProjectFolders> = OnceLock::new();
    RULES.get_or_init(ProjectFolders::new)
}
impl ProjectFolders {
    fn new() -> Self {
        let build_output = build_output_rules();
        let dependencies = dependency_rules();
        let mut markers: Vec<(&'static str, bool)> = vec![];
        let files = build_output
            .iter()
            .chain(&dependencies)
            .flat_map(|r| r.evidence.iter().chain(r.requires))
            .copied()
            .chain(NODE_MANIFESTS.iter().map(|(_, file)| *file));
        for file in files {
            // Only the first component is looked for: `.cargo/config.toml` needs `.cargo`.
            let folder = file.contains('/');
            let first = file.split('/').next().unwrap_or(file);
            if !first.is_empty() && !markers.contains(&(first, folder)) {
                markers.push((first, folder));
            }
        }
        Self {
            build_output,
            dependencies,
            markers,
        }
    }
    /// The build-output rule matching the folder at `path`.
    pub fn build_output(&self, s: &Services, path: &str) -> Option<ArtifactMatch<'_>> {
        match_rules(&self.build_output, s, path)
    }
    /// The installed-packages rule matching the folder at `path`, other than `node_modules`.
    pub fn dependency(&self, s: &Services, path: &str) -> Option<ArtifactMatch<'_>> {
        match_rules(&self.dependencies, s, path)
    }
    /// The runtime of the project that the `node_modules` in `project` belongs to.
    pub fn node_runtime(&self, s: &Services, project: &str) -> Option<&'static str> {
        NODE_MANIFESTS
            .iter()
            .find(|(_, file)| s.is_file(&format!("{project}/{file}")))
            .map(|(runtime, _)| *runtime)
    }
    /// Whether the folder at `path` is one Build Artifacts or Dependencies offers whole:
    /// a project's `node_modules`, other installed packages, or build output. Removing one
    /// file from inside leaves a broken build or install, so file tools neither list nor
    /// search inside it. A `node_modules` with no project beside it is not offered, so it is
    /// not skipped either.
    pub fn rebuildable(&self, s: &Services, path: &str) -> bool {
        if last(path) == "node_modules" {
            return self.node_runtime(s, parent(path)).is_some();
        }
        self.dependency(s, path).is_some() || self.build_output(s, path).is_some()
    }
    /// Whether `folder` holds a project: a manifest, build file or lockfile any rule knows, or
    /// a version-control checkout.
    pub fn is_project(&self, s: &Services, folder: &str) -> bool {
        s.children(folder, &mut vec![]).iter().any(|e| {
            policy::checkout_marker(e.name()).is_some()
                || self.markers.iter().any(|(pattern, folder)| {
                    (!folder || e.directory) && glob(pattern, e.name()).is_some()
                })
        })
    }
    /// Whether a search of the user's own files skips the folder at `path`: a package tree,
    /// a rebuildable folder, or a folder with a generated folder's usual name in a project.
    /// A `Documents/Build` or `Music/Packages` folder is still the user's.
    pub fn generated(&self, s: &Services, path: &str) -> bool {
        let name = last(path);
        package_tree(name)
            || self.rebuildable(s, path)
            || policy::generated_suffix(name)
            || (policy::generated_name(name) && self.is_project(s, parent(path)))
    }
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

/// Build output, identified by folder name and owning-project evidence. Mobile projects
/// with nested native folders come before generic rules; the first rule with evidence wins.
pub fn build_output_rules() -> Vec<ArtifactRule> {
    vec![
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
    ]
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

/// The first of `rules` whose names and evidence match the folder at `path`.
pub fn match_rules<'a>(
    rules: &'a [ArtifactRule],
    s: &Services,
    path: &str,
) -> Option<ArtifactMatch<'a>> {
    // Nearly every folder a walk visits matches no rule's last name; settle that without
    // allocating, before splitting paths or patterns.
    let name = path.rsplit('/').next()?;
    if !rules.iter().any(|rule| {
        rule.names
            .iter()
            .any(|p| glob(p.rsplit('/').next().unwrap_or(p), name).is_some())
    }) {
        return None;
    }
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
                || !all(containing, rule.beside)
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
