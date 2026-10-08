use super::{descriptor, flush, ScanModule};
use crate::{
    git::{Git, Safety, Worktree},
    model::*,
    policy,
    ports::*,
    services::Services,
};
use std::{collections::HashSet, path::Path};

pub struct WorktreeModule;
fn title(path: &str) -> &str {
    Path::new(path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(path)
}
/// The worktree's state, shown as its row badge.
fn state_badge(record: &Worktree) -> &'static str {
    if record.prunable {
        "Orphaned"
    } else if record.locked {
        "Locked"
    } else if record.branch.is_empty() {
        "Detached"
    } else {
        "Linked"
    }
}
/// The checked-out branch, or the short commit for a detached HEAD.
fn head_label(record: &Worktree) -> String {
    if record.branch.is_empty() {
        format!("Detached at {}", &record.head[..record.head.len().min(8)])
    } else {
        record.branch.clone()
    }
}
fn is_repository(s: &Services, path: &str) -> bool {
    s.exists(&format!("{path}/.git"))
        || (s.is_file(&format!("{path}/HEAD")) && s.is_dir(&format!("{path}/objects")))
}
impl WorktreeModule {
    fn inspect(
        s: &Services,
        c: &ScanContext,
        k: &ScanControl,
        sink: &mut dyn Sink,
        repository: &str,
        common: &mut HashSet<String>,
    ) -> Result<()> {
        let git = Git(s.commands.as_ref());
        let directory = git.common_directory(repository, k)?;
        if !common.insert(directory.clone()) {
            return Ok(());
        }
        for record in git.worktrees(repository, k)? {
            k.check()?;
            // A repository's main checkout is not a cleanup candidate; only linked trees are.
            if record.main || record.bare {
                continue;
            }
            // Excluded, or outside the chosen folders when the scan is limited to them.
            if !c.allows(&record.path) {
                continue;
            }
            sink.progress(format!("Inspecting {}", record.path));
            let Ok(e) = s.entry(&record.path) else {
                sink.finding(Self::missing(&record, repository, &directory));
                continue;
            };
            let in_scope = c
                .roots
                .iter()
                .any(|root| policy::contains(&record.path, root))
                && !policy::protected(&record.path);
            let state = if in_scope {
                git.safety(s, &record, k)?
            } else {
                Safety::blocked(
                    "Registered outside the chosen roots. Add its parent as a root to inspect it.",
                    "Not inspected",
                    "Not checked",
                )
            };
            let size = if in_scope {
                s.sizer.size(e.path(), k)?
            } else {
                Default::default()
            };
            let mut identity = s.entry(e.path())?.identity;
            identity.tree_signature = size.signature.clone();
            let mut f = Finding::new(
                "worktrees",
                e.path(),
                title(&record.path),
                Resource::Worktree {
                    file: identity,
                    repository: repository.into(),
                    head: record.head.clone(),
                },
                &state.reason,
            );
            f.bytes = in_scope.then_some(size.logical);
            f.allocated_bytes = in_scope.then_some(size.allocated);
            f.modified_at = Some(e.modified());
            f.last_used_at = in_scope.then(|| git.last_commit(&record.path, k)).flatten();
            f.risk = Risk::Permanent;
            f.subtitle = format!("{} · {}", head_label(&record), record.path);
            f.badge = Some(state_badge(&record).into());
            f.details = vec![
                detail("Repository", directory.clone()),
                detail(
                    "Branch",
                    if record.branch.is_empty() {
                        "Detached HEAD".into()
                    } else {
                        record.branch.clone()
                    },
                ),
                detail("HEAD", record.head.clone()),
                detail("State", state.status.clone()),
                detail("Upstream (local ref)", state.upstream.clone()),
                detail(
                    "Ignored files deleted",
                    if state.ignored.is_empty() {
                        "None".into()
                    } else {
                        crate::git::sample(&state.ignored, 20)
                    },
                ),
                detail("Type", if record.main { "Main" } else { "Linked" }),
            ];
            // Removing a worktree also deletes its record in the main repository.
            if c.protects(&directory) {
                f.blocked_reason = Some(format!(
                    "Its repository ({directory}) is excluded, and removing the worktree changes it."
                ));
            } else if in_scope && state.eligible && size.complete {
                f.actions.push(ActionKind::RemoveWorktree);
            } else {
                f.blocked_reason = Some(if in_scope && state.eligible {
                    "Incomplete size coverage; inspect manually.".into()
                } else {
                    state.reason
                });
            }
            sink.finding(f);
        }
        Ok(())
    }
    /// A registration whose folder is gone. Shown with an orphaned badge; never pruned here.
    fn missing(record: &Worktree, repository: &str, directory: &str) -> Finding {
        let mut f = Finding::new(
            "worktrees",
            &record.path,
            title(&record.path),
            Resource::WorktreeRegistration {
                path: record.path.clone(),
                repository: repository.into(),
            },
            "Git retains this registration, but its folder is missing. Inspect repository metadata before pruning it manually.",
        );
        f.subtitle = format!("{} · {}", head_label(record), record.path);
        f.badge = Some("Orphaned".into());
        f.blocked_reason = Some("Missing worktree registrations are inspection only.".into());
        f.details = vec![
            detail("Repository", directory),
            detail("Branch", record.branch.clone()),
            detail("State", "Registered folder is missing"),
            detail(
                "Git metadata",
                if record.prunable {
                    "Prunable"
                } else {
                    "Retained"
                },
            ),
        ];
        f
    }
}
impl ScanModule for WorktreeModule {
    fn descriptor(&self) -> ModuleDescriptor {
        descriptor(
            "worktrees",
            "Git Worktrees",
            "Developer",
            "arrow.triangle.branch",
            "Branches, local changes, locks and orphaned registrations.",
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
        let mut repositories = HashSet::new();
        for root in s.roots(&c.roots) {
            if !c.excludes(&root) && !policy::system_excluded(&root) && is_repository(s, &root) {
                repositories.insert(root);
            }
        }
        let mut warnings = vec![];
        let walked = s.walk(c, k, &mut warnings, &mut |e| {
            // Build output and installed packages hold no worktrees of the user's.
            if !e.directory
                || super::project::package_tree(e.name())
                || super::project::projects().rebuildable(s, e.path())
            {
                return Ok(false);
            }
            if is_repository(s, e.path()) {
                repositories.insert(e.path().into());
            }
            Ok(true)
        });
        flush(sink, &mut warnings);
        walked?;
        let mut repositories: Vec<_> = repositories.into_iter().collect();
        repositories.sort();
        let mut common = HashSet::new();
        for repository in repositories {
            k.check()?;
            if let Err(e) = Self::inspect(s, c, k, sink, &repository, &mut common) {
                k.check()?;
                sink.warning(format!("Cannot fully inspect {repository}: {e}"));
            }
        }
        Ok(())
    }
}
