use crate::model::*;
use std::{collections::BTreeMap, path::PathBuf};

/// The disk location a finding measures, if any.
fn disk_path(f: &Finding) -> Option<PathBuf> {
    match &f.resource {
        Resource::File { file } | Resource::Worktree { file, .. } => Some(file.path.clone()),
        Resource::Simulator { .. } => f.value("Data path").map(Into::into),
        _ => None,
    }
    .map(|p| PathBuf::from(crate::policy::canonical(&p)))
}
/// Sums bytes for findings whose paths are not inside (or equal to) another counted path.
fn outermost<'a>(rows: impl Iterator<Item = &'a Finding>) -> (u64, u64) {
    let mut rows: Vec<(PathBuf, &Finding)> = rows
        .filter(|f| f.bytes.is_some())
        .filter_map(|f| disk_path(f).map(|p| (p, f)))
        .collect();
    // Component-wise ordering places every descendant directly after its ancestor.
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    let mut kept: Option<PathBuf> = None;
    let (mut bytes, mut allocated) = (0u64, 0u64);
    for (path, f) in rows {
        if kept.as_ref().is_some_and(|k| path.starts_with(k)) {
            continue;
        }
        bytes = bytes.saturating_add(f.bytes.unwrap_or(0));
        allocated = allocated.saturating_add(f.allocated_bytes.or(f.bytes).unwrap_or(0));
        kept = Some(path);
    }
    (bytes, allocated)
}
fn actionable(f: &Finding) -> bool {
    !f.actions.is_empty() && f.blocked_reason.is_none()
}
pub fn analytics(findings: &[Finding]) -> Analytics {
    let mut by_module: BTreeMap<&str, Vec<&Finding>> = BTreeMap::new();
    for f in findings {
        by_module.entry(&f.module_id).or_default().push(f);
    }
    let processes: Vec<_> = findings
        .iter()
        .filter(|f| matches!(f.resource, Resource::Process { .. }))
        .collect();
    let (disk_bytes, allocated_bytes) = outermost(findings.iter());
    Analytics {
        findings: findings.len(),
        disk_bytes,
        allocated_bytes,
        reclaimable_bytes: outermost(findings.iter().filter(|f| actionable(f))).0,
        blocked: findings
            .iter()
            .filter(|f| f.blocked_reason.is_some())
            .count(),
        process_count: processes.len(),
        process_memory_bytes: processes.iter().filter_map(|f| f.memory_bytes).sum(),
        modules: by_module
            .into_iter()
            .map(|(id, rows)| ModuleTotal {
                module_id: id.into(),
                count: rows.len(),
                bytes: outermost(rows.iter().copied()).0,
                reclaimable_bytes: outermost(rows.iter().copied().filter(|f| actionable(f))).0,
            })
            .collect(),
    }
}
