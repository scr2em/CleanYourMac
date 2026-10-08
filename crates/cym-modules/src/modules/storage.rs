use super::{add_files, descriptor, flush, Candidate, LastUsed, ScanModule};
use crate::{model::*, policy, ports::*, services::Services};
use rayon::prelude::*;
use std::{
    collections::{HashMap, HashSet},
    path::Path,
};

pub struct StorageModule;
impl ScanModule for StorageModule {
    fn descriptor(&self) -> ModuleDescriptor {
        descriptor(
            "storage",
            "Storage Explorer",
            "Storage",
            "internaldrive",
            "Explore folder sizes and find room to work.",
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
        let mut warnings = vec![];
        let mut candidates = vec![];
        let library = format!("{}/Library", policy::home());
        for root in s.roots(&c.roots) {
            if c.excludes(&root) || policy::system_excluded(&root) {
                continue;
            }
            candidates.extend(
                s.children(&root, &mut warnings)
                    .into_iter()
                    .filter(|e| {
                        c.allows(e.path())
                            && !policy::system_excluded(e.path())
                            && !finder_file(e.name())
                    })
                    .map(|e| {
                        // Anything outside the protected locations can be moved to Trash
                        // after review; nothing here is a cleanup recommendation. The
                        // Library's own folders (Application Support, Containers, Mail, …)
                        // hold macOS and app data as a whole, so only their contents are.
                        let blocked = if policy::protected(e.path()) {
                            Some("Protected location; open it to inspect what is inside.")
                        } else if Path::new(e.path())
                            .parent()
                            .is_some_and(|p| p.to_string_lossy().eq_ignore_ascii_case(&library))
                        {
                            Some("Holds macOS and app data as a whole; open it to inspect what is inside.")
                        } else {
                            None
                        };
                        Candidate::new(
                            e,
                            "Part of your storage, not a cleanup recommendation. Inspect it before moving it to Trash.",
                            if blocked.is_some() { vec![] } else { vec![ActionKind::Trash] },
                            Risk::Review,
                        )
                        .last_used(LastUsed::Spotlight)
                        .blocked(blocked)
                    }),
            );
        }
        flush(sink, &mut warnings);
        add_files(s, sink, "storage", candidates, k)
    }
}

pub struct LargeFilesModule {
    pub minimum_bytes: u64,
}
impl Default for LargeFilesModule {
    fn default() -> Self {
        Self {
            minimum_bytes: 100_000_000,
        }
    }
}
impl ScanModule for LargeFilesModule {
    fn descriptor(&self) -> ModuleDescriptor {
        descriptor(
            "large",
            "Large Files",
            "Storage",
            "doc",
            "Review files at least 100 MB, with size and date filters.",
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
        let mut warnings = vec![];
        let project_folders = super::developer::ProjectFolders::default();
        let result = s.walk(c, k, &mut warnings, &mut |e| {
            // A file inside build output or installed packages is part of that build; Build
            // Artifacts and Dependencies offer the whole folder instead.
            if e.directory {
                return Ok(!project_folders.contains(s, e.path()));
            }
            if e.regular && e.bytes >= self.minimum_bytes {
                let mut f = Finding::new(
                    "large",
                    e.path(),
                    e.name(),
                    Resource::File {
                        file: e.identity.clone(),
                    },
                    "Large personal file. Size or age alone does not make it disposable; inspect its contents before removal.",
                );
                f.bytes = Some(e.bytes);
                f.allocated_bytes = Some(e.allocated);
                f.modified_at = Some(e.modified());
                f.last_used_at = s.usage.last_used(e.path());
                if app_data(e.path(), &policy::home()) {
                    f.blocked_reason = Some("Part of an app's or developer tool's data, such as a virtual machine disk, a model, a backup or a database. Remove it with that app or tool.".into());
                } else if !policy::protected(e.path()) {
                    f.actions.push(ActionKind::Trash);
                }
                sink.finding(f);
            }
            Ok(true)
        });
        flush(sink, &mut warnings);
        result
    }
    /// A file a repository tracks is part of a project, however large; refused as for
    /// duplicates.
    fn preflight(&self, s: &Services, f: &Finding, _: ActionKind, k: &ScanControl) -> Result<()> {
        let path = f.resource.path().ok_or("This item has no file path.")?;
        super::developer::untracked(s, path, k)
    }
}

/// Exact duplicates found in stages: equal size, then an equal digest of the first
/// `prefix_bytes`, then an equal full digest. Each stage hashes in parallel, so most
/// non-duplicates are rejected after reading only their beginning.
pub struct DuplicatesModule {
    pub minimum_bytes: u64,
    pub prefix_bytes: u64,
}
impl Default for DuplicatesModule {
    fn default() -> Self {
        Self {
            minimum_bytes: 4_096,
            prefix_bytes: 65_536,
        }
    }
}
/// Groups entries by key and keeps only groups that still contain more than one file.
fn regroup<K: std::hash::Hash + Eq>(rows: Vec<(K, Entry)>) -> Vec<(K, Vec<Entry>)> {
    let mut groups: HashMap<K, Vec<Entry>> = HashMap::new();
    for (key, entry) in rows {
        groups.entry(key).or_default().push(entry);
    }
    groups.into_iter().filter(|(_, g)| g.len() > 1).collect()
}
impl DuplicatesModule {
    /// Hashes every candidate in parallel; failures become warnings and drop the file.
    fn stage(
        io: &rayon::ThreadPool,
        sink: &mut dyn Sink,
        k: &ScanControl,
        groups: Vec<Vec<Entry>>,
        hash: impl Fn(&Entry) -> Result<String> + Sync,
    ) -> Result<Vec<(String, Vec<Entry>)>> {
        let rows: Vec<_> = groups
            .into_iter()
            .enumerate()
            .flat_map(|(group, entries)| entries.into_iter().map(move |e| (group, e)))
            .collect::<Vec<_>>();
        let rows: Vec<_> = io.install(|| {
            rows.into_par_iter()
                .map(|(group, e)| (group, hash(&e), e))
                .collect()
        });
        k.check()?;
        let mut keyed = vec![];
        for (group, digest, e) in rows {
            match digest {
                Ok(digest) => keyed.push(((group, digest), e)),
                Err(error) => {
                    sink.warning(format!("Skipped duplicate candidate {}: {error}", e.path()))
                }
            }
        }
        Ok(regroup(keyed)
            .into_iter()
            .map(|((_, digest), entries)| (digest, entries))
            .collect())
    }
}
impl ScanModule for DuplicatesModule {
    fn descriptor(&self) -> ModuleDescriptor {
        descriptor(
            "duplicates",
            "Exact Duplicates",
            "Storage",
            "doc.on.doc",
            "Verified matching files. Build and dependency folders are skipped.",
            true,
        )
        // Reading every candidate file is slow; it runs only when the user asks for it.
        .explicit()
    }
    fn scan(
        &self,
        s: &Services,
        c: &ScanContext,
        k: &ScanControl,
        sink: &mut dyn Sink,
    ) -> Result<()> {
        let mut warnings = vec![];
        let mut sizes = vec![];
        let mut context = c.clone();
        context.roots.retain(|r| !policy::duplicate_excluded(r));
        let walked = s.walk(&context, k, &mut warnings, &mut |e| {
            if e.directory {
                // Roots were checked in full above, and an ignored folder is never entered,
                // so only the new folder's own name needs checking.
                // Hidden folders (the Trash, tool data, settings) and Library folders hold
                // app and system data, not documents the user manages.
                let name = e.name();
                return Ok(!(policy::duplicate_ignored_name(name)
                    || name.starts_with('.')
                    || name.eq_ignore_ascii_case("Library")));
            }
            if e.regular && e.bytes >= self.minimum_bytes {
                sizes.push((e.bytes, e.clone()));
            }
            Ok(true)
        });
        flush(sink, &mut warnings);
        walked?;
        // Hard links to one inode are the same data, not duplicates.
        let groups: Vec<Vec<Entry>> = regroup(sizes)
            .into_iter()
            .map(|(_, g)| {
                let mut inodes = HashSet::new();
                g.into_iter()
                    .filter(|e| inodes.insert((e.identity.device, e.identity.inode)))
                    .collect::<Vec<_>>()
            })
            .filter(|g| g.len() > 1)
            .collect();
        let count: usize = groups.iter().map(Vec::len).sum();
        sink.progress(format!("Comparing {count} candidates"));
        let prefix = self.prefix_bytes;
        let groups = Self::stage(&s.io, sink, k, groups, |e| {
            if e.bytes > prefix {
                s.hasher.hash(e.path(), Some(prefix), k)
            } else {
                Ok(String::new())
            }
        })?;
        let groups: Vec<_> = groups.into_iter().map(|(_, g)| g).collect();
        let count: usize = groups.iter().map(Vec::len).sum();
        sink.progress(format!("Verifying {count} files"));
        let mut groups = Self::stage(&s.io, sink, k, groups, |e| s.verified_hash(e.path(), k))?;
        // The copy kept is the one in the most deliberate place: Documents, Desktop or a
        // media folder before other folders, Downloads, and last caches or the Trash.
        let home = policy::home();
        for (_, group) in &mut groups {
            group.sort_by(|a, b| {
                (keep_rank(a.path(), &home), a.path()).cmp(&(keep_rank(b.path(), &home), b.path()))
            });
        }
        groups.sort_by(|a, b| a.1[0].path().cmp(b.1[0].path()));
        for (hash, group) in groups {
            let original = group[0].path().to_owned();
            let disposable = keep_rank(&original, &home) == DISPOSABLE;
            let count = group.len();
            for e in group {
                let preserved = e.path() == original;
                let Ok(fresh) = s.entry(e.path()) else {
                    continue;
                };
                let mut f = Finding::new(
                    "duplicates",
                    e.path(),
                    e.name(),
                    Resource::File {
                        file: fresh.identity,
                    },
                    if preserved {
                        "This verified original is preserved; other copies can be reviewed."
                    } else {
                        "Contents match the preserved original. Both files are verified again before removal."
                    },
                );
                f.bytes = Some(e.bytes);
                f.allocated_bytes = Some(e.allocated);
                f.modified_at = Some(e.modified());
                f.last_used_at = s.usage.last_used(e.path());
                f.details = vec![
                    detail("Preserved original", original.clone()),
                    detail("Group files", count.to_string()),
                    detail(s.hasher.algorithm(), hash.clone()),
                ];
                if preserved {
                    f.blocked_reason = Some("One original is preserved in each group.".into());
                    f.badge = Some("Original".into());
                } else if disposable {
                    f.blocked_reason = Some(KEPT_IN_DISPOSABLE.into());
                    f.badge = Some("Duplicate".into());
                } else {
                    f.actions.push(ActionKind::Trash);
                    f.badge = Some("Duplicate".into());
                }
                sink.finding(f);
            }
        }
        Ok(())
    }
    fn preflight(&self, s: &Services, f: &Finding, _: ActionKind, k: &ScanControl) -> Result<()> {
        let changed = || "Duplicate content or preserved original changed. Scan again.".to_owned();
        let path = f.resource.path().ok_or_else(changed)?;
        let original = f.value("Preserved original").ok_or_else(changed)?;
        let expected = f.value(s.hasher.algorithm()).ok_or_else(changed)?;
        if original == path || expected.is_empty() {
            return Err(changed());
        }
        if keep_rank(original, &policy::home()) == DISPOSABLE {
            return Err(KEPT_IN_DISPOSABLE.into());
        }
        // A copy that Git tracks is part of a project, whatever else matches it.
        super::developer::untracked(s, path, k)?;
        let (a, b) = rayon::join(|| s.verified_hash(original, k), || s.verified_hash(path, k));
        if a.ok().as_deref() != Some(expected) || b.ok().as_deref() != Some(expected) {
            return Err(changed());
        }
        Ok(())
    }
}

/// Files Finder keeps for itself: folder view settings and a folder's localized name, which
/// removal would change. Never listed.
fn finder_file(name: &str) -> bool {
    name == ".DS_Store" || name == ".localized"
}

/// Whether a file belongs to an app or tool: anything in the Library folder or in a hidden
/// folder of the home folder (`~/.ollama`, `~/.colima`, `~/.android`, …).
pub fn app_data(path: &str, home: &str) -> bool {
    path.strip_prefix(home).is_some_and(|rest| {
        let mut parts = rest.split('/').filter(|c| !c.is_empty());
        let first = parts.next().unwrap_or_default();
        first.eq_ignore_ascii_case("Library")
            || rest.split('/').any(|c| c.len() > 1 && c.starts_with('.'))
    })
}
const DISPOSABLE: u8 = 3;
const KEPT_IN_DISPOSABLE: &str =
    "The kept copy is in the Trash or a cache, where other actions can remove it. Keep this copy.";
/// How deliberately a path is kept; lower ranks are kept over higher ones.
pub fn keep_rank(path: &str, home: &str) -> u8 {
    let lower = path.to_ascii_lowercase();
    // Hidden folders in the home folder hold tool data and settings.
    let hidden = path
        .strip_prefix(home)
        .is_some_and(|rest| rest.split('/').any(|c| c.len() > 1 && c.starts_with('.')));
    if hidden
        || lower.contains("/caches/")
        || lower.contains("/cache/")
        || lower.contains("/tmp/")
        || lower.contains("/.trash")
    {
        return DISPOSABLE;
    }
    let under = |folder: &str| policy::contains_folded(path, &format!("{home}/{folder}"));
    if ["Documents", "Desktop", "Pictures", "Movies", "Music"]
        .iter()
        .any(|f| under(f))
    {
        0
    } else if under("Downloads") {
        2
    } else {
        1
    }
}

/// Lists the immediate contents of one known folder in the user's home.
pub struct FolderModule {
    pub descriptor: ModuleDescriptor,
    pub home_relative: &'static str,
    pub action: ActionKind,
    pub risk: Risk,
    pub reason: &'static str,
}
impl FolderModule {
    pub fn downloads() -> Self {
        Self {
            descriptor: descriptor(
                "downloads",
                "Downloads",
                "Storage",
                "arrow.down.circle",
                "Downloaded files and installers, ready for your review.",
                false,
            ),
            home_relative: "Downloads",
            action: ActionKind::Trash,
            risk: Risk::Review,
            reason: "Downloaded file or folder. Inspect whether it is still needed before moving it to Trash.",
        }
    }
    pub fn trash() -> Self {
        Self {
            descriptor: descriptor(
                "trash",
                "Trash",
                "Storage",
                "trash",
                "Review exact items before permanent removal.",
                false,
            ),
            home_relative: ".Trash",
            action: ActionKind::EmptyTrash,
            risk: Risk::Permanent,
            reason: "Already in Trash. Permanent removal cannot be undone.",
        }
    }
    pub fn folder(&self) -> String {
        format!("{}/{}", policy::home(), self.home_relative)
    }
}
impl ScanModule for FolderModule {
    fn descriptor(&self) -> ModuleDescriptor {
        self.descriptor.clone()
    }
    fn action_roots(&self) -> Vec<String> {
        vec![self.folder()]
    }
    fn scan(
        &self,
        s: &Services,
        c: &ScanContext,
        k: &ScanControl,
        sink: &mut dyn Sink,
    ) -> Result<()> {
        let folder = self.folder();
        if c.excludes(&folder) {
            return Ok(());
        }
        let mut warnings = vec![];
        let candidates = s
            .children(&folder, &mut warnings)
            .into_iter()
            .filter(|e| c.allows(e.path()) && !finder_file(e.name()))
            .map(|e| {
                Candidate::new(e, self.reason, vec![self.action], self.risk)
                    .last_used(LastUsed::Spotlight)
            })
            .collect();
        flush(sink, &mut warnings);
        add_files(s, sink, &self.descriptor.id, candidates, k)
    }
}

#[cfg(test)]
mod keep_tests {
    use super::*;

    #[test]
    fn the_deliberate_copy_is_kept() {
        let home = "/Users/me";
        let rank = |p: &str| keep_rank(p, home);
        assert_eq!(rank("/Users/me/Documents/Tax.pdf"), 0);
        assert_eq!(rank("/Users/me/Projects/site/logo.png"), 1);
        assert_eq!(rank("/Users/me/Downloads/Tax.pdf"), 2);
        for disposable in [
            "/Users/me/.Trash/Tax.pdf",
            "/Users/me/Library/Caches/app/holiday.jpg",
            "/Users/me/.cache/x/model.bin",
            "/Volumes/Disk/.Trashes/501/a.pdf",
        ] {
            assert_eq!(rank(disposable), DISPOSABLE, "{disposable}");
        }
        // Alphabetical order no longer decides: ~/.Trash sorts first but is never kept.
        let mut group = ["/Users/me/.Trash/Tax.pdf", "/Users/me/Documents/Tax.pdf"];
        group.sort_by_key(|p| (rank(p), *p));
        assert_eq!(group[0], "/Users/me/Documents/Tax.pdf");
    }
}
