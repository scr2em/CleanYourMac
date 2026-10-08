//! Turns scan results into a short list of one-click fixes. Rules are a table, conservative
//! by default: only eligible rows, only what rebuilds or is verified, and for project
//! folders only projects left alone for a while.
use crate::{analytics::analytics_of, model::*};
use serde::Serialize;

/// One recommended fix: the rows it covers and what applying it frees.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Recommendation {
    pub id: String,
    pub module_id: String,
    pub title: String,
    pub detail: String,
    pub action: ActionKind,
    pub risk: Risk,
    /// Rows the fix applies to.
    pub count: usize,
    /// Bytes freed on disk, with folders inside another selected folder counted once.
    pub bytes: u64,
    /// The most common ecosystem logo among the rows, for the card's icon.
    pub brand: Option<String>,
    pub ids: Vec<String>,
}

/// A recommendation rule: which rows of a module qualify.
pub struct Rule {
    pub id: &'static str,
    pub module: &'static str,
    pub title: &'static str,
    pub detail: &'static str,
    pub action: ActionKind,
    /// Only rows at these risks qualify.
    pub risks: &'static [Risk],
    /// Only rows whose last use (or modification) is at least this many days old qualify.
    pub idle_days: Option<u32>,
    /// Only rows that belong to a project qualify, which leaves out shared package stores
    /// another rule already offers.
    pub project_only: bool,
}

/// The default rules, in the order fixes are offered when sizes tie.
pub const RULES: &[Rule] = &[
    Rule {
        id: "idle-dependencies",
        module: "node",
        title: "Dependencies of inactive projects",
        detail: "node_modules, Python environments, Pods and other installed packages of projects untouched for 30 days or more. Reinstall them with the project's package manager when you return.",
        action: ActionKind::Trash,
        risks: &[Risk::Rebuild],
        idle_days: Some(30),
        project_only: true,
    },
    Rule {
        id: "idle-build-output",
        module: "artifacts",
        title: "Build output of inactive projects",
        detail: "Generated folders such as .next, target and build in projects untouched for 30 days or more. The next build recreates them.",
        action: ActionKind::Trash,
        risks: &[Risk::Rebuild],
        idle_days: Some(30),
        project_only: true,
    },
    Rule {
        id: "rebuildable-caches",
        module: "caches",
        title: "Caches that rebuild themselves",
        detail: "Developer tool, package and app caches, saved window state and other leftovers that are downloaded or regenerated when needed.",
        action: ActionKind::Trash,
        risks: &[Risk::Rebuild],
        idle_days: None,
        project_only: false,
    },
    Rule {
        id: "xcode-build-data",
        module: "xcode",
        title: "Xcode build data",
        detail: "DerivedData and other Xcode data that rebuilds with the next build.",
        action: ActionKind::Trash,
        risks: &[Risk::Rebuild],
        idle_days: None,
        project_only: false,
    },
    Rule {
        id: "duplicate-copies",
        module: "duplicates",
        title: "Duplicate copies",
        detail: "Exact copies of files you keep elsewhere. One original of each stays, and every file is verified again before removal.",
        action: ActionKind::Trash,
        risks: &[Risk::Review, Risk::Rebuild],
        idle_days: None,
        project_only: false,
    },
    Rule {
        id: "empty-trash",
        module: "trash",
        title: "Empty the Trash",
        detail: "Items you already moved to the Trash. This removes them permanently.",
        action: ActionKind::EmptyTrash,
        risks: &[Risk::Permanent, Risk::Review],
        idle_days: None,
        project_only: false,
    },
];

/// Fixes worth at least `minimum_bytes`, largest first. `module_rows` gives a module's rows;
/// `now` is Unix seconds.
pub fn recommendations<'a>(
    module_rows: &dyn Fn(&str) -> Vec<&'a Finding>,
    rules: &[Rule],
    now: f64,
    minimum_bytes: u64,
) -> Vec<Recommendation> {
    let mut out: Vec<Recommendation> = rules
        .iter()
        .filter_map(|rule| {
            let matching: Vec<&Finding> = module_rows(rule.module)
                .into_iter()
                .filter(|f| {
                    f.blocked_reason.is_none()
                        && f.actions.contains(&rule.action)
                        && rule.risks.contains(&f.risk)
                        && (!rule.project_only || f.value("Project").is_some())
                        && rule.idle_days.is_none_or(|days| {
                            let last = f.last_used_at.or(f.modified_at);
                            last.is_none_or(|t| now - t >= f64::from(days) * 86_400.0)
                        })
                })
                .collect();
            if matching.is_empty() {
                return None;
            }
            let bytes = analytics_of(matching.iter().copied(), false).reclaimable_bytes;
            // Emptying the Trash is offered whatever its size; other fixes must be worth it.
            if bytes < minimum_bytes && rule.action != ActionKind::EmptyTrash {
                return None;
            }
            Some(Recommendation {
                id: rule.id.into(),
                module_id: rule.module.into(),
                title: rule.title.into(),
                detail: rule.detail.into(),
                action: rule.action,
                risk: rule.risks[0],
                count: matching.len(),
                bytes,
                brand: most_common_brand(&matching),
                ids: matching.iter().map(|f| f.id.clone()).collect(),
            })
        })
        .collect();
    // Stable, so equal sizes keep the rules' order.
    out.sort_by_key(|r| std::cmp::Reverse(r.bytes));
    out
}

fn most_common_brand(rows: &[&Finding]) -> Option<String> {
    let mut counts: std::collections::BTreeMap<&str, usize> = Default::default();
    for brand in rows.iter().filter_map(|f| f.brand.as_deref()) {
        *counts.entry(brand).or_default() += 1;
    }
    counts
        .into_iter()
        .max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(a.0)))
        .map(|(brand, _)| brand.to_owned())
}
