//! Orphan-process classification adapted from the owner's MIT OrphanBar project; see
//! THIRD_PARTY_NOTICES.md. Inspection and signals go through `ProcessInspector`.
use crate::{model::*, policy, ports::*, services::Services};
use std::{
    collections::{HashMap, HashSet},
    thread,
    time::{Duration, Instant},
};

#[derive(Default)]
pub struct ProcessState {
    samples: HashMap<ProcessIdentity, (u64, Instant)>,
    requests: HashMap<ProcessIdentity, Instant>,
}

pub const SYSTEM_PREFIXES: &[&str] = &[
    "/System/",
    "/usr/libexec/",
    "/usr/sbin/",
    "/sbin/",
    "/Library/Apple/",
    "/Library/Developer/PrivateFrameworks/",
    "/Library/Developer/CoreSimulator/",
    "/Library/Filesystems/",
    "/Library/PrivilegedHelperTools/",
];
pub const DEVELOPER_TOOLS: &[&str] = &[
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

/// Why a process is not an orphan candidate, or `None` when it is one.
pub fn exclusion(
    p: &Snapshot,
    uid: u32,
    own: i32,
    managed: &HashSet<i32>,
    apps: &[RunningApp],
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
    if SYSTEM_PREFIXES
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
/// PIDs of jobs launchd manages. Failure is an error, never an empty set.
pub fn managed_jobs(services: &Services, control: &ScanControl) -> Result<HashSet<i32>> {
    let out = services.commands.run(
        "/bin/launchctl",
        &["list".into()],
        Duration::from_secs(5),
        control,
    )?;
    if out.status != 0 || !out.text().starts_with("PID") {
        return Err("Managed-service inspection failed; orphan classification is incomplete and termination is unavailable.".into());
    }
    Ok(out
        .text()
        .lines()
        .skip(1)
        .filter_map(|line| line.split_whitespace().next()?.parse().ok())
        .collect())
}
fn in_scope(p: &Snapshot, c: &ScanContext) -> bool {
    !c.excludes(&p.identity.executable)
        && !p.cwd.as_ref().is_some_and(|p| c.excludes(p))
        && (!c.limit_to_roots
            || c.allows(&p.identity.executable)
            || p.cwd.as_ref().is_some_and(|p| c.allows(p)))
}
fn classify(
    services: &Services,
    p: &Snapshot,
    managed: &HashSet<i32>,
    apps: &[RunningApp],
    c: &ScanContext,
) -> bool {
    let inspector = services.processes.as_ref();
    exclusion(
        p,
        inspector.current_uid(),
        inspector.current_pid(),
        managed,
        apps,
        &c.ignored_process_names,
    )
    .is_none()
        && in_scope(p, c)
}
/// Current orphan candidates with a CPU rate when a previous sample exists.
pub fn candidates(
    services: &Services,
    c: &ScanContext,
    k: &ScanControl,
) -> Result<Vec<(Snapshot, Option<f64>)>> {
    let managed = managed_jobs(services, k)?;
    let apps = services.apps.running()?;
    let now = Instant::now();
    let mut next = HashMap::new();
    let mut result = vec![];
    for pid in services.processes.pids() {
        k.check()?;
        let Some(p) = services.processes.inspect(pid) else {
            continue;
        };
        if classify(services, &p, &managed, &apps, c) {
            result.push(p);
        }
    }
    let mut state = services
        .process_state
        .lock()
        .map_err(|_| "Process state unavailable")?;
    let result: Vec<_> = result
        .into_iter()
        .map(|p| {
            let percent = p.cpu_nanos.and_then(|cpu| {
                let prior = state.samples.get(&p.identity).copied();
                next.insert(p.identity.clone(), (cpu, now));
                prior.and_then(|(old, time)| {
                    let elapsed = now.duration_since(time).as_nanos();
                    (cpu >= old && elapsed > 0).then(|| (cpu - old) as f64 / elapsed as f64 * 100.)
                })
            });
            (p, percent)
        })
        .collect();
    state.samples = next;
    let ids: HashSet<_> = result.iter().map(|(p, _)| p.identity.clone()).collect();
    state.requests.retain(|id, _| ids.contains(id));
    let mut result = result;
    result.sort_by(|a, b| {
        b.1.unwrap_or(0.)
            .total_cmp(&a.1.unwrap_or(0.))
            .then(b.0.memory.cmp(&a.0.memory))
    });
    Ok(result)
}
/// Sends SIGTERM, or SIGKILL only after a graceful request at least two seconds old, to the
/// exact process instance, then observes whether it exited.
pub fn signal(
    services: &Services,
    identity: &ProcessIdentity,
    force: bool,
    c: &ScanContext,
    k: &ScanControl,
) -> Result<Outcome> {
    if force {
        let state = services
            .process_state
            .lock()
            .map_err(|_| "Process state unavailable")?;
        if state
            .requests
            .get(identity)
            .is_none_or(|t| t.elapsed() < Duration::from_secs(2))
        {
            return Err("Request graceful termination first. Force Quit is available after two seconds if the process remains alive.".into());
        }
    }
    let managed = managed_jobs(services, k)?;
    let apps = services.apps.running()?;
    let Some(current) = services.processes.inspect(identity.pid) else {
        return Ok(Outcome::Skipped);
    };
    if current.identity != *identity {
        return Err("PID now belongs to a different process. Refresh before acting.".into());
    }
    if !classify(services, &current, &managed, &apps, c) {
        return Err("Process no longer meets the orphan or scope rules.".into());
    }
    services.processes.signal(identity.pid, force)?;
    let state = |f: &dyn Fn(&mut ProcessState)| {
        if let Ok(mut s) = services.process_state.lock() {
            f(&mut s)
        }
    };
    if !force {
        state(&|s| {
            s.requests.insert(identity.clone(), Instant::now());
        });
    }
    // A delivered signal is not proof of exit; observe this instance separately.
    for _ in 0..10 {
        thread::sleep(Duration::from_millis(200));
        if services
            .processes
            .inspect(identity.pid)
            .is_none_or(|p| p.identity != *identity)
        {
            state(&|s| {
                s.requests.remove(identity);
            });
            return Ok(Outcome::Applied);
        }
    }
    Ok(Outcome::Requested)
}
/// Current-user developer tools, optionally limited to a working directory.
pub fn active_tools(services: &Services, directory: Option<&str>) -> Vec<String> {
    let uid = services.processes.current_uid();
    services
        .processes
        .pids()
        .into_iter()
        .filter_map(|pid| services.processes.inspect(pid))
        .filter(|p| {
            p.identity.uid == uid
                && DEVELOPER_TOOLS.contains(&p.name.to_lowercase().as_str())
                && directory.is_none_or(|root| {
                    p.cwd
                        .as_ref()
                        .is_some_and(|cwd| policy::contains(cwd, root))
                })
        })
        .map(|p| format!("{} (PID {})", p.name, p.identity.pid))
        .collect()
}
