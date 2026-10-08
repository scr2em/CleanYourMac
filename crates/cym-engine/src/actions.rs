//! Reviewed actions. Every item is revalidated against the current scope and its own fresh
//! state immediately before it is changed, and each outcome is journaled as it completes.
use crate::{
    git, model::*, modules::Registry, orphans, policy, ports::*, services::Services,
    simulator::Simctl,
};
use std::path::Path;

pub fn execute(
    services: &Services,
    registry: &Registry,
    request: &ActionRequest,
    control: &ScanControl,
) -> Vec<ActionResult> {
    let mut results = vec![];
    for finding in policy::normalized_selection(&request.findings) {
        if control.is_cancelled() {
            break;
        }
        let failed =
            |message: &str| row(&finding, request.kind, Outcome::Failed, message, None, None);
        // Eligibility and scope first; then whether something is using the item, which the
        // user may override; then the module's own checks and fresh validation in `apply`.
        // One item failing, even by a panic in a module, never stops the rest of the batch.
        let attempt = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if let Err(message) = check(&finding, request) {
                return failed(&message);
            }
            let in_use = (!request.force)
                .then(|| registry.get(&finding.module_id))
                .flatten()
                .and_then(|module| module.in_use(services, &finding, request.kind));
            match in_use {
                Some(reason) => ActionResult {
                    overridable: true,
                    ..failed(&reason)
                },
                None => apply(services, registry, &finding, request, control)
                    .unwrap_or_else(|message| failed(&message)),
            }
        }));
        let mut result = attempt.unwrap_or_else(|_| {
            failed("An unexpected error stopped this item; the item was left unchanged or partly moved. Check it in Finder.")
        });
        // Persist after each item so an interrupted batch keeps its completed outcomes.
        if let Err(e) = services.journal.append(std::slice::from_ref(&result)) {
            result.journal_warning = Some(format!(
                "Action finished, but local history could not be saved: {e}"
            ));
        }
        results.push(result);
    }
    results
}

/// Eligibility and scope checks against the request's current context.
fn check(f: &Finding, request: &ActionRequest) -> Result<()> {
    let c = &request.context;
    // A protected item becomes eligible only for the actions the user acknowledged, and
    // never while something blocks it.
    let acknowledged =
        request.acknowledged.contains(&f.id) && f.acknowledged_actions.contains(&request.kind);
    if !(f.actions.contains(&request.kind) || acknowledged) || f.blocked_reason.is_some() {
        return Err("The requested action is not eligible for this finding.".into());
    }
    if let Some(path) = f.resource.path() {
        if c.protects(path) {
            return Err("The item or an item inside it is now excluded.".into());
        }
    }
    if c.limit_to_roots {
        let path = f.value("Data path").or(f.resource.path());
        let working = f.value("Working folder");
        let allowed = path.is_some_and(|p| c.allows(p))
            || (f.module_id == "orphans" && working.is_some_and(|p| c.allows(p)));
        if !allowed {
            return Err("This item is outside the finder's current included paths.".into());
        }
    }
    if f.value("Working folder").is_some_and(|p| c.excludes(p)) {
        return Err("This process working folder is now excluded.".into());
    }
    if f.value("Data path").is_some_and(|p| c.protects(p)) {
        return Err("This simulator data path is now excluded.".into());
    }
    Ok(())
}

fn apply(
    services: &Services,
    registry: &Registry,
    f: &Finding,
    request: &ActionRequest,
    control: &ScanControl,
) -> Result<ActionResult> {
    let module = registry
        .get(&f.module_id)
        .ok_or("The module that found this item is not available.")?;
    module.preflight(services, f, request.kind, control)?;
    let c = &request.context;
    let done = |message: &str| Ok(row(f, request.kind, Outcome::Applied, message, None, None));
    match (request.kind, &f.resource) {
        (ActionKind::Trash, Resource::File { file }) => {
            services.validate(file, c, &module.action_roots(), control)?;
            let destination = services.trash.move_to_trash(&file.path)?;
            let identity = services.snapshot(&destination, control).ok();
            Ok(row(
                f,
                request.kind,
                Outcome::Applied,
                "Moved to Trash; storage is still occupied until Trash is emptied.",
                Some(destination),
                identity,
            ))
        }
        (ActionKind::EmptyTrash, Resource::File { file }) => {
            let trash = policy::home() + "/.Trash";
            if !policy::contains(&file.path, &trash) || policy::canonical(&file.path) == trash {
                return Err("Only reviewed items inside your Trash are eligible.".into());
            }
            services.validate(file, c, &[trash], control)?;
            services.fs.remove(&file.path)?;
            done("Permanently removed the reviewed Trash item.")
        }
        (
            ActionKind::RemoveWorktree,
            Resource::Worktree {
                file,
                repository,
                head,
            },
        ) => {
            git::remove(services, file, repository, head, c, control)?;
            done("Removed linked worktree; branch retained.")
        }
        (
            ActionKind::ResetSimulator | ActionKind::DeleteSimulator,
            Resource::Simulator { id, .. },
        ) => {
            Simctl(services.commands.as_ref()).act(id, request.kind, c, control)?;
            done(if request.kind == ActionKind::ResetSimulator {
                "Simulator reset."
            } else {
                "Simulator deleted."
            })
        }
        (ActionKind::Terminate | ActionKind::ForceQuit, Resource::Process { process }) => {
            let outcome = orphans::signal(
                services,
                process,
                request.kind == ActionKind::ForceQuit,
                c,
                control,
            )?;
            let message = match outcome {
                Outcome::Applied => "Observed process exit.",
                Outcome::Skipped => "Process already exited.",
                _ => "Signal delivered; process remains alive. Force Quit requires a separate review.",
            };
            Ok(row(f, request.kind, outcome, message, None, None))
        }
        _ => Err("Action and resource type do not match.".into()),
    }
}

// Process command arguments are deliberately omitted from the journal.
fn row(
    f: &Finding,
    action: ActionKind,
    outcome: Outcome,
    message: &str,
    trash_path: Option<String>,
    trash_identity: Option<FileIdentity>,
) -> ActionResult {
    ActionResult {
        id: uuid::Uuid::new_v4().to_string(),
        date: now(),
        title: f.title.clone(),
        original_path: f.resource.path().map(Into::into),
        action,
        outcome,
        message: message.into(),
        trash_path,
        finding_id: Some(f.id.clone()),
        trash_identity,
        journal_warning: None,
        overridable: false,
    }
}

/// Moves an unchanged item from Trash back to its unoccupied original location.
pub fn restore(services: &Services, row: &ActionResult, control: &ScanControl) -> Result<()> {
    let (Some(original), Some(trashed)) = (&row.original_path, &row.trash_path) else {
        return Err("This action has no restorable item.".into());
    };
    if services.exists(original) {
        return Err("An item already exists at the original path.".into());
    }
    if !services.exists(trashed) {
        return Err("The item no longer exists in Trash.".into());
    }
    let expected = row
        .trash_identity
        .as_ref()
        .ok_or("Restore identity is unavailable. Inspect Trash in Finder.")?;
    if policy::canonical(&expected.path) != policy::canonical(trashed) {
        return Err("Restore identity does not match the Trash item.".into());
    }
    services.validate_identity(expected, control)?;
    services.no_symlink_ancestors(trashed)?;
    let parent = Path::new(original)
        .parent()
        .and_then(|p| p.to_str())
        .ok_or("Invalid original path.")?;
    services.no_symlink_ancestors(parent)?;
    services.fs.rename(trashed, original)
}
