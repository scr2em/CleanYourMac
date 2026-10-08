//! Standard-library filesystem adapters.
use crate::{model::*, policy, ports::*};
use rayon::prelude::*;
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs,
    io::Read,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc, Arc,
    },
};

pub struct StdFileSystem;
impl FileSystem for StdFileSystem {
    fn inspect(&self, path: &str, allow_link: bool) -> Result<Entry> {
        let path = policy::canonical(path);
        let m = fs::symlink_metadata(&path).map_err(|e| format!("Cannot inspect {path}: {e}"))?;
        if m.file_type().is_symlink() && !allow_link {
            return Err(format!("Symbolic links are not traversed: {path}"));
        }
        Ok(Entry::from_metadata(path, &m))
    }
    fn children(&self, path: &str) -> Result<Vec<Result<Entry>>> {
        let iter = fs::read_dir(path).map_err(|e| format!("Cannot read {path}: {e}"))?;
        Ok(iter
            .map(|item| {
                let item = item.map_err(|e| format!("Cannot enumerate {path}: {e}"))?;
                let child = item.path();
                let child = child
                    .to_str()
                    .ok_or_else(|| format!("Skipped non-UTF-8 path in {path}"))?;
                // A child of an already-normalized folder needs no further normalization;
                // the entry's own metadata is an lstat that never follows a link.
                let metadata = item
                    .metadata()
                    .map_err(|e| format!("Cannot inspect {child}: {e}"))?;
                Ok(Entry::from_metadata(child.to_owned(), &metadata))
            })
            .collect())
    }
    fn resolve(&self, path: &str) -> Option<String> {
        fs::canonicalize(path)
            .ok()
            .and_then(|p| p.to_str().map(str::to_owned))
    }
    fn read(&self, path: &str, limit: u64) -> Result<Vec<u8>> {
        use std::os::unix::fs::OpenOptionsExt;
        let file = fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path)
            .map_err(|e| format!("Cannot read {path}: {e}"))?;
        if !file.metadata().is_ok_and(|m| m.is_file()) {
            return Err(format!("Not a regular file: {path}"));
        }
        let mut data = vec![];
        file.take(limit)
            .read_to_end(&mut data)
            .map_err(|e| format!("Cannot read {path}: {e}"))?;
        Ok(data)
    }
    fn remove(&self, path: &str) -> Result<()> {
        let m = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
        if m.is_dir() {
            fs::remove_dir_all(path)
        } else {
            fs::remove_file(path)
        }
        .map_err(|e| e.to_string())
    }
    fn rename(&self, from: &str, to: &str) -> Result<()> {
        rename_exclusive(from, to)
    }
}

/// Renames without replacing: fails if something already exists at `to`, so an item that
/// appears there after a check is never overwritten.
pub fn rename_exclusive(from: &str, to: &str) -> Result<()> {
    use std::ffi::CString;
    let a = CString::new(from).map_err(|_| "Invalid path")?;
    let b = CString::new(to).map_err(|_| "Invalid path")?;
    #[cfg(target_vendor = "apple")]
    let status = unsafe { libc::renamex_np(a.as_ptr(), b.as_ptr(), libc::RENAME_EXCL) };
    #[cfg(target_os = "linux")]
    let status = unsafe {
        libc::renameat2(
            libc::AT_FDCWD,
            a.as_ptr(),
            libc::AT_FDCWD,
            b.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    #[cfg(not(any(target_vendor = "apple", target_os = "linux")))]
    let status = {
        let _ = (&a, &b);
        if fs::symlink_metadata(to).is_ok() {
            return Err("An item already exists at the destination.".into());
        }
        fs::rename(from, to).map(|_| 0).unwrap_or(-1)
    };
    if status == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error().to_string())
    }
}

/// Iterative depth-first walker on top of any `FileSystem`. Symbolic links are reported
/// as entries but never descended.
pub struct StackWalker(pub Arc<dyn FileSystem>);
impl Walker for StackWalker {
    fn walk(
        &self,
        root: &str,
        control: &ScanControl,
        problem: &mut dyn FnMut(String),
        visit: &mut dyn FnMut(&Entry) -> Result<Visit>,
    ) -> Result<()> {
        let mut stack = vec![root.to_owned()];
        while let Some(directory) = stack.pop() {
            control.check()?;
            let children = match self.0.children(&directory) {
                Ok(children) => children,
                Err(e) => {
                    problem(e);
                    continue;
                }
            };
            for child in children {
                control.check()?;
                let entry = match child {
                    Ok(entry) => entry,
                    Err(e) => {
                        problem(e);
                        continue;
                    }
                };
                match visit(&entry)? {
                    Visit::Stop => return Ok(()),
                    Visit::Descend if entry.directory && !entry.symlink => {
                        stack.push(entry.identity.path)
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }
}

type Listing = Result<Vec<Result<Entry>>>;

/// A folder waiting to be listed, possibly already being read on the pool.
enum Pending {
    Queued(String),
    Started {
        path: String,
        claimed: Arc<AtomicBool>,
        listing: mpsc::Receiver<Listing>,
    },
}

/// Depth-first traversal in exactly `StackWalker`'s order, with the folders nearest the top
/// of the stack listed ahead of time on a thread pool while listings are slow. The visitor
/// still runs on the calling thread, one entry at a time. Whoever claims a listing first reads
/// it, so the walk never waits on a job that has not started, even when it runs on the same
/// pool.
///
/// Reading ahead only pays when a listing waits on the disk: from the cache a folder lists in
/// microseconds, less than handing it to another thread costs. So the walker times its
/// listings and prefetches only while their recent average exceeds `slow_listing`.
pub struct PrefetchWalker {
    pub fs: Arc<dyn FileSystem>,
    pub pool: Option<Arc<rayon::ThreadPool>>,
    /// How many folders at the top of the stack are read ahead.
    pub window: usize,
    /// The average listing time above which folders are read ahead.
    pub slow_listing: std::time::Duration,
}
impl PrefetchWalker {
    pub fn new(fs: Arc<dyn FileSystem>, pool: Option<Arc<rayon::ThreadPool>>) -> Self {
        Self {
            fs,
            pool,
            window: 32,
            slow_listing: std::time::Duration::from_micros(50),
        }
    }
    fn start(&self, path: String) -> Pending {
        let claimed = Arc::new(AtomicBool::new(false));
        let (send, listing) = mpsc::sync_channel(1);
        let (fs, job_path, job_claimed) = (self.fs.clone(), path.clone(), claimed.clone());
        let job = move || {
            if !job_claimed.swap(true, Ordering::AcqRel) {
                let _ = send.send(fs.children(&job_path));
            }
        };
        match &self.pool {
            Some(pool) => pool.spawn(job),
            None => rayon::spawn(job),
        }
        Pending::Started {
            path,
            claimed,
            listing,
        }
    }
    fn list(&self, pending: Pending) -> Listing {
        match pending {
            Pending::Queued(path) => self.fs.children(&path),
            Pending::Started {
                path,
                claimed,
                listing,
            } => {
                if claimed.swap(true, Ordering::AcqRel) {
                    listing.recv().unwrap_or_else(|_| self.fs.children(&path))
                } else {
                    self.fs.children(&path)
                }
            }
        }
    }
}
/// Abandons queued reads when a walk ends early.
struct Abandon(Vec<Pending>);
impl Drop for Abandon {
    fn drop(&mut self) {
        for pending in &self.0 {
            if let Pending::Started { claimed, .. } = pending {
                claimed.store(true, Ordering::Release);
            }
        }
    }
}
impl Walker for PrefetchWalker {
    fn walk(
        &self,
        root: &str,
        control: &ScanControl,
        problem: &mut dyn FnMut(String),
        visit: &mut dyn FnMut(&Entry) -> Result<Visit>,
    ) -> Result<()> {
        let mut stack = Abandon(vec![Pending::Queued(root.to_owned())]);
        // Exponential moving average of listing time, in nanoseconds.
        let mut average = 0u128;
        let slow = self.slow_listing.as_nanos();
        while let Some(pending) = stack.0.pop() {
            control.check()?;
            let started = std::time::Instant::now();
            let listing = self.list(pending);
            average = (average * 7 + started.elapsed().as_nanos()) / 8;
            let children = match listing {
                Ok(children) => children,
                Err(e) => {
                    problem(e);
                    continue;
                }
            };
            for child in children {
                control.check()?;
                let entry = match child {
                    Ok(entry) => entry,
                    Err(e) => {
                        problem(e);
                        continue;
                    }
                };
                match visit(&entry)? {
                    Visit::Stop => return Ok(()),
                    Visit::Descend if entry.directory && !entry.symlink => {
                        stack.0.push(Pending::Queued(entry.identity.path))
                    }
                    _ => {}
                }
            }
            if average < slow {
                continue;
            }
            // The next folders popped are the newest pushes; start reading them now.
            let floor = stack.0.len().saturating_sub(self.window);
            for slot in &mut stack.0[floor..] {
                if let Pending::Queued(path) = slot {
                    *slot = self.start(std::mem::take(path));
                }
            }
        }
        Ok(())
    }
}

/// Logical and allocated size with hard links counted once, plus an order-independent
/// metadata fingerprint of every descendant. Subdirectories are sized in parallel.
pub struct MetadataSizer {
    pub fs: Arc<dyn FileSystem>,
    pub limit: u64,
    /// Pool for the parallel folder walk; the caller's pool when `None`.
    pub pool: Option<Arc<rayon::ThreadPool>>,
}
impl MetadataSizer {
    pub fn new(fs: Arc<dyn FileSystem>) -> Self {
        Self {
            fs,
            limit: 500_000,
            pool: None,
        }
    }
    pub fn on(mut self, pool: Arc<rayon::ThreadPool>) -> Self {
        self.pool = Some(pool);
        self
    }
}
#[derive(Default)]
struct Tally {
    logical: u64,
    allocated: u64,
    files: u64,
    complete: bool,
    digest: [u8; 32],
    /// Multiply-linked files, deduplicated after all branches merge.
    shared: Vec<(u64, u64, u64)>,
}
impl Tally {
    fn merge(mut self, other: Tally) -> Tally {
        self.logical = self.logical.saturating_add(other.logical);
        self.allocated = self.allocated.saturating_add(other.allocated);
        self.files += other.files;
        self.complete &= other.complete;
        for (a, b) in self.digest.iter_mut().zip(other.digest) {
            *a ^= b;
        }
        self.shared.extend(other.shared);
        self
    }
}
struct Walk<'a> {
    fs: &'a dyn FileSystem,
    root: &'a str,
    control: &'a ScanControl,
    count: AtomicU64,
    limit: u64,
}
impl Walk<'_> {
    fn directory(&self, path: &str) -> Tally {
        let mut tally = Tally {
            complete: true,
            ..Default::default()
        };
        if self.control.is_cancelled() {
            tally.complete = false;
            return tally;
        }
        let Ok(children) = self.fs.children(path) else {
            tally.complete = false;
            return tally;
        };
        let mut folders = vec![];
        for child in children {
            let Ok(e) = child else {
                tally.complete = false;
                continue;
            };
            if self.count.fetch_add(1, Ordering::Relaxed) >= self.limit {
                tally.complete = false;
                break;
            }
            let relative = e.path().strip_prefix(self.root).unwrap_or(e.path());
            // Hash the entry's raw fields without formatting a string per entry.
            let mut hasher = blake3::Hasher::new();
            hasher.update(relative.as_bytes());
            hasher.update(&[0, u8::from(e.directory), u8::from(e.symlink)]);
            for field in [
                e.identity.device,
                e.identity.inode,
                e.identity.modified_seconds as u64,
                e.identity.modified_nanos as u64,
                e.bytes,
            ] {
                hasher.update(&field.to_le_bytes());
            }
            for (a, b) in tally.digest.iter_mut().zip(hasher.finalize().as_bytes()) {
                *a ^= b;
            }
            if e.regular || e.symlink {
                tally.files += 1;
                tally.logical = tally.logical.saturating_add(e.bytes);
                if e.links > 1 {
                    tally
                        .shared
                        .push((e.identity.device, e.identity.inode, e.allocated));
                } else {
                    tally.allocated = tally.allocated.saturating_add(e.allocated);
                }
            }
            // An iCloud-evicted folder is counted as itself; entering it would download it.
            if e.directory && !e.symlink && !e.dataless {
                folders.push(e.identity.path);
            }
        }
        folders
            .par_iter()
            .map(|folder| self.directory(folder))
            .reduce(Tally::default_complete, Tally::merge)
            .merge(tally)
    }
}
impl Tally {
    fn default_complete() -> Tally {
        Tally {
            complete: true,
            ..Default::default()
        }
    }
}
impl Sizer for MetadataSizer {
    fn size(&self, path: &str, control: &ScanControl) -> Result<FileSize> {
        let root = self.fs.inspect(path, false)?;
        if !root.directory {
            return Ok(FileSize {
                logical: root.bytes,
                allocated: root.allocated,
                files: 1,
                complete: true,
                signature: None,
            });
        }
        let walk = Walk {
            fs: self.fs.as_ref(),
            root: root.path(),
            control,
            count: AtomicU64::new(0),
            limit: self.limit,
        };
        let mut tally = match &self.pool {
            Some(pool) => pool.install(|| walk.directory(root.path())),
            None => walk.directory(root.path()),
        };
        control.check()?;
        let mut physical = HashSet::new();
        for (device, inode, allocated) in std::mem::take(&mut tally.shared) {
            if physical.insert((device, inode)) {
                tally.allocated = tally.allocated.saturating_add(allocated);
            }
        }
        let count = walk.count.load(Ordering::Relaxed).min(self.limit);
        Ok(FileSize {
            logical: tally.logical,
            allocated: tally.allocated,
            files: tally.files,
            complete: tally.complete,
            signature: Some(format!("{count}:{}", hex(&tally.digest))),
        })
    }
}

fn read_digest(
    path: &str,
    limit: Option<u64>,
    control: &ScanControl,
    mut update: impl FnMut(&[u8]),
) -> Result<()> {
    // Never follow a link swapped in after the scan, and never block on a FIFO.
    let file = {
        use std::os::unix::fs::OpenOptionsExt;
        fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(path)
            .map_err(|e| e.to_string())?
    };
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("Not a regular file".into());
    }
    let mut reader = file.take(limit.unwrap_or(u64::MAX));
    let mut buffer = vec![0; 1_048_576];
    loop {
        control.check()?;
        let count = reader.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 {
            return Ok(());
        }
        update(&buffer[..count]);
    }
}
/// BLAKE3: the default, several times faster than SHA-256 on large files.
pub struct Blake3Hasher;
impl Hasher for Blake3Hasher {
    fn algorithm(&self) -> &'static str {
        "BLAKE3"
    }
    fn hash(&self, path: &str, limit: Option<u64>, control: &ScanControl) -> Result<String> {
        let mut digest = blake3::Hasher::new();
        read_digest(path, limit, control, |bytes| {
            digest.update(bytes);
        })?;
        Ok(digest.finalize().to_hex().to_string())
    }
}
pub struct Sha256Hasher;
impl Hasher for Sha256Hasher {
    fn algorithm(&self) -> &'static str {
        "SHA256"
    }
    fn hash(&self, path: &str, limit: Option<u64>, control: &ScanControl) -> Result<String> {
        let mut digest = Sha256::new();
        read_digest(path, limit, control, |bytes| digest.update(bytes))?;
        Ok(hex(&digest.finalize()))
    }
}
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
