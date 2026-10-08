//! Versioned C ABI. Requests and results are UTF-8 JSON. Every string returned by this
//! library is released with `cym_string_free`; see `include/cym_core.h` for ownership rules.
use crate::{
    adapters::journal::JsonJournal,
    model::*,
    modules, policy,
    ports::ScanControl,
    results::{Query, SnapshotInfo},
    services::Services,
    Engine,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    ffi::{c_char, c_void, CStr, CString},
    panic::{catch_unwind, AssertUnwindSafe},
    sync::Arc,
};

pub const ABI_VERSION: u32 = 1;

pub struct CymEngine(Arc<Engine>);
pub struct CymScan(ScanControl);
pub type CymScanCallback = extern "C" fn(context: *mut c_void, event_json: *const c_char);

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct Config {
    journal_path: Option<String>,
}
#[derive(Deserialize)]
struct Request {
    method: String,
    #[serde(default)]
    params: Value,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScanRequest {
    modules: Vec<String>,
    context: ScanContext,
    #[serde(default = "concurrency")]
    concurrency: usize,
    /// Write findings to the engine's result store instead of streaming each one.
    #[serde(default)]
    store: bool,
}
fn concurrency() -> usize {
    3
}

unsafe fn text(p: *const c_char) -> Option<String> {
    (!p.is_null()).then(|| CStr::from_ptr(p).to_string_lossy().into_owned())
}
fn owned(s: String) -> *mut c_char {
    CString::new(s)
        .unwrap_or_else(|_| CString::new("{\"ok\":false,\"error\":\"Invalid string\"}").unwrap())
        .into_raw()
}
fn param<T: for<'de> Deserialize<'de>>(params: &Value, key: &str) -> Result<T> {
    serde_json::from_value(params.get(key).cloned().unwrap_or(Value::Null))
        .map_err(|e| format!("Invalid parameter {key}: {e}"))
}
fn value<T: serde::Serialize>(v: T) -> Result<Value> {
    serde_json::to_value(v).map_err(|e| e.to_string())
}

/// Dispatches one synchronous request. Shared by the C ABI and tests.
pub fn call(engine: &Engine, request: &str) -> Value {
    let response = serde_json::from_str::<Request>(request)
        .map_err(|e| format!("Invalid request: {e}"))
        .and_then(|r| dispatch(engine, &r.method, &r.params));
    match response {
        Ok(result) => json!({"ok": true, "result": result}),
        Err(error) => json!({"ok": false, "error": error}),
    }
}
fn dispatch(engine: &Engine, method: &str, p: &Value) -> Result<Value> {
    let s = &engine.services;
    match method {
        "version" => value(ABI_VERSION),
        "modules" => value(engine.registry.descriptors()),
        "projectRoots" => value(engine.project_roots()),
        "normalizeRoots" => value(s.roots(&param::<Vec<String>>(p, "paths")?)),
        "pathContains" => value(policy::contains(
            &param::<String>(p, "path")?,
            &param::<String>(p, "root")?,
        )),
        "isDirectory" => value(s.is_dir(&param::<String>(p, "path")?)),
        "normalizedSelection" => value(policy::normalized_selection(&param::<Vec<Finding>>(
            p, "findings",
        )?)),
        "analytics" => value(engine.analytics(&param::<Vec<Finding>>(p, "findings")?)),
        // Acts on the stored findings with these IDs; a caller cannot supply a finding's
        // path, actions or details.
        "execute" => {
            let request: ActionRequest = param(p, "request")?;
            let ids: Vec<String> = request.findings.iter().map(|f| f.id.clone()).collect();
            value(engine.execute_acknowledged(
                &ids,
                request.kind,
                &request.context,
                &request.acknowledged,
                request.force,
                &ScanControl::default(),
            ))
        }
        "history" => value(engine.history()),
        "clearHistory" => engine.clear_history().map(|_| Value::Null),
        "restore" => engine
            .restore(&param::<ActionResult>(p, "result")?)
            .map(|_| Value::Null),
        "query" => {
            let snapshot = engine.results.query(&param::<Query>(p, "query")?);
            value(SnapshotInfo::from(snapshot.as_ref()))
        }
        "rows" => {
            let snapshot = engine
                .results
                .snapshot(param(p, "queryId")?)
                .ok_or("This result view expired; query again.")?;
            value(snapshot.page(param(p, "offset")?, param::<usize>(p, "limit")?.min(5_000)))
        }
        "pruneMissing" => value(engine.prune_missing()),
        "recommendations" => value(
            engine.results.recommendations(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0.0, |d| d.as_secs_f64()),
            ),
        ),
        "eligibleIds" => {
            let snapshot = engine
                .results
                .snapshot(param(p, "queryId")?)
                .ok_or("This result view expired; query again.")?;
            value(snapshot.eligible_ids(param(p, "offset")?, param(p, "limit")?))
        }
        "position" => {
            let snapshot = engine
                .results
                .snapshot(param(p, "queryId")?)
                .ok_or("This result view expired; query again.")?;
            value(snapshot.position(&param::<String>(p, "id")?))
        }
        "finding" => value(
            engine
                .results
                .get(&param::<String>(p, "id")?)
                .map(|f| f.as_ref().clone()),
        ),
        "selection" => value(engine.results.selection(
            &param::<Vec<String>>(p, "ids")?,
            param::<Option<usize>>(p, "preview")?.unwrap_or(200),
        )),
        "executeSelection" => value(engine.execute_acknowledged(
            &param::<Vec<String>>(p, "ids")?,
            param(p, "kind")?,
            &param(p, "context")?,
            &param::<Option<Vec<String>>>(p, "acknowledged")?.unwrap_or_default(),
            param::<Option<bool>>(p, "force")?.unwrap_or(false),
            &ScanControl::default(),
        )),
        "retainInScope" => value(engine.retain_in_scope(&param(p, "context")?)),
        "removeResults" => value(engine.results.remove(&param::<Vec<String>>(p, "ids")?)),
        "loadResults" => {
            engine.results.clear();
            engine.results.insert(param(p, "findings")?);
            value(engine.results.len())
        }
        "synthesize" => {
            engine.results.clear();
            engine.results.insert(crate::results::synthetic(
                param::<usize>(p, "count")?.min(5_000_000),
            ));
            value(engine.results.len())
        }
        "resultCount" => value(json!({
            "total": engine.results.len(),
            "generation": engine.results.generation(),
        })),
        "scan" => {
            let request: ScanRequest =
                serde_json::from_value(p.clone()).map_err(|e| e.to_string())?;
            value(engine.scan_report(&request.modules, &request.context, &ScanControl::default()))
        }
        _ => Err(format!("Unknown method: {method}")),
    }
}

#[no_mangle]
pub extern "C" fn cym_abi_version() -> u32 {
    ABI_VERSION
}

/// # Safety
/// `config_json` must be NULL or a valid NUL-terminated string.
#[no_mangle]
pub unsafe extern "C" fn cym_engine_new(config_json: *const c_char) -> *mut CymEngine {
    catch_unwind(|| {
        let config: Config = match text(config_json) {
            Some(json) if !json.trim().is_empty() => match serde_json::from_str(&json) {
                Ok(c) => c,
                Err(_) => return std::ptr::null_mut(),
            },
            _ => Config::default(),
        };
        let services = match config.journal_path {
            Some(path) => Services::with_journal(Arc::new(JsonJournal::new(path))),
            None => Services::native(),
        };
        Box::into_raw(Box::new(CymEngine(Arc::new(Engine::new(
            services,
            modules::builtin(),
        )))))
    })
    .unwrap_or(std::ptr::null_mut())
}

/// # Safety
/// `engine` must be NULL or a pointer from `cym_engine_new` not yet freed. Running scans keep
/// their own reference and finish normally.
#[no_mangle]
pub unsafe extern "C" fn cym_engine_free(engine: *mut CymEngine) {
    if !engine.is_null() {
        drop(Box::from_raw(engine));
    }
}

/// Blocking request. Never returns NULL. May take seconds (actions); call off the main thread.
///
/// # Safety
/// `engine` must be a live engine and `request_json` a valid NUL-terminated string.
#[no_mangle]
pub unsafe extern "C" fn cym_call(
    engine: *const CymEngine,
    request_json: *const c_char,
) -> *mut c_char {
    let response = catch_unwind(AssertUnwindSafe(|| {
        let Some(engine) = engine.as_ref() else {
            return json!({"ok": false, "error": "Missing engine"});
        };
        call(&engine.0, &text(request_json).unwrap_or_default())
    }))
    .unwrap_or_else(|_| json!({"ok": false, "error": "Internal core failure"}));
    owned(response.to_string())
}

/// # Safety
/// `s` must be NULL or a string returned by this library, freed at most once.
#[no_mangle]
pub unsafe extern "C" fn cym_string_free(s: *mut c_char) {
    if !s.is_null() {
        drop(CString::from_raw(s));
    }
}

struct Context(*mut c_void);
unsafe impl Send for Context {}
unsafe impl Sync for Context {}
impl Context {
    fn get(&self) -> *mut c_void {
        self.0
    }
}

/// Starts a scan on a background thread. Events are delivered to `callback` one at a time,
/// from that thread or its workers; the event string is valid only during the call. The last
/// event is always `{"type":"finished",...}`, after which `context` is never used again.
/// Returns NULL, with no callbacks, if the request is invalid.
///
/// # Safety
/// `engine` must be live, `request_json` a valid string, and `context` must remain valid
/// until the finished event.
#[no_mangle]
pub unsafe extern "C" fn cym_scan_start(
    engine: *const CymEngine,
    request_json: *const c_char,
    callback: CymScanCallback,
    context: *mut c_void,
) -> *mut CymScan {
    let Some(engine) = engine.as_ref() else {
        return std::ptr::null_mut();
    };
    let Ok(request) = serde_json::from_str::<ScanRequest>(&text(request_json).unwrap_or_default())
    else {
        return std::ptr::null_mut();
    };
    let engine = engine.0.clone();
    let control = ScanControl::default();
    let worker = control.clone();
    let context = Context(context);
    std::thread::spawn(move || {
        let context = context;
        let deliver = |event: &ScanEvent| {
            let json = serde_json::to_string(event).unwrap_or_default();
            if let Ok(json) = CString::new(json) {
                callback(context.get(), json.as_ptr());
            }
        };
        let finished = catch_unwind(AssertUnwindSafe(|| {
            let emit = |event: ScanEvent| deliver(&event);
            if request.store {
                engine.scan_to_store(
                    &request.modules,
                    &request.context,
                    &worker,
                    request.concurrency,
                    &emit,
                )
            } else {
                engine.scan(
                    &request.modules,
                    &request.context,
                    &worker,
                    request.concurrency,
                    &emit,
                )
            }
        }));
        if finished.is_err() {
            deliver(&ScanEvent::Warning {
                module_id: String::new(),
                message: "Internal scan failure".into(),
            });
            deliver(&ScanEvent::Finished { cancelled: true });
        }
    });
    Box::into_raw(Box::new(CymScan(control)))
}

/// Requests cancellation. The finished event still follows.
///
/// # Safety
/// `scan` must be NULL or a live handle from `cym_scan_start`.
#[no_mangle]
pub unsafe extern "C" fn cym_scan_cancel(scan: *const CymScan) {
    if let Some(scan) = scan.as_ref() {
        scan.0.cancel();
    }
}

/// Cancels the scan and releases the handle. Callbacks may still arrive until finished.
///
/// # Safety
/// `scan` must be NULL or a live handle from `cym_scan_start`, freed at most once.
#[no_mangle]
pub unsafe extern "C" fn cym_scan_free(scan: *mut CymScan) {
    if !scan.is_null() {
        let scan = Box::from_raw(scan);
        scan.0.cancel();
    }
}
