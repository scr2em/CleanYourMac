use crate::model::{Finding, Resource, ScanContext};
use std::path::{Component, Path, PathBuf};

pub fn home() -> String {
    std::env::var("HOME").unwrap_or_default()
}
pub fn canonical(path: &str) -> String {
    let expanded = if path == "~" {
        home()
    } else if let Some(suffix) = path.strip_prefix("~/") {
        format!("{}/{}", home(), suffix)
    } else {
        path.into()
    };
    let path = Path::new(&expanded);
    let mut output = if path.is_absolute() {
        PathBuf::from("/")
    } else {
        std::env::current_dir().unwrap_or_default()
    };
    for component in path.components() {
        match component {
            Component::ParentDir => {
                output.pop();
            }
            Component::Normal(value) => output.push(value),
            _ => {}
        }
    }
    output.to_string_lossy().into()
}
/// Whether `path` is `root` or inside it, comparing whole components lexically.
pub fn contains(path: &str, root: &str) -> bool {
    if is_canonical(path) && is_canonical(root) {
        return under(path, root);
    }
    Path::new(&canonical(path)).starts_with(canonical(root))
}
/// An absolute path with no empty, `.` or `..` components and no trailing slash: what
/// `canonical` returns, and what traversal produces below a canonical root.
fn is_canonical(path: &str) -> bool {
    path == "/"
        || (path.starts_with('/')
            && !path[1..]
                .split('/')
                .any(|c| c.is_empty() || c == "." || c == ".."))
}
/// `contains` for two canonical paths: a byte prefix that ends at a component boundary.
fn under(path: &str, root: &str) -> bool {
    root == "/"
        || (path.starts_with(root)
            && (path.len() == root.len() || path.as_bytes()[root.len()] == b'/'))
}
impl ScanContext {
    pub fn excludes(&self, path: &str) -> bool {
        self.exclusions.iter().any(|r| contains(path, r))
    }
    pub fn protects(&self, path: &str) -> bool {
        self.excludes(path) || self.exclusions.iter().any(|r| contains(r, path))
    }
    pub fn allows(&self, path: &str) -> bool {
        !self.excludes(path)
            && (!self.limit_to_roots || self.roots.iter().any(|r| contains(path, r)))
    }
}
/// Operating-system locations no scan enters.
const SYSTEM_ROOTS: &[&str] = &[
    "/System", "/Library", "/bin", "/sbin", "/usr", "/dev", "/Network", "/private", "/var", "/etc",
    "/tmp",
];
/// Credential and sync locations in the home folder no scan enters.
const SENSITIVE_HOME: &[&str] = &[
    "/.ssh",
    "/.aws",
    "/.gnupg",
    "/Library/Keychains",
    "/Library/Mobile Documents",
    "/Library/CloudStorage",
];
pub fn system_excluded(path: &str) -> bool {
    SYSTEM_ROOTS.iter().any(|r| contains(path, r)) || sensitive(path)
}
fn sensitive(path: &str) -> bool {
    let h = home();
    SENSITIVE_HOME
        .iter()
        .any(|r| contains(path, &(h.clone() + r)))
        || sensitive_name(path)
}
/// Whether any component is Git metadata or an environment file.
fn sensitive_name(path: &str) -> bool {
    path.split('/')
        .any(|c| c == ".git" || c == ".env" || c.starts_with(".env."))
}

/// The exclusion and system-location rules for one scan, prepared once so each visited
/// entry is checked without allocating. Gives the same answers as `ScanContext::excludes`
/// and `system_excluded`.
pub struct Scope {
    exclusions: Vec<String>,
    system: Vec<String>,
}
impl Scope {
    pub fn new(context: &ScanContext) -> Self {
        let h = home();
        Self {
            exclusions: context.exclusions.iter().map(|e| canonical(e)).collect(),
            system: SYSTEM_ROOTS
                .iter()
                .map(|r| (*r).to_owned())
                .chain(SENSITIVE_HOME.iter().map(|r| canonical(&(h.clone() + r))))
                .collect(),
        }
    }
    fn inside(path: &str, roots: &[String]) -> bool {
        if is_canonical(path) {
            roots.iter().any(|r| under(path, r))
        } else {
            roots.iter().any(|r| contains(path, r))
        }
    }
    pub fn excludes(&self, path: &str) -> bool {
        Self::inside(path, &self.exclusions)
    }
    pub fn system_excluded(&self, path: &str) -> bool {
        Self::inside(path, &self.system) || sensitive_name(path)
    }
}
pub fn protected(path: &str) -> bool {
    let p = canonical(path);
    let h = home();
    [
        "/",
        &h,
        &(h.clone() + "/Library"),
        &(h.clone() + "/Documents"),
        &(h.clone() + "/Desktop"),
        &(h.clone() + "/Downloads"),
        "/Applications",
        &(h.clone() + "/Applications"),
    ]
    .contains(&p.as_str())
        || system_excluded(&p)
}
/// Generated output, dependency stores and tool caches across ecosystems. Duplicate
/// detection never descends into these (matched case-insensitively, at any depth).
pub const DUPLICATE_IGNORES: &[&str] = &[
    // Version control
    ".git",
    ".hg",
    ".svn",
    ".jj",
    // JavaScript and TypeScript
    "node_modules",
    "bower_components",
    "jspm_packages",
    "web_modules",
    ".pnpm-store",
    ".yarn",
    ".npm",
    ".angular",
    ".next",
    ".nuxt",
    ".output",
    ".svelte-kit",
    ".vite",
    ".turbo",
    ".parcel-cache",
    ".expo",
    ".docusaurus",
    ".astro",
    ".vercel",
    ".netlify",
    ".serverless",
    "storybook-static",
    ".nyc_output",
    "coverage",
    ".sass-cache",
    "dist",
    "out",
    "build",
    // Swift, Objective-C and Xcode
    ".build",
    ".swiftpm",
    "deriveddata",
    "pods",
    "carthage",
    "xcuserdata",
    // Rust, Go, Zig, C and C++
    "target",
    ".cargo",
    "zig-cache",
    ".zig-cache",
    "zig-out",
    "cmake-build-debug",
    "cmake-build-release",
    "cmakefiles",
    ".cxx",
    ".ccls-cache",
    ".clangd",
    "bazel-bin",
    "bazel-out",
    "bazel-testlogs",
    "buck-out",
    // JVM and Android
    ".gradle",
    ".m2",
    ".ivy2",
    ".sbt",
    ".bloop",
    ".metals",
    ".kotlin",
    ".externalnativebuild",
    "bin",
    "obj",
    // .NET
    "packages",
    ".vs",
    // Python
    "__pycache__",
    "__pypackages__",
    ".venv",
    "venv",
    "env",
    ".tox",
    ".nox",
    ".eggs",
    ".pytest_cache",
    ".mypy_cache",
    ".ruff_cache",
    ".hypothesis",
    ".ipynb_checkpoints",
    "site-packages",
    "htmlcov",
    // Ruby, PHP, Elixir, Erlang, Haskell, Elm, Dart
    ".bundle",
    "vendor",
    "_build",
    "deps",
    ".elixir_ls",
    ".stack-work",
    "dist-newstyle",
    ".cabal",
    "elm-stuff",
    ".dart_tool",
    ".pub-cache",
    ".pub",
    // Infrastructure and general caches
    ".terraform",
    ".aws-sam",
    ".cache",
    ".tmp",
];
/// Name suffixes treated like `DUPLICATE_IGNORES` entries.
pub const DUPLICATE_IGNORE_SUFFIXES: &[&str] = &[".egg-info", ".xcarchive", ".dSYM"];
pub fn duplicate_excluded(path: &str) -> bool {
    system_excluded(path) || path.split('/').any(duplicate_ignored_name)
}
/// Whether one path component names a folder duplicate detection skips; compared without
/// allocating, ignoring ASCII case.
pub fn duplicate_ignored_name(name: &str) -> bool {
    !name.is_empty()
        && (DUPLICATE_IGNORES
            .iter()
            .any(|ignored| ignored.eq_ignore_ascii_case(name))
            || DUPLICATE_IGNORE_SUFFIXES.iter().any(|suffix| {
                name.len() >= suffix.len()
                    && name.is_char_boundary(name.len() - suffix.len())
                    && name[name.len() - suffix.len()..].eq_ignore_ascii_case(suffix)
            }))
}
pub fn normalized_selection(rows: &[Finding]) -> Vec<Finding> {
    let mut seen = std::collections::HashSet::new();
    rows.iter()
        .filter(|f| seen.insert(f.id.clone()))
        .filter(|f| {
            let Resource::File { file } = &f.resource else {
                return true;
            };
            !rows.iter().any(|other| match &other.resource {
                Resource::File { file: parent } => {
                    parent.path != file.path && contains(&file.path, &parent.path)
                }
                _ => false,
            })
        })
        .cloned()
        .collect()
}
/// macOS packages are listed as single items; traversal does not enter them.
pub const PACKAGE_EXTENSIONS: &[&str] = &[
    "app",
    "bundle",
    "framework",
    "xcarchive",
    "photoslibrary",
    "playground",
];
pub fn package(path: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(|s| s.to_str())
        .is_some_and(|e| PACKAGE_EXTENSIONS.contains(&e))
}
