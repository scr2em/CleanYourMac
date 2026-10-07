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
/// Totals for any set of findings. Rows are grouped by path; a path counts toward a total
/// only when no ancestor path in the same subset (all rows, actionable rows, one module) is
/// counted, and rows sharing a path count once. Process memory stays separate.
///
/// Paths are found by a hash that extends byte by byte, so every ancestor of a path is
/// looked up in one pass over it; rows sharing a path are chained by index rather than
/// collected per path. If two different paths ever share a hash, the exact
/// `analytics_reference` computes the totals instead.
pub fn analytics_of<'a>(findings: impl Iterator<Item = &'a Finding>, modules: bool) -> Analytics {
    let rows: Vec<&Finding> = findings.collect();
    fast(&rows, modules).unwrap_or_else(|| analytics_reference(rows.into_iter(), modules))
}

const NONE: u32 = u32::MAX;
const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
fn extend(hash: u64, byte: u8) -> u64 {
    (hash ^ u64::from(byte)).wrapping_mul(FNV_PRIME)
}
fn path_hash(path: &str) -> u64 {
    path.bytes().fold(FNV_OFFSET, extend)
}
/// The hash of each proper ancestor of `path` (as `analytics_reference` derives them:
/// the text before each `/`, and `/` itself), nearest last.
fn for_each_ancestor(path: &str, mut visit: impl FnMut(u64, &str)) {
    let mut hash = FNV_OFFSET;
    for (i, byte) in path.bytes().enumerate() {
        if byte == b'/' && i > 0 {
            visit(hash, &path[..i]);
        }
        hash = extend(hash, byte);
        if i == 0 && byte == b'/' && path.len() > 1 {
            visit(hash, "/");
        }
    }
}

fn fast(rows: &[&Finding], modules: bool) -> Option<Analytics> {
    // One pass over the rows for module names and per-row counts.
    #[derive(Default)]
    struct Counts<'a> {
        modules: BTreeMap<&'a str, usize>,
        blocked: usize,
        processes: usize,
        memory: u64,
    }
    let counts = rows
        .par_iter()
        .fold(Counts::default, |mut c, f| {
            *c.modules.entry(f.module_id.as_str()).or_default() += 1;
            c.blocked += usize::from(f.blocked_reason.is_some());
            if matches!(f.resource, Resource::Process { .. }) {
                c.processes += 1;
                c.memory += f.memory_bytes.unwrap_or(0);
            }
            c
        })
        .reduce(Counts::default, |mut a, b| {
            for (module, n) in b.modules {
                *a.modules.entry(module).or_default() += n;
            }
            a.blocked += b.blocked;
            a.processes += b.processes;
            a.memory += b.memory;
            a
        });
    let names: Vec<&str> = counts.modules.keys().copied().collect();
    if names.len() > 64 {
        return None;
    }
    let module_index = |id: &str| names.binary_search(&id).unwrap_or(0);
    // Sized rows with a disk location, and each path's hash.
    let sized: Vec<(u64, &str, &Finding)> = rows
        .par_iter()
        .filter_map(|f| {
            f.bytes?;
            let path = disk_path(f)?;
            Some((path_hash(path), path, *f))
        })
        .collect();
    // The first row of each path, keyed by hash; later rows sharing the path chain from it.
    let mut heads: FxHashMap<u64, u32> =
        FxHashMap::with_capacity_and_hasher(sized.len(), Default::default());
    let mut next = vec![NONE; sized.len()];
    let mut last = vec![NONE; sized.len()];
    let mut groups: Vec<u32> = Vec::with_capacity(sized.len());
    for (k, &(hash, path, _)) in sized.iter().enumerate() {
        let k = k as u32;
        match heads.entry(hash) {
            std::collections::hash_map::Entry::Vacant(e) => {
                e.insert(k);
                last[k as usize] = k;
                groups.push(k);
            }
            std::collections::hash_map::Entry::Occupied(e) => {
                let head = *e.get() as usize;
                if sized[head].1 != path {
                    return None;
                }
                // Keep rows in their original order within a path.
                let tail = last[head] as usize;
                next[tail] = k;
                last[head] = k;
            }
        }
    }
    let (rows_by_index, next) = (&sized, &next);
    let group = move |head: u32| {
        let mut k = head;
        std::iter::from_fn(move || {
            (k != NONE).then(|| {
                let f = rows_by_index[k as usize].2;
                k = next[k as usize];
                f
            })
        })
    };
    // Size on disk: what removing the item frees, not its logical length.
    let size = |f: &Finding| f.disk_bytes().unwrap_or(0);
    #[derive(Clone, Default)]
    struct Sum {
        disk: u64,
        logical: u64,
        ready: u64,
        modules: Vec<(u64, u64)>,
    }
    let width = if modules { names.len() } else { 0 };
    let sum = groups
        .par_iter()
        .fold(
            || Sum {
                modules: vec![(0, 0); width],
                ..Default::default()
            },
            |mut acc, &head| {
                let (_, path, first) = sized[head as usize];
                let mut ancestors: Vec<u32> = Vec::new();
                for_each_ancestor(path, |hash, prefix| {
                    if let Some(&h) = heads.get(&hash) {
                        if sized[h as usize].1 == prefix {
                            ancestors.push(h);
                        }
                    }
                });
                let covered =
                    |test: &dyn Fn(&Finding) -> bool| ancestors.iter().any(|&h| group(h).any(test));
                if ancestors.is_empty() {
                    acc.disk = acc.disk.saturating_add(size(first));
                    acc.logical = acc.logical.saturating_add(first.bytes.unwrap_or(0));
                }
                if let Some(f) = group(head).find(|f| actionable(f)) {
                    if ancestors.is_empty() || !covered(&|a| actionable(a)) {
                        acc.ready = acc.ready.saturating_add(size(f));
                    }
                }
                if modules {
                    let mut seen = 0u64;
                    for f in group(head) {
                        let m = module_index(&f.module_id);
                        if seen & (1 << m) != 0 {
                            continue;
                        }
                        seen |= 1 << m;
                        if ancestors.is_empty() || !covered(&|a| a.module_id == f.module_id) {
                            acc.modules[m].0 = acc.modules[m].0.saturating_add(size(f));
                        }
                        if let Some(r) =
                            group(head).find(|r| r.module_id == f.module_id && actionable(r))
                        {
                            if ancestors.is_empty()
                                || !covered(&|a| a.module_id == f.module_id && actionable(a))
                            {
                                acc.modules[m].1 = acc.modules[m].1.saturating_add(size(r));
                            }
                        }
                    }
                }
                acc
            },
        )
        .reduce(
            || Sum {
                modules: vec![(0, 0); width],
                ..Default::default()
            },
            |mut a, b| {
                a.disk = a.disk.saturating_add(b.disk);
                a.logical = a.logical.saturating_add(b.logical);
                a.ready = a.ready.saturating_add(b.ready);
                for (x, y) in a.modules.iter_mut().zip(b.modules) {
                    x.0 = x.0.saturating_add(y.0);
                    x.1 = x.1.saturating_add(y.1);
                }
                a
            },
        );
    Some(Analytics {
        findings: rows.len(),
        disk_bytes: sum.disk,
        allocated_bytes: sum.disk,
        logical_bytes: sum.logical,
        reclaimable_bytes: sum.ready,
        blocked: counts.blocked,
        process_count: counts.processes,
        process_memory_bytes: counts.memory,
        modules: if modules {
            names
                .iter()
                .enumerate()
                .map(|(i, id)| ModuleTotal {
                    module_id: (*id).into(),
                    count: counts.modules[id],
                    bytes: sum.modules.get(i).map_or(0, |m| m.0),
                    reclaimable_bytes: sum.modules.get(i).map_or(0, |m| m.1),
                })
                .collect()
        } else {
            vec![]
        },
    })
}

/// The straightforward implementation `analytics_of` must agree with; used when path
/// hashes collide, and by tests.
#[doc(hidden)]
pub fn analytics_reference<'a>(
    findings: impl Iterator<Item = &'a Finding>,
    modules: bool,
) -> Analytics {
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
        logical: u64,
        ready: u64,
        modules: Vec<(u64, u64)>,
    }
    let merge = |mut a: Sum, b: Sum| {
        a.disk = a.disk.saturating_add(b.disk);
        a.logical = a.logical.saturating_add(b.logical);
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
                // Size on disk: what removing the item frees, not its logical length.
                let size = |f: &Finding| f.disk_bytes().unwrap_or(0);
                if ancestors.is_empty() {
                    acc.disk = acc.disk.saturating_add(size(group[0]));
                    acc.logical = acc.logical.saturating_add(group[0].bytes.unwrap_or(0));
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
        allocated_bytes: sum.disk,
        logical_bytes: sum.logical,
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
