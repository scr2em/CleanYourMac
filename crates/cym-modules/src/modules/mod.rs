//! Registered finders. A module owns its discovery rules, the known locations its actions may
//! touch, and its own pre-action checks; the shared executor applies the common safety rules.
use crate::{model::*, ports::*, services::Services};
use rayon::prelude::*;
use std::{collections::HashSet, sync::Arc};

/// The Chromium caches, logs and crash reports in an Electron app's data folder, written
/// once for AI Tools' editors, Caches & Logs' VS Code and the leftover rule for other
/// Electron apps. `$root` goes in front of each folder and `$title` in front of each name.
/// Each entry is (folder, name, what removing it costs, whether it is a cache).
macro_rules! electron_folders {
    ($root:literal, $title:literal) => {
        [
            (
                concat!($root, "Cache"),
                concat!($title, "Web cache"),
                "Rebuilt by the app.",
                true,
            ),
            (
                concat!($root, "Code Cache"),
                concat!($title, "Script cache"),
                "Rebuilt by the app.",
                true,
            ),
            (
                concat!($root, "GPUCache"),
                concat!($title, "GPU cache"),
                "Rebuilt by the app.",
                true,
            ),
            (
                concat!($root, "Dawn*"),
                concat!($title, "Graphics cache"),
                "Rebuilt by the app.",
                true,
            ),
            (
                concat!($root, "Service Worker/CacheStorage"),
                concat!($title, "Offline web cache"),
                "Rebuilt by the app; pages download it again.",
                true,
            ),
            (
                concat!($root, "CachedData"),
                concat!($title, "Compiled code cache"),
                "Rebuilt by the app.",
                true,
            ),
            (
                concat!($root, "CachedProfilesData"),
                concat!($title, "Profile cache"),
                "Rebuilt by the app.",
                true,
            ),
            (
                concat!($root, "CachedExtensionVSIXs"),
                concat!($title, "Extension downloads"),
                "Extensions are downloaded again when needed.",
                true,
            ),
            (
                concat!($root, "logs"),
                concat!($title, "Logs"),
                "Logs of past sessions.",
                false,
            ),
            (
                concat!($root, "Crashpad/completed"),
                concat!($title, "Crash reports"),
                "Reports of past crashes.",
                false,
            ),
        ]
    };
}
pub(crate) use electron_folders;

pub mod ai;
pub mod applications;
pub mod containers;
pub mod developer;
pub mod installers;
pub mod junk;
pub mod processes;
pub mod project_folders;
pub mod projects;
pub mod simulators;
pub mod storage;
pub mod system;
pub mod toolchains;
pub mod worktrees;
pub mod xcode;

pub trait ScanModule: Send + Sync {
    fn descriptor(&self) -> ModuleDescriptor;
    fn scan(
        &self,
        services: &Services,
        context: &ScanContext,
        control: &ScanControl,
        sink: &mut dyn Sink,
    ) -> Result<()>;
    /// Known locations outside the user's chosen roots where this module's actions may apply.
    fn action_roots(&self) -> Vec<String> {
        vec![]
    }
    /// Why the item is in use right now (a running tool or app), if it is. Unlike
    /// `preflight`, the user may override this after confirming; everything else is still
    /// checked.
    fn in_use(
        &self,
        _services: &Services,
        _finding: &Finding,
        _kind: ActionKind,
    ) -> Option<String> {
        None
    }
    /// Module-specific checks run immediately before a reviewed action on one of its findings.
    fn preflight(
        &self,
        _services: &Services,
        _finding: &Finding,
        _kind: ActionKind,
        _control: &ScanControl,
    ) -> Result<()> {
        Ok(())
    }
    /// Runs the tool's own cleanup command for a `Resource::Command` finding, chosen again
    /// from the module's fixed list, and says what it did.
    fn run(
        &self,
        _services: &Services,
        _finding: &Finding,
        _context: &ScanContext,
        _control: &ScanControl,
    ) -> Result<String> {
        Err("This tool has no cleanup command.".into())
    }
    /// Compresses an `Archive` finding's folder into a checked archive beside it and returns
    /// the archive's path. The executor moves the folder to the Trash afterwards.
    fn archive(
        &self,
        _services: &Services,
        _finding: &Finding,
        _control: &ScanControl,
    ) -> Result<String> {
        Err("This tool does not archive items.".into())
    }
}

/// An ordered set of modules with unique IDs. Built-ins can be replaced or removed, and new
/// modules registered, without touching the coordinator, executor, CLI or app.
#[derive(Clone)]
pub struct Registry {
    modules: Vec<Arc<dyn ScanModule>>,
}
impl Registry {
    pub fn new(modules: Vec<Arc<dyn ScanModule>>) -> Result<Self> {
        let mut ids = HashSet::new();
        if modules.iter().any(|m| !ids.insert(m.descriptor().id)) {
            return Err("Module IDs must be unique".into());
        }
        Ok(Self { modules })
    }
    /// Adds a module, replacing any module with the same ID in place.
    pub fn register(mut self, module: Arc<dyn ScanModule>) -> Self {
        let id = module.descriptor().id;
        match self.modules.iter().position(|m| m.descriptor().id == id) {
            Some(index) => self.modules[index] = module,
            None => self.modules.push(module),
        }
        self
    }
    pub fn remove(mut self, id: &str) -> Self {
        self.modules.retain(|m| m.descriptor().id != id);
        self
    }
    pub fn get(&self, id: &str) -> Option<&Arc<dyn ScanModule>> {
        self.modules.iter().find(|m| m.descriptor().id == id)
    }
    pub fn descriptors(&self) -> Vec<ModuleDescriptor> {
        self.modules.iter().map(|m| m.descriptor()).collect()
    }
    /// Runs one module to completion and returns its report. Failures become warnings so a
    /// broken integration never looks like an empty, successful scan.
    pub fn scan(
        &self,
        id: &str,
        services: &Services,
        context: &ScanContext,
        control: &ScanControl,
    ) -> Result<ScanReport> {
        let module = self.get(id).ok_or("Unknown module")?;
        let mut report = ScanReport::default();
        let mut sink = Guard::new(&mut report, services.result_limit, context);
        run(module.as_ref(), services, context, control, &mut sink);
        report.cancelled = control.is_cancelled();
        Ok(report)
    }
}
pub fn run(
    module: &dyn ScanModule,
    services: &Services,
    context: &ScanContext,
    control: &ScanControl,
    sink: &mut dyn Sink,
) {
    // A panic in one module (for example on malformed tool data) ends only that module's
    // scan; the others continue.
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        module.scan(services, context, control, &mut *sink)
    }));
    match outcome {
        Ok(Err(error)) if !control.is_cancelled() => sink.warning(error),
        Ok(_) => {}
        Err(_) => sink.warning(format!(
            "{} stopped on unexpected data. Its other results are still listed.",
            module.descriptor().name
        )),
    }
}

/// Every finding of a module passes through here: it removes duplicate IDs, bounds the result
/// count, and enforces the scan's scope whatever the module checked itself. Findings that
/// are excluded, outside the roots of a limited scan, or in a system location are dropped;
/// a folder that holds an excluded path is listed but cannot be acted on.
pub struct Guard<'a> {
    inner: &'a mut dyn Sink,
    seen: HashSet<String>,
    limited: bool,
    limit: usize,
    context: &'a ScanContext,
}
impl<'a> Guard<'a> {
    pub fn new(inner: &'a mut dyn Sink, limit: usize, context: &'a ScanContext) -> Self {
        Self {
            inner,
            seen: HashSet::new(),
            limited: false,
            limit,
            context,
        }
    }
}
impl Sink for Guard<'_> {
    fn finding(&mut self, mut finding: Finding) {
        if let Some(path) = finding.scope_path() {
            match self.context.verdict(path) {
                crate::policy::Verdict::Drop => return,
                crate::policy::Verdict::Block(reason) => {
                    finding.blocked_reason.get_or_insert(reason);
                }
                crate::policy::Verdict::Keep => {}
            }
        }
        settle(&mut finding);
        if finding.brand.is_none() {
            finding.brand = crate::brand::of(&finding).map(Into::into);
        }
        if self.seen.len() >= self.limit {
            if !self.limited {
                self.limited = true;
                self.inner
                    .warning("Result limit reached; choose narrower folders.".into());
            }
        } else if self.seen.insert(finding.id.clone()) {
            self.inner.finding(finding);
        }
    }
    fn warning(&mut self, warning: String) {
        self.inner.warning(warning);
    }
    fn progress(&mut self, message: String) {
        self.inner.progress(message);
    }
}

/// Risk rules that hold for every tool, applied to each finding after its module labelled
/// it, so no tool can label the same thing differently.
///
/// Permanent means what is removed does not go through the Trash and cannot be restored:
/// a tool's own cleanup command (`docker builder prune`, `brew cleanup`), removing a
/// worktree, resetting a simulator, quitting a process or emptying the Trash. Whether the
/// tool rebuilds it says nothing about whether you can undo it.
fn settle(finding: &mut Finding) {
    let mut offered = finding
        .actions
        .iter()
        .chain(&finding.acknowledged_actions)
        .peekable();
    if offered.peek().is_some()
        && offered.all(|a| !matches!(a, ActionKind::Trash | ActionKind::Archive))
    {
        finding.risk = Risk::Permanent;
    }
}

/// `Candidate::irreplaceable` for a finding a module builds itself, such as a command.
pub(crate) fn confirm_first(finding: &mut Finding, loss: &str) {
    if !finding.actions.is_empty() {
        finding.acknowledged_actions = std::mem::take(&mut finding.actions);
        finding.acknowledgement = Some(loss.into());
    }
}

/// Forwards collected traversal warnings to a sink.
pub(crate) fn flush(sink: &mut dyn Sink, warnings: &mut Vec<String>) {
    for warning in warnings.drain(..) {
        sink.warning(warning);
    }
}
/// Where a candidate's "last used" date comes from.
#[derive(Clone, Default)]
pub(crate) enum LastUsed {
    #[default]
    Unknown,
    /// Finder's "Last opened" date from Spotlight.
    Spotlight,
    /// The newest change among a project's own files, ignoring generated folders.
    Project {
        folder: String,
    },
    At(f64),
}
impl LastUsed {
    fn resolve(&self, s: &Services, path: &str) -> Option<(f64, &'static str)> {
        match self {
            Self::Unknown => None,
            Self::Spotlight => s.usage.last_used(path).map(|t| (t, "Finder “Last opened”")),
            Self::Project { folder } => {
                project_activity(s, folder).map(|t| (t, "Newest change to project files"))
            }
            Self::At(t) => Some((*t, "Recorded by the owning tool")),
        }
    }
}
/// The newest modification among a project's immediate entries, skipping generated and
/// dependency folders whose timestamps change on every build.
pub(crate) fn project_activity(s: &Services, folder: &str) -> Option<f64> {
    s.fs.children(folder)
        .ok()?
        .into_iter()
        .flatten()
        // Version-control metadata changes with every commit, so it counts as activity.
        .filter(|e| !project_folders::generated_name(e.name()))
        .map(|e| e.modified())
        .reduce(f64::max)
}

/// Home-relative folders that AI Tools and Containers & VMs list with their own checks, so
/// the leftovers in Caches & Logs never list them a second time under a weaker one.
pub(crate) fn listed_elsewhere() -> Vec<String> {
    ai::AiToolsModule::default()
        .tools
        .iter()
        .flat_map(ai::AiTool::home_folders)
        .chain(containers::cache_folders().map(Into::into))
        .collect()
}

/// The version-control system whose checkout `folder` is the root of, if any; Git first,
/// since a Jujutsu or Sapling checkout may also hold a `.git`.
///
/// Names come from the folder's listing, not a lookup, so on a case-insensitive volume a
/// user's `cvs` folder is not taken for a `CVS` checkout.
pub(crate) fn checkout(s: &Services, folder: &str) -> Option<&'static str> {
    let systems: Vec<&'static str> =
        s.fs.children(folder)
            .ok()?
            .into_iter()
            .flatten()
            .filter_map(|e| crate::policy::checkout_marker(e.name()))
            .collect();
    systems
        .iter()
        .find(|system| **system == "Git")
        .or(systems.first())
        .copied()
}

/// One item to size and report.
pub(crate) struct Candidate {
    pub entry: Entry,
    pub title: Option<String>,
    pub reason: String,
    pub actions: Vec<ActionKind>,
    pub risk: Risk,
    pub details: Vec<Detail>,
    /// Overrides the generic blocked reason; callers pass no actions when set.
    pub blocked: Option<String>,
    pub last_used: LastUsed,
    /// What removing a protected item loses, and the actions the user may confirm.
    pub acknowledge: Option<(String, Vec<ActionKind>)>,
    /// Bytes removing the item frees, when that differs from its own size: an Ollama model's
    /// manifest frees the model files only it uses.
    pub frees: Option<u64>,
}
impl Candidate {
    pub fn new(entry: Entry, reason: &str, actions: Vec<ActionKind>, risk: Risk) -> Self {
        Self {
            entry,
            title: None,
            reason: reason.into(),
            actions,
            risk,
            details: vec![],
            blocked: None,
            last_used: LastUsed::Unknown,
            acknowledge: None,
            frees: None,
        }
    }
    pub fn frees(mut self, bytes: u64) -> Self {
        self.frees = Some(bytes);
        self
    }
    /// Protected, but removable once the user confirms they accept `loss`.
    /// Data nothing can make again (a device backup, an archive, a transcript): its actions
    /// are offered only after the user confirms losing it. The same rule in every tool.
    pub fn irreplaceable(mut self, loss: &str) -> Self {
        if !self.actions.is_empty() {
            let actions = std::mem::take(&mut self.actions);
            self.acknowledge = Some((loss.into(), actions));
        }
        self
    }
    pub fn acknowledge(mut self, loss: &str, actions: Vec<ActionKind>) -> Self {
        self.acknowledge = Some((loss.into(), actions));
        self
    }
    pub fn last_used(mut self, source: LastUsed) -> Self {
        self.last_used = source;
        self
    }
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }
    pub fn details(mut self, details: Vec<Detail>) -> Self {
        self.details = details;
        self
    }
    pub fn blocked(mut self, reason: Option<&str>) -> Self {
        if let Some(reason) = reason {
            self.blocked = Some(reason.into());
            self.actions.clear();
        }
        self
    }
}
/// Sizes candidates in parallel and reports them in their original order.
pub(crate) fn add_files(
    services: &Services,
    sink: &mut dyn Sink,
    module: &str,
    candidates: Vec<Candidate>,
    control: &ScanControl,
) -> Result<()> {
    control.check()?;
    if candidates.is_empty() {
        return Ok(());
    }
    let total = candidates.len();
    sink.progress(format!(
        "Sizing {total} item{}",
        if total == 1 { "" } else { "s" }
    ));
    // Size on the I/O pool and report each item the moment its size is known, so results
    // appear while the rest are still being measured. The sink stays on this thread.
    let (send, receive) = std::sync::mpsc::channel();
    std::thread::scope(|scope| {
        scope.spawn(|| {
            services.io.install(|| {
                candidates.into_par_iter().for_each_with(send, |send, c| {
                    let row = services.file_finding(
                        &c.entry,
                        module,
                        c.title.as_deref(),
                        &c.reason,
                        c.actions,
                        c.risk,
                        control,
                    );
                    let used = c.last_used.resolve(services, c.entry.path());
                    let row = row.map(|mut f| {
                        if let Some(bytes) = c.frees {
                            f.bytes = Some(bytes);
                            f.allocated_bytes = Some(bytes);
                        }
                        f
                    });
                    let _ = send.send((
                        c.entry.identity.path,
                        c.details,
                        c.blocked,
                        used,
                        c.acknowledge,
                        row,
                    ));
                });
            })
        });
        for (done, (path, details, blocked, used, acknowledge, row)) in receive.iter().enumerate() {
            match row {
                Ok(mut f) => {
                    f.details.extend(details);
                    if let Some((at, source)) = used {
                        f.last_used_at = Some(at);
                        f.details.push(detail("Last used from", source));
                    }
                    if blocked.is_some() {
                        f.blocked_reason = blocked;
                    }
                    if let Some((loss, actions)) = acknowledge {
                        f.acknowledgement = Some(loss);
                        f.acknowledged_actions = actions;
                    }
                    sink.finding(f);
                }
                Err(e) => sink.warning(format!("Cannot size {path}: {e}")),
            }
            sink.progress(format!("Sized {} of {total}", done + 1));
        }
    });
    control.check()?;
    Ok(())
}
/// Matches a name against a pattern with at most one `*` and returns the part `*` matched,
/// or the whole name for a literal pattern.
pub fn glob<'a>(pattern: &str, name: &'a str) -> Option<&'a str> {
    match pattern.split_once('*') {
        None => (pattern == name).then_some(name),
        Some((prefix, suffix)) => (name.len() >= prefix.len() + suffix.len()
            && name.starts_with(prefix)
            && name.ends_with(suffix))
        .then(|| &name[prefix.len()..name.len() - suffix.len()]),
    }
}
/// Matches a name against a pattern in which each `*` stands for any run of characters.
pub(crate) fn wildcard(pattern: &str, name: &str) -> bool {
    let mut pieces = pattern.split('*');
    let first = pieces.next().unwrap_or_default();
    let Some(mut rest) = name.strip_prefix(first) else {
        return false;
    };
    let pieces: Vec<&str> = pieces.collect();
    for (i, piece) in pieces.iter().enumerate() {
        if i + 1 == pieces.len() {
            return rest.len() >= piece.len() && rest.ends_with(piece);
        }
        match rest.find(piece) {
            Some(at) => rest = &rest[at + piece.len()..],
            None => return false,
        }
    }
    rest.is_empty()
}
/// Fails while an app whose bundle identifier starts with one of `prefixes` is running.
pub(crate) fn owners_closed(services: &Services, prefixes: &[&str]) -> Result<()> {
    if prefixes.is_empty() {
        return Ok(());
    }
    let running = services.apps.running()?;
    match running
        .iter()
        .find(|a| prefixes.iter().any(|p| a.bundle_id.starts_with(p)))
    {
        Some(app) => Err(format!(
            "Quit {} before removing its data.",
            std::path::Path::new(&app.path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(&app.bundle_id)
        )),
        None => Ok(()),
    }
}
/// Bundle-identifier prefixes of apps that own developer data or app caches.
pub mod owners {
    pub const XCODE: &[&str] = &["com.apple.dt.Xcode", "com.apple.iphonesimulator"];
    pub const ANDROID: &[&str] = &["com.google.android.studio", "com.jetbrains."];
    pub const JETBRAINS: &[&str] = &["com.jetbrains."];
    pub const VSCODE: &[&str] = &["com.microsoft.VSCode"];
    pub const UNITY: &[&str] = &["com.unity3d.UnityEditor", "com.unity3d.unityhub"];
    pub const UNREAL: &[&str] = &["com.epicgames.UnrealEditor"];
    pub const CHROME: &[&str] = &["com.google.Chrome"];
    pub const EDGE: &[&str] = &["com.microsoft.edgemac"];
    pub const BRAVE: &[&str] = &["com.brave.Browser"];
    pub const STEAM: &[&str] = &["com.valvesoftware.steam"];
    pub const ADOBE: &[&str] = &[
        "com.adobe.PremierePro",
        "com.adobe.AfterEffects",
        "com.adobe.ame",
        "com.adobe.Audition",
    ];
}
pub(crate) fn descriptor(
    id: &str,
    name: &str,
    category: &str,
    symbol: &str,
    summary: &str,
    uses_roots: bool,
) -> ModuleDescriptor {
    ModuleDescriptor::new(id, name, category, symbol, summary, uses_roots)
}

/// The modules shipped with CleanYourMac, in sidebar order.
pub fn builtin() -> Registry {
    Registry::new(vec![
        Arc::new(storage::StorageModule),
        Arc::new(storage::LargeFilesModule::default()),
        Arc::new(storage::DuplicatesModule::default()),
        Arc::new(developer::DependenciesModule::default()),
        Arc::new(developer::ArtifactsModule::default()),
        Arc::new(worktrees::WorktreeModule),
        Arc::new(projects::ProjectsModule::default()),
        Arc::new(simulators::SimulatorModule),
        Arc::new(xcode::XcodeModule::default()),
        Arc::new(developer::CachesModule::default()),
        Arc::new(toolchains::ToolchainsModule::default()),
        Arc::new(containers::ContainersModule::default()),
        Arc::new(ai::AiToolsModule::default()),
        Arc::new(applications::ApplicationsModule),
        Arc::new(applications::LeftoversModule),
        Arc::new(storage::FolderModule::downloads()),
        Arc::new(installers::InstallersModule::default()),
        Arc::new(storage::FolderModule::trash()),
        Arc::new(system::SystemModule::default()),
        Arc::new(processes::OrphanModule),
    ])
    .expect("Built-in module IDs are unique")
}
