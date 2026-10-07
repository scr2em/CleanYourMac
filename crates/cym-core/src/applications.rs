use crate::{
    files::{self, ScanControl},
    model::*,
    modules::ScanModule,
    native, policy,
};
use std::{collections::HashSet, path::Path};
pub struct AppModule(pub ModuleDescriptor);
pub fn bundle_info(path: &str) -> (String, String) {
    let info = plist::Value::from_file(path.to_owned() + "/Contents/Info.plist").ok();
    let dict = info.as_ref().and_then(|i| i.as_dictionary());
    let value = |key: &str| {
        dict.and_then(|d| d.get(key))
            .and_then(|v| v.as_string())
            .unwrap_or("")
            .to_owned()
    };
    (
        value("CFBundleIdentifier"),
        value("CFBundleShortVersionString"),
    )
}
pub fn leftover_id(name: &str) -> Option<String> {
    let id = name
        .strip_suffix(".savedState")
        .or_else(|| name.strip_suffix(".plist"))
        .unwrap_or(name);
    if id.starts_with("com.apple.")
        || id.starts_with("group.")
        || id.split('.').count() < 3
        || id.split('.').any(|s| {
            s.is_empty()
                || !s
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        })
    {
        None
    } else {
        Some(id.into())
    }
}
impl ScanModule for AppModule {
    fn descriptor(&self) -> ModuleDescriptor {
        self.0.clone()
    }
    fn scan(&self, c: &ScanContext, k: &ScanControl, r: &mut ScanReport) -> Result<()> {
        let active = native::running_apps()?;
        let mut installed = HashSet::new();
        let mut apps = vec![];
        for root in ["/Applications".into(), policy::home() + "/Applications"]
            .into_iter()
            .filter(|p: &String| Path::new(p).is_dir())
        {
            files::walk(
                &ScanContext {
                    roots: vec![root],
                    ..Default::default()
                },
                k,
                &mut r.warnings,
                |e| {
                    if e.directory && e.path().ends_with(".app") {
                        apps.push(e.clone());
                        return Ok(false);
                    }
                    Ok(true)
                },
            )?;
        }
        for app in apps {
            k.check()?;
            let (id, version) = bundle_info(app.path());
            if !id.is_empty() {
                installed.insert(id.clone());
            }
            if self.0.id == "leftovers" {
                continue;
            }
            let apple = id.starts_with("com.apple.");
            let running = active
                .iter()
                .any(|a| a.path == app.path() || (!id.is_empty() && a.bundle_id == id));
            let blocked = if apple {
                Some("Apple applications are protected")
            } else if running {
                Some("Quit this application before removal")
            } else if id.is_empty() {
                Some("Unknown application identity; inspect manually")
            } else {
                None
            };
            if c.allows(app.path()) {
                let mut f=files::finding(&app,"applications",None,"Moving an app to Trash does not remove all of its data. Related files are separate selections.",if blocked.is_some(){vec![]}else{vec![ActionKind::Trash]},Risk::Review,k)?;
                if let Some(reason) = blocked {
                    f.blocked_reason = Some(reason.into());
                }
                f.details.extend([
                    detail("Bundle identifier", id.clone()),
                    detail("Version", version),
                    detail("State", if running { "Running" } else { "Not running" }),
                    detail("Application path", app.path()),
                ]);
                r.findings.push(f);
            }
            if id.is_empty() || apple {
                continue;
            }
            for path in [
                format!("{}/Library/Caches/{id}", policy::home()),
                format!("{}/Library/Preferences/{id}.plist", policy::home()),
                format!(
                    "{}/Library/Saved Application State/{id}.savedState",
                    policy::home()
                ),
                format!("{}/Library/Application Support/{id}", policy::home()),
            ] {
                if !c.allows(&path) || !Path::new(&path).exists() {
                    continue;
                }
                let e = files::entry(&path)?;
                let title = format!("{} · {}", app.name(), e.name());
                let mut f=files::finding(&e,"applications",Some(&title),"Precisely named related data may contain preferences or user documents. Review it separately.",if running{vec![]}else{vec![ActionKind::Trash]},Risk::Review,k)?;
                f.details.extend([
                    detail("Application path", app.path()),
                    detail("Bundle identifier", id.clone()),
                ]);
                if running {
                    f.blocked_reason = Some("Quit the owning application first".into());
                }
                r.findings.push(f);
            }
        }
        if self.0.id == "leftovers" {
            for directory in ["Caches", "Preferences", "Saved Application State"] {
                let root = format!("{}/Library/{directory}", policy::home());
                if !Path::new(&root).is_dir() || c.excludes(&root) {
                    continue;
                }
                for e in files::children(&root, &mut r.warnings) {
                    k.check()?;
                    if !c.allows(e.path()) {
                        continue;
                    }
                    let Some(id) = leftover_id(e.name()) else {
                        continue;
                    };
                    if installed.contains(&id) {
                        continue;
                    }
                    let running = active.iter().any(|a| a.bundle_id == id);
                    let mut f=files::finding(&e,"leftovers",None,"No matching app in the usual install folders. An app elsewhere may still use this data. Ownership is uncertain; inspect before removal.",if running{vec![]}else{vec![ActionKind::Trash]},Risk::Review,k)?;
                    f.details.extend([
                        detail("Bundle identifier", id),
                        detail("Location type", directory),
                        detail(
                            "Classification",
                            "Possible leftover; ownership is uncertain",
                        ),
                    ]);
                    if running {
                        f.blocked_reason = Some("Owning application is running".into());
                    }
                    r.findings.push(f);
                }
            }
        }
        Ok(())
    }
}
