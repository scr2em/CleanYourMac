//! Replaceable boundaries. Scanners, safety checks and actions depend only on these traits,
//! so an adapter (directory walker, hasher, process inspector, …) can be swapped for another
//! implementation or a library without changing discovery or policy code.
use crate::model::*;
use std::{
    fs::Metadata,
    os::unix::fs::MetadataExt,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

/// Cooperative cancellation shared by a scan or action and every adapter it calls.
#[derive(Clone, Default)]
pub struct ScanControl(pub Arc<AtomicBool>);
impl ScanControl {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
    pub fn check(&self) -> Result<()> {
        if self.is_cancelled() {
            Err("Scan cancelled".into())
        } else {
            Ok(())
        }
    }
}

/// Metadata for one path, captured without following a final symbolic link.
#[derive(Clone, Debug)]
pub struct Entry {
    pub identity: FileIdentity,
    pub directory: bool,
    pub regular: bool,
    pub symlink: bool,
    pub bytes: u64,
    pub allocated: u64,
    /// Hard-link count; items with one link cannot be shared with another path.
    pub links: u64,
}
impl Entry {
    /// Builds an entry from `lstat` metadata; adapters backed by other libraries can reuse it.
    pub fn from_metadata(path: String, m: &Metadata) -> Self {
        Self {
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
            links: m.nlink(),
        }
    }
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

/// Path metadata and the few direct mutations the action executor needs.
pub trait FileSystem: Send + Sync {
    /// `lstat` the path. Unless `allow_link`, a symbolic link is an error.
    fn inspect(&self, path: &str, allow_link: bool) -> Result<Entry>;
    /// Immediate children, each with its own error so one unreadable item does not hide the rest.
    fn children(&self, path: &str) -> Result<Vec<Result<Entry>>>;
    /// The physical path with symbolic links resolved, if it exists.
    fn resolve(&self, path: &str) -> Option<String>;
    fn remove(&self, path: &str) -> Result<()>;
    fn rename(&self, from: &str, to: &str) -> Result<()>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Visit {
    Descend,
    Skip,
    Stop,
}
/// Depth-first traversal of one root. Implementations must not follow symbolic links,
/// must honor `Visit::Skip`/`Visit::Stop`, and should report unreadable items via `problem`.
/// Scope, exclusions, package boundaries and entry limits are enforced by the caller.
pub trait Walker: Send + Sync {
    fn walk(
        &self,
        root: &str,
        control: &ScanControl,
        problem: &mut dyn FnMut(String),
        visit: &mut dyn FnMut(&Entry) -> Result<Visit>,
    ) -> Result<()>;
}

#[derive(Clone, Debug, Default)]
pub struct FileSize {
    pub logical: u64,
    pub allocated: u64,
    pub files: u64,
    pub complete: bool,
    /// A fingerprint of the folder's metadata, compared again before acting on it.
    pub signature: Option<String>,
}
/// Recursive sizing. The same instance must be used for scanning and validation because the
/// tree signature format belongs to the implementation.
pub trait Sizer: Send + Sync {
    fn size(&self, path: &str, control: &ScanControl) -> Result<FileSize>;
}

/// Content digest used for duplicate detection. Identity checks around it are done by the
/// caller. `limit` hashes only the first bytes, for cheap candidate filtering.
pub trait Hasher: Send + Sync {
    fn algorithm(&self) -> &'static str;
    fn hash(&self, path: &str, limit: Option<u64>, control: &ScanControl) -> Result<String>;
}

#[derive(Debug, Default)]
pub struct Output {
    pub data: Vec<u8>,
    pub error: String,
    pub status: i32,
}
impl Output {
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.data).into()
    }
}
/// Runs a platform tool with an argument array. Never a shell string.
pub trait CommandRunner: Send + Sync {
    fn run(
        &self,
        executable: &str,
        args: &[String],
        timeout: Duration,
        control: &ScanControl,
    ) -> Result<Output>;
}

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub identity: ProcessIdentity,
    pub parent: i32,
    pub name: String,
    pub command: String,
    pub cwd: Option<String>,
    pub cpu_nanos: Option<u64>,
    pub memory: Option<u64>,
}
pub trait ProcessInspector: Send + Sync {
    fn pids(&self) -> Vec<i32>;
    fn inspect(&self, pid: i32) -> Option<Snapshot>;
    fn signal(&self, pid: i32, force: bool) -> Result<()>;
    fn current_uid(&self) -> u32;
    fn current_pid(&self) -> i32;
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunningApp {
    pub pid: i32,
    #[serde(rename = "bundleID")]
    pub bundle_id: String,
    pub path: String,
}
#[derive(Clone, Debug, Default)]
pub struct BundleInfo {
    pub identifier: String,
    pub version: String,
}
pub trait Applications: Send + Sync {
    fn running(&self) -> Result<Vec<RunningApp>>;
    fn bundle_info(&self, app_path: &str) -> BundleInfo;
}

/// Moves one item to the user's Trash and returns its new location.
pub trait Trash: Send + Sync {
    fn move_to_trash(&self, path: &str) -> Result<String>;
}

/// Durable local history of action outcomes, newest first.
pub trait Journal: Send + Sync {
    fn read(&self) -> Vec<ActionResult>;
    fn append(&self, results: &[ActionResult]) -> Result<()>;
    fn clear(&self) -> Result<()>;
}

/// Receives scan output as it is produced.
pub trait Sink {
    fn finding(&mut self, finding: Finding);
    fn warning(&mut self, warning: String);
    fn progress(&mut self, _message: String) {}
}
impl Sink for ScanReport {
    fn finding(&mut self, finding: Finding) {
        self.findings.push(finding);
    }
    fn warning(&mut self, warning: String) {
        if self.warnings.len() < 200 {
            self.warnings.push(warning);
        }
    }
}
