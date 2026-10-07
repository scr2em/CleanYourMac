use super::{add_file, descriptor, flush, ScanModule};
use crate::{model::*, policy, ports::*, services::Services};
use std::collections::{HashMap, HashSet};

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
        for root in s.roots(&c.roots) {
            if c.excludes(&root) || policy::system_excluded(&root) {
                continue;
            }
            for e in s.children(&root, &mut warnings) {
                if !c.allows(e.path()) || policy::system_excluded(e.path()) {
                    continue;
                }
                add_file(
                    s,
                    sink,
                    &e,
                    "storage",
                    None,
                    "Storage inventory, not a cleanup recommendation. Inspect this item in Finder.",
                    vec![],
                    Risk::Review,
                    k,
                )?;
            }
        }
        flush(sink, &mut warnings);
        Ok(())
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
        let result = s.walk(c, k, &mut warnings, &mut |e| {
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
                if !policy::protected(e.path()) {
                    f.actions.push(ActionKind::Trash);
                }
                sink.finding(f);
            }
            Ok(true)
        });
        flush(sink, &mut warnings);
        result
    }
}

pub struct DuplicatesModule {
    pub minimum_bytes: u64,
}
impl Default for DuplicatesModule {
    fn default() -> Self {
        Self {
            minimum_bytes: 4_096,
        }
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
    }
    fn scan(
        &self,
        s: &Services,
        c: &ScanContext,
        k: &ScanControl,
        sink: &mut dyn Sink,
    ) -> Result<()> {
        let mut warnings = vec![];
        let mut sizes: HashMap<u64, Vec<Entry>> = HashMap::new();
        let mut context = c.clone();
        context.roots.retain(|r| !policy::duplicate_excluded(r));
        let walked = s.walk(&context, k, &mut warnings, &mut |e| {
            if e.directory {
                return Ok(!policy::duplicate_excluded(e.path()));
            }
            if e.regular && e.bytes >= self.minimum_bytes {
                sizes.entry(e.bytes).or_default().push(e.clone());
            }
            Ok(true)
        });
        flush(sink, &mut warnings);
        walked?;
        let mut groups: Vec<_> = sizes.into_values().filter(|v| v.len() > 1).collect();
        groups.sort_by(|a, b| a[0].path().cmp(b[0].path()));
        for entries in groups {
            let mut hashes: HashMap<String, Vec<Entry>> = HashMap::new();
            let mut inodes = HashSet::new();
            for e in entries {
                k.check()?;
                if !inodes.insert((e.identity.device, e.identity.inode)) {
                    continue;
                }
                sink.progress(format!("Verifying {}", e.name()));
                match s.verified_hash(e.path(), k) {
                    Ok(hash) => hashes.entry(hash).or_default().push(e),
                    Err(error) => {
                        k.check()?;
                        sink.warning(format!("Skipped duplicate candidate {}: {error}", e.path()))
                    }
                }
            }
            for (hash, mut group) in hashes.into_iter().filter(|(_, v)| v.len() > 1) {
                group.sort_by(|a, b| a.path().cmp(b.path()));
                let original = group[0].path().to_owned();
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
                    f.details = vec![
                        detail("Preserved original", original.clone()),
                        detail("Group files", count.to_string()),
                        detail(s.hasher.algorithm(), hash.clone()),
                    ];
                    if preserved {
                        f.blocked_reason = Some("One original is preserved in each group.".into());
                        f.badge = Some("Original".into());
                    } else {
                        f.actions.push(ActionKind::Trash);
                        f.badge = Some("Duplicate".into());
                    }
                    sink.finding(f);
                }
            }
        }
        Ok(())
    }
    fn preflight(&self, s: &Services, f: &Finding, _: ActionKind, k: &ScanControl) -> Result<()> {
        let changed = || "Duplicate content or preserved original changed. Scan again.".to_owned();
        let path = f.resource.path().ok_or_else(changed)?;
        let original = f.value("Preserved original").ok_or_else(changed)?;
        let expected = f.value(s.hasher.algorithm()).ok_or_else(changed)?;
        if original == path
            || s.verified_hash(original, k).ok().as_deref() != Some(expected)
            || s.verified_hash(path, k).ok().as_deref() != Some(expected)
        {
            return Err(changed());
        }
        Ok(())
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
        for e in s.children(&folder, &mut warnings) {
            if !c.allows(e.path()) {
                continue;
            }
            add_file(
                s,
                sink,
                &e,
                &self.descriptor.id,
                None,
                self.reason,
                vec![self.action],
                self.risk,
                k,
            )?;
        }
        flush(sink, &mut warnings);
        Ok(())
    }
}
