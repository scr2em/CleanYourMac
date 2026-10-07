use serde::{Deserialize, Serialize};

pub type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub struct FileIdentity {
    pub path: String,
    pub device: u64,
    pub inode: u64,
    pub modified_seconds: i64,
    pub modified_nanos: i64,
    pub tree_signature: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub struct ProcessIdentity {
    pub pid: i32,
    pub uid: u32,
    pub started_seconds: u64,
    pub started_microseconds: u64,
    pub executable: String,
}
impl ProcessIdentity {
    pub fn key(&self) -> String {
        format!(
            "{}:{}:{}",
            self.pid, self.started_seconds, self.started_microseconds
        )
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Resource {
    File {
        file: FileIdentity,
    },
    Process {
        process: ProcessIdentity,
    },
    Worktree {
        file: FileIdentity,
        repository: String,
        head: String,
    },
    WorktreeRegistration {
        path: String,
        repository: String,
    },
    Simulator {
        id: String,
        state: String,
    },
}
impl Resource {
    pub fn path(&self) -> Option<&str> {
        match self {
            Self::File { file } | Self::Worktree { file, .. } => Some(&file.path),
            Self::Process { process } => Some(&process.executable),
            Self::WorktreeRegistration { path, .. } => Some(path),
            Self::Simulator { .. } => None,
        }
    }
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Risk {
    #[serde(rename = "Review")]
    Review,
    #[serde(rename = "Rebuild required")]
    Rebuild,
    #[serde(rename = "Permanent")]
    Permanent,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ActionKind {
    Trash,
    RemoveWorktree,
    ResetSimulator,
    DeleteSimulator,
    Terminate,
    ForceQuit,
    EmptyTrash,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Detail {
    pub label: String,
    pub value: String,
}
pub fn detail(label: &str, value: impl Into<String>) -> Detail {
    Detail {
        label: label.into(),
        value: value.into(),
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    pub id: String,
    pub module_id: String,
    pub title: String,
    pub subtitle: String,
    pub resource: Resource,
    pub bytes: Option<u64>,
    pub allocated_bytes: Option<u64>,
    pub cpu_percent: Option<f64>,
    pub memory_bytes: Option<u64>,
    pub modified_at: Option<f64>,
    /// When the item (or, for generated folders, its project) was last used, in Unix seconds.
    #[serde(default)]
    pub last_used_at: Option<f64>,
    pub details: Vec<Detail>,
    pub actions: Vec<ActionKind>,
    pub risk: Risk,
    pub reason: String,
    pub blocked_reason: Option<String>,
    pub badge: Option<String>,
    /// The ecosystem's brand (a Simple Icons slug), drawn as the row's icon.
    #[serde(default)]
    pub brand: Option<String>,
}
impl Finding {
    pub fn new(module: &str, key: &str, title: &str, resource: Resource, reason: &str) -> Self {
        Self {
            id: format!("{module}:{key}"),
            module_id: module.into(),
            title: title.into(),
            subtitle: key.into(),
            resource,
            bytes: None,
            allocated_bytes: None,
            cpu_percent: None,
            memory_bytes: None,
            modified_at: None,
            last_used_at: None,
            details: vec![],
            actions: vec![],
            risk: Risk::Review,
            reason: reason.into(),
            blocked_reason: None,
            badge: None,
            brand: None,
        }
    }
    pub fn value(&self, label: &str) -> Option<&str> {
        self.details
            .iter()
            .find(|d| d.label == label)
            .map(|d| d.value.as_str())
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanContext {
    pub roots: Vec<String>,
    #[serde(default)]
    pub exclusions: Vec<String>,
    #[serde(default)]
    pub ignored_process_names: Vec<String>,
    #[serde(default)]
    pub limit_to_roots: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModuleDescriptor {
    pub id: String,
    pub name: String,
    pub category: String,
    pub symbol: String,
    pub summary: String,
    pub uses_roots: bool,
    /// Whether the overview's scan runs this module. Slow or deliberate tools run only when
    /// the user scans them from their own page.
    #[serde(default = "included")]
    pub in_overview: bool,
}
fn included() -> bool {
    true
}
impl ModuleDescriptor {
    pub fn new(
        id: &str,
        name: &str,
        category: &str,
        symbol: &str,
        summary: &str,
        uses_roots: bool,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            category: category.into(),
            symbol: symbol.into(),
            summary: summary.into(),
            uses_roots,
            in_overview: true,
        }
    }
    /// Leaves the module out of the overview's scan; the user runs it explicitly.
    pub fn explicit(mut self) -> Self {
        self.in_overview = false;
        self
    }
}
#[derive(Default, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanReport {
    pub findings: Vec<Finding>,
    pub warnings: Vec<String>,
    pub cancelled: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ActionRequest {
    pub findings: Vec<Finding>,
    pub kind: ActionKind,
    pub context: ScanContext,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Outcome {
    Applied,
    Skipped,
    Failed,
    Requested,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionResult {
    pub id: String,
    pub date: f64,
    pub title: String,
    pub original_path: Option<String>,
    pub action: ActionKind,
    pub outcome: Outcome,
    pub message: String,
    pub trash_path: Option<String>,
    pub finding_id: Option<String>,
    pub trash_identity: Option<FileIdentity>,
    pub journal_warning: Option<String>,
}
pub fn now() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModuleTotal {
    pub module_id: String,
    pub count: usize,
    pub bytes: u64,
    pub reclaimable_bytes: u64,
}
/// Aggregate figures for a set of findings. Disk totals count each path once even when
/// findings overlap; process memory is reported separately and never added to disk usage.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Analytics {
    pub findings: usize,
    pub disk_bytes: u64,
    pub allocated_bytes: u64,
    pub reclaimable_bytes: u64,
    pub blocked: usize,
    pub process_count: usize,
    pub process_memory_bytes: u64,
    pub modules: Vec<ModuleTotal>,
}
/// Scan output delivered while modules run.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
#[allow(clippy::large_enum_variant)]
pub enum ScanEvent {
    #[serde(rename_all = "camelCase")]
    Finding {
        module_id: String,
        finding: Finding,
    },
    #[serde(rename_all = "camelCase")]
    Warning {
        module_id: String,
        message: String,
    },
    #[serde(rename_all = "camelCase")]
    Progress {
        module_id: String,
        message: String,
    },
    #[serde(rename_all = "camelCase")]
    ModuleFinished {
        module_id: String,
    },
    /// Findings were added to the engine's result store; `total` counts every stored row.
    #[serde(rename_all = "camelCase")]
    Stored {
        module_id: String,
        total: usize,
    },
    Finished {
        cancelled: bool,
    },
}
