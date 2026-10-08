//! The composed core: adapters plus registered modules. The app bridge, the CLI and tests
//! all drive the same `Engine`.
use crate::{
    actions, analytics,
    model::*,
    modules::{self, Guard, Registry, ScanModule},
    ports::*,
    results::ResultStore,
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
    /// The app's current result set; scans started with `store` write here.
    pub results: Arc<ResultStore>,
    /// Set while the store holds example data loaded by the app (demo and screenshots)
    /// rather than findings this engine scanned; no action runs on it.
    pub example: Arc<std::sync::atomic::AtomicBool>,
    /// Held while example data is loaded or replaced, so the flag and the rows change
    /// together.
    examples: Arc<std::sync::Mutex<()>>,
}
impl Engine {
    pub fn new(services: Services, registry: Registry) -> Self {
        Self {
            services,
            registry,
            results: Arc::default(),
            example: Default::default(),
            examples: Default::default(),
        }
    }
    /// Replaces the results with example data (demo and screenshots); no action ever
    /// changes anything for it.
    pub fn load_examples(&self, findings: Vec<Finding>) -> usize {
        let _guard = self.examples.lock().unwrap_or_else(|e| e.into_inner());
        self.example
            .store(true, std::sync::atomic::Ordering::SeqCst);
        self.results.clear();
        self.results.insert(findings);
        self.results.len()
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
        self.run(ids, context, control, concurrency, emit, None)
    }
    /// Like `scan`, but findings replace those modules' rows in `results` and only throttled
    /// `Stored` counts are emitted, so very large result sets never stream through `emit`.
    pub fn scan_to_store(
        &self,
        ids: &[String],
        context: &ScanContext,
        control: &ScanControl,
        concurrency: usize,
        emit: &(dyn Fn(ScanEvent) + Sync),
    ) {
        // A real scan replaces example data entirely, so no example row stays beside it.
        {
            let _guard = self.examples.lock().unwrap_or_else(|e| e.into_inner());
            if self
                .example
                .swap(false, std::sync::atomic::Ordering::SeqCst)
            {
                self.results.clear();
            }
            self.results.clear_modules(ids);
        }
        self.run(
            ids,
            context,
            control,
            concurrency,
            emit,
            Some(&self.results),
        )
    }
    fn run(
        &self,
        ids: &[String],
        context: &ScanContext,
        control: &ScanControl,
        concurrency: usize,
        emit: &(dyn Fn(ScanEvent) + Sync),
        store: Option<&ResultStore>,
    ) {
        let context = &self.services.scoped(context);
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
                        epoch: store.map_or(0, |s| s.epoch(&descriptor.id)),
                        store,
                        buffer: vec![],
                        last_stored: None,
                    };
                    let mut guard = Guard::new(&mut sink, self.services.result_limit, context);
                    modules::run(
                        module.as_ref(),
                        &self.services,
                        context,
                        control,
                        &mut guard,
                    );
                    sink.flush(true);
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
    /// Drops stored results whose files were deleted outside the app; returns their IDs.
    pub fn prune_missing(&self) -> Vec<String> {
        let services = &self.services;
        services
            .io
            .install(|| self.results.remove_missing(&|path| services.exists(path)))
    }
    pub fn execute(&self, request: &ActionRequest, control: &ScanControl) -> Vec<ActionResult> {
        let request = ActionRequest {
            context: self.services.scoped(&request.context),
            ..request.clone()
        };
        self.services
            .io
            .install(|| actions::execute(&self.services, &self.registry, &request, control))
    }
    /// Executes an action on stored findings by ID and drops rows that were applied or are
    /// already gone.
    pub fn execute_ids(
        &self,
        ids: &[String],
        kind: ActionKind,
        context: &ScanContext,
        control: &ScanControl,
    ) -> Vec<ActionResult> {
        self.execute_acknowledged(ids, kind, context, &[], false, control)
    }
    /// `execute_ids`, also allowing protected findings the user confirmed removing.
    pub fn execute_acknowledged(
        &self,
        ids: &[String],
        kind: ActionKind,
        context: &ScanContext,
        acknowledged: &[String],
        force: bool,
        control: &ScanControl,
    ) -> Vec<ActionResult> {
        if self.example.load(std::sync::atomic::Ordering::SeqCst) {
            return ids
                .iter()
                .map(|id| ActionResult {
                    message: "These are example results; nothing on your Mac was changed.".into(),
                    ..actions::unlisted(id, kind)
                })
                .collect();
        }
        let request = ActionRequest {
            findings: self.results.findings(ids),
            kind,
            context: context.clone(),
            acknowledged: acknowledged.to_vec(),
            force,
        };
        let mut results = self.execute(&request, control);
        // IDs no longer in the results (removed elsewhere, or out of scope now) are reported,
        // so a batch summary counts every item the user selected.
        let found: std::collections::HashSet<&str> =
            request.findings.iter().map(|f| f.id.as_str()).collect();
        for id in ids.iter().filter(|id| !found.contains(id.as_str())) {
            results.push(actions::unlisted(id, kind));
        }
        let done: Vec<String> = results
            .iter()
            .filter(|r| matches!(r.outcome, Outcome::Applied | Outcome::Skipped))
            .filter_map(|r| r.finding_id.clone())
            .collect();
        self.results.remove(&done);
        results
    }
    /// Forgets results outside `context`, after the user changes what to scan or protects a
    /// folder. Returns the removed IDs.
    pub fn retain_in_scope(&self, context: &ScanContext) -> Vec<String> {
        self.results.retain_in_scope(&self.services.scoped(context))
    }
    /// Restores an action from the journal by its ID; the paths come from the journal's own
    /// record, never from the caller.
    pub fn restore(&self, row: &ActionResult) -> Result<()> {
        let recorded = self
            .history()
            .into_iter()
            .find(|r| r.id == row.id)
            .ok_or("This action is not in the local history.")?;
        actions::restore(&self.services, &recorded, &ScanControl::default())
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

/// Labels a module's output and throttles progress so the UI is not flooded. With a store,
/// findings are inserted in batches and only their running count is emitted.
struct Forward<'a> {
    id: String,
    name: String,
    send: &'a (dyn Fn(ScanEvent) + Sync),
    last_progress: Option<Instant>,
    store: Option<&'a ResultStore>,
    buffer: Vec<Finding>,
    last_stored: Option<Instant>,
    /// The module's store epoch when this scan started.
    epoch: u64,
}
impl Forward<'_> {
    fn flush(&mut self, finished: bool) {
        let Some(store) = self.store else { return };
        // A newer scan of this module cleared it; these rows belong to an older scope.
        if !store.insert_at(&self.id, self.epoch, std::mem::take(&mut self.buffer)) {
            return;
        }
        if finished
            || self
                .last_stored
                .is_none_or(|t| t.elapsed() >= Duration::from_millis(250))
        {
            self.last_stored = Some(Instant::now());
            (self.send)(ScanEvent::Stored {
                module_id: self.id.clone(),
                total: store.len(),
            });
        }
    }
}
impl Sink for Forward<'_> {
    fn finding(&mut self, finding: Finding) {
        if self.store.is_some() {
            self.buffer.push(finding);
            if self.buffer.len() >= 2_048 {
                self.flush(false);
            }
            return;
        }
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
