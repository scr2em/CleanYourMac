//! Git worktree inspection and removal through the `CommandRunner` port.
use crate::{model::*, ports::*, services::Services};
use std::time::Duration;

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
    /// Ignored files and folders that removal deletes, as Git lists them.
    pub ignored: Vec<String>,
}
/// What `Git::inspect` found in a working tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Inspection {
    /// Nothing Git cannot give back. Ignored files go with the folder; a detached HEAD's
    /// commit is kept on `kept_on`.
    Clean {
        ignored: Vec<String>,
        kept_on: Option<String>,
    },
    /// Why the folder must stay, and a short status for it.
    Keep {
        reason: String,
        status: &'static str,
    },
}
impl Safety {
    pub fn blocked(reason: &str, status: &str, upstream: &str) -> Self {
        Self {
            eligible: false,
            reason: reason.into(),
            status: status.into(),
            upstream: upstream.into(),
            ignored: vec![],
        }
    }
}
fn plural(count: usize) -> &'static str {
    if count == 1 {
        ""
    } else {
        "s"
    }
}
/// The first `limit` names, then how many more.
pub fn sample(names: &[String], limit: usize) -> String {
    let shown = names
        .iter()
        .take(limit)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    match names.len().saturating_sub(limit) {
        0 => shown,
        more => format!("{shown} and {more} more"),
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
            "-c",
            "log.showSignature=false",
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
    /// A setting in the repository's own configuration that makes `status`, `log` or
    /// `worktree remove` run a program (a clean filter, a text converter, a signature
    /// program, an included file). Such a repository came from someone else's archive or
    /// a shared drive, so it is left for manual inspection rather than run.
    pub fn risky_config(&self, repository: &str, control: &ScanControl) -> Result<Option<String>> {
        let out = self.run(
            repository,
            &["config", "--list", "--show-scope", "-z"],
            control,
        )?;
        if out.status != 0 {
            // Unreadable settings could hold anything; do not run more Git here.
            return Ok(Some("settings Git could not read".into()));
        }
        if let Some(key) = risky_key(&out.data) {
            return Ok(Some(key));
        }
        // A nested repository (a gitlink in the index) has settings of its own, which
        // `status` would load; it is left to inspect too. Listing the index runs nothing.
        let index = self.run(repository, &["ls-files", "--stage", "-z"], control)?;
        if index.status != 0 {
            return Ok(Some("an index Git could not read".into()));
        }
        Ok(gitlink(&index.data).map(|path| format!("nested repository {path}")))
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
    /// Whether removing a linked worktree loses anything Git cannot give back. Main, bare,
    /// locked and stale registrations stay; the folder itself is checked by `inspect`.
    pub fn safety(&self, s: &Services, record: &Worktree, control: &ScanControl) -> Result<Safety> {
        if record.main || record.bare {
            return Ok(Safety::blocked(
                "Main and bare repositories are protected.",
                "Protected",
                "—",
            ));
        }
        if record.locked {
            return Ok(Safety::blocked(
                "This worktree is locked. Unlock it with `git worktree unlock` to remove it.",
                "Locked",
                "—",
            ));
        }
        if record.prunable {
            return Ok(Safety::blocked(
                "Git reports stale registration metadata; inspect manually.",
                "Orphaned",
                "Unknown",
            ));
        }
        let (ignored, kept_on) =
            match self.inspect(s, &record.path, Some(&record.branch), control)? {
                Inspection::Keep { reason, status } => {
                    let upstream = match status {
                        "Detached" => "None",
                        "Local changes" => "Not checked",
                        _ => "Unknown",
                    };
                    return Ok(Safety::blocked(&reason, status, upstream));
                }
                Inspection::Clean { ignored, kept_on } => (ignored, kept_on),
            };
        let kept = match kept_on {
            Some(reference) => format!("its commit is kept on {reference}"),
            None => format!(
                "branch {} and its commits stay in the repository",
                record.branch
            ),
        };
        let upstream = self.upstream(record, control);
        let mut reason =
            format!("No uncommitted or untracked work. Removing deletes the folder; {kept}.");
        if !ignored.is_empty() {
            reason += &format!(" Ignored files are deleted too: {}.", sample(&ignored, 4));
        }
        Ok(Safety {
            eligible: true,
            reason,
            status: "Clean".into(),
            upstream,
            ignored,
        })
    }
    /// What removing the working tree at `path` would lose that Git cannot give back. Git
    /// Worktrees and AI Tools both ask here, so a worktree an agent made is held to the same
    /// rules as any other.
    ///
    /// Deleting the folder keeps the branch, its commits and the repository's stashes. What
    /// it cannot keep is uncommitted work, files that were never added, work in submodules or
    /// in a repository kept inside an ignored folder, and commits reachable only from a
    /// detached HEAD. Settings that make Git run a program stop the check before `status`.
    /// `branch` is the checked-out branch when the caller knows it (empty when detached);
    /// `None` asks Git.
    pub fn inspect(
        &self,
        s: &Services,
        path: &str,
        branch: Option<&str>,
        control: &ScanControl,
    ) -> Result<Inspection> {
        let keep = |reason: String, status| Ok(Inspection::Keep { reason, status });
        if s.exists(&format!("{path}/.gitmodules")) {
            return keep(
                "Worktrees with submodules need manual inspection.".into(),
                "Submodules",
            );
        }
        match self.risky_config(path, control)? {
            None => {}
            // A nested repository's work is invisible to the status below.
            Some(key) if key.starts_with("nested repository") => {
                return keep(
                    format!("This worktree contains a {key}, whose work is not checked here. Inspect it manually."),
                    "Inspect",
                )
            }
            Some(key) => {
                return keep(
                    format!("This repository's own Git settings run a program ({key}). Inspect it manually."),
                    "Inspect",
                )
            }
        }
        // Untracked and ignored folders are reported once, not file by file.
        let state = self.run(
            path,
            &[
                "status",
                "--porcelain=v1",
                "-z",
                "--untracked-files=normal",
                "--ignored=traditional",
                "--ignore-submodules=all",
            ],
            control,
        )?;
        if state.status != 0 {
            return Err("Git status is unavailable.".into());
        }
        let (mut changed, mut untracked, mut ignored) = (0, 0, vec![]);
        let text = String::from_utf8_lossy(&state.data);
        let mut entries = text.split('\0');
        while let Some(entry) = entries.next() {
            if entry.len() < 4 || !entry.is_char_boundary(3) {
                continue;
            }
            match &entry[..2] {
                "!!" => ignored.push(entry[3..].to_owned()),
                "??" => untracked += 1,
                code => {
                    changed += 1;
                    // A rename or copy is followed by its original path.
                    if code.contains('R') || code.contains('C') {
                        entries.next();
                    }
                }
            }
        }
        if changed > 0 || untracked > 0 {
            let mut parts = vec![];
            if changed > 0 {
                parts.push(format!("{changed} uncommitted change{}", plural(changed)));
            }
            if untracked > 0 {
                parts.push(format!("{untracked} untracked item{}", plural(untracked)));
            }
            return keep(
                format!(
                    "Has {}. Commit, stash or delete them first; removal would lose them.",
                    parts.join(" and ")
                ),
                "Local changes",
            );
        }
        // A repository kept in an ignored folder is invisible to `status`; one at the top of
        // an ignored folder, or a folder below it, blocks.
        for folder in ignored.iter().filter(|e| e.ends_with('/')) {
            let folder = format!("{path}/{}", folder.trim_end_matches('/'));
            let mut warnings = vec![];
            let below = s.children(&folder, &mut warnings);
            if !warnings.is_empty() || below.len() > 4_096 {
                return Err("Git could not inspect an ignored folder.".into());
            }
            let nested = std::iter::once(folder.clone())
                .chain(
                    below
                        .iter()
                        .filter(|e| e.directory)
                        .map(|e| e.path().to_owned()),
                )
                .find(|d| crate::modules::checkout(s, d).is_some());
            if let Some(nested) = nested {
                let relative = nested.strip_prefix(&format!("{path}/")).unwrap_or(&nested);
                return keep(
                    format!("This worktree keeps another repository in an ignored folder ({relative}). Inspect it manually."),
                    "Inspect",
                );
            }
        }
        let ignored: Vec<String> = ignored
            .into_iter()
            .map(|e| e.trim_end_matches('/').to_owned())
            .collect();
        let detached = match branch {
            Some(branch) => branch.is_empty(),
            None => {
                let head = self.run(path, &["symbolic-ref", "-q", "HEAD"], control)?;
                match head.status {
                    0 => false,
                    1 => true,
                    _ => return Err("Git could not read HEAD.".into()),
                }
            }
        };
        if !detached {
            return Ok(Inspection::Clean {
                ignored,
                kept_on: None,
            });
        }
        // A detached HEAD's commits survive only if a branch or tag also reaches them.
        let refs = self.run(
            path,
            &[
                "for-each-ref",
                "--count=1",
                "--contains=HEAD",
                "--format=%(refname:short)",
                "refs/heads",
                "refs/remotes",
                "refs/tags",
            ],
            control,
        )?;
        let reference = refs.text().trim().to_owned();
        if refs.status != 0 || reference.is_empty() {
            return keep(
                "Detached HEAD has commits no branch or tag contains. Create a branch to keep them before removing.".into(),
                "Detached",
            );
        }
        Ok(Inspection::Clean {
            ignored,
            kept_on: Some(reference),
        })
    }
    /// The branch's upstream and how far ahead it is, for information only: removing a
    /// worktree keeps the branch, so unpushed commits are not lost.
    fn upstream(&self, record: &Worktree, control: &ScanControl) -> String {
        if record.branch.is_empty() {
            return "—".into();
        }
        let name = self.run(
            &record.path,
            &[
                "rev-parse",
                "--abbrev-ref",
                "--symbolic-full-name",
                "@{upstream}",
            ],
            control,
        );
        let Some(name) = name.ok().filter(|o| o.status == 0) else {
            return "None".into();
        };
        let name = name.text().trim().to_owned();
        let ahead = self
            .run(
                &record.path,
                &["rev-list", "--count", "@{upstream}..HEAD"],
                control,
            )
            .ok()
            .filter(|o| o.status == 0)
            .and_then(|o| o.text().trim().parse::<u64>().ok());
        match ahead {
            Some(0) => name,
            Some(n) => format!("{name} ({n} ahead)"),
            None => format!("{name} (unknown)"),
        }
    }
    /// Unix time of the checked-out commit, the worktree's last activity Git can vouch for.
    pub fn last_commit(&self, worktree: &str, control: &ScanControl) -> Option<f64> {
        if !matches!(self.risky_config(worktree, control), Ok(None)) {
            return None;
        }
        let out = self
            .run(
                worktree,
                &["log", "-1", "--no-show-signature", "--format=%ct"],
                control,
            )
            .ok()?;
        (out.status == 0)
            .then(|| out.text().trim().parse().ok())
            .flatten()
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
    let state = git.safety(services, &current, control)?;
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

/// The first repository-level setting in `git config --list --show-scope -z` output that
/// names a program Git may run while inspecting or removing a worktree.
///
/// With `-z`, each setting is two NUL-terminated fields: its scope, then `key\nvalue`.
pub fn risky_key(output: &[u8]) -> Option<String> {
    let fields: Vec<&[u8]> = output.split(|b| *b == 0).collect();
    fields
        .chunks(2)
        .filter_map(|pair| {
            let [scope, entry] = pair else { return None };
            let key = String::from_utf8_lossy(entry)
                .split('\n')
                .next()?
                .to_ascii_lowercase();
            matches!(*scope, b"local" | b"worktree").then_some(key)
        })
        .find(|key| {
            let part = |i: usize| key.split('.').nth(i).unwrap_or_default().to_owned();
            let last = key.rsplit('.').next().unwrap_or_default();
            matches!(part(0).as_str(), "filter" | "gpg" | "include" | "includeif")
                || matches!(
                    key.as_str(),
                    "diff.external"
                        | "core.pager"
                        | "core.fsmonitor"
                        | "core.sshcommand"
                        | "core.askpass"
                        | "core.hookspath"
                )
                || (part(0) == "diff" && matches!(last, "textconv" | "command"))
        })
}

/// The path of the first gitlink (mode 160000, a nested repository) in
/// `git ls-files --stage -z` output.
pub fn gitlink(output: &[u8]) -> Option<String> {
    output
        .split(|b| *b == 0)
        .filter(|entry| entry.starts_with(b"160000 "))
        .find_map(|entry| {
            let text = String::from_utf8_lossy(entry);
            text.split_once('\t').map(|(_, path)| path.to_owned())
        })
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
