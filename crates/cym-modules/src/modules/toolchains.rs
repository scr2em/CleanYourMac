//! Installed language toolchains, SDK components and virtual devices, one finding per version.
use super::{add_files, descriptor, flush, glob, owners, owners_closed, Candidate, ScanModule};
use crate::{model::*, policy, ports::*, services::Services};
use std::path::Path;

/// How a version manager records what is in use. Paths are relative to the home folder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InUse {
    /// rustup `settings.toml`: `default_toolchain` and directory overrides.
    Rustup(&'static str),
    /// nvm's alias folder; `default` is followed until it names a version.
    Nvm(&'static str),
    /// A file of whitespace-separated version names (pyenv, rbenv).
    VersionFile(&'static str),
    /// asdf-style `tool version…` lines; the first `*` captures the tool.
    ToolVersions(&'static str),
    /// A mise config `[tools]` table; the first `*` captures the tool.
    MiseConfig(&'static str),
    /// A symbolic link whose target lies inside the default version (fvm, fnm).
    Link(&'static str),
    /// A symbolic link with this name beside the versions (SDKMAN `current`).
    Sibling(&'static str),
    /// Volta's `platform.json` default Node runtime.
    Volta(&'static str),
    /// Android system images named by a virtual device's `config.ini` in this folder.
    AvdImages(&'static str),
    /// Lock files inside the folder while an Android emulator is running (stale locks from
    /// a crash are ignored).
    Locked,
    /// A running process whose name starts with one of these prefixes.
    Process(&'static [&'static str]),
}

/// Installed versions under the home folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolchainLocation {
    /// Shown as "Ecosystem"; `None` uses the first `*` (multi-tool managers).
    pub ecosystem: Option<&'static str>,
    /// Title prefix; empty uses the ecosystem.
    pub title: &'static str,
    pub manager: &'static str,
    /// Home-relative; each component with `*` is one listed folder level.
    pub pattern: &'static str,
    pub in_use: &'static [InUse],
    pub reinstall: &'static str,
    /// The manager's own uninstall command. `{name}` is the folder name and `{0}`, `{1}`, …
    /// are the `*` captures.
    pub command: Option<&'static str>,
    /// Bundle-identifier prefixes of apps that must be closed first.
    pub apps: &'static [&'static str],
    pub risk: Risk,
}
fn versions(
    ecosystem: &'static str,
    title: &'static str,
    manager: &'static str,
    pattern: &'static str,
    reinstall: &'static str,
) -> ToolchainLocation {
    ToolchainLocation {
        ecosystem: Some(ecosystem),
        title,
        manager,
        pattern,
        in_use: &[],
        reinstall,
        command: None,
        apps: &[],
        risk: Risk::Rebuild,
    }
}
fn tools(
    manager: &'static str,
    pattern: &'static str,
    reinstall: &'static str,
) -> ToolchainLocation {
    ToolchainLocation {
        ecosystem: None,
        ..versions("", "", manager, pattern, reinstall)
    }
}
impl ToolchainLocation {
    pub fn in_use(mut self, checks: &'static [InUse]) -> Self {
        self.in_use = checks;
        self
    }
    pub fn command(mut self, command: &'static str) -> Self {
        self.command = Some(command);
        self
    }
    pub fn apps(mut self, apps: &'static [&'static str]) -> Self {
        self.apps = apps;
        self
    }
    pub fn risk(mut self, risk: Risk) -> Self {
        self.risk = risk;
        self
    }
    fn command_for(&self, name: &str, captures: &[String]) -> Option<String> {
        let mut command = self.command?.replace("{name}", name);
        for (i, captured) in captures.iter().enumerate() {
            command = command.replace(&format!("{{{i}}}"), captured);
        }
        Some(command)
    }
    /// What the `*` components of `pattern` matched in `relative`, if it is one of this
    /// location's versions.
    fn captures(&self, relative: &str) -> Option<Vec<String>> {
        let wanted: Vec<&str> = self.pattern.split('/').collect();
        let parts: Vec<&str> = relative.split('/').collect();
        if wanted.len() != parts.len() {
            return None;
        }
        let mut captures = vec![];
        for (pattern, part) in wanted.iter().zip(&parts) {
            let captured = glob(pattern, part)?;
            if pattern.contains('*') {
                if part.starts_with('.') || *part == "current" {
                    return None;
                }
                captures.push(captured.to_owned());
            }
        }
        Some(captures)
    }
    fn ecosystem<'a>(&'a self, captures: &'a [String]) -> &'a str {
        self.ecosystem
            .or(captures.first().map(String::as_str))
            .unwrap_or("")
    }
    fn version(&self, captures: &[String]) -> String {
        let skip = usize::from(self.ecosystem.is_none());
        captures[skip.min(captures.len())..].join(" · ")
    }
}

/// Toolchains, SDK components and virtual devices installed by version managers.
pub struct ToolchainsModule {
    /// The home folder the locations are relative to; `None` uses the current user's.
    pub home: Option<String>,
    pub locations: Vec<ToolchainLocation>,
}
impl Default for ToolchainsModule {
    fn default() -> Self {
        const SDK: &str = "Reinstall with Android Studio's SDK Manager if a project needs it.";
        Self {
            home: None,
            locations: vec![
                versions(
                    "Rust",
                    "Rust toolchain",
                    "rustup",
                    ".rustup/toolchains/*",
                    "Projects may pin it in a rust-toolchain file this scan cannot see; reinstall with rustup toolchain install if needed.",
                )
                .in_use(&[InUse::Rustup(".rustup/settings.toml")])
                .command("rustup toolchain uninstall {name}")
                .risk(Risk::Review),
                versions(
                    "Node.js",
                    "Node.js",
                    "nvm",
                    ".nvm/versions/node/*",
                    "Reinstall with nvm install; global packages for this version are lost.",
                )
                .in_use(&[InUse::Nvm(".nvm/alias")])
                .command("nvm uninstall {name}"),
                versions(
                    "Node.js",
                    "Node.js",
                    "fnm",
                    ".local/share/fnm/node-versions/*",
                    "Reinstall with fnm install if a project needs it.",
                )
                .in_use(&[InUse::Link(".local/share/fnm/aliases/default")])
                .command("fnm uninstall {name}"),
                versions(
                    "Node.js",
                    "Node.js",
                    "fnm",
                    "Library/Application Support/fnm/node-versions/*",
                    "Reinstall with fnm install if a project needs it.",
                )
                .in_use(&[InUse::Link("Library/Application Support/fnm/aliases/default")])
                .command("fnm uninstall {name}"),
                versions(
                    "Node.js",
                    "Node.js",
                    "Volta",
                    ".volta/tools/image/node/*",
                    "Volta downloads it again when a project pins it.",
                )
                .in_use(&[InUse::Volta(".volta/tools/user/platform.json")]),
                versions(
                    "Python",
                    "Python",
                    "pyenv",
                    ".pyenv/versions/*",
                    "Reinstall with pyenv install; packages installed into it are lost.",
                )
                .in_use(&[InUse::VersionFile(".pyenv/version")])
                .command("pyenv uninstall {name}"),
                versions(
                    "Python",
                    "Python",
                    "uv",
                    ".local/share/uv/python/*",
                    "uv downloads it again when a project needs it; environments made from it stop working until they are made again.",
                )
                .command("uv python uninstall {name}")
                .risk(Risk::Review),
                versions(
                    "Python",
                    "Conda environment",
                    "Miniconda",
                    "miniconda3/envs/*",
                    "Packages installed into it are lost; make it again from its environment.yml.",
                )
                .command("conda env remove -n {name}")
                .risk(Risk::Review),
                versions(
                    "Python",
                    "Conda environment",
                    "Anaconda",
                    "anaconda3/envs/*",
                    "Packages installed into it are lost; make it again from its environment.yml.",
                )
                .command("conda env remove -n {name}")
                .risk(Risk::Review),
                versions(
                    "Python",
                    "Conda environment",
                    "Miniforge",
                    "miniforge3/envs/*",
                    "Packages installed into it are lost; make it again from its environment.yml.",
                )
                .command("conda env remove -n {name}")
                .risk(Risk::Review),
                versions(
                    "Java",
                    "JDK",
                    "an IDE's JDK download",
                    "Library/Java/JavaVirtualMachines/*",
                    "Download it again in the IDE's Project Structure › SDKs if a project needs it.",
                )
                .apps(owners::JETBRAINS)
                .risk(Risk::Review),
                versions(
                    "Ruby",
                    "Ruby",
                    "rbenv",
                    ".rbenv/versions/*",
                    "Reinstall with rbenv install; gems installed into it are lost.",
                )
                .in_use(&[InUse::VersionFile(".rbenv/version")])
                .command("rbenv uninstall {name}"),
                versions(
                    "Flutter",
                    "Flutter SDK",
                    "FVM",
                    "fvm/versions/*",
                    "Reinstall with fvm install if a project needs it.",
                )
                .in_use(&[InUse::Link("fvm/default")])
                .command("fvm remove {name}"),
                versions(
                    "Swift",
                    "Swift toolchain",
                    "swift.org",
                    "Library/Developer/Toolchains/*.xctoolchain",
                    "Reinstall from swift.org or with swiftly if a project needs it.",
                ),
                tools(
                    "SDKMAN",
                    ".sdkman/candidates/*/*",
                    "Reinstall with sdk install if a project needs it.",
                )
                .in_use(&[InUse::Sibling("current")])
                .command("sdk uninstall {0} {1}"),
                tools(
                    "asdf",
                    ".asdf/installs/*/*",
                    "Reinstall with asdf install if a project needs it.",
                )
                .in_use(&[InUse::ToolVersions(".tool-versions")])
                .command("asdf uninstall {0} {1}"),
                tools(
                    "mise",
                    ".local/share/mise/installs/*/*",
                    "Reinstall with mise install if a project needs it.",
                )
                .in_use(&[
                    InUse::MiseConfig(".config/mise/config.toml"),
                    InUse::ToolVersions(".tool-versions"),
                ])
                .command("mise uninstall {0}@{1}"),
                versions(
                    "Android",
                    "Android system image",
                    "Android SDK",
                    "Library/Android/sdk/system-images/*/*/*",
                    SDK,
                )
                .in_use(&[InUse::AvdImages(".android/avd")])
                .command("sdkmanager --uninstall \"system-images;{0};{1};{2}\"")
                .apps(owners::ANDROID),
                versions(
                    "Android",
                    "Android NDK",
                    "Android SDK",
                    "Library/Android/sdk/ndk/*",
                    SDK,
                )
                .command("sdkmanager --uninstall \"ndk;{name}\"")
                .apps(owners::ANDROID),
                versions(
                    "Android",
                    "Android build-tools",
                    "Android SDK",
                    "Library/Android/sdk/build-tools/*",
                    SDK,
                )
                .command("sdkmanager --uninstall \"build-tools;{name}\"")
                .apps(owners::ANDROID),
                versions(
                    "Android",
                    "Android virtual device",
                    "Android Studio",
                    ".android/avd/*.avd",
                    "Apps and data on the device are lost. Deleting it in Device Manager also removes its .ini entry.",
                )
                .in_use(&[InUse::Locked, InUse::Process(EMULATOR)])
                .command("avdmanager delete avd -n {0}")
                .apps(owners::ANDROID)
                .risk(Risk::Review),
            ],
        }
    }
}

/// Values in a small TOML file as (section, key, quoted strings on the right-hand side).
fn toml_values(text: &str) -> Vec<(String, String, Vec<String>)> {
    let mut section = String::new();
    let mut rows = vec![];
    for line in text.lines() {
        let line = line.split(" #").next().unwrap_or("").trim();
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            section = name.trim().into();
        } else if let Some((key, value)) = line.split_once('=') {
            let strings = value
                .split('"')
                .skip(1)
                .step_by(2)
                .map(str::to_owned)
                .collect();
            rows.push((
                section.clone(),
                key.trim().trim_matches('"').into(),
                strings,
            ));
        }
    }
    rows
}
/// Whether an installed folder name satisfies a version request such as `20`, `v20.1.0`,
/// `3.12` or `stable` (rustup names carry a host suffix).
fn satisfies(name: &str, request: &str) -> bool {
    let request = request.trim();
    let bare = request.strip_prefix('v').unwrap_or(request);
    if bare.is_empty() {
        return false;
    }
    let name = name.strip_prefix('v').unwrap_or(name);
    name == bare || name.starts_with(&format!("{bare}.")) || name.starts_with(&format!("{bare}-"))
}
/// Numeric components for ordering version names, newest last.
fn version_key(name: &str) -> Vec<u64> {
    name.split(|c: char| !c.is_ascii_digit())
        .filter_map(|p| p.parse().ok())
        .collect()
}

impl ToolchainsModule {
    fn home(&self) -> String {
        self.home.clone().unwrap_or_else(policy::home)
    }
    fn locate(&self, path: &str) -> Option<(&ToolchainLocation, Vec<String>)> {
        let home = self.home();
        let relative = policy::home_relative(path, &home)?;
        self.locations
            .iter()
            .find_map(|l| l.captures(relative).map(|c| (l, c)))
    }
    /// Every installed version folder of a location with its captures.
    fn expand(
        &self,
        s: &Services,
        location: &ToolchainLocation,
        warnings: &mut Vec<String>,
    ) -> Vec<(Entry, Vec<String>)> {
        let mut found = vec![(self.home(), vec![])];
        for part in location.pattern.split('/') {
            let mut next = vec![];
            for (path, captures) in found {
                if !part.contains('*') {
                    next.push((format!("{path}/{part}"), captures));
                    continue;
                }
                if !s.is_dir(&path) {
                    continue;
                }
                for e in s.children(&path, warnings) {
                    if !e.directory || e.name().starts_with('.') || e.name() == "current" {
                        continue;
                    }
                    if let Some(captured) = glob(part, e.name()) {
                        let mut captures = captures.clone();
                        captures.push(captured.to_owned());
                        next.push((e.path().to_owned(), captures));
                    }
                }
            }
            found = next;
        }
        found
            .into_iter()
            .filter_map(|(path, captures)| {
                let e = s.entry(&path).ok().filter(|e| e.directory)?;
                Some((e, captures))
            })
            .collect()
    }
    /// Why this version is the default or otherwise in use, when that is cheap to tell.
    pub fn in_use(&self, s: &Services, path: &str) -> Option<String> {
        let (location, captures) = self.locate(path)?;
        let home = self.home();
        let at = |relative: &str| format!("{home}/{relative}");
        let name = Path::new(path).file_name()?.to_str()?;
        let parent = Path::new(path).parent()?.to_str()?;
        let newest = || {
            s.children(parent, &mut vec![])
                .into_iter()
                .filter(|e| e.directory && !e.name().starts_with('.'))
                .max_by_key(|e| version_key(e.name()))
                .is_some_and(|e| e.name() == name)
        };
        let points_here = |link: &str| {
            let resolved = s.fs.resolve(link)?;
            let target = s.fs.resolve(path).unwrap_or_else(|| path.into());
            policy::contains(&resolved, &target).then(|| "This is the default version.".into())
        };
        for check in location.in_use {
            let found: Option<String> = match *check {
                InUse::Rustup(file) => s.read_text(&at(file)).and_then(|text| {
                    toml_values(&text)
                        .into_iter()
                        .find_map(|(section, key, values)| {
                            let used = (section.is_empty() && key == "default_toolchain")
                                || section == "overrides";
                            (used && values.iter().any(|v| satisfies(name, v))).then(|| {
                                if section == "overrides" {
                                    format!("A directory override uses this toolchain: {key}")
                                } else {
                                    "This is rustup's default toolchain.".into()
                                }
                            })
                        })
                }),
                InUse::Nvm(folder) => {
                    let mut request = s.read_text(&at(&format!("{folder}/default")));
                    // Aliases such as `lts/iron` are files naming another alias or a version.
                    for _ in 0..4 {
                        let alias = request.as_deref().map(str::trim).unwrap_or("");
                        if alias.is_empty() || alias.contains("..") {
                            break;
                        }
                        match s.read_text(&at(&format!("{folder}/{alias}"))) {
                            Some(next) => request = Some(next),
                            None => break,
                        }
                    }
                    request.and_then(|r| {
                        let r = r.trim();
                        let hit = if r == "node" || r == "stable" {
                            newest()
                        } else {
                            satisfies(name, r)
                        };
                        hit.then(|| "This is nvm's default version.".into())
                    })
                }
                InUse::VersionFile(file) => s.read_text(&at(file)).and_then(|text| {
                    text.split_whitespace()
                        .any(|v| v == name)
                        .then(|| format!("This is the global version in ~/{file}."))
                }),
                InUse::ToolVersions(file) => s.read_text(&at(file)).and_then(|text| {
                    text.lines()
                        .map(|l| l.split('#').next().unwrap_or(""))
                        .any(|line| {
                            let mut words = line.split_whitespace();
                            words.next() == captures.first().map(String::as_str)
                                && words.any(|v| {
                                    if v == "latest" {
                                        newest()
                                    } else {
                                        satisfies(name, v)
                                    }
                                })
                        })
                        .then(|| format!("This is the global version in ~/{file}."))
                }),
                InUse::MiseConfig(file) => s.read_text(&at(file)).and_then(|text| {
                    toml_values(&text)
                        .into_iter()
                        .any(|(section, key, values)| {
                            section == "tools"
                                && Some(&key) == captures.first()
                                && values.iter().any(|v| {
                                    if v == "latest" {
                                        newest()
                                    } else {
                                        satisfies(name, v)
                                    }
                                })
                        })
                        .then(|| format!("This is a global version in ~/{file}."))
                }),
                InUse::Link(link) => points_here(&at(link)),
                InUse::Sibling(link) => points_here(&format!("{parent}/{link}")),
                InUse::Volta(file) => s.read_text(&at(file)).and_then(|text| {
                    let json: serde_json::Value = serde_json::from_str(&text).ok()?;
                    let runtime = json.pointer("/node/runtime")?.as_str()?;
                    (runtime == name).then(|| "This is Volta's default Node version.".into())
                }),
                InUse::AvdImages(folder) => s
                    .children(&at(folder), &mut vec![])
                    .into_iter()
                    .filter(|e| e.directory && e.name().ends_with(".avd"))
                    .find(|avd| {
                        s.read_text(&format!("{}/config.ini", avd.path()))
                            .is_some_and(|text| {
                                text.lines().any(|line| {
                                    line.split_once('=').is_some_and(|(key, value)| {
                                        let value = value.trim().trim_end_matches('/');
                                        key.trim().starts_with("image.sysdir")
                                            && !value.is_empty()
                                            && path.ends_with(&format!("/{value}"))
                                    })
                                })
                            })
                    })
                    .map(|avd| {
                        format!(
                            "Used by Android virtual device {}.",
                            avd.name().trim_end_matches(".avd")
                        )
                    }),
                // A crashed emulator or Android Studio leaves its lock files behind, so they
                // mean "in use" only while an emulator is actually running.
                InUse::Locked => s
                    .children(path, &mut vec![])
                    .iter()
                    .any(|e| e.name().ends_with(".lock"))
                    .then(|| running(s, EMULATOR))
                    .flatten()
                    .map(|p| {
                        format!(
                            "The emulator is running this device ({} PID {}).",
                            p.name, p.identity.pid
                        )
                    }),
                InUse::Process(prefixes) => running(s, prefixes)
                    .map(|p| format!("Close {} (PID {}) first.", p.name, p.identity.pid)),
            };
            if found.is_some() {
                return found;
            }
        }
        None
    }
}

/// Process-name prefixes of the Android emulator.
const EMULATOR: &[&str] = &["qemu-system-", "emulator"];
/// The first running process whose name starts with one of `prefixes`.
fn running(s: &Services, prefixes: &[&str]) -> Option<Snapshot> {
    s.processes
        .pids()
        .into_iter()
        .filter_map(|pid| s.processes.inspect(pid))
        .find(|p| {
            let name = p.name.to_lowercase();
            prefixes.iter().any(|prefix| name.starts_with(prefix))
        })
}

impl ScanModule for ToolchainsModule {
    fn descriptor(&self) -> ModuleDescriptor {
        descriptor(
            "toolchains",
            "Toolchains & SDKs",
            "Developer",
            "square.stack.3d.up",
            "Installed language versions, SDK components and virtual devices.",
            false,
        )
    }
    fn action_roots(&self) -> Vec<String> {
        let home = self.home();
        self.locations
            .iter()
            .map(|l| {
                let fixed: Vec<&str> = l
                    .pattern
                    .split('/')
                    .take_while(|p| !p.contains('*'))
                    .collect();
                format!("{home}/{}", fixed.join("/"))
            })
            .collect()
    }
    fn scan(
        &self,
        s: &Services,
        c: &ScanContext,
        k: &ScanControl,
        sink: &mut dyn Sink,
    ) -> Result<()> {
        let mut warnings = vec![];
        let mut candidates = vec![];
        for location in &self.locations {
            k.check()?;
            for (e, captures) in self.expand(s, location, &mut warnings) {
                if !c.allows(e.path()) {
                    continue;
                }
                let ecosystem = location.ecosystem(&captures).to_owned();
                let version = location.version(&captures);
                let title = format!(
                    "{} {version}",
                    if location.title.is_empty() {
                        &ecosystem
                    } else {
                        location.title
                    }
                );
                let mut reason = format!(
                    "Installed with {}. {}",
                    location.manager, location.reinstall
                );
                let mut details = vec![
                    detail("Ecosystem", ecosystem),
                    detail("Version", version.clone()),
                    detail("Manager", location.manager),
                ];
                if let Some(command) = location.command_for(e.name(), &captures) {
                    reason += &format!(" Official command: `{command}`.");
                    details.push(detail("Official command", command));
                }
                let blocked = self.in_use(s, e.path());
                candidates.push(
                    Candidate::new(e, &reason, vec![ActionKind::Trash], location.risk)
                        .title(title)
                        .details(details)
                        .blocked(blocked.as_deref()),
                );
            }
        }
        flush(sink, &mut warnings);
        add_files(s, sink, "toolchains", candidates, k)
    }
    fn preflight(&self, s: &Services, f: &Finding, _: ActionKind, _: &ScanControl) -> Result<()> {
        let path = f.resource.path().ok_or("Missing path")?;
        let (location, _) = self
            .locate(path)
            .ok_or("This is no longer a known toolchain location.")?;
        owners_closed(s, location.apps)?;
        if let Some(reason) = self.in_use(s, path) {
            return Err(reason);
        }
        let running: Vec<_> = s
            .processes
            .pids()
            .into_iter()
            .filter_map(|pid| s.processes.inspect(pid))
            .filter(|p| policy::contains(&p.identity.executable, path))
            .map(|p| format!("{} (PID {})", p.name, p.identity.pid))
            .collect();
        if running.is_empty() {
            Ok(())
        } else {
            Err(format!(
                "Quit programs running from this version first: {}",
                running[..running.len().min(3)].join(", ")
            ))
        }
    }
}
