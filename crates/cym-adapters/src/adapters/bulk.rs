//! Directory listing with `getattrlistbulk(2)`: one call returns names and metadata for many
//! entries, instead of a `readdir` plus one `lstat` per child. Everything other than listing
//! delegates to `StdFileSystem`. On other platforms listing delegates too.
use super::fs::StdFileSystem;
use crate::{model::*, ports::*};

/// `StdFileSystem` with bulk directory listing on macOS.
pub struct BulkFileSystem;

impl FileSystem for BulkFileSystem {
    fn inspect(&self, path: &str, allow_link: bool) -> Result<Entry> {
        StdFileSystem.inspect(path, allow_link)
    }
    fn children(&self, path: &str) -> Result<Vec<Result<Entry>>> {
        #[cfg(target_os = "macos")]
        {
            macos::children(path)
        }
        #[cfg(not(target_os = "macos"))]
        {
            StdFileSystem.children(path)
        }
    }
    fn resolve(&self, path: &str) -> Option<String> {
        StdFileSystem.resolve(path)
    }
    fn read(&self, path: &str, limit: u64) -> Result<Vec<u8>> {
        StdFileSystem.read(path, limit)
    }
    fn remove(&self, path: &str) -> Result<()> {
        StdFileSystem.remove(path)
    }
    fn rename(&self, from: &str, to: &str) -> Result<()> {
        StdFileSystem.rename(from, to)
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use super::StdFileSystem;
    use crate::{model::*, ports::*};
    use std::{cell::RefCell, ffi::CString, io};

    const VREG: u32 = 1;
    const VDIR: u32 = 2;
    const SF_DATALESS: u32 = 0x4000_0000;
    // Not exported by `libc`; see getattrlist(2).
    const ATTR_CMN_ERROR: u32 = 0x2000_0000;
    const COMMON: u32 = libc::ATTR_CMN_RETURNED_ATTRS
        | libc::ATTR_CMN_NAME
        | ATTR_CMN_ERROR
        | libc::ATTR_CMN_DEVID
        | libc::ATTR_CMN_OBJTYPE
        | libc::ATTR_CMN_MODTIME
        | libc::ATTR_CMN_FLAGS
        | libc::ATTR_CMN_FILEID;
    const FILE: u32 =
        libc::ATTR_FILE_LINKCOUNT | libc::ATTR_FILE_ALLOCSIZE | libc::ATTR_FILE_DATALENGTH;

    thread_local! {
        // 8-byte aligned scratch space reused by every listing on this thread.
        static BUFFER: RefCell<Vec<u64>> = RefCell::new(vec![0; 32 * 1024]);
    }

    /// A cursor over one packed entry. Attributes are 4-byte aligned, so reads are unaligned.
    struct Fields<'a> {
        data: &'a [u8],
        at: usize,
    }
    impl Fields<'_> {
        fn take<const N: usize>(&mut self) -> Option<[u8; N]> {
            let bytes = self.data.get(self.at..self.at + N)?.try_into().ok()?;
            self.at += N;
            Some(bytes)
        }
        fn u32(&mut self) -> Option<u32> {
            self.take().map(u32::from_ne_bytes)
        }
        fn i32(&mut self) -> Option<i32> {
            self.take().map(i32::from_ne_bytes)
        }
        fn i64(&mut self) -> Option<i64> {
            self.take().map(i64::from_ne_bytes)
        }
        fn u64(&mut self) -> Option<u64> {
            self.take().map(u64::from_ne_bytes)
        }
    }

    struct Directory(libc::c_int);
    impl Drop for Directory {
        fn drop(&mut self) {
            unsafe { libc::close(self.0) };
        }
    }

    pub fn children(path: &str) -> Result<Vec<Result<Entry>>> {
        let cannot = |e: io::Error| format!("Cannot read {path}: {e}");
        let c_path = CString::new(path).map_err(|_| format!("Cannot read {path}: invalid path"))?;
        let fd = unsafe {
            libc::open(
                c_path.as_ptr(),
                // Never through a link swapped in after the folder was listed.
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            return Err(cannot(io::Error::last_os_error()));
        }
        let directory = Directory(fd);
        let prefix = if path.ends_with('/') {
            path.to_owned()
        } else {
            format!("{path}/")
        };
        let mut request = libc::attrlist {
            bitmapcount: libc::ATTR_BIT_MAP_COUNT,
            reserved: 0,
            commonattr: COMMON,
            volattr: 0,
            dirattr: 0,
            fileattr: FILE,
            forkattr: 0,
        };
        let mut entries = vec![];
        BUFFER.with_borrow_mut(|buffer| loop {
            let size = buffer.len() * 8;
            let count = unsafe {
                libc::getattrlistbulk(
                    directory.0,
                    (&mut request as *mut libc::attrlist).cast(),
                    buffer.as_mut_ptr().cast(),
                    size,
                    0,
                )
            };
            if count < 0 {
                let error = io::Error::last_os_error();
                if error.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                // A volume without bulk attribute support is listed the standard way.
                if entries.is_empty()
                    && matches!(error.raw_os_error(), Some(libc::ENOTSUP | libc::EINVAL))
                {
                    return StdFileSystem.children(path);
                }
                return Err(cannot(error));
            }
            if count == 0 {
                return Ok(entries);
            }
            let bytes = unsafe { std::slice::from_raw_parts(buffer.as_ptr().cast::<u8>(), size) };
            let mut offset = 0;
            for _ in 0..count {
                let Some(length) = bytes
                    .get(offset..offset + 4)
                    .and_then(|b| b.try_into().ok())
                    .map(|b| u32::from_ne_bytes(b) as usize)
                    .filter(|&n| n >= 4 && offset + n <= size)
                else {
                    return Err(format!("Cannot read {path}: malformed attribute buffer"));
                };
                if let Some(entry) = parse(&bytes[offset..offset + length], &prefix) {
                    entries.push(entry);
                }
                offset += length;
            }
        })
    }

    /// One packed entry: the length, the returned-attribute set, then each returned attribute
    /// in bit order within its group, except that `ATTR_CMN_ERROR` directly follows the set.
    fn parse(data: &[u8], prefix: &str) -> Option<Result<Entry>> {
        let mut f = Fields { data, at: 4 };
        let common = f.u32()?;
        let _volume = f.u32()?;
        let _directory = f.u32()?;
        let file = f.u32()?;
        let _fork = f.u32()?;
        let error = if common & ATTR_CMN_ERROR != 0 {
            f.u32()?
        } else {
            0
        };
        if common & libc::ATTR_CMN_NAME == 0 {
            return None;
        }
        let name_at = f.at;
        let name_offset = f.i32()?;
        let name_length = f.u32()? as usize;
        let start = usize::try_from(name_at as i64 + name_offset as i64).ok()?;
        let raw = data.get(start..start + name_length)?;
        let raw = raw.strip_suffix(&[0]).unwrap_or(raw);
        let Ok(name) = std::str::from_utf8(raw) else {
            return Some(Err(format!(
                "Skipped non-UTF-8 path in {}",
                prefix.trim_end_matches('/')
            )));
        };
        let path = format!("{prefix}{name}");
        if error != 0 {
            let error = io::Error::from_raw_os_error(error as i32);
            return Some(Err(format!("Cannot inspect {path}: {error}")));
        }
        let mut device = 0;
        if common & libc::ATTR_CMN_DEVID != 0 {
            // `dev_t` is a signed 32-bit value; widen it as `MetadataExt::dev` does.
            device = f.i32()? as u64;
        }
        let kind = (common & libc::ATTR_CMN_OBJTYPE != 0)
            .then(|| f.u32())
            .flatten();
        let mut modified = (0, 0);
        if common & libc::ATTR_CMN_MODTIME != 0 {
            modified = (f.i64()?, f.i64()?);
        }
        let flags = if common & libc::ATTR_CMN_FLAGS != 0 {
            f.u32()?
        } else {
            0
        };
        let inode = (common & libc::ATTR_CMN_FILEID != 0)
            .then(|| f.u64())
            .flatten();
        let (Some(kind), Some(inode)) = (kind, inode) else {
            return Some(StdFileSystem.inspect(&path, true));
        };
        if kind != VREG && kind != VDIR {
            // Links and special files keep exact `lstat` semantics.
            return Some(StdFileSystem.inspect(&path, true));
        }
        let (mut links, mut allocated, mut bytes) = (1, 0, 0);
        if kind == VREG {
            if file & libc::ATTR_FILE_LINKCOUNT == 0
                || file & libc::ATTR_FILE_ALLOCSIZE == 0
                || file & libc::ATTR_FILE_DATALENGTH == 0
            {
                return Some(StdFileSystem.inspect(&path, true));
            }
            links = u64::from(f.u32()?);
            allocated = f.i64()?.max(0) as u64;
            bytes = f.i64()?.max(0) as u64;
        }
        Some(Ok(Entry {
            identity: FileIdentity {
                path,
                device,
                inode,
                modified_seconds: modified.0,
                modified_nanos: modified.1,
                tree_signature: None,
            },
            directory: kind == VDIR,
            regular: kind == VREG,
            symlink: false,
            bytes,
            allocated,
            links,
            dataless: flags & SF_DATALESS != 0,
        }))
    }
}
