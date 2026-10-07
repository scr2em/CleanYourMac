use crate::{
    command,
    files::{self, ScanControl},
    model::*,
    modules::ScanModule,
    policy,
};
use std::{collections::HashSet, path::Path, time::Duration};

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
pub fn run(repository: &str, args: &[&str], control: &ScanControl) -> Result<command::Output> {
    let mut arguments = vec![
        "-c".into(),
        "core.fsmonitor=false".into(),
        "-c".into(),
        "core.hooksPath=/dev/null".into(),
        "-c".into(),
        "core.untrackedCache=false".into(),
        "-C".into(),
        repository.into(),
    ];
    arguments.extend(args.iter().map(|s| s.to_string()));
    command::run("/usr/bin/git", &arguments, Duration::from_secs(20), control)
}
pub fn common_directory(repository: &str, control: &ScanControl) -> Result<String> {
    let out = run(
        repository,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        control,
    )?;
    if out.status != 0 {
        return Err(format!("Cannot inspect repository: {}", out.error));
    }
    let text = String::from_utf8(out.data).map_err(|_| "Git path is not UTF-8")?;
    Ok(text.strip_suffix('\n').unwrap_or(&text).into())
}
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
pub fn worktrees(repository: &str, control: &ScanControl) -> Result<Vec<Worktree>> {
    let out = run(
        repository,
        &["worktree", "list", "--porcelain", "-z"],
        control,
    )?;
    if out.status != 0 {
        return Err(out.error);
    }
    parse(&out.data)
}
pub struct Safety {
    pub eligible: bool,
    pub reason: String,
    pub status: String,
    pub upstream: String,
}
impl Safety {
    fn blocked(reason: &str, status: &str, upstream: &str) -> Self {
        Self {
            eligible: false,
            reason: reason.into(),
            status: status.into(),
            upstream: upstream.into(),
        }
    }
}
pub fn safety(record: &Worktree, control: &ScanControl) -> Result<Safety> {
    if record.main || record.bare {
        return Ok(Safety::blocked(
            "Main and bare repositories are protected",
            "Protected",
            "—",
        ));
    }
    if record.locked {
        return Ok(Safety::blocked("Worktree is locked", "Locked", "—"));
    }
    if record.prunable {
        return Ok(Safety::blocked(
            "Git reports stale registration metadata; inspect manually",
            "Orphaned",
            "Unknown",
        ));
    }
    if record.branch.is_empty() {
        return Ok(Safety::blocked(
            "Detached HEAD needs manual inspection",
            "Detached",
            "Unknown",
        ));
    }
    if Path::new(&(record.path.clone() + "/.gitmodules")).exists() {
        return Ok(Safety::blocked(
            "Worktrees with submodules need manual inspection",
            "Submodules",
            "Unknown",
        ));
    }
    let state = run(
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
        return Err("Git status is unavailable".into());
    }
    if !state.data.is_empty() {
        return Ok(Safety::blocked(
            "Contains changes, untracked or ignored files. Inspect before removal.",
            "Local files",
            "Not checked",
        ));
    }
    let upstream = run(
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
            "No verified upstream; local commits may exist only here",
            "Clean",
            "None",
        ));
    }
    let ahead = run(
        &record.path,
        &["rev-list", "--count", "@{upstream}..HEAD"],
        control,
    )?;
    if ahead.status != 0 || ahead.text().trim().parse::<u64>() != Ok(0) {
        return Ok(Safety::blocked(
            "Has unpushed or unknown local commits",
            "Clean",
            upstream.text().trim(),
        ));
    }
    Ok(Safety { eligible: true, reason: "Clean linked worktree, no ignored files or commits ahead of its local upstream reference. Review before removal.".into(), status: "Clean".into(), upstream: upstream.text().trim().into() })
}
pub fn remove(
    file: &FileIdentity,
    repository: &str,
    head: &str,
    context: &ScanContext,
    control: &ScanControl,
) -> Result<()> {
    files::validate(file, context, &[], control)?;
    let current = worktrees(repository, control)?
        .into_iter()
        .find(|w| w.path == file.path && w.head == head)
        .ok_or("Worktree registration or HEAD changed")?;
    let state = safety(&current, control)?;
    if !state.eligible {
        return Err(state.reason);
    }
    let out = run(
        repository,
        &["worktree", "remove", "--", &file.path],
        control,
    )?;
    if out.status != 0 {
        return Err(out.error);
    }
    Ok(())
}
pub struct GitModule(pub ModuleDescriptor);
impl ScanModule for GitModule {
    fn descriptor(&self) -> ModuleDescriptor {
        self.0.clone()
    }
    fn scan(&self, c: &ScanContext, k: &ScanControl, r: &mut ScanReport) -> Result<()> {
        let mut repositories = HashSet::new();
        let is_repo = |p: &str| {
            Path::new(&(p.to_owned() + "/.git")).exists()
                || (Path::new(&(p.to_owned() + "/HEAD")).is_file()
                    && Path::new(&(p.to_owned() + "/objects")).is_dir())
        };
        for root in files::roots(&c.roots) {
            if !c.excludes(&root) && !policy::system_excluded(&root) && is_repo(&root) {
                repositories.insert(root);
            }
        }
        files::walk(c, k, &mut r.warnings, |e| {
            if e.directory {
                if ["node_modules", ".build", "target"].contains(&e.name()) {
                    return Ok(false);
                }
                if is_repo(e.path()) {
                    repositories.insert(e.path().into());
                }
            }
            Ok(true)
        })?;
        let mut common = HashSet::new();
        let mut repos: Vec<_> = repositories.into_iter().collect();
        repos.sort();
        for repository in repos {
            k.check()?;
            let scan_repository = |r: &mut ScanReport,
                                   common: &mut HashSet<String>|
             -> Result<()> {
                let directory = common_directory(&repository, k)?;
                if !common.insert(directory.clone()) {
                    return Ok(());
                }
                for record in worktrees(&repository, k)? {
                    k.check()?;
                    if c.excludes(&record.path) {
                        continue;
                    }
                    let title = Path::new(&record.path)
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or(&record.path);
                    let Ok(e) = files::entry(&record.path) else {
                        let mut f=Finding::new("worktrees",&record.path,title,Resource::WorktreeRegistration {path:record.path.clone(),repository:repository.clone()},"Git retains this registration, but its folder is missing. Inspect repository metadata before pruning manually.");
                        f.badge = Some("Orphaned".into());
                        f.blocked_reason = Some("Missing registrations are inspection only".into());
                        f.details = vec![
                            detail("Repository", directory.clone()),
                            detail("Branch", record.branch),
                            detail("State", "Registered folder is missing"),
                        ];
                        r.findings.push(f);
                        continue;
                    };
                    let in_scope = c
                        .roots
                        .iter()
                        .any(|root| policy::contains(&record.path, root))
                        && !policy::protected(&record.path);
                    let state = if in_scope {
                        safety(&record, k)?
                    } else {
                        Safety::blocked(
                            "Registered outside the chosen roots. Add its parent to inspect it.",
                            "Not inspected",
                            "Not checked",
                        )
                    };
                    let size = if in_scope {
                        files::size(e.path(), k)?
                    } else {
                        files::FileSize::default()
                    };
                    let mut identity = files::entry(e.path())?.identity;
                    identity.tree_signature = size.signature;
                    let mut f = Finding::new(
                        "worktrees",
                        e.path(),
                        title,
                        Resource::Worktree {
                            file: identity,
                            repository: repository.clone(),
                            head: record.head.clone(),
                        },
                        &state.reason,
                    );
                    f.bytes = in_scope.then_some(size.logical);
                    f.allocated_bytes = in_scope.then_some(size.allocated);
                    f.modified_at = Some(e.modified());
                    f.risk = Risk::Permanent;
                    f.badge = Some(
                        if record.prunable {
                            "Orphaned"
                        } else if record.main {
                            "Main"
                        } else {
                            "Linked"
                        }
                        .into(),
                    );
                    f.details = vec![
                        detail("Repository", directory.clone()),
                        detail(
                            "Branch",
                            if record.branch.is_empty() {
                                "Detached HEAD".into()
                            } else {
                                record.branch
                            },
                        ),
                        detail("HEAD", record.head),
                        detail("State", state.status),
                        detail("Upstream (local ref)", state.upstream),
                    ];
                    if in_scope && state.eligible && size.complete {
                        f.actions.push(ActionKind::RemoveWorktree);
                    } else {
                        f.blocked_reason = Some(if in_scope && state.eligible && !size.complete {
                            "Incomplete size coverage".into()
                        } else {
                            state.reason
                        });
                    }
                    r.findings.push(f);
                }
                Ok(())
            };
            if let Err(e) = scan_repository(r, &mut common) {
                files::warn(
                    &mut r.warnings,
                    format!("Cannot fully inspect {repository}: {e}"),
                );
            }
        }
        Ok(())
    }
}
