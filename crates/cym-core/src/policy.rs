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
pub const DUPLICATE_IGNORES: &[&str] = &[
    "node_modules",
    "packages",
    ".git",
    "build",
    ".build",
    "dist",
    "out",
    "target",
    "bin",
    "obj",
    ".next",
    ".nuxt",
    ".turbo",
    ".parcel-cache",
    "coverage",
    ".cache",
    ".gradle",
    ".m2",
    "vendor",
    "pods",
    "carthage",
    ".swiftpm",
    "deriveddata",
    ".venv",
    "venv",
    "env",
    "__pycache__",
    ".tox",
    ".nox",
    ".pytest_cache",
    ".mypy_cache",
    ".ruff_cache",
    "site-packages",
    ".dart_tool",
    ".pub-cache",
    ".pub",
    ".bundle",
    ".cargo",
    ".cabal",
    "_build",
    "deps",
    "bower_components",
];
pub fn duplicate_excluded(path: &str) -> bool {
    system_excluded(path)
        || Path::new(path).components().any(|c| {
            DUPLICATE_IGNORES.contains(&c.as_os_str().to_string_lossy().to_lowercase().as_str())
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
