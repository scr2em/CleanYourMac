use super::{descriptor, ScanModule};
use crate::{model::*, ports::*, services::Services, simulator::Simctl};

pub struct SimulatorModule;
impl ScanModule for SimulatorModule {
    fn descriptor(&self) -> ModuleDescriptor {
        descriptor(
            "simulators",
            "Simulators",
            "Developer",
            "iphone",
            "Device app data and runtimes in your selected Xcode installation.",
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
        let inventory = Simctl(s.commands.as_ref()).inventory(k)?;
        for (runtime, devices) in inventory.devices {
            for device in devices {
                k.check()?;
                let path = device.data_path();
                if !c.allows(&path) {
                    continue;
                }
                sink.progress(format!("Sizing {}", device.name));
                let size = s.sizer.size(&path, k).ok();
                if size.as_ref().is_none_or(|s| !s.complete) {
                    sink.warning(format!(
                        "Device data size is incomplete for {}. Device actions use fresh simctl state independently.",
                        device.name
                    ));
                }
                let mut f = Finding::new(
                    "simulators",
                    &device.udid,
                    &device.name,
                    Resource::Simulator {
                        id: device.udid.clone(),
                        state: device.state.clone(),
                    },
                    "Device data includes installed apps, settings and test documents. Reset or delete cannot be undone.",
                );
                f.subtitle = runtime.clone();
                f.last_used_at = device
                    .last_booted_at
                    .as_deref()
                    .and_then(crate::simulator::parse_timestamp);
                f.bytes = size.as_ref().map(|s| s.logical);
                f.allocated_bytes = size.map(|s| s.allocated);
                f.risk = Risk::Permanent;
                f.details = vec![
                    detail("Identifier", device.udid.clone()),
                    detail("State", device.state.clone()),
                    detail(
                        "Available",
                        if device.is_available == Some(true) {
                            "Yes"
                        } else {
                            "No"
                        },
                    ),
                    detail("Data path", path),
                ];
                if device.state == "Shutdown" {
                    f.actions = vec![ActionKind::ResetSimulator, ActionKind::DeleteSimulator];
                } else {
                    f.blocked_reason =
                        Some("Shut down this simulator in Xcode before acting.".into());
                }
                sink.finding(f);
            }
        }
        for runtime in inventory.runtimes {
            k.check()?;
            let hidden = match &runtime.bundle_path {
                Some(path) => !c.allows(path),
                None => c.limit_to_roots,
            };
            if hidden {
                continue;
            }
            let key = format!(
                "runtime:{}:{}",
                runtime.identifier,
                runtime
                    .bundle_path
                    .as_deref()
                    .or(runtime.version.as_deref())
                    .unwrap_or("")
            );
            let mut f = Finding::new(
                "simulators",
                &key,
                &runtime.name,
                Resource::Simulator {
                    id: runtime.identifier.clone(),
                    state: "Runtime".into(),
                },
                "Runtime installations are managed separately in Xcode Settings → Components.",
            );
            f.subtitle = "Installed runtime".into();
            f.details = vec![
                detail("Version", runtime.version.unwrap_or("Unknown".into())),
                detail(
                    "Location",
                    runtime.bundle_path.unwrap_or("Managed by Xcode".into()),
                ),
            ];
            f.blocked_reason =
                Some("Manage this runtime through Xcode's component manager.".into());
            sink.finding(f);
        }
        Ok(())
    }
}
