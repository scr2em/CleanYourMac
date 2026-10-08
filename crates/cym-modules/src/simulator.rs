//! Simulator inventory and specific-device actions through `xcrun simctl`.
use crate::{model::*, policy, ports::*};
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
    pub last_booted_at: Option<String>,
}
impl Device {
    pub fn data_path(&self) -> String {
        self.data_path.clone().unwrap_or_else(|| {
            format!(
                "{}/Library/Developer/CoreSimulator/Devices/{}/data",
                policy::home(),
                self.udid
            )
        })
    }
}
#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Runtime {
    pub identifier: String,
    pub name: String,
    pub version: Option<String>,
    pub bundle_path: Option<String>,
    pub is_available: Option<bool>,
}
/// A downloaded runtime image, from `simctl runtime list -j`.
#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeImage {
    pub identifier: String,
    pub runtime_identifier: Option<String>,
    pub version: Option<String>,
    pub build: Option<String>,
    pub size_bytes: Option<u64>,
    pub path: Option<String>,
    pub deletable: Option<bool>,
    pub kind: Option<String>,
    pub last_used_at: Option<String>,
    pub platform_identifier: Option<String>,
}
#[derive(Deserialize, Debug)]
pub struct Inventory {
    pub devices: BTreeMap<String, Vec<Device>>,
    pub runtimes: Vec<Runtime>,
}

/// Parses simctl's UTC timestamps (`2025-01-15T09:41:12Z`, optionally with fractions).
pub fn parse_timestamp(text: &str) -> Option<f64> {
    let (date, time) = text.trim_end_matches('Z').split_once('T')?;
    let mut d = date.splitn(3, '-').map(|p| p.parse::<i64>().ok());
    let (year, month, day) = (d.next()??, d.next()??, d.next()??);
    let mut t = time.splitn(3, ':');
    let (hour, minute) = (
        t.next()?.parse::<i64>().ok()?,
        t.next()?.parse::<i64>().ok()?,
    );
    let second = t.next()?.parse::<f64>().ok()?;
    // Days from civil (Howard Hinnant's algorithm).
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * ((month + 9) % 12) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some((days * 86_400 + hour * 3_600 + minute * 60) as f64 + second)
}

pub struct Simctl<'a>(pub &'a dyn CommandRunner);
impl Simctl<'_> {
    pub fn run(&self, args: &[&str], timeout: u64, control: &ScanControl) -> Result<Output> {
        let mut arguments = vec!["simctl".to_owned()];
        arguments.extend(args.iter().map(|s| s.to_string()));
        self.0.run(
            "/usr/bin/xcrun",
            &arguments,
            Duration::from_secs(timeout),
            control,
        )
    }
    pub fn inventory(&self, control: &ScanControl) -> Result<Inventory> {
        let out = self.run(&["list", "--json"], 20, control)?;
        if out.status != 0 {
            return Err(format!(
                "Xcode simulator service unavailable. Open Xcode once and check its selected developer directory. {}",
                out.error.chars().take(200).collect::<String>()
            ));
        }
        serde_json::from_slice(&out.data).map_err(|e| e.to_string())
    }
    /// Downloaded runtime images by identifier. Older Xcode versions have no such command.
    pub fn images(&self, control: &ScanControl) -> Result<BTreeMap<String, RuntimeImage>> {
        let out = self.run(&["runtime", "list", "-j"], 30, control)?;
        if out.status != 0 {
            return Err(out.error);
        }
        serde_json::from_slice(&out.data).map_err(|e| e.to_string())
    }
    /// Deletes one downloaded runtime image after fresh checks: it still exists, Xcode lets
    /// it be deleted, and no simulator of it is running.
    pub fn delete_runtime(&self, id: &str, control: &ScanControl) -> Result<()> {
        uuid::Uuid::parse_str(id).map_err(|_| "Invalid runtime identifier.")?;
        let image = self
            .images(control)?
            .into_values()
            .find(|i| i.identifier.eq_ignore_ascii_case(id))
            .ok_or("This runtime is no longer installed. Scan again.")?;
        if image.deletable != Some(true) {
            return Err("Xcode does not let this runtime be deleted.".into());
        }
        let inventory = self.inventory(control)?;
        if let Some(runtime) = &image.runtime_identifier {
            if inventory
                .devices
                .get(runtime)
                .is_some_and(|d| d.iter().any(|d| d.state != "Shutdown"))
            {
                return Err("A simulator of this runtime is running. Shut it down first.".into());
            }
        }
        let out = self.run(&["runtime", "delete", &image.identifier], 600, control)?;
        if out.status != 0 {
            return Err(out.error);
        }
        Ok(())
    }
    /// Erases or deletes one specific, currently shut-down device after a fresh inventory.
    pub fn act(
        &self,
        id: &str,
        kind: ActionKind,
        context: &ScanContext,
        control: &ScanControl,
    ) -> Result<()> {
        uuid::Uuid::parse_str(id).map_err(|_| "Invalid simulator identifier.")?;
        let verb = match kind {
            ActionKind::ResetSimulator => "erase",
            ActionKind::DeleteSimulator => "delete",
            _ => return Err("Unsupported simulator action.".into()),
        };
        let inventory = self.inventory(control)?;
        let device = inventory
            .devices
            .values()
            .flatten()
            .find(|d| d.udid.eq_ignore_ascii_case(id))
            .ok_or("The selected simulator no longer exists. Refresh first.")?;
        if device.state != "Shutdown" {
            return Err("Shut down the simulator before acting.".into());
        }
        let path = device.data_path();
        if !context.allows(&path) || context.protects(&path) {
            return Err(
                "This simulator data path is outside the current scope or excluded.".into(),
            );
        }
        let out = self.run(&[verb, &device.udid], 45, control)?;
        if out.status != 0 {
            return Err(out.error);
        }
        Ok(())
    }
}
