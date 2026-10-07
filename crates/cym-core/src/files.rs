use crate::{model::*, policy};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs,
    io::Read,
    os::unix::fs::MetadataExt,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

#[derive(Clone, Default)]
pub struct ScanControl(pub Arc<AtomicBool>);
impl ScanControl {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }
    pub fn check(&self) -> Result<()> {
        if self.0.load(Ordering::Relaxed) {
            Err("Scan cancelled".into())
        } else {
            Ok(())
        }
    }
}
#[derive(Clone, Debug)]
pub struct Entry {
    pub identity: FileIdentity,
    pub directory: bool,
    pub regular: bool,
    pub symlink: bool,
    pub bytes: u64,
    pub allocated: u64,
}
impl Entry {
    pub fn path(&self) -> &str {
        &self.identity.path
    }
    pub fn name(&self) -> &str {
        Path::new(self.path())
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
    }
    pub fn modified(&self) -> f64 {
        self.identity.modified_seconds as f64 + self.identity.modified_nanos as f64 / 1e9
    }
}
pub fn entry(path: &str) -> Result<Entry> {
    inspect(path, false)
}
fn inspect(path: &str, allow_link: bool) -> Result<Entry> {
    let path = policy::canonical(path);
    let m = fs::symlink_metadata(&path).map_err(|e| format!("Cannot inspect {path}: {e}"))?;
    if m.file_type().is_symlink() && !allow_link {
        return Err(format!("Symbolic links are not traversed: {path}"));
    }
    Ok(Entry {
        identity: FileIdentity {
            path,
            device: m.dev(),
            inode: m.ino(),
            modified_seconds: m.mtime(),
            modified_nanos: m.mtime_nsec(),
            tree_signature: None,
        },
        directory: m.is_dir(),
        regular: m.is_file(),
        symlink: m.file_type().is_symlink(),
        bytes: m.size(),
        allocated: m.blocks().saturating_mul(512),
    })
}
pub fn roots(paths: &[String]) -> Vec<String> {
    let mut output = vec![];
    let mut physical = HashSet::new();
    for path in paths {
        let path = policy::canonical(path);
        let path = fs::canonicalize(&path)
            .ok()
            .and_then(|p| p.to_str().map(str::to_owned))
            .unwrap_or(path);
        if let Ok(e) = entry(&path) {
            if !physical.insert((e.identity.device, e.identity.inode)) {
                continue;
            }
        }
        if !output.contains(&path) {
            output.push(path);
        }
    }
    output.retain(|p| {
        !paths
            .iter()
            .map(|r| {
                fs::canonicalize(policy::canonical(r))
                    .ok()
                    .and_then(|p| p.to_str().map(str::to_owned))
                    .unwrap_or_else(|| policy::canonical(r))
            })
            .any(|r| r != *p && policy::contains(p, &r))
    });
    output.sort();
    output
}
pub fn children(path: &str, warnings: &mut Vec<String>) -> Vec<Entry> {
    match fs::read_dir(path) {
        Ok(iter) => iter
            .filter_map(|value| match value {
                Ok(value) => match value
                    .path()
                    .to_str()
                    .ok_or("Non-UTF-8 path")
                    .and_then(|p| entry(p).map_err(|_| "Unavailable or symbolic path"))
                {
                    Ok(e) => Some(e),
                    Err(e) => {
                        warn(warnings, format!("Skipped {}: {e}", value.path().display()));
                        None
                    }
                },
                Err(e) => {
                    warn(warnings, format!("Cannot enumerate {path}: {e}"));
                    None
                }
            })
            .collect(),
        Err(e) => {
            warn(warnings, format!("Cannot read {path}: {e}"));
            vec![]
        }
    }
}
pub fn warn(warnings: &mut Vec<String>, warning: String) {
    if warnings.len() < 200 {
        warnings.push(warning);
    }
}
fn package(path: &str) -> bool {
    [
        "app",
        "bundle",
        "framework",
        "xcarchive",
        "photoslibrary",
        "playground",
    ]
    .contains(
        &Path::new(path)
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or(""),
    )
}
pub fn walk(
    context: &ScanContext,
    control: &ScanControl,
    warnings: &mut Vec<String>,
    mut visit: impl FnMut(&Entry) -> Result<bool>,
) -> Result<()> {
    let mut stack = vec![];
    let mut count = 0;
    for root in roots(&context.roots) {
        if context.excludes(&root) || policy::system_excluded(&root) {
            continue;
        }
        match entry(&root) {
            Ok(e) if e.directory => stack.extend(children(&root, warnings)),
            _ => warn(warnings, format!("Root is unavailable: {root}")),
        }
    }
    while let Some(e) = stack.pop() {
        control.check()?;
        count += 1;
        if count > 300_000 {
            warn(
                warnings,
                "Scan reached its 300000-entry limit; choose narrower folders.".into(),
            );
            break;
        }
        if context.excludes(e.path()) || policy::system_excluded(e.path()) {
            continue;
        }
        let descend = visit(&e)?;
        if descend && e.directory && !package(e.path()) {
            stack.extend(children(e.path(), warnings));
        }
    }
    Ok(())
}
#[derive(Debug, Default)]
pub struct FileSize {
    pub logical: u64,
    pub allocated: u64,
    pub files: u64,
    pub complete: bool,
    pub signature: Option<String>,
}
pub fn size(path: &str, control: &ScanControl) -> Result<FileSize> {
    let root = entry(path)?;
    if !root.directory {
        return Ok(FileSize {
            logical: root.bytes,
            allocated: root.allocated,
            files: 1,
            complete: true,
            signature: None,
        });
    }
    let mut output = FileSize {
        complete: true,
        ..Default::default()
    };
    let mut stack = vec![root.identity.path.clone()];
    let mut count = 0;
    let mut physical = HashSet::new();
    let mut digest = [0u8; 32];
    while let Some(directory) = stack.pop() {
        control.check()?;
        let iter = match fs::read_dir(&directory) {
            Ok(v) => v,
            Err(_) => {
                output.complete = false;
                continue;
            }
        };
        for item in iter {
            control.check()?;
            if count >= 500_000 {
                output.complete = false;
                stack.clear();
                break;
            }
            let item = match item {
                Ok(v) => v,
                Err(_) => {
                    output.complete = false;
                    continue;
                }
            };
            let Some(path) = item.path().to_str().map(str::to_owned) else {
                output.complete = false;
                continue;
            };
            let e = match inspect(&path, true) {
                Ok(e) => e,
                Err(_) => {
                    output.complete = false;
                    continue;
                }
            };
            count += 1;
            let relative = e.path().strip_prefix(root.path()).unwrap_or(e.path());
            let metadata = format!(
                "{}\0{}\0{}\0{}\0{}\0{}\0{}\0{}",
                relative,
                e.identity.device,
                e.identity.inode,
                e.identity.modified_seconds,
                e.identity.modified_nanos,
                e.bytes,
                e.directory,
                e.symlink
            );
            for (i, byte) in Sha256::digest(metadata.as_bytes()).iter().enumerate() {
                digest[i] ^= byte;
            }
            if e.directory && !e.symlink {
                stack.push(path);
            }
            if e.regular || e.symlink {
                output.files += 1;
                output.logical = output.logical.saturating_add(e.bytes);
                if physical.insert((e.identity.device, e.identity.inode)) {
                    output.allocated = output.allocated.saturating_add(e.allocated);
                }
            }
        }
    }
    output.signature = Some(format!("{count}:{}", hex(&digest)));
    Ok(output)
}
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
pub fn hash(path: &str, control: &ScanControl) -> Result<String> {
    let before = entry(path)?;
    if !before.regular {
        return Err("Only regular files can be hashed".into());
    }
    let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut digest = Sha256::new();
    let mut buffer = vec![0; 1_048_576];
    loop {
        control.check()?;
        let count = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    if entry(path)?.identity != before.identity {
        return Err("File changed during duplicate verification".into());
    }
    Ok(hex(&digest.finalize()))
}
pub fn no_symlink_ancestors(path: &str) -> Result<()> {
    let canonical = policy::canonical(path);
    let mut ancestor = std::path::PathBuf::from("/");
    for component in Path::new(&canonical).components() {
        if let std::path::Component::Normal(value) = component {
            ancestor.push(value);
            entry(ancestor.to_str().ok_or("Invalid path")?)?;
        }
    }
    Ok(())
}
pub fn snapshot(path: &str, control: &ScanControl) -> Result<FileIdentity> {
    let mut e = entry(path)?;
    if e.directory {
        let size = size(path, control)?;
        if !size.complete {
            return Err("Folder coverage is incomplete".into());
        }
        e.identity.tree_signature = size.signature;
    }
    Ok(e.identity)
}
pub fn validate_identity(expected: &FileIdentity, control: &ScanControl) -> Result<()> {
    let mut base = expected.clone();
    base.tree_signature = None;
    if entry(&expected.path)?.identity != base {
        return Err("Item changed after scanning. Scan again before acting.".into());
    }
    if let Some(signature) = &expected.tree_signature {
        let size = size(&expected.path, control)?;
        if !size.complete || size.signature.as_ref() != Some(signature) {
            return Err("Folder contents changed or cannot be verified. Scan again.".into());
        }
    }
    Ok(())
}
pub fn validate(
    expected: &FileIdentity,
    context: &ScanContext,
    additional: &[String],
    control: &ScanControl,
) -> Result<()> {
    if policy::protected(&expected.path) || context.protects(&expected.path) {
        return Err("Path or one of its descendants is protected or excluded".into());
    }
    if !context.allows(&expected.path)
        || !context
            .roots
            .iter()
            .chain(additional)
            .any(|r| policy::contains(&expected.path, r))
    {
        return Err("Item is outside the current approved scope".into());
    }
    no_symlink_ancestors(&expected.path)?;
    validate_identity(expected, control)
}
pub fn finding(
    e: &Entry,
    module: &str,
    title: Option<&str>,
    reason: &str,
    actions: Vec<ActionKind>,
    risk: Risk,
    control: &ScanControl,
) -> Result<Finding> {
    let size = size(e.path(), control)?;
    let mut fresh = entry(e.path())?;
    fresh.identity.tree_signature = size.signature;
    let blocked = if policy::protected(e.path()) {
        Some("Protected path; inspect manually".into())
    } else if !size.complete {
        Some("Incomplete folder coverage; inspect manually".into())
    } else {
        None
    };
    let mut row = Finding::new(
        module,
        e.path(),
        title.unwrap_or(e.name()),
        Resource::File {
            file: fresh.identity,
        },
        reason,
    );
    row.bytes = Some(size.logical);
    row.allocated_bytes = Some(size.allocated);
    row.modified_at = Some(e.modified());
    row.details = vec![
        detail("Files", size.files.to_string()),
        detail(
            "Size coverage",
            if size.complete {
                "Complete"
            } else {
                "Partial estimate"
            },
        ),
    ];
    row.actions = if blocked.is_none() { actions } else { vec![] };
    row.risk = risk;
    row.blocked_reason = blocked;
    Ok(row)
}
