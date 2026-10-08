//! Git projects in the chosen folders: repositories whose objects Git has not packed, and
//! projects idle long enough to hibernate.
//!
//! `git gc` packs loose objects and removes unreachable ones Git no longer keeps; reflogs and
//! the two-week grace period stay, so nothing a user can still reach is lost. Hibernating a
//! project compresses its whole folder, history included, into a `.zip` beside it, checks
//! the archive, and moves the folder to the Trash; opening the archive in Finder brings the
//! project back.
use super::{
    add_files, descriptor, flush, project_activity,
    project_folders::{app_home_folder, in_app_home_folder},
    Candidate, LastUsed, ScanModule,
};
use crate::{git::Git, model::*, orphans, policy, ports::*, services::Services};
use std::{collections::BTreeSet, path::Path, time::Duration};

const DITTO: &str = "/usr/bin/ditto";
const UNZIP: &str = "/usr/bin/unzip";
const DAY: f64 = 86_400.0;

/// Loose and garbage object counts from `git count-objects -v`, in bytes.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Objects {
    pub loose_count: u64,
    pub loose: u64,
    pub packed: u64,
    pub garbage: u64,
}
/// `git count-objects -v` output: `count`, `size`, `size-pack` and `size-garbage` in KiB.
pub fn parse_objects(output: &str) -> Objects {
    let mut objects = Objects::default();
    for line in output.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value: u64 = value.trim().parse().unwrap_or(0);
        match key.trim() {
            "count" => objects.loose_count = value,
            "size" => objects.loose = value * 1024,
            "size-pack" => objects.packed = value * 1024,
            "size-garbage" => objects.garbage = value * 1024,
            _ => {}
        }
    }
    objects
}

pub struct ProjectsModule {
    /// The home folder whose app and tool folders (`~/Library`, `~/.vim` and the like) are
    /// never treated as projects; the current user's when `None`.
    pub home: Option<String>,
    /// Days without a commit or a change to the project's own files before it is offered for
    /// hibernation.
    pub idle_days: u64,
    /// Loose and garbage objects, in bytes, before `git gc` is offered.
    pub gc_minimum: u64,
    /// The current time in Unix seconds; the clock when `None`. Tests fix it.
    pub now: Option<f64>,
}
impl Default for ProjectsModule {
    fn default() -> Self {
        Self {
            home: None,
            idle_days: 180,
            gc_minimum: 100 * 1024 * 1024,
            now: None,
        }
    }
}
fn name(path: &str) -> &str {
    Path::new(path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(path)
}
/// A main Git checkout: a folder with a `.git` folder (not a linked worktree's `.git` file).
fn is_project(s: &Services, path: &str) -> bool {
    s.is_dir(&format!("{path}/.git"))
}
/// The archive a project hibernates into: `<parent>/<name>.zip`.
pub fn archive_path(project: &str) -> String {
    format!("{project}.zip")
}
impl ProjectsModule {
    fn home(&self) -> String {
        self.home.clone().unwrap_or_else(policy::home)
    }
    /// Inside an app's or tool's own folder: a plugin or a tap, not a project.
    fn app_data(&self, path: &str) -> bool {
        let home = self.home();
        app_home_folder(path, &home) || in_app_home_folder(path, &home)
    }
    fn now(&self) -> f64 {
        self.now.unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0.0, |d| d.as_secs_f64())
        })
    }
    fn objects(git: &Git, repository: &str, k: &ScanControl) -> Result<Objects> {
        let out = git.run(repository, &["count-objects", "-v"], k)?;
        if out.status != 0 {
            return Err(out.error.trim().chars().take(240).collect());
        }
        Ok(parse_objects(&out.text()))
    }
    /// The `git gc` finding for a repository with many unpacked objects.
    fn gc(&self, repository: &str, objects: &Objects) -> Option<Finding> {
        let reclaim = objects.loose + objects.garbage;
        if reclaim < self.gc_minimum {
            return None;
        }
        let mut f = Finding::new(
            "projects",
            &format!("git-gc:{repository}"),
            &format!("Unpacked Git objects · {}", name(repository)),
            Resource::Command {
                tool: "git".into(),
                task: "gc".into(),
            },
            "Objects Git stored one file each, and temporary files it left behind. git gc compresses them into packs and removes unreachable objects older than two weeks. Branches, stashes and reflogs stay. Up to this much is freed; packing keeps the compressed objects.",
        );
        f.subtitle = repository.into();
        f.bytes = Some(reclaim);
        f.allocated_bytes = Some(reclaim);
        f.risk = Risk::Review;
        f.details = vec![
            detail("Ecosystem", "Git"),
            detail("Data path", repository),
            detail("Loose objects", objects.loose_count.to_string()),
            detail("Packed", format!("{} bytes", objects.packed)),
            detail("Command", "git gc"),
        ];
        f.actions = vec![ActionKind::RunCommand];
        Some(f)
    }
    /// Why a project cannot be archived right now, if it cannot.
    fn archive_blocked(s: &Services, project: &str) -> Option<String> {
        let zip = archive_path(project);
        if s.exists(&zip) {
            return Some(format!(
                "{zip} already exists. Move or rename it to hibernate this project."
            ));
        }
        if s.exists(&format!("{project}/.git/index.lock")) {
            return Some("Git is working in this project (index.lock exists).".into());
        }
        None
    }
}

impl ScanModule for ProjectsModule {
    fn descriptor(&self) -> ModuleDescriptor {
        descriptor(
            "projects",
            "Projects",
            "Developer",
            "folder.badge.gearshape",
            "Git repositories with unpacked objects, and idle projects to hibernate into an archive.",
            true,
        )
    }
    fn scan(
        &self,
        s: &Services,
        c: &ScanContext,
        k: &ScanControl,
        sink: &mut dyn Sink,
    ) -> Result<()> {
        let mut projects = BTreeSet::new();
        for root in s.roots(&c.roots) {
            if !c.excludes(&root)
                && !policy::system_excluded(&root)
                && !policy::protected(&root)
                && !self.app_data(&root)
                && is_project(s, &root)
            {
                projects.insert(root);
            }
        }
        let mut warnings = vec![];
        let walked = s.walk(c, k, &mut warnings, &mut |e| {
            // Build output and installed packages are no projects of the user's.
            if !e.directory
                || super::project_folders::package_tree(e.name())
                || super::project_folders::rules().rebuildable(s, e.path())
                || self.app_data(e.path())
            {
                return Ok(false);
            }
            if is_project(s, e.path()) {
                projects.insert(e.path().into());
                // A project is one item; repositories inside it go with it.
                return Ok(false);
            }
            Ok(true)
        });
        flush(sink, &mut warnings);
        walked?;
        let git = Git(s.commands.as_ref());
        let now = self.now();
        let mut idle = vec![];
        for project in projects {
            k.check()?;
            sink.progress(format!("Inspecting {project}"));
            // A repository whose settings run programs is left alone, as for worktrees.
            if !matches!(git.risky_config(&project, k), Ok(None)) {
                continue;
            }
            match Self::objects(&git, &project, k) {
                Ok(objects) => {
                    if let Some(f) = self.gc(&project, &objects) {
                        sink.finding(f);
                    }
                }
                Err(e) => sink.warning(format!("Cannot count Git objects in {project}: {e}")),
            }
            let commit = git.last_commit(&project, k);
            let changed = project_activity(s, &project);
            let Some(active) = commit.into_iter().chain(changed).reduce(f64::max) else {
                continue;
            };
            if now - active < self.idle_days as f64 * DAY {
                continue;
            }
            let Ok(entry) = s.entry(&project) else {
                continue;
            };
            let days = ((now - active) / DAY) as u64;
            let zip = archive_path(&project);
            let mut details = vec![
                detail("Ecosystem", "Git"),
                detail("Idle", format!("{days} days")),
                detail("Archive", zip.clone()),
            ];
            if let Some(commit) = commit {
                details.push(detail(
                    "Last commit age",
                    format!("{} days", ((now - commit) / DAY) as u64),
                ));
            }
            let blocked = Self::archive_blocked(s, &project);
            idle.push(
                Candidate::new(
                    entry,
                    &format!("No commit or change for {days} days. Hibernating compresses the whole folder, Git history and uncommitted work included, into {zip}, checks the archive, then moves the folder to the Trash. Open the archive in Finder to bring the project back. Remove its dependencies and build output first for a smaller archive."),
                    vec![ActionKind::Archive],
                    Risk::Review,
                )
                .title(name(&project).to_owned())
                .details(details)
                .last_used(LastUsed::At(active))
                .blocked(blocked.as_deref()),
            );
        }
        add_files(s, sink, "projects", idle, k)
    }
    fn in_use(&self, s: &Services, f: &Finding, _: ActionKind) -> Option<String> {
        let folder = f.value("Data path").or(f.resource.path())?;
        let active = orphans::active_tools(s, Some(folder));
        (!active.is_empty()).then(|| {
            format!(
                "Developer tools are running in this project:{}",
                orphans::list(&active)
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
        let folder = f
            .value("Data path")
            .or(f.resource.path())
            .ok_or("Missing path")?;
        if !is_project(s, folder) {
            return Err("This is no longer a Git project.".into());
        }
        if self.app_data(folder) {
            return Err("Inside an app's or tool's own folder; it belongs to that app.".into());
        }
        if kind == ActionKind::Archive {
            if let Some(reason) = Self::archive_blocked(s, folder) {
                return Err(reason);
            }
        }
        if s.exists(&format!("{folder}/.git/index.lock"))
            || s.exists(&format!("{folder}/.git/gc.pid"))
        {
            return Err("Git is working in this repository; try again when it finishes.".into());
        }
        Ok(())
    }
    fn run(&self, s: &Services, f: &Finding, c: &ScanContext, k: &ScanControl) -> Result<String> {
        let (Resource::Command { tool, task }, Some(repository)) =
            (&f.resource, f.value("Data path"))
        else {
            return Err("This cleanup is not one the app runs.".into());
        };
        if tool != "git" || task != "gc" {
            return Err("This cleanup is not one the app runs.".into());
        }
        if !c.allows(repository) || policy::protected(repository) {
            return Err("This repository is outside the approved scan scope.".into());
        }
        let git = Git(s.commands.as_ref());
        if !matches!(git.risky_config(repository, k), Ok(None)) {
            return Err(
                "This repository's settings run programs; run git gc yourself after checking them."
                    .into(),
            );
        }
        let before = Self::objects(&git, repository, k)?;
        let out = git.run_for(repository, &["gc", "--quiet"], Duration::from_secs(3600), k)?;
        if out.status != 0 {
            return Err(out.error.trim().chars().take(400).collect());
        }
        let after = Self::objects(&git, repository, k).unwrap_or_default();
        let before_total = before.loose + before.garbage + before.packed;
        let after_total = after.loose + after.garbage + after.packed;
        Ok(format!(
            "Git packed its objects; the repository data is {} smaller.",
            bytes_text(before_total.saturating_sub(after_total))
        ))
    }
    fn archive(&self, s: &Services, f: &Finding, k: &ScanControl) -> Result<String> {
        let project = f.resource.path().ok_or("Missing path")?;
        let zip = archive_path(project);
        let partial = format!(
            "{}/.{}.hibernating.zip",
            Path::new(project)
                .parent()
                .and_then(|p| p.to_str())
                .ok_or("Invalid project path.")?,
            name(project)
        );
        if s.exists(&zip) || s.exists(&partial) {
            return Err(format!("{zip} or a partial archive already exists."));
        }
        let cleanup = |message: String| -> Result<String> {
            if s.exists(&partial) {
                let _ = s.fs.remove(&partial);
            }
            Err(message)
        };
        let args: Vec<String> = [
            "-c",
            "-k",
            "--sequesterRsrc",
            "--keepParent",
            project,
            &partial,
        ]
        .iter()
        .map(|a| (*a).to_owned())
        .collect();
        let out = match s
            .commands
            .run(DITTO, &args, Duration::from_secs(4 * 3600), k)
        {
            Ok(out) => out,
            Err(e) => return cleanup(e),
        };
        if out.status != 0 {
            return cleanup(format!(
                "The archive could not be made: {}",
                out.error.trim().chars().take(300).collect::<String>()
            ));
        }
        let test = match s.commands.run(
            UNZIP,
            &["-tqq".into(), partial.clone()],
            Duration::from_secs(4 * 3600),
            k,
        ) {
            Ok(out) => out,
            Err(e) => return cleanup(e),
        };
        if test.status != 0 {
            return cleanup(format!(
                "The archive failed its check, so the project stays: {}",
                test.error.trim().chars().take(300).collect::<String>()
            ));
        }
        if s.exists(&zip) {
            return cleanup(format!(
                "{zip} appeared while archiving; the project stays."
            ));
        }
        if let Err(e) = s.fs.rename(&partial, &zip) {
            return cleanup(e);
        }
        Ok(zip)
    }
}

/// A byte count for messages: `1.2 GB`, `340 MB`, `12 KB`.
fn bytes_text(bytes: u64) -> String {
    let units = ["bytes", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1000.0 && unit < units.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }
    match unit {
        0 => format!("{bytes} bytes"),
        _ => format!("{value:.1} {}", units[unit]),
    }
}
