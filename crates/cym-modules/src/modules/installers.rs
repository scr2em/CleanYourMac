//! Installers you already ran: macOS installer apps, Xcode `.xip` archives, disk images,
//! packages and ISO images in Downloads and on the Desktop.
use super::{add_files, descriptor, xcode::bundle, Candidate, LastUsed, ScanModule};
use crate::{model::*, policy, ports::*, services::Services};
use std::path::Path;

/// File types that are installers or disk images.
const EXTENSIONS: &[(&str, &str)] = &[
    ("dmg", "Disk image"),
    ("pkg", "Installer package"),
    ("mpkg", "Installer package"),
    ("xip", "Xcode archive"),
    ("iso", "ISO image"),
];
/// Folders searched for installer files, relative to the home folder, two levels deep.
const FOLDERS: &[&str] = &["Downloads", "Desktop"];
/// The bundle identifier prefix of macOS installer apps.
const INSTALL_ASSISTANT: &str = "com.apple.InstallAssistant";

#[derive(Default)]
pub struct InstallersModule {
    /// The home folder; the current user's when `None`.
    pub home: Option<String>,
}
impl InstallersModule {
    fn home(&self) -> String {
        self.home.clone().unwrap_or_else(policy::home)
    }
    fn app_folders(&self) -> Vec<String> {
        vec![
            "/Applications".into(),
            format!("{}/Applications", self.home()),
        ]
    }
}
/// The kind of installer a file is, from its extension.
fn kind(name: &str) -> Option<&'static str> {
    let ext = Path::new(name).extension()?.to_str()?.to_ascii_lowercase();
    EXTENSIONS.iter().find(|(e, _)| *e == ext).map(|(_, k)| *k)
}

impl ScanModule for InstallersModule {
    fn descriptor(&self) -> ModuleDescriptor {
        descriptor(
            "installers",
            "Installers",
            "Storage",
            "opticaldiscdrive",
            "macOS installers, Xcode archives, disk images and packages you already installed from.",
            false,
        )
    }
    fn action_roots(&self) -> Vec<String> {
        let home = self.home();
        FOLDERS
            .iter()
            .map(|f| format!("{home}/{f}"))
            .chain(self.app_folders())
            .collect()
    }
    fn scan(
        &self,
        s: &Services,
        c: &ScanContext,
        k: &ScanControl,
        sink: &mut dyn Sink,
    ) -> Result<()> {
        let home = self.home();
        let mut candidates = vec![];
        for folder in self.app_folders() {
            for e in s.children(&folder, &mut vec![]) {
                k.check()?;
                if !e.directory || !e.name().ends_with(".app") || !c.allows(e.path()) {
                    continue;
                }
                let Some((id, version)) = bundle(e.path()) else {
                    continue;
                };
                if !id.starts_with(INSTALL_ASSISTANT) {
                    continue;
                }
                let name = e.name().trim_end_matches(".app").to_owned();
                candidates.push(
                    Candidate::new(
                        e,
                        "A macOS installer. Download it again from the App Store, or with softwareupdate --fetch-full-installer, if you need it.",
                        vec![ActionKind::Trash],
                        Risk::Rebuild,
                    )
                    .title(name)
                    .details(vec![detail("Kind", "macOS installer"), detail("Version", version)])
                    .last_used(LastUsed::Spotlight),
                );
            }
        }
        for folder in FOLDERS {
            let root = format!("{home}/{folder}");
            let mut pending = vec![(root, 0)];
            while let Some((dir, depth)) = pending.pop() {
                k.check()?;
                for e in s.children(&dir, &mut vec![]) {
                    if !c.allows(e.path()) || e.name().starts_with('.') {
                        continue;
                    }
                    if e.directory {
                        // An app bundle or package folder is one item, never searched inside.
                        if depth < 1 && !e.name().contains('.') {
                            pending.push((e.path().to_owned(), depth + 1));
                        }
                        continue;
                    }
                    let Some(kind) = kind(e.name()) else { continue };
                    candidates.push(
                        Candidate::new(
                            e,
                            "An installer or disk image. Installers download again from their publisher; a disk image you made yourself, such as an encrypted one, exists nowhere else.",
                            vec![ActionKind::Trash],
                            Risk::Review,
                        )
                        .details(vec![detail("Kind", kind)])
                        .last_used(LastUsed::Spotlight),
                    );
                }
            }
        }
        add_files(s, sink, "installers", candidates, k)
    }
    fn in_use(&self, s: &Services, f: &Finding, _: ActionKind) -> Option<String> {
        let path = f.resource.path()?;
        let running = match s.apps.running() {
            Ok(running) => running,
            Err(e) => {
                return Some(format!(
                    "Cannot check whether this installer is running: {e}"
                ))
            }
        };
        running
            .iter()
            .find(|a| policy::canonical(&a.path) == policy::canonical(path))
            .map(|a| format!("This installer is running (PID {}). Quit it first.", a.pid))
    }
    fn preflight(&self, _: &Services, f: &Finding, _: ActionKind, _: &ScanControl) -> Result<()> {
        let path = f.resource.path().ok_or("Missing path")?;
        if path.ends_with(".app") {
            let (id, _) = bundle(path).ok_or("This app can no longer be read.")?;
            if !id.starts_with(INSTALL_ASSISTANT) {
                return Err("Only macOS installer apps are removed here.".into());
            }
        }
        Ok(())
    }
}
