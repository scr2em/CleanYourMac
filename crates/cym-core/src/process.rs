// Orphan rules adapted from the owner's MIT OrphanBar project; see THIRD_PARTY_NOTICES.md.
use crate::{command, files::ScanControl, model::*, modules::ScanModule, native, policy};
use std::{
    collections::{HashMap, HashSet},
    path::Path,
    sync::{Mutex, OnceLock},
    thread,
    time::{Duration, Instant},
};

#[repr(C)]
struct RawProcess {
    pid: i32,
    uid: u32,
    parent: i32,
    started_sec: u64,
    started_usec: u64,
    cpu_nanos: u64,
    memory: u64,
    has_usage: u8,
    path: [libc::c_char; 4096],
    cwd: [libc::c_char; 1024],
    command: [libc::c_char; 16384],
}
#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn cym_processes(buffer: *mut i32, capacity: i32) -> i32;
    fn cym_inspect_process(pid: i32, out: *mut RawProcess) -> i32;
}
#[derive(Clone, Debug)]
pub struct Snapshot {
    pub identity: ProcessIdentity,
    pub parent: i32,
    pub name: String,
    pub command: String,
    pub cwd: Option<String>,
    pub cpu: Option<u64>,
    pub memory: Option<u64>,
}
#[cfg(target_os = "macos")]
pub fn inspect(pid: i32) -> Option<Snapshot> {
    unsafe {
        let mut raw: RawProcess = std::mem::zeroed();
        if cym_inspect_process(pid, &mut raw) == 0 {
            return None;
        }
        let string =
            |p: *const libc::c_char| std::ffi::CStr::from_ptr(p).to_string_lossy().into_owned();
        let path = string(raw.path.as_ptr());
        let name = Path::new(&path).file_name()?.to_str()?.into();
        let cwd = string(raw.cwd.as_ptr());
        let command = string(raw.command.as_ptr());
        Some(Snapshot {
            identity: ProcessIdentity {
                pid,
                uid: raw.uid,
                started_seconds: raw.started_sec,
                started_microseconds: raw.started_usec,
                executable: path.clone(),
            },
            parent: raw.parent,
            name,
            command: if command.is_empty() { path } else { command },
            cwd: (!cwd.is_empty()).then_some(cwd),
            cpu: (raw.has_usage != 0).then_some(raw.cpu_nanos),
            memory: (raw.has_usage != 0).then_some(raw.memory),
        })
    }
}
#[cfg(not(target_os = "macos"))]
pub fn inspect(_: i32) -> Option<Snapshot> {
    None
}
#[cfg(target_os = "macos")]
fn pids() -> Vec<i32> {
    unsafe {
        let count = cym_processes(std::ptr::null_mut(), 0);
        if count <= 0 {
            return vec![];
        }
        let mut rows = vec![0; count as usize + 256];
        let actual = cym_processes(rows.as_mut_ptr(), rows.len() as i32);
        rows.truncate((actual.max(0) as usize).min(rows.len()));
        rows
    }
}
#[cfg(not(target_os = "macos"))]
fn pids() -> Vec<i32> {
    vec![]
}
pub fn exclusion(
    p: &Snapshot,
    uid: u32,
    own: i32,
    managed: &HashSet<i32>,
    apps: &[native::RunningApp],
    ignored: &[String],
) -> Option<&'static str> {
    if p.identity.uid != uid {
        return Some("Another user's process");
    }
    if p.identity.pid == own || p.identity.pid <= 1 {
        return Some("App or launchd");
    }
    if p.parent != 1 {
        return Some("Has a living parent");
    }
    if managed.contains(&p.identity.pid) {
        return Some("Managed launchd job");
    }
    if apps.iter().any(|a| a.pid == p.identity.pid) {
        return Some("Running application");
    }
    if ignored.contains(&p.name) {
        return Some("Ignored process name");
    }
    let path = &p.identity.executable;
    if [
        "/System/",
        "/usr/libexec/",
        "/usr/sbin/",
        "/sbin/",
        "/Library/Apple/",
        "/Library/Developer/PrivateFrameworks/",
        "/Library/Developer/CoreSimulator/",
        "/Library/Filesystems/",
        "/Library/PrivilegedHelperTools/",
    ]
    .iter()
    .any(|prefix| path.starts_with(prefix))
        || path.contains(".xpc/")
        || path.contains(".appex/")
    {
        return Some("System component or service");
    }
    if let Some((bundle, _)) = path.split_once(".app/") {
        let bundle = bundle.to_owned() + ".app";
        if apps.iter().any(|a| a.path == bundle) {
            return Some("Helper of a running app");
        }
    }
    None
}
fn managed(k: &ScanControl) -> Result<HashSet<i32>> {
    let out = command::run(
        "/bin/launchctl",
        &["list".into()],
        Duration::from_secs(5),
        k,
    )?;
    if out.status != 0 || !out.text().starts_with("PID") {
        return Err("Managed-service inspection failed; orphan classification is incomplete and termination is unavailable".into());
    }
    Ok(out
        .text()
        .lines()
        .skip(1)
        .filter_map(|line| line.split_whitespace().next()?.parse().ok())
        .collect())
}
#[derive(Default)]
struct State {
    samples: HashMap<ProcessIdentity, (u64, Instant)>,
    requests: HashMap<ProcessIdentity, Instant>,
}
fn state() -> &'static Mutex<State> {
    static STATE: OnceLock<Mutex<State>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(State::default()))
}
fn in_scope(p: &Snapshot, c: &ScanContext) -> bool {
    !c.excludes(&p.identity.executable)
        && !p.cwd.as_ref().is_some_and(|p| c.excludes(p))
        && (!c.limit_to_roots
            || c.allows(&p.identity.executable)
            || p.cwd.as_ref().is_some_and(|p| c.allows(p)))
}
pub fn candidates(c: &ScanContext, k: &ScanControl) -> Result<Vec<(Snapshot, Option<f64>)>> {
    let managed = managed(k)?;
    let apps = native::running_apps()?;
    let now = Instant::now();
    let mut next = HashMap::new();
    let mut result = vec![];
    let mut state = state().lock().map_err(|_| "Process state unavailable")?;
    for pid in pids() {
        k.check()?;
        let Some(p) = inspect(pid) else {
            continue;
        };
        if exclusion(
            &p,
            unsafe { libc::getuid() },
            unsafe { libc::getpid() },
            &managed,
            &apps,
            &c.ignored_process_names,
        )
        .is_some()
            || !in_scope(&p, c)
        {
            continue;
        }
        let percent = p.cpu.and_then(|cpu| {
            let prior = state.samples.get(&p.identity);
            next.insert(p.identity.clone(), (cpu, now));
            prior.and_then(|(old, time)| {
                let elapsed = now.duration_since(*time).as_nanos();
                (cpu >= *old && elapsed > 0)
                    .then_some((cpu.saturating_sub(*old)) as f64 / elapsed as f64 * 100.)
            })
        });
        result.push((p, percent));
    }
    state.samples = next;
    let ids: HashSet<_> = result.iter().map(|(p, _)| p.identity.clone()).collect();
    state.requests.retain(|id, _| ids.contains(id));
    result.sort_by(|a, b| {
        b.1.unwrap_or(0.)
            .total_cmp(&a.1.unwrap_or(0.))
            .then(b.0.memory.cmp(&a.0.memory))
    });
    Ok(result)
}
pub fn signal(
    identity: &ProcessIdentity,
    force: bool,
    c: &ScanContext,
    k: &ScanControl,
) -> Result<Outcome> {
    if force {
        let state = state().lock().map_err(|_| "Process state unavailable")?;
        if state
            .requests
            .get(identity)
            .is_none_or(|t| t.elapsed() < Duration::from_secs(2))
        {
            return Err("Request graceful termination first. Force Quit is available after two seconds if the process remains alive.".into());
        }
    }
    let managed = managed(k)?;
    let apps = native::running_apps()?;
    let Some(current) = inspect(identity.pid) else {
        return Ok(Outcome::Skipped);
    };
    if current.identity != *identity {
        return Err("PID belongs to a different process instance. Refresh before acting.".into());
    }
    if exclusion(
        &current,
        unsafe { libc::getuid() },
        unsafe { libc::getpid() },
        &managed,
        &apps,
        &c.ignored_process_names,
    )
    .is_some()
        || !in_scope(&current, c)
    {
        return Err("Process no longer meets the orphan or scope rules".into());
    }
    if unsafe {
        libc::kill(
            identity.pid,
            if force { libc::SIGKILL } else { libc::SIGTERM },
        )
    } != 0
    {
        return Err(std::io::Error::last_os_error().to_string());
    }
    if !force {
        state()
            .lock()
            .map_err(|_| "Process state unavailable")?
            .requests
            .insert(identity.clone(), Instant::now());
    }
    for _ in 0..10 {
        thread::sleep(Duration::from_millis(200));
        if inspect(identity.pid).is_none_or(|p| p.identity != *identity) {
            state()
                .lock()
                .map_err(|_| "Process state unavailable")?
                .requests
                .remove(identity);
            return Ok(Outcome::Applied);
        }
    }
    Ok(Outcome::Requested)
}
pub fn active_tools(directory: Option<&str>) -> Vec<String> {
    let names = [
        "node",
        "npm",
        "pnpm",
        "yarn",
        "bun",
        "swift",
        "swift-frontend",
        "xcodebuild",
        "clang",
        "cargo",
        "rustc",
        "python",
        "python3",
        "pip",
        "uv",
        "java",
        "gradle",
        "brew",
    ];
    pids()
        .into_iter()
        .filter_map(inspect)
        .filter(|p| {
            p.identity.uid == unsafe { libc::getuid() }
                && names.contains(&p.name.to_lowercase().as_str())
                && directory.is_none_or(|root| {
                    p.cwd
                        .as_ref()
                        .is_some_and(|cwd| policy::contains(cwd, root))
                })
        })
        .map(|p| format!("{} (PID {})", p.name, p.identity.pid))
        .collect()
}
pub struct OrphanModule(pub ModuleDescriptor);
impl ScanModule for OrphanModule {
    fn descriptor(&self) -> ModuleDescriptor {
        self.0.clone()
    }
    fn scan(&self, c: &ScanContext, k: &ScanControl, r: &mut ScanReport) -> Result<()> {
        for (p, cpu) in candidates(c, k)? {
            let id = p.identity.key();
            let mut f=Finding::new("orphans",&id,&p.name,Resource::Process{process:p.identity.clone()},"Parent is launchd; no known managed job or running app owns this process. Intentionally detached daemons can also match. Review before termination.");
            f.subtitle = p.cwd.clone().unwrap_or(p.identity.executable.clone());
            f.cpu_percent = cpu;
            f.memory_bytes = p.memory;
            f.actions = vec![ActionKind::Terminate, ActionKind::ForceQuit];
            f.risk = Risk::Permanent;
            f.details = vec![
                detail("PID", p.identity.pid.to_string()),
                detail("Working folder", p.cwd.unwrap_or("Unavailable".into())),
                detail("Executable", p.identity.executable),
                detail("Command", p.command),
                detail(
                    "CPU convention",
                    "100% is one CPU core; refresh for a rate sample",
                ),
            ];
            r.findings.push(f);
        }
        Ok(())
    }
}
