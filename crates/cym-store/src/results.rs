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
        }
    }
}

/// An ordered snapshot of the rows matching a query, with its totals.
pub struct Snapshot {
    pub id: u64,
    pub generation: u64,
    rows: Vec<Arc<Finding>>,
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
    generation: AtomicU64,
    next_query: AtomicU64,
    snapshots: Mutex<VecDeque<Arc<Snapshot>>>,
    /// Totals depend on the filter, not the order, so re-sorting reuses them.
    summaries: Mutex<VecDeque<(SummaryKey, Analytics)>>,
    /// Fully sorted rows per (module, order), so filtering never re-sorts.
    orders: Mutex<VecDeque<(OrderKey, Ordered)>>,
}
type Ordered = Arc<Vec<Arc<Finding>>>;
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
        if let Ok(mut buckets) = self.buckets.write() {
            for module in modules {
                buckets.remove(module);
            }
        }
        self.changed();
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
            for f in rows {
                // Look up by borrowed name; allocate a key only for a module's first row.
                let bucket = match buckets.get_mut(&f.module_id) {
                    Some(bucket) => bucket,
                    None => buckets.entry(f.module_id.clone()).or_default(),
                };
                bucket.insert(Arc::new(f));
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
    pub fn get(&self, id: &str) -> Option<Arc<Finding>> {
        let buckets = self.buckets.read().ok()?;
        buckets
            .values()
            .find_map(|b| b.index.get(id).map(|&i| b.rows[i].clone()))
    }
    pub fn findings(&self, ids: &[String]) -> Vec<Finding> {
        let Ok(buckets) = self.buckets.read() else {
            return vec![];
        };
        ids.iter()
            .filter_map(|id| {
                buckets
                    .values()
                    .find_map(|b| b.index.get(id).map(|&i| b.rows[i].as_ref().clone()))
            })
            .collect()
    }

    /// Filters and sorts in parallel and keeps the snapshot for paging.
    pub fn query(&self, q: &Query) -> Arc<Snapshot> {
        let generation = self.generation();
        // Sort every row of the module once per store generation, then filter that order;
        // a linear, order-preserving pass is far cheaper than re-sorting on each keystroke.
        let ordered = self.ordered(q, generation);
        let needle = Needle::new(&q.search);
        let unfiltered =
            q.min_bytes == 0 && q.modified_before.is_none() && needle.unicode.is_empty();
        let rows: Vec<Arc<Finding>> = if unfiltered {
            ordered.as_ref().clone()
        } else {
            ordered
                .par_iter()
                .filter(|f| {
                    (q.min_bytes == 0 || f.bytes.unwrap_or(0) >= q.min_bytes)
                        && q.modified_before
                            .is_none_or(|t| f.modified_at.is_some_and(|m| m < t))
                        && needle.matches(f)
                })
                .cloned()
                .collect()
        };
        let key = SummaryKey {
            generation,
            module: q.module.clone(),
            search: q.search.trim().to_owned(),
            min_bytes: q.min_bytes,
            modified_before: q.modified_before.map(f64::to_bits),
        };
        let cached = self
            .summaries
            .lock()
            .ok()
            .and_then(|c| c.iter().find(|(k, _)| *k == key).map(|(_, a)| a.clone()));
        let summary = cached.unwrap_or_else(|| {
            let summary =
                analytics::analytics_of(rows.iter().map(|f| f.as_ref()), q.module.is_none());
            if let Ok(mut cache) = self.summaries.lock() {
                cache.push_back((key, summary.clone()));
                while cache.len() > 8 {
                    cache.pop_front();
                }
            }
            summary
        });
        let mut largest: Vec<&Arc<Finding>> = rows.iter().filter(|f| f.bytes.is_some()).collect();
        let top = largest.len().min(5);
        if top > 0 {
            largest.select_nth_unstable_by(top - 1, |a, b| b.bytes.cmp(&a.bytes));
            largest.truncate(top);
            largest.sort_by(|a, b| b.bytes.cmp(&a.bytes).then_with(|| a.id.cmp(&b.id)));
        }
        let snapshot = Arc::new(Snapshot {
            id: self.next_query.fetch_add(1, Atomic::Relaxed) + 1,
            generation,
            largest: largest.into_iter().map(|f| Row::from(f.as_ref())).collect(),
            max_cpu: rows
                .par_iter()
                .filter_map(|f| f.cpu_percent)
                .max_by(|a, b| a.total_cmp(b)),
            eligible: rows.par_iter().filter(|f| eligible(f)).count(),
            summary,
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
        let mut rows: Vec<Arc<Finding>> = match self.buckets.read() {
            Ok(buckets) => match &q.module {
                Some(m) => buckets.get(m).map(|b| b.rows.clone()).unwrap_or_default(),
                None => buckets
                    .values()
                    .flat_map(|b| b.rows.iter().cloned())
                    .collect(),
            },
            Err(_) => vec![],
        };
        let compare = |a: &Arc<Finding>, b: &Arc<Finding>| {
            if q.sort == SortKey::LastUsed {
                let order = match (a.last_used_at, b.last_used_at) {
                    (Some(x), Some(y)) => {
                        let order = x.total_cmp(&y);
                        if q.ascending {
                            order
                        } else {
                            order.reverse()
                        }
                    }
                    (Some(_), None) => Ordering::Less,
                    (None, Some(_)) => Ordering::Greater,
                    (None, None) => Ordering::Equal,
                };
                return order.then_with(|| a.id.cmp(&b.id));
            }
            let order = match q.sort {
                SortKey::LastUsed => Ordering::Equal,
                SortKey::Size => a
                    .bytes
                    .or(a.memory_bytes)
                    .unwrap_or(0)
                    .cmp(&b.bytes.or(b.memory_bytes).unwrap_or(0)),
                SortKey::Cpu => a
                    .cpu_percent
                    .unwrap_or(0.)
                    .total_cmp(&b.cpu_percent.unwrap_or(0.)),
                SortKey::Name => natural(&a.title, &b.title),
            }
            .then_with(|| a.id.cmp(&b.id));
            if q.ascending {
                order
            } else {
                order.reverse()
            }
        };
        rows.par_sort_unstable_by(compare);
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
    pub fn snapshot(&self, id: u64) -> Option<Arc<Snapshot>> {
        self.snapshots
            .lock()
            .ok()?
            .iter()
            .find(|s| s.id == id)
            .cloned()
    }

    pub fn selection(&self, ids: &[String], preview: usize) -> Selection {
        let unique: Vec<String> = {
            let mut seen = HashSet::new();
            ids.iter().filter(|id| seen.insert(*id)).cloned().collect()
        };
        let rows = self.findings(&unique);
        let normalized = crate::policy::normalized_selection(&rows);
        let mut actions: Option<Vec<ActionKind>> = None;
        for f in &rows {
            let eligible: Vec<ActionKind> = if f.blocked_reason.is_some() {
                vec![]
            } else {
                f.actions.clone()
            };
            actions = Some(match actions {
                None => eligible,
                Some(current) => current
                    .into_iter()
                    .filter(|a| eligible.contains(a))
                    .collect(),
            });
        }
        Selection {
            count: rows.len(),
            bytes: normalized.iter().filter_map(|f| f.bytes).sum(),
            actions: actions.unwrap_or_default(),
            includes_processes: rows
                .iter()
                .any(|f| matches!(f.resource, Resource::Process { .. })),
            preview: rows.into_iter().take(preview).collect(),
        }
    }
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
