use super::{descriptor, ScanModule};
use crate::{model::*, orphans, ports::*, services::Services};

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
            f.actions = vec![ActionKind::Terminate, ActionKind::ForceQuit];
            f.risk = Risk::Permanent;
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
