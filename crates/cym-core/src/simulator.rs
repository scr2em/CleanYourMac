use crate::{
    command,
    files::{self, ScanControl},
    model::*,
    modules::ScanModule,
    policy,
};
use serde::Deserialize;
use std::{collections::BTreeMap, time::Duration};
#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    pub udid: String,
    pub name: String,
    pub state: String,
    pub is_available: Option<bool>,
    pub data_path: Option<String>,
}
#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Runtime {
    pub identifier: String,
    pub name: String,
    pub version: Option<String>,
    pub bundle_path: Option<String>,
}
#[derive(Deserialize, Debug)]
pub struct Inventory {
    pub devices: BTreeMap<String, Vec<Device>>,
    pub runtimes: Vec<Runtime>,
}
pub fn inventory(k: &ScanControl) -> Result<Inventory> {
    let out = command::run(
        "/usr/bin/xcrun",
        &["simctl".into(), "list".into(), "--json".into()],
        Duration::from_secs(20),
        k,
    )?;
    if out.status != 0 {
        return Err(format!("Simulator service unavailable. Open Xcode and check its selected developer directory. {}",out.error.chars().take(200).collect::<String>()));
    }
    serde_json::from_slice(&out.data).map_err(|e| e.to_string())
}
pub fn act(id: &str, kind: ActionKind, c: &ScanContext, k: &ScanControl) -> Result<()> {
    uuid::Uuid::parse_str(id).map_err(|_| "Invalid simulator identifier")?;
    let inventory = inventory(k)?;
    let device = inventory
        .devices
        .values()
        .flatten()
        .find(|d| d.udid == id)
        .ok_or("Simulator no longer exists")?;
    if device.state != "Shutdown" {
        return Err("Shut down the simulator before acting".into());
    }
    let path = device.data_path.clone().unwrap_or_else(|| {
        format!(
            "{}/Library/Developer/CoreSimulator/Devices/{id}/data",
            policy::home()
        )
    });
    if !c.allows(&path) || c.protects(&path) {
        return Err("Simulator data is outside the current scope or excluded".into());
    }
    let verb = match kind {
        ActionKind::ResetSimulator => "erase",
        ActionKind::DeleteSimulator => "delete",
        _ => return Err("Unsupported simulator action".into()),
    };
    let out = command::run(
        "/usr/bin/xcrun",
        &["simctl".into(), verb.into(), id.into()],
        Duration::from_secs(45),
        k,
    )?;
    if out.status != 0 {
        return Err(out.error);
    }
    Ok(())
}
pub struct SimulatorModule(pub ModuleDescriptor);
impl ScanModule for SimulatorModule {
    fn descriptor(&self) -> ModuleDescriptor {
        self.0.clone()
    }
    fn scan(&self, c: &ScanContext, k: &ScanControl, r: &mut ScanReport) -> Result<()> {
        let inventory = inventory(k)?;
        for (runtime, devices) in inventory.devices {
            for device in devices {
                k.check()?;
                let path = device.data_path.unwrap_or_else(|| {
                    format!(
                        "{}/Library/Developer/CoreSimulator/Devices/{}/data",
                        policy::home(),
                        device.udid
                    )
                });
                if !c.allows(&path) {
                    continue;
                }
                let size = files::size(&path, k).ok();
                if size.as_ref().is_none_or(|s| !s.complete) {
                    files::warn(&mut r.warnings,format!("Device data size is incomplete for {}. Device actions use fresh simctl state independently.",device.name));
                }
                let mut f=Finding::new("simulators",&device.udid,&device.name,Resource::Simulator{id:device.udid.clone(),state:device.state.clone()},"Reset or delete erases installed apps, settings and test documents. This cannot be undone.");
                f.subtitle = runtime.clone();
                f.bytes = size.as_ref().map(|s| s.logical);
                f.allocated_bytes = size.map(|s| s.allocated);
                f.risk = Risk::Permanent;
                f.details = vec![
                    detail("Identifier", device.udid),
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
                        Some("Shut down this simulator in Xcode before acting".into());
                }
                r.findings.push(f);
            }
        }
        for runtime in inventory.runtimes {
            if runtime
                .bundle_path
                .as_ref()
                .map(|p| !c.allows(p))
                .unwrap_or(c.limit_to_roots)
            {
                continue;
            }
            let key = format!(
                "{}:{}",
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
                    id: runtime.identifier,
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
            f.blocked_reason = Some("Manage this runtime through Xcode's component manager".into());
            r.findings.push(f);
        }
        Ok(())
    }
}
