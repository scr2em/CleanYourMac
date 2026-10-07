//! Registered finders. A module owns its discovery rules, the known locations its actions may
//! touch, and its own pre-action checks; the shared executor applies the common safety rules.
use crate::{model::*, ports::*, services::Services};
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
        let mut sink = Guard::new(&mut report);
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
}
impl<'a> Guard<'a> {
    pub(crate) fn new(inner: &'a mut dyn Sink) -> Self {
        Self {
            inner,
            seen: HashSet::new(),
            limited: false,
        }
    }
}
impl Sink for Guard<'_> {
    fn finding(&mut self, finding: Finding) {
        if self.seen.len() >= 30_000 {
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
#[allow(clippy::too_many_arguments)]
pub(crate) fn add_file(
    services: &Services,
    sink: &mut dyn Sink,
    entry: &Entry,
    module: &str,
    title: Option<&str>,
    reason: &str,
    actions: Vec<ActionKind>,
    risk: Risk,
    control: &ScanControl,
) -> Result<()> {
    control.check()?;
    sink.progress(format!("Sizing {}", entry.name()));
    match services.file_finding(entry, module, title, reason, actions, risk, control) {
        Ok(f) => sink.finding(f),
        Err(e) => {
            control.check()?;
            sink.warning(format!("Cannot size {}: {e}", entry.path()))
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
