//! The composed core: adapters plus registered modules. The app bridge, the CLI and tests
//! all drive the same `Engine`.
use crate::{
    actions, analytics,
    model::*,
    modules::{self, Guard, Registry, ScanModule},
    ports::*,
    services::Services,
};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

#[derive(Clone)]
pub struct Engine {
    pub services: Services,
    pub registry: Registry,
}
impl Engine {
    pub fn new(services: Services, registry: Registry) -> Self {
        Self { services, registry }
    }
    pub fn native() -> Self {
        Self::new(Services::native(), modules::builtin())
    }

    /// Runs modules concurrently and streams their events. `emit` is called from worker
    /// threads, one call at a time, and receives `ScanEvent::Finished` exactly once, last.
    pub fn scan(
        &self,
        ids: &[String],
        context: &ScanContext,
        control: &ScanControl,
        concurrency: usize,
        emit: &(dyn Fn(ScanEvent) + Sync),
    ) {
        let emit = Mutex::new(emit);
        let send = |event: ScanEvent| {
            if let Ok(emit) = emit.lock() {
                emit(event)
            }
        };
        let mut queue: VecDeque<Arc<dyn ScanModule>> = VecDeque::new();
        for id in ids {
            match self.registry.get(id) {
                Some(module) => queue.push_back(module.clone()),
                None => send(ScanEvent::Warning {
                    module_id: id.clone(),
                    message: format!("Unknown module: {id}"),
                }),
            }
        }
        let workers = concurrency.clamp(1, queue.len().max(1));
        let queue = Mutex::new(queue);
        thread::scope(|scope| {
            for _ in 0..workers {
                scope.spawn(|| loop {
                    if control.is_cancelled() {
                        break;
                    }
                    let Some(module) = queue.lock().ok().and_then(|mut q| q.pop_front()) else {
                        break;
                    };
                    let descriptor = module.descriptor();
                    let mut sink = Forward {
                        id: descriptor.id.clone(),
                        name: descriptor.name.clone(),
                        send: &send,
                        last_progress: None,
                    };
                    let mut guard = Guard::new(&mut sink);
                    modules::run(
                        module.as_ref(),
                        &self.services,
                        context,
                        control,
                        &mut guard,
                    );
                    send(ScanEvent::ModuleFinished {
                        module_id: descriptor.id,
                    });
                });
            }
        });
        send(ScanEvent::Finished {
            cancelled: control.is_cancelled(),
        });
    }
    /// Collects a scan of the given modules into a single report.
    pub fn scan_report(
        &self,
        ids: &[String],
        context: &ScanContext,
        control: &ScanControl,
    ) -> ScanReport {
        let report = Mutex::new(ScanReport::default());
        self.scan(ids, context, control, 3, &|event| {
            let Ok(mut r) = report.lock() else { return };
            match event {
                ScanEvent::Finding { finding, .. } => r.finding(finding),
                ScanEvent::Warning { message, .. } => r.warning(message),
                ScanEvent::Finished { cancelled } => r.cancelled = cancelled,
                _ => {}
            }
        });
        report.into_inner().unwrap_or_default()
    }
    pub fn execute(&self, request: &ActionRequest, control: &ScanControl) -> Vec<ActionResult> {
        actions::execute(&self.services, &self.registry, request, control)
    }
    pub fn restore(&self, row: &ActionResult) -> Result<()> {
        actions::restore(&self.services, row, &ScanControl::default())
    }
    pub fn history(&self) -> Vec<ActionResult> {
        self.services.journal.read()
    }
    pub fn clear_history(&self) -> Result<()> {
        self.services.journal.clear()
    }
    pub fn analytics(&self, findings: &[Finding]) -> Analytics {
        analytics::analytics(findings)
    }
    /// Existing conventional project folders, used as default roots.
    pub fn project_roots(&self) -> Vec<String> {
        self.services.roots(
            &["projects", "Projects", "Code", "dev", "GitHub", "Workspace"]
                .iter()
                .map(|p| format!("{}/{p}", crate::policy::home()))
                .filter(|p| self.services.is_dir(p))
                .collect::<Vec<_>>(),
        )
    }
}

/// Labels a module's output and throttles progress so the UI is not flooded.
struct Forward<'a> {
    id: String,
    name: String,
    send: &'a (dyn Fn(ScanEvent) + Sync),
    last_progress: Option<Instant>,
}
impl Sink for Forward<'_> {
    fn finding(&mut self, finding: Finding) {
        (self.send)(ScanEvent::Finding {
            module_id: self.id.clone(),
            finding,
        });
    }
    fn warning(&mut self, warning: String) {
        (self.send)(ScanEvent::Warning {
            module_id: self.id.clone(),
            message: format!("{}: {warning}", self.name),
        });
    }
    fn progress(&mut self, message: String) {
        if self
            .last_progress
            .is_some_and(|t| t.elapsed() < Duration::from_millis(100))
        {
            return;
        }
        self.last_progress = Some(Instant::now());
        (self.send)(ScanEvent::Progress {
            module_id: self.id.clone(),
            message: format!("{} · {message}", self.name),
        });
    }
}
