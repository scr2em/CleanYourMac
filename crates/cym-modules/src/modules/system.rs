//! Space macOS itself holds, which Settings counts as System Data: Time Machine's local
//! snapshots, and data the system downloads and manages (Apple Intelligence models, macOS
//! updates, the GarageBand and Logic sound library, aerial screen saver videos).
//!
//! Snapshots are thinned with `tmutil`. The rest is listed only so its size is known: it
//! lives in folders the system protects, and each item says where macOS lets you remove it.
use super::{descriptor, ScanModule};
use crate::{model::*, ports::*, services::Services};
use std::time::Duration;

const TMUTIL: &str = "/usr/bin/tmutil";
/// Bytes asked of `tmutil thinlocalsnapshots`: more than any disk holds, so it removes every
/// local snapshot it may.
const THIN_ALL: &str = "999999999999999";

/// Folders the system manages: (id, title, absolute folders, what it is, how to remove it).
/// A `*` in the last component matches the folder names it starts with.
pub struct SystemFolder {
    pub id: &'static str,
    pub title: &'static str,
    pub paths: &'static [&'static str],
    pub reason: &'static str,
    pub remove: &'static str,
}
pub const FOLDERS: &[SystemFolder] = &[
    SystemFolder {
        id: "apple-intelligence",
        title: "Apple Intelligence and on-device models",
        paths: &["/System/Library/AssetsV2/com_apple_MobileAsset_UAF_*"],
        reason: "Language and image models macOS downloads for Apple Intelligence, Siri and other on-device features. macOS keeps them up to date.",
        remove: "macOS manages these models. Turning off Apple Intelligence in System Settings › Apple Intelligence & Siri can remove them on some macOS versions; there is no supported way to delete them by hand.",
    },
    SystemFolder {
        id: "macos-updates",
        title: "macOS update downloads",
        paths: &["/Library/Updates"],
        reason: "Updates Software Update downloaded. macOS removes them after it installs them; a failed or postponed update can leave them behind.",
        remove: "Install or cancel the pending update in System Settings › General › Software Update. macOS removes the download afterwards.",
    },
    SystemFolder {
        id: "sound-library",
        title: "GarageBand and Logic sound library",
        paths: &[
            "/Library/Application Support/GarageBand",
            "/Library/Application Support/Logic",
            "/Library/Audio/Apple Loops",
        ],
        reason: "Instruments, loops and samples of GarageBand, Logic Pro and MainStage, shared by all three.",
        remove: "In System Settings › General › Storage, open Music Creation and choose Remove GarageBand Sound Library, or move the library to another drive in Logic Pro › Sound Library.",
    },
    SystemFolder {
        id: "aerials",
        title: "Aerial screen saver and wallpaper videos",
        paths: &["/Library/Application Support/com.apple.idleassetsd/Customer"],
        reason: "Videos of the aerial screen savers and wallpapers you downloaded.",
        remove: "In System Settings › Wallpaper or Screen Saver, choose a different one; macOS removes downloaded videos it no longer needs when space runs low.",
    },
];

pub struct SystemModule {
    /// The folders to measure, by `FOLDERS` id; `FOLDERS` when `None`. Tests point these at
    /// fixtures.
    pub folders: Option<Vec<(String, Vec<String>)>>,
    /// Whether to list Time Machine's local snapshots with `tmutil`.
    pub time_machine: bool,
}
impl Default for SystemModule {
    fn default() -> Self {
        Self {
            folders: None,
            time_machine: true,
        }
    }
}
impl SystemModule {
    /// Each folder kind's id and absolute folders.
    fn folders(&self) -> Vec<(String, Vec<String>)> {
        self.folders.clone().unwrap_or_else(|| {
            FOLDERS
                .iter()
                .map(|f| {
                    (
                        f.id.to_owned(),
                        f.paths.iter().map(|p| (*p).to_owned()).collect(),
                    )
                })
                .collect()
        })
    }
    /// Existing folders for a path, expanding a `*` in its last component.
    fn expand(s: &Services, path: &str) -> Vec<String> {
        match path.rsplit_once('/') {
            Some((parent, last)) if last.contains('*') => s
                .children(parent, &mut vec![])
                .into_iter()
                .filter(|e| e.directory && super::glob(last, e.name()).is_some())
                .map(|e| e.identity.path)
                .collect(),
            _ => s
                .entry(path)
                .ok()
                .filter(|e| e.directory)
                .map(|e| e.identity.path)
                .into_iter()
                .collect(),
        }
    }
    fn snapshots(&self, s: &Services, k: &ScanControl) -> Result<Vec<String>> {
        let out = s.commands.run(
            TMUTIL,
            &["listlocalsnapshots".into(), "/".into()],
            Duration::from_secs(30),
            k,
        )?;
        if out.status != 0 {
            return Err(out.error.trim().chars().take(240).collect());
        }
        Ok(parse_snapshots(&String::from_utf8_lossy(&out.data)))
    }
}

/// Time Machine's local snapshot names in `tmutil listlocalsnapshots` output, such as
/// `com.apple.TimeMachine.2026-10-01-101010.local`, oldest first.
pub fn parse_snapshots(output: &str) -> Vec<String> {
    let mut names: Vec<String> = output
        .lines()
        .map(str::trim)
        .filter(|l| l.starts_with("com.apple.TimeMachine.") && l.ends_with(".local"))
        .map(str::to_owned)
        .collect();
    names.sort();
    names
}
/// The date in a snapshot name: `2026-10-01-101010` becomes `2026-10-01 10:10:10`.
pub fn snapshot_date(name: &str) -> String {
    let stamp = name
        .trim_start_matches("com.apple.TimeMachine.")
        .trim_end_matches(".local");
    match stamp.rsplit_once('-') {
        Some((day, time)) if time.len() == 6 && time.chars().all(|c| c.is_ascii_digit()) => {
            format!("{day} {}:{}:{}", &time[..2], &time[2..4], &time[4..])
        }
        _ => stamp.to_owned(),
    }
}

impl ScanModule for SystemModule {
    fn descriptor(&self) -> ModuleDescriptor {
        descriptor(
            "system",
            "System Data",
            "Storage",
            "internaldrive",
            "Time Machine local snapshots, and models, updates and media macOS keeps, with how to remove each.",
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
        // System folders lie outside any chosen folder, so a limited scan lists nothing.
        if c.limit_to_roots {
            return Ok(());
        }
        let snapshots = match self.time_machine {
            true => self.snapshots(s, k),
            false => Ok(vec![]),
        };
        match snapshots {
            Ok(names) if !names.is_empty() => {
                let (oldest, newest) = (&names[0], &names[names.len() - 1]);
                let mut f = Finding::new(
                    "system",
                    "time-machine-local-snapshots",
                    "Time Machine local snapshots",
                    Resource::Command {
                        tool: "tmutil".into(),
                        task: "thin-local-snapshots".into(),
                    },
                    "Backups Time Machine keeps on this disk while its backup disk is not connected. macOS counts them as System Data and removes them by itself when space runs low, but not always soon enough for a large copy or install. Removing them leaves your Time Machine disk's backups as they are, but changes made while that disk was away exist only in these snapshots.",
                );
                f.subtitle = format!(
                    "{} {} on the startup disk",
                    names.len(),
                    if names.len() == 1 {
                        "snapshot"
                    } else {
                        "snapshots"
                    }
                );
                f.risk = Risk::Permanent;
                f.details = vec![
                    detail("Snapshots", names.len().to_string()),
                    detail("Oldest", snapshot_date(oldest)),
                    detail("Newest", snapshot_date(newest)),
                    detail("Size", "macOS does not report the size of each snapshot. Settings › General › Storage counts them in System Data."),
                    detail("Command", format!("tmutil thinlocalsnapshots / {THIN_ALL} 4")),
                ];
                f.actions = vec![ActionKind::RunCommand];
                super::confirm_first(&mut f, "Changes made while your Time Machine disk was not connected may exist only in these snapshots, and removing them cannot be undone.");
                sink.finding(f);
            }
            Ok(_) => {}
            Err(e) => sink.warning(format!(
                "Time Machine local snapshots could not be listed: {e}"
            )),
        }
        for (id, paths) in self.folders() {
            k.check()?;
            let Some(info) = FOLDERS.iter().find(|f| f.id == id) else {
                continue;
            };
            let folders: Vec<String> = paths.iter().flat_map(|p| Self::expand(s, p)).collect();
            if folders.is_empty() {
                continue;
            }
            let (mut logical, mut allocated, mut complete) = (0, 0, true);
            for folder in &folders {
                match s.sizer.size(folder, k) {
                    Ok(size) => {
                        logical += size.logical;
                        allocated += size.allocated;
                        complete &= size.complete;
                    }
                    Err(_) => complete = false,
                }
            }
            if allocated == 0 && complete {
                continue;
            }
            let mut f = Finding::new(
                "system",
                id.as_str(),
                info.title,
                Resource::Command {
                    tool: "macos".into(),
                    task: id.clone(),
                },
                info.reason,
            );
            f.subtitle = folders.join(", ");
            f.bytes = Some(logical);
            f.allocated_bytes = Some(allocated);
            f.risk = Risk::Review;
            f.details = vec![
                detail("Ecosystem", "macOS"),
                detail("Location", folders.join("\n")),
                detail("How to remove", info.remove),
            ];
            if !complete {
                f.details.push(detail(
                    "Coverage",
                    "Some files could not be read, so the size may be larger.",
                ));
            }
            f.blocked_reason = Some(format!("macOS manages this data. {}", info.remove));
            sink.finding(f);
        }
        Ok(())
    }
    fn run(&self, s: &Services, f: &Finding, _: &ScanContext, k: &ScanControl) -> Result<String> {
        match &f.resource {
            Resource::Command { tool, task }
                if tool == "tmutil" && task == "thin-local-snapshots" =>
            {
                let before = self.snapshots(s, k)?.len();
                let out = s.commands.run(
                    TMUTIL,
                    &[
                        "thinlocalsnapshots".into(),
                        "/".into(),
                        THIN_ALL.into(),
                        "4".into(),
                    ],
                    Duration::from_secs(600),
                    k,
                )?;
                if out.status != 0 {
                    return Err(out.error.trim().chars().take(400).collect());
                }
                let after = self.snapshots(s, k).map(|n| n.len()).unwrap_or(before);
                Ok(match before.saturating_sub(after) {
                    0 => "Time Machine kept its local snapshots; macOS may still be using them. Try again later.".into(),
                    1 => "Removed 1 Time Machine local snapshot.".into(),
                    n => format!("Removed {n} Time Machine local snapshots."),
                })
            }
            _ => Err("This cleanup is not one the app runs.".into()),
        }
    }
}
