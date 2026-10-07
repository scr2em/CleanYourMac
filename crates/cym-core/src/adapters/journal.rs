//! JSON-file action history, private to the user and capped in length.
use crate::{model::*, ports::Journal};
use std::{fs, io::Write, os::unix::fs::PermissionsExt, path::PathBuf, sync::Mutex};

pub struct JsonJournal {
    path: PathBuf,
    lock: Mutex<()>,
    pub limit: usize,
}
impl JsonJournal {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            lock: Mutex::new(()),
            limit: 1_000,
        }
    }
    pub fn default_path() -> PathBuf {
        PathBuf::from(crate::policy::home())
            .join("Library/Application Support/CleanYourMac/history.json")
    }
    fn load(&self) -> Vec<ActionResult> {
        fs::read(&self.path)
            .ok()
            .and_then(|data| serde_json::from_slice(&data).ok())
            .unwrap_or_default()
    }
}
impl Journal for JsonJournal {
    fn read(&self) -> Vec<ActionResult> {
        let _guard = self.lock.lock();
        self.load()
    }
    fn append(&self, results: &[ActionResult]) -> Result<()> {
        let _guard = self.lock.lock();
        let mut rows = results.to_vec();
        rows.extend(self.load());
        rows.truncate(self.limit);
        let directory = self.path.parent().ok_or("Invalid journal path")?;
        fs::create_dir_all(directory).map_err(|e| e.to_string())?;
        let _ = fs::set_permissions(directory, fs::Permissions::from_mode(0o700));
        let data = serde_json::to_vec(&rows).map_err(|e| e.to_string())?;
        // Write a private sibling and rename it so an interrupted write keeps the prior history.
        let temporary = self.path.with_extension("json.tmp");
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode_private()
            .open(&temporary)
            .map_err(|e| e.to_string())?;
        file.write_all(&data).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        fs::rename(&temporary, &self.path).map_err(|e| e.to_string())
    }
    fn clear(&self) -> Result<()> {
        let _guard = self.lock.lock();
        match fs::remove_file(&self.path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
            _ => Ok(()),
        }
    }
}
trait PrivateMode {
    fn mode_private(&mut self) -> &mut Self;
}
impl PrivateMode for fs::OpenOptions {
    fn mode_private(&mut self) -> &mut Self {
        std::os::unix::fs::OpenOptionsExt::mode(self, 0o600)
    }
}

/// In-memory journal for previews and tests.
#[derive(Default)]
pub struct MemoryJournal(pub Mutex<Vec<ActionResult>>);
impl Journal for MemoryJournal {
    fn read(&self) -> Vec<ActionResult> {
        self.0.lock().map(|rows| rows.clone()).unwrap_or_default()
    }
    fn append(&self, results: &[ActionResult]) -> Result<()> {
        let mut rows = self.0.lock().map_err(|_| "Journal unavailable")?;
        rows.splice(0..0, results.iter().cloned());
        Ok(())
    }
    fn clear(&self) -> Result<()> {
        self.0.lock().map_err(|_| "Journal unavailable")?.clear();
        Ok(())
    }
}
