use crate::model::{Finding, Resource, ScanContext};
use std::path::{Component, Path, PathBuf};

/// The current user's home folder, without a trailing slash.
pub fn home() -> String {
    let h = std::env::var("HOME").unwrap_or_default();
    match h.trim_end_matches('/') {
        "" if h.starts_with('/') => "/".into(),
        trimmed => trimmed.into(),
    }
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
/// `under`, ignoring ASCII case.
fn under_folded(path: &str, root: &str) -> bool {
    let (p, r) = (path.as_bytes(), root.as_bytes());
    root == "/"
        || (p.len() >= r.len()
            && p[..r.len()].eq_ignore_ascii_case(r)
            && (p.len() == r.len() || p[r.len()] == b'/'))
}
/// `contains`, ignoring ASCII case. Macs usually format drives case-insensitively, so
/// `~/projects/keep` and `~/Projects/keep` name the same folder. Used where matching more is
/// the safe direction: exclusions, system and credential locations.
pub fn contains_folded(path: &str, root: &str) -> bool {
    under_folded(&canonical_cow(path), &canonical_cow(root))
}
impl ScanContext {
    pub fn excludes(&self, path: &str) -> bool {
        self.exclusions.iter().any(|r| contains_folded(path, r))
    }
    pub fn protects(&self, path: &str) -> bool {
        self.excludes(path) || self.exclusions.iter().any(|r| contains_folded(r, path))
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
    SYSTEM_ROOTS.iter().any(|r| contains_folded(path, r)) || sensitive(path)
}
fn sensitive(path: &str) -> bool {
    let h = home();
    SENSITIVE_HOME
        .iter()
        .any(|r| contains_folded(path, &(h.clone() + r)))
        || sensitive_name(path)
}
/// Whether the path holds Git metadata, an environment file, or credentials and synced
/// files in any user's home folder (`.ssh`, `.aws`, `.gnupg`, `Library/Keychains`,
/// `Library/Mobile Documents`, `Library/CloudStorage`), whatever the letter case.
fn sensitive_name(path: &str) -> bool {
    let mut previous = "";
    // Empty and `.` components, as in `Library//Keychains`, don't separate a pair.
    path.split('/')
        .filter(|c| !c.is_empty() && *c != ".")
        .any(|c| {
            let named = |n: &str| c.eq_ignore_ascii_case(n);
            let hit = named(".git")
                || named(".env")
                || c.get(..5).is_some_and(|p| p.eq_ignore_ascii_case(".env."))
                || named(".ssh")
                || named(".aws")
                || named(".gnupg")
                || (previous.eq_ignore_ascii_case("Library")
                    && (named("Keychains") || named("Mobile Documents") || named("CloudStorage")));
            previous = c;
            hit
        })
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
            roots.iter().any(|r| under_folded(path, r))
        } else {
            roots.iter().any(|r| contains_folded(path, r))
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
    .iter()
    // A protected folder, or a folder holding one, such as /Users.
    .any(|q| q.eq_ignore_ascii_case(&p) || (!q.is_empty() && contains_folded(q, &p)))
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
/// Drops repeated IDs and files inside another selected file or folder, keeping order.
pub fn normalized_selection(rows: &[Finding]) -> Vec<Finding> {
    outermost(rows.iter()).into_iter().cloned().collect()
}
/// `normalized_selection` over borrowed rows. Each file's parent folders are looked up in a
/// set of the selected paths, so the cost grows with the selection times the path depth
/// rather than with the square of the selection.
pub fn outermost<'a, I>(rows: I) -> Vec<&'a Finding>
where
    I: Iterator<Item = &'a Finding> + Clone,
{
    let selected: std::collections::HashSet<std::borrow::Cow<str>> = rows
        .clone()
        .filter_map(|f| match &f.resource {
            Resource::File { file } => Some(canonical_cow(&file.path)),
            _ => None,
        })
        .collect();
    let mut seen = std::collections::HashSet::new();
    rows.filter(|f| seen.insert(f.id.as_str()))
        .filter(|f| {
            let Resource::File { file } = &f.resource else {
                return true;
            };
            let path = canonical_cow(&file.path);
            let inside_another = ancestors(&path).any(|a| selected.contains(a));
            !inside_another
        })
        .collect()
}
/// The path itself when it is already canonical, as stored findings are.
fn canonical_cow(path: &str) -> std::borrow::Cow<'_, str> {
    if is_canonical(path) {
        std::borrow::Cow::Borrowed(path)
    } else {
        std::borrow::Cow::Owned(canonical(path))
    }
}
/// The proper ancestors of a canonical path, nearest first, ending with `/`.
fn ancestors(path: &str) -> impl Iterator<Item = &str> {
    let mut end = Some(path.len());
    std::iter::from_fn(move || {
        let cut = path[..end?].rfind('/')?;
        let parent = if cut == 0 { "/" } else { &path[..cut] };
        end = (cut > 0).then_some(cut);
        (parent != path).then_some(parent)
    })
}
/// macOS packages are listed as single items; traversal does not enter them, so nothing
/// inside a library or document bundle is ever offered on its own.
pub const PACKAGE_EXTENSIONS: &[&str] = &[
    "app",
    "appex",
    "bundle",
    "docset",
    "fcpbundle",
    "framework",
    "imovielibrary",
    "kext",
    "key",
    "logicx",
    "musiclibrary",
    "numbers",
    "pages",
    "photolibrary",
    "photoslibrary",
    "playground",
    "plugin",
    "rtfd",
    "sparsebundle",
    "xcarchive",
    "xpc",
];
pub fn package(path: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(|s| s.to_str())
        .is_some_and(|e| PACKAGE_EXTENSIONS.iter().any(|p| p.eq_ignore_ascii_case(e)))
}
