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
        for (runtime, devices) in &inventory.devices {
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
                if device.is_available == Some(false) {
                    f.badge = Some("Unavailable".into());
                    f.reason = format!(
                        "Unavailable: its runtime is no longer installed, so it cannot start. {}",
                        f.reason
                    );
                }
                if device.state == "Shutdown" {
                    f.actions = vec![ActionKind::ResetSimulator, ActionKind::DeleteSimulator];
                } else {
                    f.blocked_reason =
                        Some("Shut down this simulator in Xcode before acting.".into());
                }
                sink.finding(f);
            }
        }
        // Downloaded runtimes, which simctl can delete; bundled ones stay read-only below.
        let images = Simctl(s.commands.as_ref()).images(k).unwrap_or_default();
        let mut listed = std::collections::HashSet::new();
        for image in images.into_values() {
            k.check()?;
            if c.limit_to_roots {
                continue;
            }
            let runtime_id = image.runtime_identifier.clone().unwrap_or_default();
            listed.insert(runtime_id.clone());
            let name = inventory
                .runtimes
                .iter()
                .find(|r| r.identifier == runtime_id)
                .map(|r| r.name.clone())
                .unwrap_or_else(|| {
                    let platform = image
                        .platform_identifier
                        .as_deref()
                        .and_then(|p| p.rsplit('.').next())
                        .unwrap_or("Simulator");
                    format!("{platform} {}", image.version.clone().unwrap_or_default())
                });
            let mut f = Finding::new(
                "simulators",
                &format!("runtime-image:{}", image.identifier),
                &name,
                Resource::Command {
                    tool: "xcrun".into(),
                    task: format!("runtime-delete:{}", image.identifier),
                },
                "A downloaded simulator runtime. Xcode downloads it again from Settings › Components; simulators of this runtime cannot start until then.",
            );
            f.subtitle = "Installed runtime".into();
            f.bytes = image.size_bytes;
            f.allocated_bytes = image.size_bytes;
            f.last_used_at = image
                .last_used_at
                .as_deref()
                .and_then(crate::simulator::parse_timestamp);
            f.risk = Risk::Review;
            f.details = vec![
                detail("Version", image.version.clone().unwrap_or("Unknown".into())),
                detail("Build", image.build.clone().unwrap_or_default()),
                detail("Kind", image.kind.clone().unwrap_or_default()),
                detail("Identifier", image.identifier.clone()),
                detail(
                    "Command",
                    format!("xcrun simctl runtime delete {}", image.identifier),
                ),
            ];
            if let Some(path) = &image.path {
                f.details.push(detail("Location", path.clone()));
            }
            let running = inventory
                .devices
                .get(&runtime_id)
                .is_some_and(|d| d.iter().any(|d| d.state != "Shutdown"));
            if image.deletable != Some(true) {
                f.blocked_reason =
                    Some("Installed with Xcode; manage it in Xcode Settings › Components.".into());
            } else if running {
                f.blocked_reason =
                    Some("A simulator of this runtime is running. Shut it down first.".into());
            } else {
                f.actions = vec![ActionKind::RunCommand];
            }
            sink.finding(f);
        }
        for runtime in inventory.runtimes {
            k.check()?;
            if listed.contains(&runtime.identifier) {
                continue;
            }
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
    fn run(&self, s: &Services, f: &Finding, _: &ScanContext, k: &ScanControl) -> Result<String> {
        match &f.resource {
            Resource::Command { tool, task } if tool == "xcrun" => {
                let id = task
                    .strip_prefix("runtime-delete:")
                    .ok_or("This cleanup is not one the app runs.")?;
                Simctl(s.commands.as_ref()).delete_runtime(id, k)?;
                Ok("Runtime deleted. macOS can take a few minutes to free its space.".into())
            }
            _ => Err("This cleanup is not one the app runs.".into()),
        }
    }
}
