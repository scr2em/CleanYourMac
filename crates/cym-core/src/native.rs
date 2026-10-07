use crate::model::Result;
use serde::Deserialize;
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunningApp {
    pub pid: i32,
    #[serde(rename = "bundleID")]
    pub bundle_id: String,
    pub path: String,
}
#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn cym_native_apps() -> *mut libc::c_char;
    fn cym_native_trash(
        path: *const libc::c_char,
        failure: *mut *mut libc::c_char,
    ) -> *mut libc::c_char;
}
#[cfg(target_os = "macos")]
pub fn running_apps() -> Result<Vec<RunningApp>> {
    unsafe {
        let p = cym_native_apps();
        if p.is_null() {
            return Err("Running application inspection failed".into());
        }
        let bytes = std::ffi::CStr::from_ptr(p).to_bytes().to_vec();
        libc::free(p.cast());
        serde_json::from_slice(&bytes).map_err(|e| e.to_string())
    }
}
#[cfg(not(target_os = "macos"))]
pub fn running_apps() -> Result<Vec<RunningApp>> {
    Err("Application inspection requires macOS".into())
}
#[cfg(target_os = "macos")]
pub fn trash(path: &str) -> Result<String> {
    let input = std::ffi::CString::new(path).map_err(|_| "Invalid path")?;
    unsafe {
        let mut failure = std::ptr::null_mut();
        let p = cym_native_trash(input.as_ptr(), &mut failure);
        if p.is_null() {
            let message = if failure.is_null() {
                "Trash operation failed".into()
            } else {
                let s = std::ffi::CStr::from_ptr(failure)
                    .to_string_lossy()
                    .into_owned();
                libc::free(failure.cast());
                s
            };
            return Err(message);
        }
        let output = std::ffi::CStr::from_ptr(p).to_string_lossy().into_owned();
        libc::free(p.cast());
        Ok(output)
    }
}
#[cfg(not(target_os = "macos"))]
pub fn trash(_: &str) -> Result<String> {
    Err("Native Trash requires macOS".into())
}
