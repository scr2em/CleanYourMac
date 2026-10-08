//! The in-memory result set behind the app. Scans write findings here; the UI asks for
//! filtered, sorted snapshots and reads them a page at a time, so result sets of millions of
//! rows never cross the bridge in bulk.
use crate::{analytics, model::*};
use rayon::prelude::*;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet, VecDeque},
    sync::{
        atomic::{AtomicU64, Ordering as Atomic},
        Arc, Mutex, RwLock,
    },
};

#[derive(Default)]
struct Bucket {
    rows: Vec<Arc<Finding>>,
    index: FxHashMap<String, usize>,
}
impl Bucket {
    fn insert(&mut self, f: Arc<Finding>) {
        match self.index.get(&f.id) {
            Some(&i) => self.rows[i] = f,
            None => {
                self.index.insert(f.id.clone(), self.rows.len());
                self.rows.push(f);
            }
        }
    }
    fn remove(&mut self, id: &str) -> bool {
        let Some(i) = self.index.remove(id) else {
            return false;
        };
        self.rows.swap_remove(i);
        if let Some(moved) = self.rows.get(i) {
            self.index.insert(moved.id.clone(), i);
        }
        true
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SortKey {
    #[default]
    Size,
    Name,
    Cpu,
    /// Least recently used first when ascending; items without a date always sort last.
    LastUsed,
}
/// What the UI is looking at: a module (or all modules), a search, filters and an order.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Query {
    pub module: Option<String>,
    pub search: String,
    pub min_bytes: u64,
    /// Only rows last modified before this Unix time.
    pub modified_before: Option<f64>,
    /// Only rows last used before this Unix time; rows with no recorded use count by their
    /// modification time.
    pub last_used_before: Option<f64>,
    pub sort: SortKey,
    pub ascending: bool,
}

/// A lightweight projection of a finding for list rows.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Row {
    pub id: String,
    pub module_id: String,
    pub title: String,
    pub subtitle: String,
    pub path: Option<String>,
    pub pid: Option<i32>,
    pub bytes: Option<u64>,
    pub allocated_bytes: Option<u64>,
    pub cpu_percent: Option<f64>,
    pub memory_bytes: Option<u64>,
    pub modified_at: Option<f64>,
    pub last_used_at: Option<f64>,
    pub risk: Risk,
    pub badge: Option<String>,
    pub blocked: bool,
    pub eligible: bool,
    pub brand: Option<String>,
    /// Why no action is offered, when one is blocked.
    pub blocked_reason: Option<String>,
    /// Protected, but removable from the inspector after confirming.
    pub acknowledgeable: bool,
}
impl From<&Finding> for Row {
    fn from(f: &Finding) -> Self {
        Self {
            id: f.id.clone(),
            module_id: f.module_id.clone(),
            title: f.title.clone(),
            subtitle: f.subtitle.clone(),
            path: f.resource.path().map(Into::into),
            pid: match &f.resource {
                Resource::Process { process } => Some(process.pid),
                _ => None,
            },
            bytes: f.bytes,
            allocated_bytes: f.allocated_bytes,
            cpu_percent: f.cpu_percent,
            memory_bytes: f.memory_bytes,
            modified_at: f.modified_at,
            last_used_at: f.last_used_at,
            risk: f.risk,
            badge: f.badge.clone(),
            blocked: f.blocked_reason.is_some(),
            eligible: !f.actions.is_empty() && f.blocked_reason.is_none(),
            brand: f.brand.clone(),
            blocked_reason: f.blocked_reason.clone(),
            acknowledgeable: f.acknowledgement.is_some() && f.blocked_reason.is_none(),
        }
    }
}

/// An ordered snapshot of the rows matching a query, with its totals.
pub struct Snapshot {
    pub id: u64,
    pub generation: u64,
    rows: Ordered,
    pub summary: Analytics,
    pub largest: Vec<Row>,
    pub max_cpu: Option<f64>,
    /// Rows an action can apply to, for a "select all" control.
    pub eligible: usize,
}
fn eligible(f: &Finding) -> bool {
    !f.actions.is_empty() && f.blocked_reason.is_none()
}
impl Snapshot {
    pub fn len(&self) -> usize {
        self.rows.len()
    }
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
    pub fn page(&self, offset: usize, limit: usize) -> Vec<Row> {
        self.rows
            .iter()
            .skip(offset)
            .take(limit)
            .map(|f| Row::from(f.as_ref()))
            .collect()
    }
    /// IDs of the rows in `offset..offset + limit` that can be selected for an action;
    /// selecting a range or everything needs no row projections.
    pub fn eligible_ids(&self, offset: usize, limit: usize) -> Vec<String> {
        self.rows
            .iter()
            .skip(offset)
            .take(limit)
            .filter(|f| eligible(f))
            .map(|f| f.id.clone())
            .collect()
    }
    /// The row's position in this snapshot, for keeping the inspected row in view.
    pub fn position(&self, id: &str) -> Option<usize> {
        self.rows.par_iter().position_first(|f| f.id == id)
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotInfo {
    pub query_id: u64,
    pub generation: u64,
    pub total: usize,
    pub summary: Analytics,
    pub largest: Vec<Row>,
    pub max_cpu: Option<f64>,
    pub eligible: usize,
}
impl From<&Snapshot> for SnapshotInfo {
    fn from(s: &Snapshot) -> Self {
        Self {
            query_id: s.id,
            generation: s.generation,
            total: s.len(),
            summary: s.summary.clone(),
            largest: s.largest.clone(),
            max_cpu: s.max_cpu,
            eligible: s.eligible,
        }
    }
}

/// Totals and shared actions for a selection of IDs, with a bounded preview for review.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Selection {
    pub count: usize,
    pub bytes: u64,
    pub actions: Vec<ActionKind>,
    pub includes_processes: bool,
    pub preview: Vec<Finding>,
}

#[derive(Default)]
pub struct ResultStore {
    buckets: RwLock<HashMap<String, Bucket>>,
    /// Bumped each time a module's rows are cleared for a new scan, so rows from an earlier,
    /// cancelled scan of that module can be told apart and dropped.
    epochs: Mutex<HashMap<String, u64>>,
    generation: AtomicU64,
    next_query: AtomicU64,
    snapshots: Mutex<VecDeque<Arc<Snapshot>>>,
    /// Totals depend on the filter, not the order, so re-sorting reuses them.
    summaries: Mutex<VecDeque<(SummaryKey, Derived)>>,
    /// Fully sorted rows per (module, order), so filtering never re-sorts.
    orders: Mutex<VecDeque<(OrderKey, Ordered)>>,
}
type Ordered = Arc<Vec<Arc<Finding>>>;
/// What a query reports besides its rows; it depends only on which rows match.
#[derive(Clone)]
struct Derived {
    summary: Analytics,
    largest: Vec<Row>,
    max_cpu: Option<f64>,
    eligible: usize,
}
impl Derived {
    fn of(rows: &[Arc<Finding>], modules: bool) -> Self {
        // Largest first, then by ID, as the "largest items" list shows them.
        let rank = |a: &&Arc<Finding>, b: &&Arc<Finding>| {
            b.disk_bytes()
                .cmp(&a.disk_bytes())
                .then_with(|| a.id.cmp(&b.id))
        };
        struct Pass<'a> {
            eligible: usize,
            max_cpu: Option<f64>,
            largest: Vec<&'a Arc<Finding>>,
        }
        let empty = || Pass {
            eligible: 0,
            max_cpu: None,
            largest: Vec::with_capacity(6),
        };
        let keep = |largest: &mut Vec<&Arc<Finding>>| {
            largest.sort_by(rank);
            largest.truncate(5);
        };
        // One pass for everything but the totals, so the rows are read once.
        let pass = rows
            .par_iter()
            .fold(empty, |mut p, f| {
                p.eligible += usize::from(eligible(f));
                if let Some(cpu) = f.cpu_percent {
                    p.max_cpu = Some(p.max_cpu.map_or(cpu, |m| {
                        if cpu.total_cmp(&m).is_gt() {
                            cpu
                        } else {
                            m
                        }
                    }));
                }
                if f.disk_bytes().is_some()
                    && (p.largest.len() < 5 || rank(&f, &p.largest[4]).is_lt())
                {
                    p.largest.push(f);
                    keep(&mut p.largest);
                }
                p
            })
            .reduce(empty, |mut a, b| {
                a.eligible += b.eligible;
                a.max_cpu = match (a.max_cpu, b.max_cpu) {
                    (Some(x), Some(y)) => Some(if y.total_cmp(&x).is_gt() { y } else { x }),
                    (x, y) => x.or(y),
                };
                a.largest.extend(b.largest);
                keep(&mut a.largest);
                a
            });
        Self {
            summary: analytics::analytics_of(rows.iter().map(|f| f.as_ref()), modules),
            largest: pass
                .largest
                .into_iter()
                .map(|f| Row::from(f.as_ref()))
                .collect(),
            max_cpu: pass.max_cpu,
            eligible: pass.eligible,
        }
    }
}
#[derive(PartialEq)]
struct OrderKey {
    generation: u64,
    module: Option<String>,
    sort: SortKey,
    ascending: bool,
}
#[derive(PartialEq)]
struct SummaryKey {
    generation: u64,
    module: Option<String>,
    search: String,
    min_bytes: u64,
    modified_before: Option<u64>,
    last_used_before: Option<u64>,
}
impl ResultStore {
    fn changed(&self) {
        self.generation.fetch_add(1, Atomic::Relaxed);
    }
    pub fn generation(&self) -> u64 {
        self.generation.load(Atomic::Relaxed)
    }
    pub fn len(&self) -> usize {
        self.buckets
            .read()
            .map(|b| b.values().map(|b| b.rows.len()).sum())
            .unwrap_or(0)
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn clear_modules(&self, modules: &[String]) {
        let mut epochs = self.epochs.lock().ok();
        if let Ok(mut buckets) = self.buckets.write() {
            for module in modules {
                buckets.remove(module);
                if let Some(epochs) = epochs.as_mut() {
                    *epochs.entry(module.clone()).or_default() += 1;
                }
            }
        }
        self.changed();
    }
    /// The current epoch of a module's rows; see `insert_at`.
    pub fn epoch(&self, module: &str) -> u64 {
        self.epochs
            .lock()
            .ok()
            .and_then(|e| e.get(module).copied())
            .unwrap_or_default()
    }
    /// Inserts one module's rows unless the module was cleared since `epoch` was read, which
    /// means a newer scan owns it now.
    pub fn insert_at(&self, module: &str, epoch: u64, rows: Vec<Finding>) -> bool {
        let Ok(epochs) = self.epochs.lock() else {
            return false;
        };
        if epochs.get(module).copied().unwrap_or_default() != epoch {
            return false;
        }
        // Held across the insert, so a clear cannot slip in between.
        self.insert(rows);
        drop(epochs);
        true
    }
    pub fn clear(&self) {
        if let Ok(mut buckets) = self.buckets.write() {
            buckets.clear();
        }
        self.changed();
    }
    pub fn insert(&self, rows: Vec<Finding>) {
        if rows.is_empty() {
            return;
        }
        if let Ok(mut buckets) = self.buckets.write() {
            // Size each module's rows and index once instead of growing them repeatedly.
            let mut incoming: HashMap<&str, usize> = HashMap::new();
            for f in &rows {
                *incoming.entry(f.module_id.as_str()).or_default() += 1;
            }
            for (module, count) in incoming {
                let bucket = buckets.entry(module.to_owned()).or_default();
                bucket.rows.reserve(count);
                bucket.index.reserve(count);
            }
            // Moving each row into shared storage is the bulk of the work; do it in parallel.
            let rows: Vec<Arc<Finding>> = rows.into_par_iter().map(Arc::new).collect();
            for f in rows {
                // Look up by borrowed name; allocate a key only for a module's first row.
                let bucket = match buckets.get_mut(&f.module_id) {
                    Some(bucket) => bucket,
                    None => buckets.entry(f.module_id.clone()).or_default(),
                };
                bucket.insert(f);
            }
        }
        self.changed();
    }
    pub fn remove(&self, ids: &[String]) -> usize {
        let mut removed = 0;
        if let Ok(mut buckets) = self.buckets.write() {
            for id in ids {
                removed += buckets
                    .values_mut()
                    .map(|b| usize::from(b.remove(id)))
                    .sum::<usize>();
            }
        }
        if removed > 0 {
            self.changed();
        }
        removed
    }
    /// Removes rows whose file or folder no longer exists (deleted outside the app) and
    /// returns their IDs. Other resources (processes, simulators) are left alone.
    /// Drops rows that fall outside `context` (excluded, holding an excluded path, outside
    /// the roots of a limited scope, or in a system location), as when the user changes
    /// what to scan or protects a folder. Returns the removed IDs.
    pub fn retain_in_scope(&self, context: &ScanContext) -> Vec<String> {
        let outside: Vec<String> = match self.buckets.read() {
            Ok(buckets) => buckets
                .values()
                .flat_map(|b| b.rows.iter())
                .collect::<Vec<_>>()
                .into_par_iter()
                .filter(|f| {
                    f.scope_path().is_some_and(|p| {
                        !context.allows(p)
                            || context.protects(p)
                            || crate::policy::system_excluded(p)
                    })
                })
                .map(|f| f.id.clone())
                .collect(),
            Err(_) => vec![],
        };
        self.remove(&outside);
        outside
    }
    pub fn remove_missing(&self, exists: &(dyn Fn(&str) -> bool + Sync)) -> Vec<String> {
        let missing: Vec<String> = match self.buckets.read() {
            Ok(buckets) => buckets
                .values()
                .flat_map(|b| b.rows.iter())
                .collect::<Vec<_>>()
                .into_par_iter()
                .filter(|f| match &f.resource {
                    Resource::File { file } | Resource::Worktree { file, .. } => {
                        !exists(&file.path)
                    }
                    _ => false,
                })
                .map(|f| f.id.clone())
                .collect(),
            Err(_) => return vec![],
        };
        if !missing.is_empty() {
            self.remove(&missing);
        }
        missing
    }
    pub fn get(&self, id: &str) -> Option<Arc<Finding>> {
        let buckets = self.buckets.read().ok()?;
        lookup(&buckets, id)
    }
    pub fn findings(&self, ids: &[String]) -> Vec<Finding> {
        self.rows(ids).iter().map(|f| f.as_ref().clone()).collect()
    }
    /// The stored rows for `ids`, once each, in the order given.
    fn rows(&self, ids: &[String]) -> Vec<Arc<Finding>> {
        let Ok(buckets) = self.buckets.read() else {
            return vec![];
        };
        let mut seen = HashSet::with_capacity(ids.len());
        ids.iter()
            .filter(|id| seen.insert(id.as_str()))
            .filter_map(|id| lookup(&buckets, id))
            .collect()
    }

    /// Filters and sorts in parallel and keeps the snapshot for paging.
    pub fn query(&self, q: &Query) -> Arc<Snapshot> {
        let generation = self.generation();
        // Sort every row of the module once per store generation, then filter that order;
        // a linear, order-preserving pass is far cheaper than re-sorting on each keystroke.
        let ordered = self.ordered(q, generation);
        let needle = Needle::new(&q.search);
        let unfiltered = q.min_bytes == 0
            && q.modified_before.is_none()
            && q.last_used_before.is_none()
            && needle.unicode.is_empty();
        // With no filter the snapshot shares the cached order instead of copying it.
        let rows: Ordered = if unfiltered {
            ordered
        } else {
            Arc::new(
                ordered
                    .par_iter()
                    .filter(|f| {
                        (q.min_bytes == 0 || f.disk_bytes().unwrap_or(0) >= q.min_bytes)
                            && q.modified_before
                                .is_none_or(|t| f.modified_at.is_some_and(|m| m < t))
                            && q.last_used_before.is_none_or(|t| {
                                f.last_used_at.or(f.modified_at).is_some_and(|u| u < t)
                            })
                            && needle.matches(f)
                    })
                    .cloned()
                    .collect(),
            )
        };
        let key = SummaryKey {
            generation,
            module: q.module.clone(),
            search: q.search.trim().to_owned(),
            min_bytes: q.min_bytes,
            modified_before: q.modified_before.map(f64::to_bits),
            last_used_before: q.last_used_before.map(f64::to_bits),
        };
        // Totals, largest items, busiest process and eligible count depend on which rows
        // match, not on their order, so a re-sort or repeat query reuses them.
        let cached = self
            .summaries
            .lock()
            .ok()
            .and_then(|c| c.iter().find(|(k, _)| *k == key).map(|(_, d)| d.clone()));
        let derived = cached.unwrap_or_else(|| {
            let derived = Derived::of(&rows, q.module.is_none());
            if let Ok(mut cache) = self.summaries.lock() {
                cache.retain(|(k, _)| k.generation == generation);
                cache.push_back((key, derived.clone()));
                while cache.len() > 8 {
                    cache.pop_front();
                }
            }
            derived
        });
        let snapshot = Arc::new(Snapshot {
            id: self.next_query.fetch_add(1, Atomic::Relaxed) + 1,
            generation,
            largest: derived.largest,
            max_cpu: derived.max_cpu,
            eligible: derived.eligible,
            summary: derived.summary,
            rows,
        });
        if let Ok(mut snapshots) = self.snapshots.lock() {
            snapshots.push_back(snapshot.clone());
            // Keep a few recent snapshots so pages requested for an older query still resolve.
            while snapshots.len() > 8 {
                snapshots.pop_front();
            }
        }
        snapshot
    }
    /// All rows of the queried module(s) in the requested order, cached per generation.
    fn ordered(&self, q: &Query, generation: u64) -> Ordered {
        let key = OrderKey {
            generation,
            module: q.module.clone(),
            sort: q.sort,
            ascending: q.ascending,
        };
        if let Some(hit) = self
            .orders
            .lock()
            .ok()
            .and_then(|c| c.iter().find(|(k, _)| *k == key).map(|(_, v)| v.clone()))
        {
            return hit;
        }
        let rows: Vec<Arc<Finding>> = match self.buckets.read() {
            Ok(buckets) => match &q.module {
                Some(m) => buckets.get(m).map(|b| b.rows.clone()).unwrap_or_default(),
                None => buckets
                    .values()
                    .flat_map(|b| b.rows.iter().cloned())
                    .collect(),
            },
            Err(_) => vec![],
        };
        let rows = sorted(rows, q.sort, q.ascending);
        let rows = Arc::new(rows);
        if let Ok(mut cache) = self.orders.lock() {
            cache.retain(|(k, _)| k.generation == generation);
            cache.push_back((key, rows.clone()));
            while cache.len() > 4 {
                cache.pop_front();
            }
        }
        rows
    }
    /// One-click fixes for the current results; `now` is Unix seconds.
    pub fn recommendations(&self, now: f64) -> Vec<crate::recommend::Recommendation> {
        let Ok(buckets) = self.buckets.read() else {
            return vec![];
        };
        let module_rows = |module: &str| {
            buckets
                .get(module)
                .map(|b| b.rows.iter().map(|f| f.as_ref()).collect())
                .unwrap_or_default()
        };
        crate::recommend::recommendations(&module_rows, crate::recommend::RULES, now, 1_000_000)
    }
    pub fn snapshot(&self, id: u64) -> Option<Arc<Snapshot>> {
        self.snapshots
            .lock()
            .ok()?
            .iter()
            .find(|s| s.id == id)
            .cloned()
    }

    pub fn selection(&self, ids: &[String], preview: usize) -> Selection {
        let rows = self.rows(ids);
        let normalized = crate::policy::outermost(rows.iter().map(|f| f.as_ref()));
        let mut actions: Option<Vec<ActionKind>> = None;
        for f in &rows {
            let eligible: &[ActionKind] = if f.blocked_reason.is_some() {
                &[]
            } else {
                &f.actions
            };
            actions = Some(match actions {
                None => eligible.to_vec(),
                Some(mut current) => {
                    current.retain(|a| eligible.contains(a));
                    current
                }
            });
        }
        Selection {
            count: rows.len(),
            bytes: normalized.iter().filter_map(|f| f.disk_bytes()).sum(),
            actions: actions.unwrap_or_default(),
            includes_processes: rows
                .iter()
                .any(|f| matches!(f.resource, Resource::Process { .. })),
            preview: rows
                .iter()
                .take(preview)
                .map(|f| f.as_ref().clone())
                .collect(),
        }
    }
}

/// Rows in the requested order. Numeric orders sort compact `(key, index)` pairs instead of
/// following two heap pointers per comparison; IDs break ties exactly as before (reversed
/// with a descending order, except for last used, whose undated rows always come last).
fn sorted(rows: Vec<Arc<Finding>>, sort: SortKey, ascending: bool) -> Vec<Arc<Finding>> {
    if sort == SortKey::Name {
        let mut keyed: Vec<(&str, &str, u32)> = rows
            .iter()
            .enumerate()
            .map(|(i, f)| (f.title.as_str(), f.id.as_str(), i as u32))
            .collect();
        keyed.par_sort_unstable_by(|a, b| {
            let order = natural(a.0, b.0).then_with(|| a.1.cmp(b.1));
            if ascending {
                order
            } else {
                order.reverse()
            }
        });
        return keyed
            .iter()
            .map(|&(_, _, i)| rows[i as usize].clone())
            .collect();
    }
    // An order-preserving u64 for a float, so keys compare as integers.
    fn float(value: f64) -> u64 {
        let bits = value.to_bits();
        if bits >> 63 == 1 {
            !bits
        } else {
            bits | (1 << 63)
        }
    }
    let flip = |value: u64| if ascending { value } else { !value };
    let mut keyed: Vec<(u128, u32)> = rows
        .par_iter()
        .enumerate()
        .map(|(i, f)| {
            let key = match sort {
                SortKey::Size => u128::from(flip(f.disk_bytes().or(f.memory_bytes).unwrap_or(0))),
                SortKey::Cpu => u128::from(flip(float(f.cpu_percent.unwrap_or(0.)))),
                SortKey::LastUsed => match f.last_used_at {
                    Some(t) => u128::from(flip(float(t))),
                    None => 1 << 64,
                },
                SortKey::Name => unreachable!(),
            };
            (key, i as u32)
        })
        .collect();
    let ids_reversed = !ascending && sort != SortKey::LastUsed;
    keyed.par_sort_unstable_by(|a, b| {
        a.0.cmp(&b.0).then_with(|| {
            let order = rows[a.1 as usize].id.cmp(&rows[b.1 as usize].id);
            if ids_reversed {
                order.reverse()
            } else {
                order
            }
        })
    });
    keyed
        .iter()
        .map(|&(_, i)| rows[i as usize].clone())
        .collect()
}

/// A row by ID. IDs are `module:key`, so the module's bucket is tried first.
fn lookup(buckets: &HashMap<String, Bucket>, id: &str) -> Option<Arc<Finding>> {
    let find = |b: &Bucket| b.index.get(id).map(|&i| b.rows[i].clone());
    id.split_once(':')
        .and_then(|(module, _)| buckets.get(module))
        .and_then(find)
        .or_else(|| buckets.values().find_map(find))
}

/// Case-insensitive substring search over title and subtitle without allocating per row
/// for ASCII queries.
struct Needle {
    ascii: Option<Vec<u8>>,
    unicode: String,
}
impl Needle {
    fn new(search: &str) -> Self {
        let search = search.trim();
        Self {
            ascii: search
                .is_ascii()
                .then(|| search.bytes().map(|b| b.to_ascii_lowercase()).collect()),
            unicode: search.to_lowercase(),
        }
    }
    fn matches(&self, f: &Finding) -> bool {
        if self.unicode.is_empty() {
            return true;
        }
        match &self.ascii {
            Some(needle) => contains_ascii(&f.title, needle) || contains_ascii(&f.subtitle, needle),
            None => {
                f.title.to_lowercase().contains(&self.unicode)
                    || f.subtitle.to_lowercase().contains(&self.unicode)
            }
        }
    }
}
fn contains_ascii(haystack: &str, needle: &[u8]) -> bool {
    let hay = haystack.as_bytes();
    if needle.len() > hay.len() {
        return false;
    }
    hay.windows(needle.len()).any(|w| {
        w.iter()
            .zip(needle)
            .all(|(a, b)| a.to_ascii_lowercase() == *b)
    })
}

/// Natural, case-insensitive ordering: "file2" sorts before "file10".
pub fn natural(a: &str, b: &str) -> Ordering {
    fn digits(s: &[u8], mut i: usize) -> (usize, &[u8]) {
        let start = i;
        while i < s.len() && s[i].is_ascii_digit() {
            i += 1;
        }
        let run = &s[start..i];
        let zeros = run.iter().take_while(|&&c| c == b'0').count();
        (i, &run[zeros..])
    }
    let (a, b) = (a.as_bytes(), b.as_bytes());
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        let order = if a[i].is_ascii_digit() && b[j].is_ascii_digit() {
            let (next_i, x) = digits(a, i);
            let (next_j, y) = digits(b, j);
            (i, j) = (next_i, next_j);
            x.len().cmp(&y.len()).then_with(|| x.cmp(y))
        } else {
            let order = a[i].to_ascii_lowercase().cmp(&b[j].to_ascii_lowercase());
            (i, j) = (i + 1, j + 1);
            order
        };
        if order != Ordering::Equal {
            return order;
        }
    }
    (a.len() - i).cmp(&(b.len() - j))
}

/// Deterministic synthetic findings for previews and scale tests. Paths live under a
/// fictional home and are never acted on (demo mode disables actions).
pub fn synthetic(count: usize) -> Vec<Finding> {
    let modules = ["large", "node", "artifacts", "duplicates", "downloads"];
    let mut seed = 0x2545_F491_4F6C_DD1Du64;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    (0..count)
        .map(|i| {
            let module = modules[i % modules.len()];
            let path = format!(
                "/Users/demo/Projects/project-{:04}/item-{i:07}.bin",
                i / 1_000
            );
            let mut f = Finding::new(
                module,
                &path,
                &format!("item-{i:07}.bin"),
                Resource::File {
                    file: FileIdentity {
                        path: path.clone(),
                        device: 1,
                        inode: i as u64 + 1,
                        modified_seconds: 0,
                        modified_nanos: 0,
                        tree_signature: None,
                    },
                },
                "Synthetic preview item.",
            );
            let bytes = next() % 5_000_000_000;
            f.bytes = Some(bytes);
            f.allocated_bytes = Some(bytes);
            f.modified_at = Some(1_600_000_000.0 + (next() % 150_000_000) as f64);
            f.actions = vec![ActionKind::Trash];
            if i % 17 == 0 {
                f.blocked_reason = Some("Synthetic blocked item.".into());
            }
            f
        })
        .collect()
}
