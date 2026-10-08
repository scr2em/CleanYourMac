use super::{descriptor, ScanModule};
use crate::{model::*, orphans, ports::*, services::Services};

/// Programs meant to outlive the terminal or app that started them: terminal multiplexers,
/// editor daemons, databases and local servers. Ending them loses sessions or data.
const DETACHED: &[&str] = &[
    "tmux",
    "screen",
    "mosh-server",
    "emacs",
    "nvim",
    "postgres",
    "postmaster",
    "mongod",
    "mongos",
    "redis-server",
    "valkey-server",
    "mysqld",
    "mariadbd",
    "memcached",
    "nginx",
    "httpd",
    "caddy",
    "beam.smp",
    "elasticsearch",
    "opensearch",
    "clickhouse",
    "etcd",
    "dockerd",
    "containerd",
    "colima",
    "limactl",
    "qemu-system-aarch64",
    "qemu-system-x86_64",
    "ollama",
    "syncthing",
    "tailscaled",
    "code-server",
    "jupyter",
    "jupyter-lab",
    "ssh-agent",
    "gpg-agent",
];
/// Whether a process name is one of `DETACHED` (see `orphans::named`).
pub fn detached_by_design(name: &str) -> bool {
    DETACHED.iter().any(|d| orphans::named(name, d))
}

pub struct OrphanModule;
impl ScanModule for OrphanModule {
    fn descriptor(&self) -> ModuleDescriptor {
        descriptor(
            "orphans",
            "Orphan Processes",
            "Tools",
            "cpu",
            "Suspected unmanaged processes left behind by their parents.",
            false,
        )
    }
    fn scan(
        &self,
        s: &Services,
        c: &ScanContext,
        k: &ScanControl,
        sink: &mut dyn Sink,
    ) -> Result<()> {
        for (p, cpu) in orphans::candidates(s, c, k)? {
            let mut f = Finding::new(
                "orphans",
                &p.identity.key(),
                &p.name,
                Resource::Process {
                    process: p.identity.clone(),
                },
                "Parent is launchd; no known managed job or running app owns it. Intentionally detached daemons can also match. Review before termination.",
            );
            f.subtitle = p.cwd.clone().unwrap_or(p.identity.executable.clone());
            f.cpu_percent = cpu;
            f.memory_bytes = p.memory;
            f.risk = Risk::Permanent;
            if detached_by_design(&p.name) {
                f.blocked_reason = Some(format!(
                    "{} runs detached by design: terminal sessions, servers and databases keep running after the app that started them. Stop it with its own command.",
                    p.name
                ));
            } else {
                f.actions = vec![ActionKind::Terminate, ActionKind::ForceQuit];
            }
            f.details = vec![
                detail("PID", p.identity.pid.to_string()),
                detail("Working folder", p.cwd.unwrap_or("Unavailable".into())),
                detail("Executable", p.identity.executable),
                detail("Command", p.command),
                detail(
                    "CPU convention",
                    "100% is one CPU core; refresh for a rate sample.",
                ),
            ];
            sink.finding(f);
        }
        Ok(())
    }
}
