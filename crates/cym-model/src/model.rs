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
    /// Data a tool removes with its own command. `task` names an entry in the owning
    /// module's fixed list; the command line itself is never stored or taken from input.
    Command {
        tool: String,
        task: String,
    },
}
impl Finding {
    /// The path that decides whether this finding is in a scan's scope: the folder an action
    /// would change (a simulator's data, or the item itself). Processes are scoped by their
    /// own module, by working folder or executable, so they have none.
    pub fn scope_path(&self) -> Option<&str> {
        match &self.resource {
            Resource::Process { .. } => None,
            Resource::Simulator { .. } | Resource::Command { .. } => self.value("Data path"),
            resource => resource.path(),
        }
    }
}
impl Resource {
    pub fn path(&self) -> Option<&str> {
        match self {
            Self::File { file } | Self::Worktree { file, .. } => Some(&file.path),
            Self::Process { process } => Some(&process.executable),
            Self::WorktreeRegistration { path, .. } => Some(path),
            Self::Simulator { .. } | Self::Command { .. } => None,
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
    /// Runs the owning tool's own cleanup command, such as `docker builder prune`.
    RunCommand,
    /// Compresses a folder into a checked archive beside it, then moves the folder to Trash.
    Archive,
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
    /// For a protected item the user may still remove: what they accept losing. The
    /// `acknowledged_actions` apply only when a request names this finding as acknowledged.
    #[serde(default)]
    pub acknowledgement: Option<String>,
    #[serde(default)]
    pub acknowledged_actions: Vec<ActionKind>,
}
impl Finding {
    /// Size on disk (allocated blocks), or the logical size when that is all that is known.
    /// Sparse files such as virtual disk images can be far larger logically than on disk.
    pub fn disk_bytes(&self) -> Option<u64> {
        self.allocated_bytes.or(self.bytes)
    }
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
            acknowledgement: None,
            acknowledged_actions: vec![],
        }
    }
    pub fn value(&self, label: &str) -> Option<&str> {
        self.details
            .iter()
            .find(|d| d.label == label)
            .map(|d| d.value.as_str())
    }
}
/// Process names Orphan Processes leaves out unless the user changes the list: the
/// security agents macOS and GnuPG start on demand, which are meant to outlive what
/// started them. The app, the CLI and any client that sends no list start from these.
pub const DEFAULT_IGNORED_PROCESSES: &[&str] = &["ssh-agent", "gpg-agent", "keyboxd", "dirmngr"];
fn default_ignored_processes() -> Vec<String> {
    DEFAULT_IGNORED_PROCESSES
        .iter()
        .map(|&n| n.into())
        .collect()
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanContext {
    pub roots: Vec<String>,
    #[serde(default)]
    pub exclusions: Vec<String>,
    /// `DEFAULT_IGNORED_PROCESSES` when a client sends none; an empty list ignores nothing.
    #[serde(default = "default_ignored_processes")]
    pub ignored_process_names: Vec<String>,
    #[serde(default)]
    pub limit_to_roots: bool,
}
impl Default for ScanContext {
    fn default() -> Self {
        Self {
            roots: vec![],
            exclusions: vec![],
            ignored_process_names: default_ignored_processes(),
            limit_to_roots: false,
        }
    }
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
    /// IDs of protected findings the user confirmed removing; their acknowledged actions
    /// become eligible. Blocked findings stay ineligible.
    #[serde(default)]
    pub acknowledged: Vec<String>,
    /// Proceed even though a running tool or app is using an item (`in_use`); every other
    /// check still applies.
    #[serde(default)]
    pub force: bool,
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
    /// Failed only because the item is in use; the user may force it.
    #[serde(default)]
    pub overridable: bool,
}
/// A byte count as Finder shows it, in powers of 1000: `999 bytes`, `12.3 KB`, `1.2 GB`.
/// Every message and detail uses this, so sizes read the same everywhere.
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["bytes", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 999.95 && unit < UNITS.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }
    match unit {
        0 => format!("{bytes} bytes"),
        _ => format!("{value:.1} {}", UNITS[unit]),
    }
}
/// Orders version strings as people read them, for every tool that keeps the newest: numbers
/// compare as numbers (`1.10` after `1.9`, `16` equal to `16.0`), any text before the first
/// digit is ignored (`v20.1.0`, `python-3.12`), and a pre-release (`16.1b2`, `3.13.0rc1`,
/// `1.2.0-beta.3`) comes before its release.
pub fn version_order(a: &str, b: &str) -> std::cmp::Ordering {
    fn parts(text: &str) -> (Vec<u64>, Option<Vec<u64>>) {
        let start = text
            .find(|c: char| c.is_ascii_digit())
            .unwrap_or(text.len());
        let text = &text[start..];
        let end = text
            .find(|c: char| !(c.is_ascii_digit() || c == '.'))
            .unwrap_or(text.len());
        let mut release: Vec<u64> = text[..end]
            .split('.')
            .filter_map(|p| p.parse().ok())
            .collect();
        while release.last() == Some(&0) {
            release.pop();
        }
        let rest = text[end..].trim_start_matches(['-', '.', '_']);
        let lower = rest.to_ascii_lowercase();
        let pre = ["alpha", "beta", "rc", "pre", "dev", "a", "b"]
            .iter()
            .find(|tag| {
                lower.starts_with(*tag)
                    && lower[tag.len()..]
                        .chars()
                        .next()
                        .is_none_or(|c| c.is_ascii_digit() || matches!(c, '.' | '-'))
            })
            .map(|tag| {
                // The tag's rank, then its numbers: alpha < beta < rc.
                let rank = match *tag {
                    "dev" => 0,
                    "alpha" | "a" => 1,
                    "beta" | "b" => 2,
                    _ => 3,
                };
                std::iter::once(rank)
                    .chain(
                        lower[tag.len()..]
                            .split(|c: char| !c.is_ascii_digit())
                            .filter_map(|p| p.parse().ok()),
                    )
                    .collect()
            });
        (release, pre)
    }
    let ((a_release, a_pre), (b_release, b_pre)) = (parts(a), parts(b));
    a_release
        .cmp(&b_release)
        .then_with(|| match (a_pre, b_pre) {
            (None, None) => std::cmp::Ordering::Equal,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (Some(_), None) => std::cmp::Ordering::Less,
            (Some(x), Some(y)) => x.cmp(&y),
        })
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
    /// Size on disk: allocated blocks, with hard links and nested items counted once.
    pub disk_bytes: u64,
    /// The same as `disk_bytes`; kept for older clients.
    pub allocated_bytes: u64,
    /// The items' logical length, which sparse files and clones can make exceed the disk.
    #[serde(default)]
    pub logical_bytes: u64,
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
