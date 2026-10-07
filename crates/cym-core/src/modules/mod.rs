//! Registered finders. A module owns its discovery rules, the known locations its actions may
//! touch, and its own pre-action checks; the shared executor applies the common safety rules.
use crate::{model::*, ports::*, services::Services};
use rayon::prelude::*;
use std::{collections::HashSet, sync::Arc};

pub mod applications;
pub mod developer;
pub mod processes;
pub mod simulators;
pub mod storage;
pub mod worktrees;

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
        let mut sink = Guard::new(&mut report, services.result_limit);
        run(module.as_ref(), services, context, control, &mut sink);
        report.cancelled = control.is_cancelled();
        Ok(report)
    }
}
pub(crate) fn run(
    module: &dyn ScanModule,
    services: &Services,
    context: &ScanContext,
    control: &ScanControl,
    sink: &mut dyn Sink,
) {
    if let Err(error) = module.scan(services, context, control, sink) {
        if !control.is_cancelled() {
            sink.warning(error);
        }
    }
}

/// Removes duplicate finding IDs and bounds the result count for one module.
pub(crate) struct Guard<'a> {
    inner: &'a mut dyn Sink,
    seen: HashSet<String>,
    limited: bool,
    limit: usize,
}
impl<'a> Guard<'a> {
    pub(crate) fn new(inner: &'a mut dyn Sink, limit: usize) -> Self {
        Self {
            inner,
            seen: HashSet::new(),
            limited: false,
            limit,
        }
    }
}
impl Sink for Guard<'_> {
    fn finding(&mut self, finding: Finding) {
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
        .filter(|e| {
            let name = e.name().to_lowercase();
            !crate::policy::DUPLICATE_IGNORES.contains(&name.as_str()) || name == ".git"
        })
        .map(|e| e.modified())
        .reduce(f64::max)
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
        }
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
    sink.progress(format!(
        "Sizing {} item{}",
        candidates.len(),
        if candidates.len() == 1 { "" } else { "s" }
    ));
    let rows: Vec<_> = services.io.install(|| {
        candidates
            .into_par_iter()
            .map(|c| {
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
                (c.entry.identity.path, c.details, c.blocked, used, row)
            })
            .collect()
    });
    control.check()?;
    for (path, details, blocked, used, row) in rows {
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
                sink.finding(f);
            }
            Err(e) => sink.warning(format!("Cannot size {path}: {e}")),
        }
    }
    Ok(())
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
        Arc::new(developer::NodeModule::default()),
        Arc::new(developer::ArtifactsModule::default()),
        Arc::new(worktrees::WorktreeModule),
        Arc::new(simulators::SimulatorModule),
        Arc::new(developer::XcodeModule),
        Arc::new(developer::CachesModule::default()),
        Arc::new(applications::ApplicationsModule),
        Arc::new(applications::LeftoversModule),
        Arc::new(storage::FolderModule::downloads()),
        Arc::new(storage::FolderModule::trash()),
        Arc::new(processes::OrphanModule),
    ])
    .expect("Built-in module IDs are unique")
}
