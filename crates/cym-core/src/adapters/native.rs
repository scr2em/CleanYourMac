//! macOS adapters backed by `macos.m` (libproc, NSWorkspace, FileManager). On other
//! platforms they report that the capability is unavailable instead of guessing.
use crate::{model::*, ports::*};
#[cfg(target_os = "macos")]
use std::ffi::CStr;
use std::path::Path;

#[repr(C)]
#[allow(dead_code)]
struct RawProcess {
    pid: i32,
    uid: u32,
    parent: i32,
    started_sec: u64,
    started_usec: u64,
    cpu_nanos: u64,
    memory: u64,
    has_usage: u8,
    path: [libc::c_char; 4096],
    cwd: [libc::c_char; 1024],
    command: [libc::c_char; 16384],
}
#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn cym_processes(buffer: *mut i32, capacity: i32) -> i32;
    fn cym_inspect_process(pid: i32, out: *mut RawProcess) -> i32;
    fn cym_process_struct_size() -> usize;
    fn cym_native_apps() -> *mut libc::c_char;
    fn cym_last_used(path: *const libc::c_char) -> f64;
    fn cym_native_trash(
        path: *const libc::c_char,
        failure: *mut *mut libc::c_char,
    ) -> *mut libc::c_char;
}
#[cfg(target_os = "macos")]
unsafe fn take(p: *mut libc::c_char) -> String {
    let s = CStr::from_ptr(p).to_string_lossy().into_owned();
    libc::free(p.cast());
    s
}

pub struct LibprocInspector;
impl ProcessInspector for LibprocInspector {
    #[cfg(target_os = "macos")]
    fn pids(&self) -> Vec<i32> {
        unsafe {
            let count = cym_processes(std::ptr::null_mut(), 0);
            if count <= 0 {
                return vec![];
            }
            let mut rows = vec![0; count as usize + 256];
            let actual = cym_processes(rows.as_mut_ptr(), rows.len() as i32);
            rows.truncate((actual.max(0) as usize).min(rows.len()));
            rows.retain(|pid| *pid > 0);
            rows
        }
    }
    #[cfg(not(target_os = "macos"))]
    fn pids(&self) -> Vec<i32> {
        vec![]
    }
    #[cfg(target_os = "macos")]
    fn inspect(&self, pid: i32) -> Option<Snapshot> {
        unsafe {
            // The C and Rust layouts must agree before the buffer is shared.
            if cym_process_struct_size() != std::mem::size_of::<RawProcess>() {
                return None;
            }
            let mut raw: Box<RawProcess> = Box::new(std::mem::zeroed());
            if cym_inspect_process(pid, &mut *raw) == 0 {
                return None;
            }
            let string = |p: *const libc::c_char| CStr::from_ptr(p).to_string_lossy().into_owned();
            let path = string(raw.path.as_ptr());
            let name = Path::new(&path).file_name()?.to_str()?.into();
            let cwd = string(raw.cwd.as_ptr());
            let command = string(raw.command.as_ptr());
            Some(Snapshot {
                identity: ProcessIdentity {
                    pid,
                    uid: raw.uid,
                    started_seconds: raw.started_sec,
                    started_microseconds: raw.started_usec,
                    executable: path.clone(),
                },
                parent: raw.parent,
                name,
                command: if command.is_empty() { path } else { command },
                cwd: (!cwd.is_empty()).then_some(cwd),
                cpu_nanos: (raw.has_usage != 0).then_some(raw.cpu_nanos),
                memory: (raw.has_usage != 0).then_some(raw.memory),
            })
        }
    }
    #[cfg(not(target_os = "macos"))]
    fn inspect(&self, _: i32) -> Option<Snapshot> {
        None
    }
    fn signal(&self, pid: i32, force: bool) -> Result<()> {
        if pid <= 1 {
            return Err("Refusing to signal launchd or a process group".into());
        }
        if unsafe { libc::kill(pid, if force { libc::SIGKILL } else { libc::SIGTERM }) } != 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        Ok(())
    }
    fn current_uid(&self) -> u32 {
        unsafe { libc::getuid() }
    }
    fn current_pid(&self) -> i32 {
        std::process::id() as i32
    }
}

pub struct NativeApplications;
impl Applications for NativeApplications {
    #[cfg(target_os = "macos")]
    fn running(&self) -> Result<Vec<RunningApp>> {
        unsafe {
            let p = cym_native_apps();
            if p.is_null() {
                return Err("Running application inspection failed".into());
            }
            serde_json::from_str(&take(p)).map_err(|e| e.to_string())
        }
    }
    #[cfg(not(target_os = "macos"))]
    fn running(&self) -> Result<Vec<RunningApp>> {
        Err("Application inspection requires macOS".into())
    }
    fn bundle_info(&self, app_path: &str) -> BundleInfo {
        let info = plist::Value::from_file(Path::new(app_path).join("Contents/Info.plist")).ok();
        let dict = info.as_ref().and_then(|i| i.as_dictionary());
        let value = |key: &str| {
            dict.and_then(|d| d.get(key))
                .and_then(|v| v.as_string())
                .unwrap_or("")
                .to_owned()
        };
        BundleInfo {
            identifier: value("CFBundleIdentifier"),
            version: value("CFBundleShortVersionString"),
        }
    }
}

/// Spotlight's `kMDItemLastUsedDate`, the date Finder shows as "Last opened".
pub struct SpotlightUsage;
impl Usage for SpotlightUsage {
    #[cfg(target_os = "macos")]
    fn last_used(&self, path: &str) -> Option<f64> {
        let path = std::ffi::CString::new(path).ok()?;
        let seconds = unsafe { cym_last_used(path.as_ptr()) };
        (seconds > 0.).then_some(seconds)
    }
    #[cfg(not(target_os = "macos"))]
    fn last_used(&self, _: &str) -> Option<f64> {
        None
    }
}

pub struct NativeTrash;
impl Trash for NativeTrash {
    #[cfg(target_os = "macos")]
    fn move_to_trash(&self, path: &str) -> Result<String> {
        let input = std::ffi::CString::new(path).map_err(|_| "Invalid path")?;
        unsafe {
            let mut failure = std::ptr::null_mut();
            let p = cym_native_trash(input.as_ptr(), &mut failure);
            if p.is_null() {
                return Err(if failure.is_null() {
                    "Trash operation failed".into()
                } else {
                    take(failure)
                });
            }
            Ok(take(p))
        }
    }
    #[cfg(not(target_os = "macos"))]
    fn move_to_trash(&self, _: &str) -> Result<String> {
        Err("Native Trash requires macOS".into())
    }
}
