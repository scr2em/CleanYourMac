//! Standard-library filesystem adapters.
use crate::{model::*, policy, ports::*};
use rayon::prelude::*;
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs,
    io::Read,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
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
                self.inspect(child, true)
            })
            .collect())
    }
    fn resolve(&self, path: &str) -> Option<String> {
        fs::canonicalize(path)
            .ok()
            .and_then(|p| p.to_str().map(str::to_owned))
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
        fs::rename(from, to).map_err(|e| e.to_string())
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

/// Logical and allocated size with hard links counted once, plus an order-independent
/// metadata fingerprint of every descendant. Subdirectories are sized in parallel.
pub struct MetadataSizer {
    pub fs: Arc<dyn FileSystem>,
    pub limit: u64,
}
impl MetadataSizer {
    pub fn new(fs: Arc<dyn FileSystem>) -> Self {
        Self { fs, limit: 500_000 }
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
            for (a, b) in tally
                .digest
                .iter_mut()
                .zip(Sha256::digest(metadata.as_bytes()))
            {
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
        let mut tally = walk.directory(root.path());
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
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
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
