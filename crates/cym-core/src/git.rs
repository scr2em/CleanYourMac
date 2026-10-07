//! Git worktree inspection and removal through the `CommandRunner` port.
use crate::{model::*, ports::*, services::Services};
use std::{path::Path, time::Duration};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Worktree {
    pub path: String,
    pub head: String,
    pub branch: String,
    pub locked: bool,
    pub prunable: bool,
    pub bare: bool,
    pub main: bool,
}
#[derive(Clone, Debug)]
pub struct Safety {
    pub eligible: bool,
    pub reason: String,
    pub status: String,
    pub upstream: String,
}
impl Safety {
    pub fn blocked(reason: &str, status: &str, upstream: &str) -> Self {
        Self {
            eligible: false,
            reason: reason.into(),
            status: status.into(),
            upstream: upstream.into(),
        }
    }
}

pub struct Git<'a>(pub &'a dyn CommandRunner);
impl Git<'_> {
    pub fn run(&self, repository: &str, args: &[&str], control: &ScanControl) -> Result<Output> {
        let mut arguments: Vec<String> = [
            "-c",
            "core.fsmonitor=false",
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "core.untrackedCache=false",
            "-C",
            repository,
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        arguments.extend(args.iter().map(|s| s.to_string()));
        self.0
            .run("/usr/bin/git", &arguments, Duration::from_secs(20), control)
    }
    pub fn common_directory(&self, repository: &str, control: &ScanControl) -> Result<String> {
        let out = self.run(
            repository,
            &["rev-parse", "--path-format=absolute", "--git-common-dir"],
            control,
        )?;
        if out.status != 0 {
            return Err(format!("Cannot inspect repository: {}", out.error.trim()));
        }
        let text = String::from_utf8(out.data).map_err(|_| "Git path is not UTF-8")?;
        Ok(text.strip_suffix('\n').unwrap_or(&text).into())
    }
    pub fn worktrees(&self, repository: &str, control: &ScanControl) -> Result<Vec<Worktree>> {
        let out = self.run(
            repository,
            &["worktree", "list", "--porcelain", "-z"],
            control,
        )?;
        if out.status != 0 {
            return Err(out.error);
        }
        parse(&out.data)
    }
    /// Whether a linked worktree can be removed without losing local-only work.
    pub fn safety(&self, record: &Worktree, control: &ScanControl) -> Result<Safety> {
        if record.main || record.bare {
            return Ok(Safety::blocked(
                "Main and bare repositories are protected.",
                "Protected",
                "—",
            ));
        }
        if record.locked {
            return Ok(Safety::blocked("This worktree is locked.", "Locked", "—"));
        }
        if record.prunable {
            return Ok(Safety::blocked(
                "Git reports stale registration metadata; inspect manually.",
                "Orphaned",
                "Unknown",
            ));
        }
        if record.branch.is_empty() {
            return Ok(Safety::blocked(
                "Detached HEAD needs manual inspection.",
                "Detached",
                "Unknown",
            ));
        }
        if Path::new(&record.path).join(".gitmodules").exists() {
            return Ok(Safety::blocked(
                "Worktrees with submodules need manual inspection.",
                "Submodules",
                "Unknown",
            ));
        }
        let state = self.run(
            &record.path,
            &[
                "status",
                "--porcelain=v1",
                "-z",
                "--untracked-files=all",
                "--ignored",
            ],
            control,
        )?;
        if state.status != 0 {
            return Err("Git status is unavailable.".into());
        }
        if !state.data.is_empty() {
            return Ok(Safety::blocked(
                "Contains changes, untracked or ignored files. Inspect before removal.",
                "Local files",
                "Not checked",
            ));
        }
        let upstream = self.run(
            &record.path,
            &[
                "rev-parse",
                "--abbrev-ref",
                "--symbolic-full-name",
                "@{upstream}",
            ],
            control,
        )?;
        if upstream.status != 0 {
            return Ok(Safety::blocked(
                "No verified upstream. Local commits may exist only here.",
                "Clean",
                "None",
            ));
        }
        let upstream = upstream.text().trim().to_owned();
        let ahead = self.run(
            &record.path,
            &["rev-list", "--count", "@{upstream}..HEAD"],
            control,
        )?;
        if ahead.status != 0 || ahead.text().trim().parse::<u64>() != Ok(0) {
            return Ok(Safety::blocked(
                "Has unpushed or unknown local commits.",
                "Clean",
                &upstream,
            ));
        }
        Ok(Safety {
            eligible: true,
            reason: "Clean linked worktree with no ignored files or commits ahead of its local upstream reference. Review before removal.".into(),
            status: "Clean".into(),
            upstream,
        })
    }
    /// Whether Git tracks anything at or below `path`.
    pub fn tracks(&self, directory: &str, path: &str, control: &ScanControl) -> Result<bool> {
        let out = self.run(directory, &["ls-files", "-z", "--", path], control)?;
        if out.status != 0 {
            return Err("Git inspection failed.".into());
        }
        Ok(!out.data.is_empty())
    }
}

/// Removes a linked worktree after fresh identity, registration, HEAD and safety checks.
pub fn remove(
    services: &Services,
    file: &FileIdentity,
    repository: &str,
    head: &str,
    context: &ScanContext,
    control: &ScanControl,
) -> Result<()> {
    services.validate(file, context, &[], control)?;
    let git = Git(services.commands.as_ref());
    let current = git
        .worktrees(repository, control)?
        .into_iter()
        .find(|w| w.path == file.path && w.head == head)
        .ok_or("Worktree registration or HEAD changed.")?;
    let state = git.safety(&current, control)?;
    if !state.eligible {
        return Err(state.reason);
    }
    let out = git.run(
        repository,
        &["worktree", "remove", "--", &file.path],
        control,
    )?;
    if out.status != 0 {
        return Err(out.error);
    }
    Ok(())
}

/// Parses `git worktree list --porcelain -z`, which keeps unusual paths intact.
pub fn parse(data: &[u8]) -> Result<Vec<Worktree>> {
    let text = std::str::from_utf8(data).map_err(|_| "Worktree paths are not UTF-8")?;
    let mut rows = vec![];
    let mut current = Worktree::default();
    fn flush(rows: &mut Vec<Worktree>, current: &mut Worktree) {
        if !current.path.is_empty() {
            current.main = rows.is_empty();
            rows.push(std::mem::take(current));
        }
    }
    for token in text.split('\0') {
        if token.is_empty() {
            flush(&mut rows, &mut current);
        } else if let Some(path) = token.strip_prefix("worktree ") {
            flush(&mut rows, &mut current);
            current.path = path.into();
        } else if let Some(head) = token.strip_prefix("HEAD ") {
            current.head = head.into();
        } else if let Some(branch) = token.strip_prefix("branch ") {
            current.branch = branch.strip_prefix("refs/heads/").unwrap_or(branch).into();
        } else if token == "bare" {
            current.bare = true;
        } else if token.starts_with("locked") {
            current.locked = true;
        } else if token.starts_with("prunable") {
            current.prunable = true;
        }
    }
    flush(&mut rows, &mut current);
    Ok(rows)
}
