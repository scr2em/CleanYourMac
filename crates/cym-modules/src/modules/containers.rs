//! Container engines and the Linux virtual machines they run in: Docker Desktop, OrbStack,
//! Colima, Lima, Podman, Rancher Desktop and Apple's `container`.
//!
//! Each engine keeps every image, container and volume in one virtual disk. The disk never
//! goes to the Trash: that would delete all of them. It is listed so its real size shows (the
//! file is sparse, so its apparent size is far larger), and the space inside it is offered
//! through the engine's own commands: `docker builder prune`, `docker image prune --all` and
//! `docker container prune`, or the Podman equivalents. Volumes hold data that exists nowhere
//! else, so they are only listed.
use super::{add_files, descriptor, wildcard, Candidate, LastUsed, ScanModule};
use crate::{adapters::command, model::*, orphans, policy, ports::*, services::Services};
use std::time::Duration;

/// An engine and where it keeps its virtual disks.
struct Engine {
    name: &'static str,
    /// Home-relative paths of its virtual disks; `*` matches one path component.
    disks: &'static [&'static str],
    /// `docker context` names that point at it.
    contexts: &'static [&'static str],
    /// The engine's own cleanup, shown with its disk.
    cleanup: &'static str,
    /// The brand shown with its items.
    ecosystem: &'static str,
}

const ENGINES: &[Engine] = &[
    Engine {
        name: "Docker Desktop",
        disks: &[
            "Library/Containers/com.docker.docker/Data/vms/0/data/Docker.raw",
            "Library/Containers/com.docker.docker/Data/vms/0/data/Docker.qcow2",
        ],
        contexts: &["desktop-linux", "default"],
        cleanup: "docker system prune, or Docker Desktop › Troubleshoot › Clean / Purge data.",
        ecosystem: "Docker",
    },
    Engine {
        name: "OrbStack",
        disks: &["Library/Group Containers/HUAQ24HBR6.dev.orbstack/data/data.img"],
        contexts: &["orbstack"],
        cleanup: "docker system prune. OrbStack gives freed space back to macOS by itself.",
        ecosystem: "Docker",
    },
    Engine {
        name: "Colima",
        disks: &[".colima/_lima/_disks/*/datadisk", ".colima/_lima/*/diffdisk"],
        contexts: &["colima", "colima-*"],
        cleanup: "docker system prune, then colima ssh -- sudo fstrim -a to give the space back to macOS.",
        ecosystem: "Docker",
    },
    Engine {
        name: "Lima",
        disks: &[".lima/*/diffdisk", ".lima/*/disk"],
        contexts: &["lima-*"],
        cleanup: "limactl delete <name> removes a virtual machine you no longer use.",
        ecosystem: "Docker",
    },
    Engine {
        name: "Rancher Desktop",
        disks: &["Library/Application Support/rancher-desktop/lima/0/diffdisk"],
        contexts: &["rancher-desktop"],
        cleanup: "docker system prune, or Rancher Desktop › Troubleshooting.",
        ecosystem: "Docker",
    },
    Engine {
        name: "Podman",
        disks: &[
            ".local/share/containers/podman/machine/*/*.raw",
            ".local/share/containers/podman/machine/*/*.img",
            ".local/share/containers/podman/machine/*/*.qcow2",
        ],
        contexts: &[],
        cleanup: "podman system prune, or podman machine rm for a machine you no longer use.",
        ecosystem: "Podman",
    },
    Engine {
        name: "Apple container",
        disks: &["Library/Application Support/com.apple.container"],
        contexts: &[],
        cleanup: "container image prune --all removes unused images.",
        ecosystem: "",
    },
];

/// Downloaded VM images the tools fetch again.
const CACHES: &[(&str, &str, &str)] = &[
    ("Library/Caches/lima", "Lima downloads", "limactl"),
    ("Library/Caches/colima", "Colima downloads", "colima"),
];

/// What an engine's `system df` reports for one kind of data.
#[derive(Debug, PartialEq, Eq)]
pub struct Usage {
    pub kind: String,
    pub total: String,
    pub active: String,
    pub size: u64,
    pub reclaimable: u64,
}

/// One cleanup the module may run: the tool, its task name and the fixed arguments.
const TASKS: &[(&str, &str, &[&str])] = &[
    ("docker", "builder-prune", &["builder", "prune", "--force"]),
    (
        "docker",
        "image-prune",
        &["image", "prune", "--all", "--force"],
    ),
    (
        "docker",
        "container-prune",
        &["container", "prune", "--force"],
    ),
    (
        "podman",
        "image-prune",
        &["image", "prune", "--all", "--force"],
    ),
    (
        "podman",
        "container-prune",
        &["container", "prune", "--force"],
    ),
];
fn task_args(tool: &str, task: &str) -> Option<&'static [&'static str]> {
    TASKS
        .iter()
        .find(|(t, k, _)| *t == tool && *k == task)
        .map(|(_, _, args)| *args)
}

#[derive(Default)]
pub struct ContainersModule {
    /// The home folder; the current user's when `None`.
    pub home: Option<String>,
}
impl ContainersModule {
    fn home(&self) -> String {
        self.home.clone().unwrap_or_else(policy::home)
    }
    /// The first installed command-line tool named `name` (`docker` or `podman`).
    pub fn cli(&self, name: &str) -> Option<String> {
        let home = self.home();
        // System locations first; links in the home folder are checked by the runner.
        command::CONTAINER_TOOLS
            .iter()
            .map(|t| (*t).to_owned())
            .chain(
                command::HOME_CONTAINER_TOOLS
                    .iter()
                    .map(|t| format!("{home}/{t}")),
            )
            .filter(|p| p.ends_with(&format!("/{name}")))
            .find(|p| std::fs::metadata(p).is_ok_and(|m| m.is_file()))
    }
    /// Existing virtual disks of an engine.
    fn disks(&self, s: &Services, engine: &Engine) -> Vec<Entry> {
        let home = self.home();
        engine
            .disks
            .iter()
            .flat_map(|pattern| matches(s, &home, pattern))
            .collect()
    }
    fn run_tool(
        &self,
        s: &Services,
        exe: &str,
        args: &[&str],
        seconds: u64,
        k: &ScanControl,
    ) -> Result<Output> {
        let args: Vec<String> = args.iter().map(|a| (*a).to_owned()).collect();
        s.commands.run(exe, &args, Duration::from_secs(seconds), k)
    }
    /// The `docker context` in use, which names the engine Docker commands reach.
    fn context(&self, s: &Services, exe: &str, k: &ScanControl) -> Option<String> {
        let out = self.run_tool(s, exe, &["context", "show"], 10, k).ok()?;
        let name = String::from_utf8_lossy(&out.data).trim().to_owned();
        (out.status == 0 && !name.is_empty()).then_some(name)
    }
    /// Where the current Docker context sends commands, such as
    /// `unix:///Users/x/.docker/run/docker.sock`.
    fn endpoint(&self, s: &Services, exe: &str, k: &ScanControl) -> Option<String> {
        let out = self
            .run_tool(
                s,
                exe,
                &[
                    "context",
                    "inspect",
                    "--format",
                    "{{.Endpoints.docker.Host}}",
                ],
                10,
                k,
            )
            .ok()?;
        let host = String::from_utf8_lossy(&out.data).trim().to_owned();
        (out.status == 0 && !host.is_empty()).then_some(host)
    }
    /// The URI of Podman's default connection, such as
    /// `ssh://core@127.0.0.1:52345/run/user/501/podman/podman.sock` for the local machine.
    fn podman_endpoint(&self, s: &Services, exe: &str, k: &ScanControl) -> Option<String> {
        let out = self
            .run_tool(
                s,
                exe,
                &["system", "connection", "list", "--format", "json"],
                10,
                k,
            )
            .ok()?;
        if out.status != 0 {
            return None;
        }
        let list: Vec<serde_json::Value> = serde_json::from_slice(&out.data).ok()?;
        let default = list
            .iter()
            .find(|c| c["Default"].as_bool() == Some(true))
            .or(list.first())?;
        default["URI"].as_str().map(str::to_owned)
    }
    /// Reclaimable space by kind, from `<tool> system df`.
    fn usage(&self, s: &Services, exe: &str, k: &ScanControl) -> Result<Vec<Usage>> {
        let out = self.run_tool(s, exe, &["system", "df", "--format", "{{json .}}"], 60, k)?;
        if out.status != 0 {
            return Err(out.error.trim().chars().take(240).collect());
        }
        Ok(parse_usage(&String::from_utf8_lossy(&out.data)))
    }
}

/// Existing paths for a home-relative pattern in which `*` matches one component.
fn matches(s: &Services, home: &str, pattern: &str) -> Vec<Entry> {
    let mut current = vec![home.to_owned()];
    for part in pattern.split('/') {
        current = if part.contains('*') {
            current
                .iter()
                .flat_map(|dir| s.children(dir, &mut vec![]))
                .filter(|e| wildcard(part, e.name()) && !e.name().starts_with('_'))
                .map(|e| e.path().to_owned())
                .collect()
        } else {
            current.into_iter().map(|d| format!("{d}/{part}")).collect()
        };
    }
    current.iter().filter_map(|p| s.entry(p).ok()).collect()
}

/// Sizes as Docker and Podman print them: decimal units, such as `1.2GB` or `512kB`, with
/// an optional percentage after them.
pub fn parse_size(text: &str) -> Option<u64> {
    let text = text.split_whitespace().next()?;
    let split = text.find(|c: char| !(c.is_ascii_digit() || c == '.'))?;
    let (number, unit) = text.split_at(split);
    let number: f64 = number.parse().ok()?;
    let scale = match unit.to_ascii_lowercase().as_str() {
        "b" => 1.0,
        "kb" | "k" => 1e3,
        "mb" | "m" => 1e6,
        "gb" | "g" => 1e9,
        "tb" | "t" => 1e12,
        "kib" => 1024.0,
        "mib" => 1024.0 * 1024.0,
        "gib" => 1024.0 * 1024.0 * 1024.0,
        _ => return None,
    };
    Some((number * scale) as u64)
}
/// `system df --format "{{json .}}"`: one JSON object per line from Docker, or one JSON
/// array from Podman. Numbers in `RawSize`/`RawReclaimable` win over printed sizes.
pub fn parse_usage(output: &str) -> Vec<Usage> {
    let values: Vec<serde_json::Value> =
        match serde_json::from_str::<serde_json::Value>(output.trim()) {
            Ok(serde_json::Value::Array(items)) => items,
            _ => output
                .lines()
                .filter_map(|l| serde_json::from_str(l).ok())
                .collect(),
        };
    let text = |v: &serde_json::Value, key: &str| match &v[key] {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        _ => String::new(),
    };
    values
        .iter()
        .filter_map(|v| {
            let kind = v["Type"].as_str()?.to_owned();
            let size = v["RawSize"]
                .as_u64()
                .or_else(|| parse_size(&text(v, "Size")))?;
            let reclaimable = v["RawReclaimable"]
                .as_u64()
                .or_else(|| parse_size(&text(v, "Reclaimable")))
                .unwrap_or(0);
            let total = [text(v, "TotalCount"), text(v, "Total")]
                .into_iter()
                .find(|t| !t.is_empty())
                .unwrap_or_default();
            Some(Usage {
                kind,
                total,
                active: text(v, "Active"),
                size,
                reclaimable,
            })
        })
        .collect()
}

/// Whether an engine endpoint is on this Mac: a local socket, or SSH to this computer's
/// own address (how Podman reaches its virtual machine).
pub fn local_endpoint(uri: &str) -> bool {
    if uri.starts_with("unix://") {
        return true;
    }
    let Some(rest) = uri.strip_prefix("ssh://") else {
        return false;
    };
    // A query, fragment, escape or backslash could make SSH read a different host than this
    // check does, so such addresses are refused rather than interpreted.
    if rest
        .chars()
        .any(|c| matches!(c, '?' | '#' | '%' | '\\') || c.is_whitespace() || c.is_control())
    {
        return false;
    }
    let authority = rest.split('/').next().unwrap_or_default();
    let host = authority.rsplit('@').next().unwrap_or_default();
    let (host, port) = match host.strip_prefix('[') {
        Some(v6) => v6.split_once(']').unwrap_or((v6, "x")),
        None => host.split_once(':').map_or((host, ""), |(h, p)| (h, p)),
    };
    let port = port.strip_prefix(':').unwrap_or(port);
    port.chars().all(|c| c.is_ascii_digit()) && matches!(host, "127.0.0.1" | "localhost" | "::1")
}

/// The engine a Docker context reaches.
fn engine_for_context(context: &str) -> Option<&'static Engine> {
    ENGINES
        .iter()
        .find(|e| e.contexts.iter().any(|c| wildcard(c, context)))
}

impl ScanModule for ContainersModule {
    fn descriptor(&self) -> ModuleDescriptor {
        descriptor(
            "containers",
            "Containers & VMs",
            "Developer",
            "shippingbox.circle",
            "Docker, OrbStack, Colima, Lima and Podman: their real disk size, and their own cleanup.",
            false,
        )
    }
    fn action_roots(&self) -> Vec<String> {
        let home = self.home();
        CACHES
            .iter()
            .map(|(p, _, _)| format!("{home}/{p}"))
            .collect()
    }
    fn scan(
        &self,
        s: &Services,
        c: &ScanContext,
        k: &ScanControl,
        sink: &mut dyn Sink,
    ) -> Result<()> {
        let home = self.home();
        let mut candidates = vec![];
        let mut disks: Vec<(&Engine, String)> = vec![];
        for engine in ENGINES {
            k.check()?;
            for e in self.disks(s, engine) {
                if !c.allows(e.path()) {
                    continue;
                }
                disks.push((engine, e.path().to_owned()));
                let blocked = format!(
                    "Holds every image, container and volume of {}. Free space inside it with the cleanup items, or: {}",
                    engine.name, engine.cleanup
                );
                let mut details = vec![
                    detail("Engine", engine.name),
                    detail("Kind", "Virtual disk"),
                    detail("Official cleanup", engine.cleanup),
                ];
                if !engine.ecosystem.is_empty() {
                    details.push(detail("Ecosystem", engine.ecosystem));
                }
                let modified = e.modified();
                candidates.push(
                    Candidate::new(
                        e,
                        &format!("The virtual disk of {}. It never goes to the Trash, because that deletes every image, container and volume in it. Its size here is the space it really uses; the file reports a larger size because it is sparse.", engine.name),
                        vec![],
                        Risk::Review,
                    )
                    .title(format!("{} virtual disk", engine.name))
                    .details(details)
                    .last_used(LastUsed::At(modified))
                    .blocked(Some(&blocked)),
                );
            }
        }
        for (path, title, tool) in CACHES {
            let path = format!("{home}/{path}");
            let Ok(e) = s.entry(&path) else { continue };
            if !e.directory || !c.allows(&path) {
                continue;
            }
            candidates.push(
                Candidate::new(
                    e,
                    &format!("Virtual machine images {tool} downloaded. It downloads them again when it creates a machine."),
                    vec![ActionKind::Trash],
                    Risk::Rebuild,
                )
                .title(*title)
                .details(vec![detail("Engine", if *tool == "limactl" { "Lima" } else { "Colima" })]),
            );
        }
        add_files(s, sink, "containers", candidates, k)?;

        // Space inside the engines, through their own command-line tools.
        for tool in ["docker", "podman"] {
            k.check()?;
            let Some(exe) = self.cli(tool) else { continue };
            let endpoint: Option<String>;
            let engine = if tool == "docker" {
                let Some(name) = self.context(s, &exe, k) else {
                    continue;
                };
                // Only an engine on this Mac: a context can point at a remote host.
                match self.endpoint(s, &exe, k) {
                    Some(host) if host.starts_with("unix://") => endpoint = Some(host),
                    other => {
                        sink.warning(format!(
                            "Docker points at {} (context {name}), not an engine on this Mac, so its data is not listed.",
                            other.unwrap_or_else(|| "an unknown engine".into())
                        ));
                        continue;
                    }
                }
                Some((engine_for_context(&name), name))
            } else {
                // Only the Podman machine on this Mac: a connection can point at a server.
                match self.podman_endpoint(s, &exe, k) {
                    Some(uri) if local_endpoint(&uri) => endpoint = Some(uri),
                    other => {
                        sink.warning(format!(
                            "Podman points at {}, not a machine on this Mac, so its data is not listed.",
                            other.unwrap_or_else(|| "no machine".into())
                        ));
                        continue;
                    }
                }
                ENGINES
                    .iter()
                    .find(|e| e.name == "Podman")
                    .map(|e| (Some(e), String::new()))
            };
            let Some((engine, context)) = engine else {
                continue;
            };
            let usage = match self.usage(s, &exe, k) {
                Ok(usage) => usage,
                Err(_) => {
                    if engine.is_some_and(|e| disks.iter().any(|(d, _)| d.name == e.name)) {
                        sink.warning(format!(
                            "{} is not running, so the space its own cleanup can free was not measured. Start it and scan again.",
                            engine.map_or(tool, |e| e.name)
                        ));
                    }
                    continue;
                }
            };
            let disk = engine
                .and_then(|e| disks.iter().find(|(d, _)| d.name == e.name))
                .map(|(_, p)| p.clone());
            // With chosen folders only, an item must have a place inside them.
            if c.limit_to_roots && disk.is_none() {
                continue;
            }
            for u in usage {
                if u.reclaimable == 0 {
                    continue;
                }
                let (task, title, reason, risk) = match u.kind.as_str() {
                    "Build Cache" => ("builder-prune", "build cache", "Layers saved by earlier builds. The next build makes them again, so it takes longer once.", Risk::Rebuild),
                    "Images" => ("image-prune", "unused images", "Images that no container uses. A pull downloads them again; an image you built and never pushed must be built again.", Risk::Review),
                    "Containers" => ("container-prune", "stopped containers", "Containers that are not running, with every change made inside them.", Risk::Review),
                    "Local Volumes" | "Volumes" => ("volumes", "unused volumes", "Volumes that no container uses. They can hold databases and other data that exists nowhere else.", Risk::Review),
                    _ => continue,
                };
                let label = if tool == "docker" { "Docker" } else { "Podman" };
                let mut f = Finding::new(
                    "containers",
                    &format!("{tool}:{context}:{task}"),
                    &format!("{label} {title}"),
                    Resource::Command {
                        tool: tool.into(),
                        task: task.into(),
                    },
                    reason,
                );
                f.subtitle = match (engine, context.is_empty()) {
                    (Some(e), false) => format!("{} · context {context}", e.name),
                    (Some(e), true) => e.name.to_owned(),
                    (None, _) => format!("context {context}"),
                };
                f.bytes = Some(u.reclaimable);
                f.allocated_bytes = Some(u.reclaimable);
                f.risk = risk;
                f.details = vec![
                    detail("Ecosystem", label),
                    detail("Engine", engine.map_or("Docker", |e| e.name)),
                    detail("Total size", format!("{:.1} GB", u.size as f64 / 1e9)),
                    detail("Items", format!("{} ({} in use)", u.total, u.active)),
                ];
                if !context.is_empty() {
                    f.details.push(detail("Context", context.clone()));
                }
                if let Some(host) = &endpoint {
                    f.details.push(detail("Endpoint", host.clone()));
                }
                if let Some(disk) = &disk {
                    // A place inside the disk, so totals count it once with the disk.
                    f.details
                        .push(detail("Data path", format!("{disk}/{task}")));
                }
                match task_args(tool, task) {
                    Some(args) => {
                        f.details
                            .push(detail("Command", format!("{tool} {}", args.join(" "))));
                        f.actions = vec![ActionKind::RunCommand];
                    }
                    None => {
                        f.blocked_reason = Some(format!("Check volumes with `{tool} volume ls` and remove the ones you no longer need there. The app never removes volumes."));
                    }
                }
                sink.finding(f);
            }
        }
        Ok(())
    }
    fn in_use(&self, s: &Services, f: &Finding, _: ActionKind) -> Option<String> {
        let path = f.resource.path()?;
        let home = self.home();
        let (_, _, process) = CACHES
            .iter()
            .find(|(p, _, _)| policy::contains(path, &format!("{home}/{p}")))?;
        let running: Vec<String> = orphans::active_tools(s, None)
            .into_iter()
            .filter(|t| t.starts_with(&format!("{process} ")))
            .collect();
        (!running.is_empty()).then(|| format!("Stop {process} first:{}", orphans::list(&running)))
    }
    fn preflight(&self, _: &Services, f: &Finding, _: ActionKind, _: &ScanControl) -> Result<()> {
        if f.value("Kind") == Some("Virtual disk") {
            return Err("A virtual disk never goes to the Trash.".into());
        }
        Ok(())
    }
    fn run(&self, s: &Services, f: &Finding, _: &ScanContext, k: &ScanControl) -> Result<String> {
        let Resource::Command { tool, task } = &f.resource else {
            return Err("Not a cleanup command.".into());
        };
        let args = task_args(tool, task).ok_or("This cleanup is not one the app runs.")?;
        let exe = self
            .cli(tool)
            .ok_or_else(|| format!("{tool} is no longer installed."))?;
        // Docker commands reach whichever engine the current context names; it must still
        // be the one this item was measured in.
        if tool == "docker" {
            let now = self.context(s, &exe, k).unwrap_or_default();
            let host = self.endpoint(s, &exe, k).unwrap_or_default();
            if Some(now.as_str()) != f.value("Context")
                || Some(host.as_str()) != f.value("Endpoint")
            {
                return Err(format!(
                    "Docker now reaches a different engine ({now}). Scan again."
                ));
            }
            if !host.starts_with("unix://") {
                return Err("Docker no longer points at an engine on this Mac.".into());
            }
        } else {
            let uri = self.podman_endpoint(s, &exe, k).unwrap_or_default();
            if Some(uri.as_str()) != f.value("Endpoint") || !local_endpoint(&uri) {
                return Err("Podman now points at a different machine. Scan again.".into());
            }
        }
        // The command removes what exists when it runs, so it must still match what was
        // reviewed: refuse when it would now free clearly more.
        let kind = match task.as_str() {
            "builder-prune" => "Build Cache",
            "image-prune" => "Images",
            _ => "Containers",
        };
        let reviewed = f.bytes.unwrap_or(0);
        let now = self
            .usage(s, &exe, k)?
            .into_iter()
            .find(|u| u.kind == kind)
            .map_or(0, |u| u.reclaimable);
        if now > reviewed + reviewed / 10 + 100_000_000 {
            return Err(format!(
                "{tool} can now free {:.1} GB, more than the {:.1} GB you reviewed. Scan again to review what changed.",
                now as f64 / 1e9,
                reviewed as f64 / 1e9
            ));
        }
        let out = self.run_tool(s, &exe, args, 900, k)?;
        if out.status != 0 {
            return Err(out.error.trim().chars().take(400).collect());
        }
        let freed = String::from_utf8_lossy(&out.data)
            .lines()
            .rev()
            .find(|l| l.to_ascii_lowercase().contains("reclaimed space"))
            .map(|l| format!(" {}.", l.trim()))
            .unwrap_or_default();
        Ok(format!("Ran {tool} {}.{freed}", args.join(" ")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_and_usage() {
        assert_eq!(parse_size("2.1GB (52%)"), Some(2_100_000_000));
        assert_eq!(parse_size("512kB"), Some(512_000));
        assert_eq!(parse_size("0B"), Some(0));
        assert_eq!(parse_size("1.5GiB"), Some(1_610_612_736));
        let docker = parse_usage(
            r#"{"Active":"2","Reclaimable":"1.2GB (50%)","Size":"2.4GB","TotalCount":"5","Type":"Images"}
{"Active":"0","Reclaimable":"3.5GB","Size":"3.5GB","TotalCount":"40","Type":"Build Cache"}"#,
        );
        assert_eq!(docker[1].kind, "Build Cache");
        assert_eq!(docker[1].reclaimable, 3_500_000_000);
        let podman = parse_usage(
            r#"[{"Type":"Images","Total":3,"Active":1,"RawSize":900,"RawReclaimable":600,"Size":"900B","Reclaimable":"600B (66%)"}]"#,
        );
        assert_eq!(podman[0].reclaimable, 600);
        assert_eq!(podman[0].total, "3");
        assert_eq!(engine_for_context("colima-work").unwrap().name, "Colima");
        assert!(local_endpoint("unix:///Users/me/.docker/run/docker.sock"));
        assert!(local_endpoint(
            "ssh://core@127.0.0.1:52345/run/user/501/podman/podman.sock"
        ));
        assert!(local_endpoint("ssh://root@[::1]:22/run/podman/podman.sock"));
        assert!(!local_endpoint(
            "ssh://deploy@prod.example.com/run/podman/podman.sock"
        ));
        assert!(!local_endpoint("ssh://user@127.0.0.1.evil.com/x"));
        assert!(!local_endpoint("tcp://127.0.0.1:2375"));
        // The host SSH reads must be the one checked here.
        assert!(!local_endpoint(
            "ssh://core@evil.example:22?@127.0.0.1/run/podman/podman.sock"
        ));
        assert!(!local_endpoint(
            "ssh://core@evil.example:22#@127.0.0.1/run/podman/podman.sock"
        ));
        assert!(!local_endpoint("ssh://core@evil.example%40127.0.0.1/x"));
        assert!(!local_endpoint("ssh://core@[::1]evil.example/x"));
        assert!(!local_endpoint("ssh://core@127.0.0.1:22evil/x"));
        assert_eq!(
            task_args("docker", "volumes"),
            None,
            "volumes are never removed"
        );
    }
}
