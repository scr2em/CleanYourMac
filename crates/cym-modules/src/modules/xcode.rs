//! Xcode's per-user data: DerivedData, device support for every platform, archives, device
//! logs, documentation caches, SwiftUI preview simulators, simulator caches, and Xcode
//! installations other than the selected one.
use super::{add_files, descriptor, flush, wildcard, Candidate, LastUsed, ScanModule};
use crate::{
    adapters::plist_keys, model::*, orphans, policy, ports::*, services::Services,
    simulator::Simctl,
};
use std::path::Path;

/// Folders whose every item is listed: (home-relative folder, what an item is, risk).
const PER_ITEM: &[(&str, &str, Risk)] = &[
    (
        "Library/Developer/Xcode/DerivedData",
        "Build data",
        Risk::Rebuild,
    ),
    (
        "Library/Developer/Xcode/iOS DeviceSupport",
        "iOS device support",
        Risk::Rebuild,
    ),
    (
        "Library/Developer/Xcode/watchOS DeviceSupport",
        "watchOS device support",
        Risk::Rebuild,
    ),
    (
        "Library/Developer/Xcode/tvOS DeviceSupport",
        "tvOS device support",
        Risk::Rebuild,
    ),
    (
        "Library/Developer/Xcode/visionOS DeviceSupport",
        "visionOS device support",
        Risk::Rebuild,
    ),
    (
        "Library/Developer/Xcode/macOS DeviceSupport",
        "macOS device support",
        Risk::Rebuild,
    ),
    ("Library/Developer/Xcode/Archives", "Archive", Risk::Review),
];
/// Folders listed as one item: (home-relative path, title, reason, risk).
const WHOLE: &[(&str, &str, &str, Risk)] = &[
    (
        "Library/Developer/Xcode/iOS Device Logs",
        "Device logs",
        "Logs Xcode copied from connected devices. They help read old crashes.",
        Risk::Review,
    ),
    (
        "Library/Developer/Xcode/DocumentationCache",
        "Documentation cache",
        "Xcode downloads documentation again when you open it.",
        Risk::Rebuild,
    ),
    (
        "Library/Developer/Xcode/DocumentationIndex",
        "Documentation index",
        "Xcode indexes documentation again when you open it.",
        Risk::Rebuild,
    ),
    (
        "Library/Developer/CoreSimulator/Caches",
        "Simulator caches",
        "Shared caches of the simulators; made again when a simulator starts.",
        Risk::Rebuild,
    ),
];
/// Containers of apps removed from a simulator that the simulator did not clean up.
const DEAD: &str =
    "Library/Developer/CoreSimulator/Devices/*/data/Library/Caches/com.apple.containermanagerd/Dead";
const PREVIEWS: &str = "Library/Developer/Xcode/UserData/Previews";
const XCODE: &str = "com.apple.dt.Xcode";

#[derive(Default)]
pub struct XcodeModule {
    /// The home folder; the current user's when `None`.
    pub home: Option<String>,
    /// The developer folder `xcode-select` points at; read from the system when `None`.
    pub developer_dir: Option<String>,
}
impl XcodeModule {
    fn home(&self) -> String {
        self.home.clone().unwrap_or_else(policy::home)
    }
    /// Folders the module's actions may touch.
    pub fn roots(&self) -> Vec<String> {
        let home = self.home();
        PER_ITEM
            .iter()
            .map(|(p, _, _)| *p)
            .chain(WHOLE.iter().map(|(p, _, _, _)| *p))
            .chain(["Library/Developer/CoreSimulator/Devices", "Applications"])
            .map(|p| format!("{home}/{p}"))
            .chain(["/Applications".to_owned()])
            .collect()
    }
    /// The Xcode that `xcode-select` selects.
    fn selected(&self) -> String {
        let developer = self.developer_dir.clone().unwrap_or_else(|| {
            std::fs::read_link("/var/db/xcode_select_link")
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_else(|_| "/Applications/Xcode.app/Contents/Developer".into())
        });
        developer
            .strip_suffix("/Contents/Developer")
            .unwrap_or(&developer)
            .to_owned()
    }
    /// The Xcode that always stays: the one xcode-select selects or, when it selects the
    /// Command Line Tools or something else, the newest installed version.
    fn kept(&self, installs: &[(Entry, String)]) -> Option<(String, &'static str)> {
        let selected = policy::canonical(&self.selected());
        if installs
            .iter()
            .any(|(e, _)| policy::canonical(e.path()) == selected)
        {
            return Some((selected, "This is the Xcode that xcode-select selects."));
        }
        let number =
            |v: &str| -> Vec<u64> { v.split('.').map(|p| p.parse().unwrap_or(0)).collect() };
        installs
            .iter()
            .max_by(|(a, x), (b, y)| {
                number(x)
                    .cmp(&number(y))
                    .then_with(|| b.path().cmp(a.path()))
            })
            .map(|(e, _)| {
                (
                    policy::canonical(e.path()),
                    "xcode-select does not select an installed Xcode, so the newest one stays.",
                )
            })
    }
    /// Installed Xcode apps, by bundle identifier, with their versions.
    fn installs(&self, s: &Services) -> Vec<(Entry, String)> {
        let home = self.home();
        ["/Applications".to_owned(), format!("{home}/Applications")]
            .iter()
            .flat_map(|folder| s.children(folder, &mut vec![]))
            .filter(|e| e.directory && e.name().ends_with(".app"))
            .filter_map(|e| {
                let (id, version) = bundle(e.path())?;
                (id == XCODE).then_some((e, version))
            })
            .collect()
    }
}

/// When Xcode last opened a DerivedData folder, from its `info.plist`.
fn derived_data_accessed(folder: &str) -> Option<f64> {
    let info = plist_keys::read(&format!("{folder}/info.plist"), &["LastAccessedDate"])?;
    match info.get("LastAccessedDate")? {
        plist_keys::Scalar::Date(seconds) => Some(*seconds),
        _ => None,
    }
}
/// An app's bundle identifier and short version, from its `Info.plist`, read with bounds.
pub fn bundle(app: &str) -> Option<(String, String)> {
    let d = plist_keys::read(
        &format!("{app}/Contents/Info.plist"),
        &["CFBundleIdentifier", "CFBundleShortVersionString"],
    )?;
    let text = |k: &str| {
        d.get(k)
            .and_then(|v| v.text())
            .unwrap_or_default()
            .to_owned()
    };
    Some((
        text("CFBundleIdentifier"),
        text("CFBundleShortVersionString"),
    ))
}
/// Existing folders for a home-relative pattern in which `*` matches one component.
fn matches(s: &Services, home: &str, pattern: &str) -> Vec<Entry> {
    let mut current = vec![home.to_owned()];
    for part in pattern.split('/') {
        current = if part.contains('*') {
            current
                .iter()
                .flat_map(|dir| s.children(dir, &mut vec![]))
                .filter(|e| e.directory && wildcard(part, e.name()))
                .map(|e| e.path().to_owned())
                .collect()
        } else {
            current.into_iter().map(|d| format!("{d}/{part}")).collect()
        };
    }
    current.iter().filter_map(|p| s.entry(p).ok()).collect()
}

impl ScanModule for XcodeModule {
    fn descriptor(&self) -> ModuleDescriptor {
        descriptor(
            "xcode",
            "Xcode Data",
            "Developer",
            "wrench.and.screwdriver",
            "Build data, device support, preview simulators and extra Xcode installs. Archives remain protected.",
            false,
        )
    }
    fn action_roots(&self) -> Vec<String> {
        self.roots()
    }
    fn scan(
        &self,
        s: &Services,
        c: &ScanContext,
        k: &ScanControl,
        sink: &mut dyn Sink,
    ) -> Result<()> {
        let home = self.home();
        let mut warnings = vec![];
        let mut candidates = vec![];
        for (folder, kind, risk) in PER_ITEM {
            let root = format!("{home}/{folder}");
            if !s.is_dir(&root) || c.excludes(&root) {
                continue;
            }
            let archive = folder.ends_with("/Archives");
            for e in s.children(&root, &mut warnings) {
                if !c.allows(e.path()) {
                    continue;
                }
                let details = vec![detail("Kind", *kind)];
                candidates.push(if archive {
                    Candidate::new(
                        e,
                        "Archives may contain irreplaceable release builds and dSYMs.",
                        vec![],
                        *risk,
                    )
                    .details(details)
                    .acknowledge(
                        "This archive may be the only copy of a build you shipped, with the debug symbols (dSYMs) needed to read its crash reports.",
                        vec![ActionKind::Trash],
                    )
                } else {
                    let accessed = derived_data_accessed(e.path());
                    Candidate::new(
                        e,
                        "Xcode regenerates this build or device-support data when needed.",
                        vec![ActionKind::Trash],
                        *risk,
                    )
                    .details(details)
                    .last_used(accessed.map_or(LastUsed::Unknown, LastUsed::At))
                });
            }
        }
        for (path, title, reason, risk) in WHOLE {
            let path = format!("{home}/{path}");
            let Ok(e) = s.entry(&path) else { continue };
            if e.directory && c.allows(&path) {
                let modified = e.modified();
                candidates.push(
                    Candidate::new(e, reason, vec![ActionKind::Trash], *risk)
                        .title(*title)
                        .last_used(LastUsed::At(modified)),
                );
            }
        }
        for e in matches(s, &home, DEAD) {
            if !c.allows(e.path()) {
                continue;
            }
            let device = e
                .path()
                .split("/Devices/")
                .nth(1)
                .and_then(|r| r.split('/').next())
                .unwrap_or_default()
                .to_owned();
            let modified = e.modified();
            candidates.push(
                Candidate::new(
                    e,
                    "Containers of apps removed from this simulator, which the simulator did not clean up. Quit Simulator first.",
                    vec![ActionKind::Trash],
                    Risk::Review,
                )
                .title(format!("Removed app data · simulator {}", device.chars().take(8).collect::<String>()))
                .details(vec![detail("Simulator", device)])
                .last_used(LastUsed::At(modified)),
            );
        }
        // Xcode installations other than the selected one.
        let installs = self.installs(s);
        if installs.len() > 1 {
            let selected = policy::canonical(&self.selected());
            let kept = self.kept(&installs);
            for (e, version) in installs {
                if !c.allows(e.path()) {
                    continue;
                }
                let current = kept
                    .as_ref()
                    .filter(|(path, _)| *path == policy::canonical(e.path()))
                    .map(|(_, reason)| *reason);
                let modified = e.modified();
                let candidate = Candidate::new(
                    e,
                    &format!("Another Xcode installation. Command-line builds use the one xcode-select selects ({selected}). Download it again from Apple Developer if you need this version."),
                    vec![ActionKind::Trash],
                    Risk::Review,
                )
                .title(format!("Xcode {version}"))
                .details(vec![detail("Kind", "Xcode installation"), detail("Version", version.clone())])
                .last_used(LastUsed::At(modified));
                candidates.push(candidate.blocked(current));
            }
        }
        flush(sink, &mut warnings);
        add_files(s, sink, "xcode", candidates, k)?;

        // SwiftUI preview simulators, removed with simctl so Xcode's records stay consistent.
        let previews = format!("{home}/{PREVIEWS}");
        if let Ok(e) = s.entry(&previews) {
            if e.directory && c.allows(&previews) {
                let size = s.sizer.size(&previews, k).ok();
                let mut f = Finding::new(
                    "xcode",
                    "previews",
                    "SwiftUI preview simulators",
                    Resource::Command {
                        tool: "xcrun".into(),
                        task: "previews-delete-all".into(),
                    },
                    "Simulators Xcode made to show SwiftUI previews. Xcode makes them again the next time it shows a preview.",
                );
                f.subtitle = previews.clone();
                f.bytes = size.as_ref().map(|s| s.logical);
                f.allocated_bytes = size.as_ref().map(|s| s.allocated);
                f.modified_at = Some(e.modified());
                f.last_used_at = Some(e.modified());
                f.risk = Risk::Rebuild;
                f.details = vec![
                    detail("Data path", previews.clone()),
                    detail("Command", "xcrun simctl --set previews delete all"),
                ];
                f.actions = vec![ActionKind::RunCommand];
                if size.is_none_or(|s| !s.complete) {
                    f.blocked_reason =
                        Some("Incomplete size or permission coverage; inspect manually.".into());
                }
                sink.finding(f);
            }
        }
        Ok(())
    }
    fn in_use(&self, s: &Services, f: &Finding, _: ActionKind) -> Option<String> {
        let apps = s.apps.running().unwrap_or_default();
        let simulator = f
            .resource
            .path()
            .is_some_and(|p| p.contains("/Library/Developer/CoreSimulator/"));
        let open: Vec<String> = apps
            .iter()
            .filter(|a| {
                a.bundle_id == XCODE || (simulator && a.bundle_id == "com.apple.iphonesimulator")
            })
            .map(|a| {
                let name = Path::new(&a.path)
                    .file_stem()
                    .and_then(|n| n.to_str())
                    .unwrap_or("Xcode");
                format!("{name} (PID {})", a.pid)
            })
            .collect();
        let building: Vec<String> = orphans::active_tools(s, None)
            .into_iter()
            .filter(|t| t.starts_with("xcodebuild ") || t.starts_with("swift-frontend "))
            .chain(open)
            .collect();
        (!building.is_empty()).then(|| {
            format!(
                "Quit Xcode and Simulator and stop builds before removing their data:{}",
                orphans::list(&building)
            )
        })
    }
    fn preflight(
        &self,
        s: &Services,
        f: &Finding,
        kind: ActionKind,
        _: &ScanControl,
    ) -> Result<()> {
        // Xcode, Simulator and builds use this data while they run; this is never overridden.
        if let Some(reason) = self.in_use(s, f, kind) {
            return Err(reason);
        }
        let Some(path) = f.resource.path() else {
            return Ok(());
        };
        // In an Applications folder, only an Xcode other than the selected one.
        if path.ends_with(".app") {
            let (id, _) = bundle(path).ok_or("This app can no longer be read.")?;
            if id != XCODE {
                return Err("Only Xcode installations are removed here.".into());
            }
            let installs = self.installs(s);
            match self.kept(&installs) {
                Some((kept, reason)) if kept == policy::canonical(path) => {
                    return Err(reason.into())
                }
                None => return Err("No Xcode installation could be read.".into()),
                _ if installs.len() < 2 => {
                    return Err("This is the only Xcode installation.".into())
                }
                _ => {}
            }
        }
        Ok(())
    }
    fn run(&self, s: &Services, f: &Finding, _: &ScanContext, k: &ScanControl) -> Result<String> {
        match &f.resource {
            Resource::Command { tool, task }
                if tool == "xcrun" && task == "previews-delete-all" =>
            {
                let out = Simctl(s.commands.as_ref()).run(
                    &["--set", "previews", "delete", "all"],
                    600,
                    k,
                )?;
                if out.status != 0 {
                    return Err(out.error.trim().chars().take(400).collect());
                }
                Ok("Deleted the SwiftUI preview simulators.".into())
            }
            _ => Err("This cleanup is not one the app runs.".into()),
        }
    }
}
