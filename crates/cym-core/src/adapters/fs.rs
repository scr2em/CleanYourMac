//! Standard-library filesystem adapters.
use crate::{model::*, policy, ports::*};
use sha2::{Digest, Sha256};
use std::{collections::HashSet, fs, io::Read, sync::Arc};

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
/// metadata fingerprint of every descendant.
pub struct MetadataSizer {
    pub fs: Arc<dyn FileSystem>,
    pub limit: u64,
}
impl MetadataSizer {
    pub fn new(fs: Arc<dyn FileSystem>) -> Self {
        Self { fs, limit: 500_000 }
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
        let mut output = FileSize {
            complete: true,
            ..Default::default()
        };
        let mut stack = vec![root.identity.path.clone()];
        let mut count = 0u64;
        let mut physical = HashSet::new();
        let mut digest = [0u8; 32];
        'outer: while let Some(directory) = stack.pop() {
            control.check()?;
            let Ok(children) = self.fs.children(&directory) else {
                output.complete = false;
                continue;
            };
            for child in children {
                control.check()?;
                if count >= self.limit {
                    output.complete = false;
                    break 'outer;
                }
                let Ok(e) = child else {
                    output.complete = false;
                    continue;
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
                if e.regular || e.symlink {
                    output.files += 1;
                    output.logical = output.logical.saturating_add(e.bytes);
                    if physical.insert((e.identity.device, e.identity.inode)) {
                        output.allocated = output.allocated.saturating_add(e.allocated);
                    }
                }
                if e.directory && !e.symlink {
                    stack.push(e.identity.path);
                }
            }
        }
        output.signature = Some(format!("{count}:{}", hex(&digest)));
        Ok(output)
    }
}

pub struct Sha256Hasher;
impl Hasher for Sha256Hasher {
    fn algorithm(&self) -> &'static str {
        "SHA256"
    }
    fn hash(&self, path: &str, control: &ScanControl) -> Result<String> {
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
        Ok(hex(&digest.finalize()))
    }
}
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
