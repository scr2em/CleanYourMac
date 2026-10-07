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
pub fn contains(path: &str, root: &str) -> bool {
    Path::new(&canonical(path)).starts_with(canonical(root))
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
    system_excluded(path)
        || Path::new(path).components().any(|c| {
            let name = c.as_os_str().to_string_lossy().to_lowercase();
            DUPLICATE_IGNORES.contains(&name.as_str())
                || DUPLICATE_IGNORE_SUFFIXES
                    .iter()
                    .any(|suffix| name.ends_with(&suffix.to_lowercase()))
        })
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
