use crate::model::*;
use rayon::prelude::*;
use rustc_hash::FxHashMap;
use std::collections::BTreeMap;

/// The disk location a finding measures, if any. Scanner paths are already canonical.
fn disk_path(f: &Finding) -> Option<&str> {
    match &f.resource {
        Resource::File { file } | Resource::Worktree { file, .. } => Some(&file.path),
        Resource::Simulator { .. } => f.value("Data path"),
        _ => None,
    }
}
fn actionable(f: &Finding) -> bool {
    !f.actions.is_empty() && f.blocked_reason.is_none()
}
pub fn analytics(findings: &[Finding]) -> Analytics {
    analytics_of(findings.iter(), true)
}
/// Totals for any set of findings in one pass. Rows are grouped by path; a path counts toward
/// a total only when no ancestor path in the same subset (all rows, actionable rows, one
/// module) is counted, and rows sharing a path count once. Process memory stays separate.
pub fn analytics_of<'a>(findings: impl Iterator<Item = &'a Finding>, modules: bool) -> Analytics {
    let rows: Vec<&Finding> = findings.collect();
    let names: Vec<&str> = {
        let set: BTreeMap<&str, ()> = rows.iter().map(|f| (f.module_id.as_str(), ())).collect();
        set.into_keys().collect()
    };
    let module_index = |id: &str| names.binary_search(&id).unwrap_or(0);
    let mut by_path: FxHashMap<&str, Vec<&Finding>> =
        FxHashMap::with_capacity_and_hasher(rows.len(), Default::default());
    for f in &rows {
        if f.bytes.is_some() {
            if let Some(path) = disk_path(f) {
                by_path.entry(path).or_default().push(f);
            }
        }
    }
    #[derive(Clone, Default)]
    struct Sum {
        disk: u64,
        allocated: u64,
        ready: u64,
        modules: Vec<(u64, u64)>,
    }
    let merge = |mut a: Sum, b: Sum| {
        a.disk = a.disk.saturating_add(b.disk);
        a.allocated = a.allocated.saturating_add(b.allocated);
        a.ready = a.ready.saturating_add(b.ready);
        if a.modules.len() < b.modules.len() {
            a.modules.resize(b.modules.len(), (0, 0));
        }
        for (x, y) in a.modules.iter_mut().zip(b.modules) {
            x.0 = x.0.saturating_add(y.0);
            x.1 = x.1.saturating_add(y.1);
        }
        a
    };
    let sum = by_path
        .par_iter()
        .fold(
            || Sum {
                modules: vec![(0, 0); if modules { names.len() } else { 0 }],
                ..Default::default()
            },
            |mut acc, (path, group)| {
                let ancestors: Vec<&Vec<&Finding>> = path
                    .bytes()
                    .enumerate()
                    .rev()
                    .filter(|&(_, c)| c == b'/')
                    .map(|(i, _)| if i == 0 { "/" } else { &path[..i] })
                    .filter(|prefix| prefix != path)
                    .filter_map(|prefix| by_path.get(prefix))
                    .collect();
                let covered = |test: &dyn Fn(&Finding) -> bool| {
                    ancestors.iter().any(|g| g.iter().any(|f| test(f)))
                };
                let size = |f: &Finding| f.bytes.unwrap_or(0);
                if ancestors.is_empty() {
                    acc.disk = acc.disk.saturating_add(size(group[0]));
                    acc.allocated = acc
                        .allocated
                        .saturating_add(group[0].allocated_bytes.unwrap_or(size(group[0])));
                }
                if let Some(f) = group.iter().find(|f| actionable(f)) {
                    if !covered(&|a| actionable(a)) {
                        acc.ready = acc.ready.saturating_add(size(f));
                    }
                }
                if modules {
                    let mut seen = vec![];
                    for f in group {
                        let m = module_index(&f.module_id);
                        if seen.contains(&m) {
                            continue;
                        }
                        seen.push(m);
                        if !covered(&|a| a.module_id == f.module_id) {
                            acc.modules[m].0 = acc.modules[m].0.saturating_add(size(f));
                        }
                        if let Some(r) = group
                            .iter()
                            .find(|r| r.module_id == f.module_id && actionable(r))
                        {
                            if !covered(&|a| a.module_id == f.module_id && actionable(a)) {
                                acc.modules[m].1 = acc.modules[m].1.saturating_add(size(r));
                            }
                        }
                    }
                }
                acc
            },
        )
        .reduce(Sum::default, merge);
    let mut counts = vec![0usize; names.len()];
    for f in &rows {
        counts[module_index(&f.module_id)] += 1;
    }
    let processes: Vec<&&Finding> = rows
        .iter()
        .filter(|f| matches!(f.resource, Resource::Process { .. }))
        .collect();
    Analytics {
        findings: rows.len(),
        disk_bytes: sum.disk,
        allocated_bytes: sum.allocated,
        reclaimable_bytes: sum.ready,
        blocked: rows.iter().filter(|f| f.blocked_reason.is_some()).count(),
        process_count: processes.len(),
        process_memory_bytes: processes.iter().filter_map(|f| f.memory_bytes).sum(),
        modules: if modules {
            names
                .iter()
                .enumerate()
                .map(|(i, id)| ModuleTotal {
                    module_id: (*id).into(),
                    count: counts[i],
                    bytes: sum.modules.get(i).map_or(0, |m| m.0),
                    reclaimable_bytes: sum.modules.get(i).map_or(0, |m| m.1),
                })
                .collect()
        } else {
            vec![]
        },
    }
}
