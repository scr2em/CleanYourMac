//! The adapter bundle every scanner and action receives, plus the scope and identity rules
//! built on top of it. Replace any field to swap an implementation:
//!
//! ```ignore
//! let services = Services { walker: Arc::new(MyWalker), ..Services::native() };
//! ```
use crate::{
    adapters::{bulk, command, fs, journal, native},
    model::*,
    policy,
    ports::*,
};
use std::{
    collections::{HashMap, HashSet},
    path::Path,
    sync::{Arc, Mutex},
    time::Instant,
};

/// CPU samples and graceful-termination requests for orphan processes, kept between scans.
#[derive(Default)]
pub struct ProcessState {
    pub samples: HashMap<ProcessIdentity, (u64, Instant)>,
    pub requests: HashMap<ProcessIdentity, Instant>,
}

#[derive(Clone)]
pub struct Services {
    pub fs: Arc<dyn FileSystem>,
    pub walker: Arc<dyn Walker>,
    pub sizer: Arc<dyn Sizer>,
    pub hasher: Arc<dyn Hasher>,
    pub commands: Arc<dyn CommandRunner>,
    pub processes: Arc<dyn ProcessInspector>,
    pub apps: Arc<dyn Applications>,
    pub trash: Arc<dyn Trash>,
    pub usage: Arc<dyn Usage>,
    pub journal: Arc<dyn Journal>,
    /// CPU samples and graceful-termination requests shared across orphan refreshes.
    pub process_state: Arc<Mutex<ProcessState>>,
    /// Maximum entries visited by one module's traversal.
    pub walk_limit: usize,
    /// Maximum findings one module may report in a scan.
    pub result_limit: usize,
    /// Pool for blocking filesystem work (walking, sizing, hashing), kept apart from the
    /// global pool that answers queries so the UI stays responsive during scans.
    pub io: Arc<rayon::ThreadPool>,
}
/// Up to eight I/O threads: beyond that, measured filesystem walks stop getting faster.
fn io_pool() -> rayon::ThreadPool {
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get().clamp(2, 8));
    rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .thread_name(|i| format!("cym-io-{i}"))
        .build()
        .or_else(|_| rayon::ThreadPoolBuilder::new().num_threads(1).build())
        .expect("a single-thread pool can always be built")
}
impl Services {
    pub fn native() -> Self {
        Self::with_journal(Arc::new(journal::JsonJournal::new(
            journal::JsonJournal::default_path(),
        )))
    }
    pub fn with_journal(journal: Arc<dyn Journal>) -> Self {
        let files: Arc<dyn FileSystem> = Arc::new(bulk::BulkFileSystem);
        let io = Arc::new(io_pool());
        Self {
            walker: Arc::new(fs::PrefetchWalker::new(files.clone(), Some(io.clone()))),
            sizer: Arc::new(fs::MetadataSizer::new(files.clone()).on(io.clone())),
            fs: files,
            hasher: Arc::new(fs::Blake3Hasher),
            commands: Arc::new(command::SystemRunner),
            processes: Arc::new(native::LibprocInspector),
            apps: Arc::new(native::NativeApplications),
            trash: Arc::new(native::NativeTrash),
            usage: Arc::new(native::SpotlightUsage),
            journal,
            process_state: Default::default(),
            walk_limit: 20_000_000,
            result_limit: 2_000_000,
            io,
        }
    }

    pub fn entry(&self, path: &str) -> Result<Entry> {
        self.fs.inspect(path, false)
    }
    pub fn exists(&self, path: &str) -> bool {
        self.fs.inspect(path, true).is_ok()
    }
    pub fn is_file(&self, path: &str) -> bool {
        self.entry(path).is_ok_and(|e| e.regular)
    }
    pub fn is_dir(&self, path: &str) -> bool {
        self.entry(path).is_ok_and(|e| e.directory)
    }
    /// A small text file such as a tool's settings, or `None` when it is missing or unreadable.
    pub fn read_text(&self, path: &str) -> Option<String> {
        self.fs
            .read(path, 256 * 1024)
            .ok()
            .map(|data| String::from_utf8_lossy(&data).into_owned())
    }

    /// The context with every root and exclusion also spelled the way the disk resolves it,
    /// so an exclusion typed through a symbolic link or firmlink (`/tmp`, a linked `~/Code`)
    /// still matches the resolved paths a scan produces.
    pub fn scoped(&self, context: &ScanContext) -> ScanContext {
        let both = |paths: &[String]| {
            let mut out: Vec<String> = vec![];
            for path in paths {
                let path = policy::canonical(path);
                let resolved = self.fs.resolve(&path);
                for p in [Some(path), resolved].into_iter().flatten() {
                    if !out.contains(&p) {
                        out.push(p);
                    }
                }
            }
            out
        };
        ScanContext {
            roots: both(&context.roots),
            exclusions: both(&context.exclusions),
            ..context.clone()
        }
    }

    /// Canonical, physically distinct roots with nested roots folded into their ancestors.
    pub fn roots(&self, paths: &[String]) -> Vec<String> {
        let resolved: Vec<String> = paths
            .iter()
            .map(|p| {
                let p = policy::canonical(p);
                self.fs.resolve(&p).unwrap_or(p)
            })
            .collect();
        let mut output: Vec<String> = vec![];
        let mut physical = HashSet::new();
        for path in &resolved {
            if let Ok(e) = self.entry(path) {
                if !physical.insert((e.identity.device, e.identity.inode)) {
                    continue;
                }
            }
            if !output.contains(path) {
                output.push(path.clone());
            }
        }
        let all = output.clone();
        output.retain(|p| !all.iter().any(|r| r != p && policy::contains(p, r)));
        output.sort();
        output
    }

    /// Immediate children of a known folder. Symbolic links are omitted; unreadable items warn.
    pub fn children(&self, path: &str, warnings: &mut Vec<String>) -> Vec<Entry> {
        match self.fs.children(path) {
            Ok(rows) => rows
                .into_iter()
                .filter_map(|row| match row {
                    Ok(e) if e.symlink => None,
                    Ok(e) => Some(e),
                    Err(e) => {
                        warnings.push(e);
                        None
                    }
                })
                .collect(),
            Err(e) => {
                warnings.push(e);
                vec![]
            }
        }
    }

    /// Walks the context's roots through the configured `Walker`, enforcing exclusions, system
    /// locations, package boundaries and the entry limit. `visit` returns whether to descend.
    pub fn walk(
        &self,
        context: &ScanContext,
        control: &ScanControl,
        warnings: &mut Vec<String>,
        visit: &mut dyn FnMut(&Entry) -> Result<bool>,
    ) -> Result<()> {
        let mut count = 0usize;
        // Prepared once: every visited entry is checked against these.
        let scope = policy::Scope::new(context);
        for root in self.roots(&context.roots) {
            control.check()?;
            if context.excludes(&root) {
                continue;
            }
            if policy::system_excluded(&root) {
                warnings.push(format!(
                    "Skipped {root}: system and credential locations are never scanned."
                ));
                continue;
            }
            let Some(device) = self
                .entry(&root)
                .ok()
                .filter(|e| e.directory)
                .map(|e| e.identity.device)
            else {
                warnings.push(format!("Root is unavailable: {root}"));
                continue;
            };
            let mut limited = false;
            let mut mounts = vec![];
            self.walker.walk(
                &root,
                control,
                &mut |problem| warnings.push(problem),
                &mut |e| {
                    count += 1;
                    if count > self.walk_limit {
                        limited = true;
                        return Ok(Visit::Stop);
                    }
                    if e.symlink
                        || e.dataless
                        || scope.excludes(e.path())
                        || scope.system_excluded(e.path())
                    {
                        return Ok(Visit::Skip);
                    }
                    // Another drive, disk image or network share mounted inside the root is
                    // never entered; choose it as a folder to scan it.
                    if e.directory && e.identity.device != device {
                        mounts.push(e.path().to_owned());
                        return Ok(Visit::Skip);
                    }
                    Ok(if visit(e)? && e.directory && !policy::package(e.path()) {
                        Visit::Descend
                    } else {
                        Visit::Skip
                    })
                },
            )?;
            for mount in mounts {
                warnings.push(format!(
                    "Skipped {mount}: it is on another drive. Choose it as a folder to scan it."
                ));
            }
            if limited {
                warnings.push(format!(
                    "Scan reached its {}-entry limit; choose narrower folders.",
                    self.walk_limit
                ));
                break;
            }
        }
        Ok(())
    }

    /// Hashes a regular file and fails if its identity changed while it was read.
    pub fn verified_hash(&self, path: &str, control: &ScanControl) -> Result<String> {
        let before = self.entry(path)?;
        if !before.regular {
            return Err("Only regular files can be hashed".into());
        }
        if before.dataless {
            return Err("File content is in iCloud only; hashing would download it".into());
        }
        let hash = self.hasher.hash(path, None, control)?;
        if self.entry(path)?.identity != before.identity {
            return Err("File changed during duplicate verification".into());
        }
        Ok(hash)
    }

    /// Identity of an item, with a folder fingerprint when it is a directory.
    pub fn snapshot(&self, path: &str, control: &ScanControl) -> Result<FileIdentity> {
        let mut e = self.entry(path)?;
        if e.directory {
            let size = self.sizer.size(path, control)?;
            if !size.complete {
                return Err("Folder coverage is incomplete".into());
            }
            e.identity.tree_signature = size.signature;
        }
        Ok(e.identity)
    }
    pub fn validate_identity(&self, expected: &FileIdentity, control: &ScanControl) -> Result<()> {
        let mut base = expected.clone();
        base.tree_signature = None;
        if self.entry(&expected.path)?.identity != base {
            return Err("Item changed after scanning. Scan again before acting.".into());
        }
        if let Some(signature) = &expected.tree_signature {
            let size = self.sizer.size(&expected.path, control)?;
            if !size.complete || size.signature.as_ref() != Some(signature) {
                return Err("Folder contents changed or cannot be verified. Scan again.".into());
            }
        }
        Ok(())
    }
    pub fn no_symlink_ancestors(&self, path: &str) -> Result<()> {
        let mut ancestor = std::path::PathBuf::from("/");
        for component in Path::new(&policy::canonical(path)).components() {
            if let std::path::Component::Normal(value) = component {
                ancestor.push(value);
                self.entry(ancestor.to_str().ok_or("Invalid path")?)?;
            }
        }
        Ok(())
    }
    /// Fresh checks before any file mutation: protection, exclusions (including protected
    /// descendants), approved scope, symbolic-link ancestors and an unchanged identity.
    pub fn validate(
        &self,
        expected: &FileIdentity,
        context: &ScanContext,
        additional_roots: &[String],
        control: &ScanControl,
    ) -> Result<()> {
        if policy::protected(&expected.path) || context.protects(&expected.path) {
            return Err("This path or an item inside it is protected or excluded.".into());
        }
        if !context.allows(&expected.path)
            || !context
                .roots
                .iter()
                .chain(additional_roots)
                .any(|r| policy::contains(&expected.path, r))
        {
            return Err("This item is outside the approved scan scope.".into());
        }
        self.no_symlink_ancestors(&expected.path)?;
        self.validate_identity(expected, control)
    }

    /// A sized file or folder finding with a fresh identity and coverage details.
    #[allow(clippy::too_many_arguments)]
    pub fn file_finding(
        &self,
        e: &Entry,
        module: &str,
        title: Option<&str>,
        reason: &str,
        actions: Vec<ActionKind>,
        risk: Risk,
        control: &ScanControl,
    ) -> Result<Finding> {
        let size = self.sizer.size(e.path(), control)?;
        let mut fresh = self.entry(e.path())?;
        fresh.identity.tree_signature = size.signature;
        let blocked = if policy::protected(e.path()) {
            Some("Protected path; inspect manually.")
        } else if !size.complete {
            Some("Incomplete size or permission coverage; inspect manually.")
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
        row.blocked_reason = blocked.map(Into::into);
        Ok(row)
    }
}
