use super::{descriptor, flush, ScanModule};
use crate::{model::*, policy, ports::*, services::Services};
use std::path::Path;

fn install_roots() -> Vec<String> {
    vec!["/Applications".into(), policy::home() + "/Applications"]
}
fn data_roots() -> Vec<String> {
    let h = policy::home();
    vec![
        h.clone() + "/Library/Caches",
        h.clone() + "/Library/Preferences",
        h.clone() + "/Library/Saved Application State",
        h + "/Library/Application Support",
    ]
}
/// Every `.app` bundle in the usual install folders, including nested ones.
fn installed_apps(s: &Services, k: &ScanControl, warnings: &mut Vec<String>) -> Result<Vec<Entry>> {
    let mut apps = vec![];
    for root in install_roots().into_iter().filter(|p| s.is_dir(p)) {
        let context = ScanContext {
            roots: vec![root],
            ..Default::default()
        };
        s.walk(&context, k, warnings, &mut |e| {
            if e.directory && e.path().ends_with(".app") {
                apps.push(e.clone());
                return Ok(false);
            }
            Ok(true)
        })?;
    }
    Ok(apps)
}
/// Refuses to act while the owning application runs.
fn owner_not_running(s: &Services, f: &Finding) -> Result<()> {
    let path = f.resource.path().unwrap_or("");
    let owner = f.value("Application path").unwrap_or(path);
    let id = f.value("Bundle identifier").filter(|id| !id.is_empty());
    if s.apps
        .running()?
        .iter()
        .any(|a| a.path == owner || id.is_some_and(|id| a.bundle_id == id))
    {
        return Err("Quit this application before removing it or its data.".into());
    }
    Ok(())
}

pub struct ApplicationsModule;
impl ScanModule for ApplicationsModule {
    fn descriptor(&self) -> ModuleDescriptor {
        descriptor(
            "applications",
            "Applications",
            "Applications",
            "app.badge",
            "Apps and precisely named related files, selected separately.",
            false,
        )
    }
    fn action_roots(&self) -> Vec<String> {
        install_roots().into_iter().chain(data_roots()).collect()
    }
    fn scan(
        &self,
        s: &Services,
        c: &ScanContext,
        k: &ScanControl,
        sink: &mut dyn Sink,
    ) -> Result<()> {
        let active = s.apps.running()?;
        let mut warnings = vec![];
        let apps = installed_apps(s, k, &mut warnings)?;
        flush(sink, &mut warnings);
        for app in apps {
            k.check()?;
            let info = s.apps.bundle_info(app.path());
            let id = info.identifier;
            let apple = id.starts_with("com.apple.");
            let running = active
                .iter()
                .any(|a| a.path == app.path() || (!id.is_empty() && a.bundle_id == id));
            let blocked = if apple {
                Some("Apple applications are protected.")
            } else if running {
                Some("Quit this application before removal.")
            } else if id.is_empty() {
                Some("Unknown app identity; inspect manually.")
            } else {
                None
            };
            if c.allows(app.path()) {
                sink.progress(format!("Sizing {}", app.name()));
                let mut f = s.file_finding(
                    &app,
                    "applications",
                    None,
                    "Moving an application to Trash does not remove all its data. Related files are separate selections.",
                    if blocked.is_some() { vec![] } else { vec![ActionKind::Trash] },
                    Risk::Review,
                    k,
                )?;
                if let Some(reason) = blocked {
                    f.blocked_reason = Some(reason.into());
                }
                f.details.extend([
                    detail("Bundle identifier", id.clone()),
                    detail(
                        "Version",
                        if info.version.is_empty() {
                            "Unknown".into()
                        } else {
                            info.version
                        },
                    ),
                    detail("State", if running { "Running" } else { "Not running" }),
                    detail("Application path", app.path()),
                ]);
                sink.finding(f);
            }
            if id.is_empty() || apple {
                continue;
            }
            let h = policy::home();
            for path in [
                format!("{h}/Library/Caches/{id}"),
                format!("{h}/Library/Preferences/{id}.plist"),
                format!("{h}/Library/Saved Application State/{id}.savedState"),
                format!("{h}/Library/Application Support/{id}"),
            ] {
                if !c.allows(&path) || !s.exists(&path) {
                    continue;
                }
                let Ok(e) = s.entry(&path) else {
                    continue;
                };
                let title = format!("{} · {}", app.name(), e.name());
                let mut f = s.file_finding(
                    &e,
                    "applications",
                    Some(&title),
                    "Precisely named related data. May contain preferences or user documents; review it separately.",
                    if running { vec![] } else { vec![ActionKind::Trash] },
                    Risk::Review,
                    k,
                )?;
                f.details.extend([
                    detail("Application path", app.path()),
                    detail("Bundle identifier", id.clone()),
                ]);
                if running {
                    f.blocked_reason = Some("Quit the owning application first.".into());
                }
                sink.finding(f);
            }
        }
        Ok(())
    }
    fn preflight(&self, s: &Services, f: &Finding, _: ActionKind, _: &ScanControl) -> Result<()> {
        owner_not_running(s, f)
    }
}

/// A reverse-DNS bundle identifier from a Library item name, excluding Apple and shared
/// group containers.
pub fn leftover_id(name: &str) -> Option<String> {
    let id = name
        .strip_suffix(".savedState")
        .or_else(|| name.strip_suffix(".plist"))
        .unwrap_or(name);
    let valid = id.split('.').count() >= 3
        && id.split('.').all(|s| {
            !s.is_empty()
                && s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        });
    (valid && !id.starts_with("com.apple.") && !id.starts_with("group.")).then(|| id.into())
}

pub struct LeftoversModule;
impl ScanModule for LeftoversModule {
    fn descriptor(&self) -> ModuleDescriptor {
        descriptor(
            "leftovers",
            "App Leftovers",
            "Applications",
            "app.dashed",
            "Possible leftovers with uncertain ownership made explicit.",
            false,
        )
    }
    fn action_roots(&self) -> Vec<String> {
        data_roots()
    }
    fn scan(
        &self,
        s: &Services,
        c: &ScanContext,
        k: &ScanControl,
        sink: &mut dyn Sink,
    ) -> Result<()> {
        let mut warnings = vec![];
        let installed: std::collections::HashSet<String> = installed_apps(s, k, &mut warnings)?
            .iter()
            .map(|app| s.apps.bundle_info(app.path()).identifier)
            .filter(|id| !id.is_empty())
            .collect();
        let active = s.apps.running()?;
        for directory in ["Caches", "Preferences", "Saved Application State"] {
            let root = format!("{}/Library/{directory}", policy::home());
            if !Path::new(&root).is_dir() || c.excludes(&root) {
                continue;
            }
            for e in s.children(&root, &mut warnings) {
                k.check()?;
                let Some(id) = leftover_id(e.name()) else {
                    continue;
                };
                if installed.contains(&id) || !c.allows(e.path()) {
                    continue;
                }
                let running = active.iter().any(|a| a.bundle_id == id);
                let mut f = s.file_finding(
                    &e,
                    "leftovers",
                    None,
                    "No matching app was found in /Applications or ~/Applications. An app elsewhere may still use this file. Preferences may contain settings or license information; inspect before removal.",
                    if running { vec![] } else { vec![ActionKind::Trash] },
                    Risk::Review,
                    k,
                )?;
                f.details.extend([
                    detail("Bundle identifier", id),
                    detail("Location type", directory),
                    detail(
                        "Classification",
                        "Possible leftover; ownership is uncertain",
                    ),
                ]);
                if running {
                    f.blocked_reason = Some("The owning application is running.".into());
                }
                sink.finding(f);
            }
        }
        flush(sink, &mut warnings);
        Ok(())
    }
    fn preflight(&self, s: &Services, f: &Finding, _: ActionKind, _: &ScanControl) -> Result<()> {
        owner_not_running(s, f)
    }
}
