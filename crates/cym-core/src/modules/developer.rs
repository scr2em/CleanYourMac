use super::{add_files, descriptor, flush, Candidate, ScanModule};
use crate::{git::Git, model::*, orphans, policy, ports::*, services::Services};
use std::path::Path;

fn parent(path: &str) -> String {
    Path::new(path)
        .parent()
        .map(|p| p.to_string_lossy().into())
        .unwrap_or_default()
}
fn name(path: &str) -> &str {
    Path::new(path)
        .file_name()
        .and_then(|p| p.to_str())
        .unwrap_or(path)
}
fn active_project_tools(s: &Services, project: &str) -> Result<()> {
    let active = orphans::active_tools(s, Some(project));
    if active.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "Close active project tools first: {}",
            active[..active.len().min(3)].join(", ")
        ))
    }
}
/// Refuses to move generated output that Git tracks inside its repository.
fn untracked(s: &Services, path: &str, k: &ScanControl) -> Result<()> {
    let directory = parent(path);
    let mut ancestor = Some(Path::new(&directory));
    while let Some(folder) = ancestor {
        if s.exists(&folder.join(".git").to_string_lossy()) {
            if Git(s.commands.as_ref()).tracks(&directory, path, k)? {
                return Err("Artifact contains tracked files.".into());
            }
            return Ok(());
        }
        ancestor = folder.parent();
    }
    Ok(())
}

/// `node_modules` folders grouped by owning project. Traversal stops at each `node_modules`
/// and never descends to find nested dependency folders.
pub struct NodeModule {
    /// Package manager name and the lockfile that identifies it, in priority order.
    pub lockfiles: Vec<(&'static str, &'static str)>,
}
impl Default for NodeModule {
    fn default() -> Self {
        Self {
            lockfiles: vec![
                ("pnpm", "pnpm-lock.yaml"),
                ("Yarn", "yarn.lock"),
                ("Bun", "bun.lock"),
                ("Bun", "bun.lockb"),
                ("npm", "package-lock.json"),
            ],
        }
    }
}
impl ScanModule for NodeModule {
    fn descriptor(&self) -> ModuleDescriptor {
        descriptor(
            "node",
            "Node Dependencies",
            "Developer",
            "shippingbox",
            "Find node_modules by project without descending into dependencies.",
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
        let mut candidates = vec![];
        let mut context = c.clone();
        // A chosen root that is itself a node_modules folder is a candidate, not a place to search.
        context.roots.retain(|root| {
            if name(root) != "node_modules" {
                return true;
            }
            if c.allows(root) && !policy::system_excluded(root) {
                if let Some(e) = s.entry(root).ok().filter(|e| e.directory) {
                    candidates.push(e);
                }
            }
            false
        });
        let mut warnings = vec![];
        let walked = s.walk(&context, k, &mut warnings, &mut |e| {
            if e.directory && e.name() == "node_modules" {
                candidates.push(e.clone());
                return Ok(false);
            }
            Ok(true)
        });
        flush(sink, &mut warnings);
        walked?;
        let candidates = candidates
            .into_iter()
            .map(|e| {
                let project = parent(e.path());
                let manifest = s.is_file(&format!("{project}/package.json"));
                let manager = self
                    .lockfiles
                    .iter()
                    .find(|(_, lock)| s.is_file(&format!("{project}/{lock}")))
                    .map(|(n, _)| *n)
                    .unwrap_or("Unknown");
                Candidate::new(
                    e,
                    "Project dependencies can be reinstalled, but local patches or edits may be lost. Manifests and lockfiles are preserved.",
                    vec![ActionKind::Trash],
                    Risk::Rebuild,
                )
                .title(name(&project))
                .details(vec![
                    detail("Project", project.clone()),
                    detail("Package manager", manager),
                    detail("Artifact", "node_modules"),
                ])
                .blocked((!manifest).then_some(
                    "No owning package.json. This directory needs manual inspection.",
                ))
            })
            .collect();
        add_files(s, sink, "node", candidates, k)
    }
    fn preflight(&self, s: &Services, f: &Finding, _: ActionKind, k: &ScanControl) -> Result<()> {
        let path = f.resource.path().ok_or("Missing path")?;
        let project = parent(path);
        active_project_tools(s, &project)?;
        if !s.is_file(&format!("{project}/package.json")) {
            return Err("The owning package.json is no longer available.".into());
        }
        untracked(s, path, k)
    }
}

/// A generated folder name and the project files that prove what produced it.
pub struct ArtifactRule {
    pub names: &'static [&'static str],
    pub evidence: &'static [&'static str],
    pub label: &'static str,
}
pub struct ArtifactsModule {
    pub rules: Vec<ArtifactRule>,
}
impl Default for ArtifactsModule {
    fn default() -> Self {
        Self {
            rules: vec![
                ArtifactRule {
                    names: &[".next", ".nuxt", ".turbo", ".parcel-cache", "coverage"],
                    evidence: &["package.json"],
                    label: "JavaScript generated output",
                },
                ArtifactRule {
                    names: &[".build"],
                    evidence: &["Package.swift"],
                    label: "SwiftPM build output",
                },
                ArtifactRule {
                    names: &["target"],
                    evidence: &["Cargo.toml"],
                    label: "Cargo build output",
                },
                ArtifactRule {
                    names: &["build"],
                    evidence: &["build.gradle", "build.gradle.kts"],
                    label: "Gradle build output",
                },
                ArtifactRule {
                    names: &["__pycache__", ".venv", "venv"],
                    evidence: &["pyproject.toml", "requirements.txt", "setup.py"],
                    label: "Python generated environment or bytecode",
                },
            ],
        }
    }
}
impl ArtifactsModule {
    pub fn rule(&self, s: &Services, name: &str, parent: &str) -> Option<&'static str> {
        self.rules
            .iter()
            .find(|r| {
                r.names.contains(&name)
                    && r.evidence
                        .iter()
                        .any(|f| s.is_file(&format!("{parent}/{f}")))
            })
            .map(|r| r.label)
    }
}
impl ScanModule for ArtifactsModule {
    fn descriptor(&self) -> ModuleDescriptor {
        descriptor(
            "artifacts",
            "Build Artifacts",
            "Developer",
            "hammer",
            "Generated outputs with evidence from their owning projects.",
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
        let mut candidates = vec![];
        let mut warnings = vec![];
        let walked = s.walk(c, k, &mut warnings, &mut |e| {
            if e.name() == "node_modules" {
                return Ok(false);
            }
            if e.directory {
                if let Some(rule) = self.rule(s, e.name(), &parent(e.path())) {
                    candidates.push((e.clone(), rule));
                    return Ok(false);
                }
            }
            Ok(true)
        });
        flush(sink, &mut warnings);
        walked?;
        let candidates = candidates
            .into_iter()
            .map(|(e, rule)| {
                Candidate::new(
                    e,
                    &format!("{rule}. Rebuilding requires the project's tools and dependencies."),
                    vec![ActionKind::Trash],
                    Risk::Rebuild,
                )
            })
            .collect();
        add_files(s, sink, "artifacts", candidates, k)
    }
    fn preflight(&self, s: &Services, f: &Finding, _: ActionKind, k: &ScanControl) -> Result<()> {
        let path = f.resource.path().ok_or("Missing path")?;
        active_project_tools(s, &parent(path))?;
        untracked(s, path, k)
    }
}

pub struct XcodeModule;
impl XcodeModule {
    pub fn roots() -> Vec<String> {
        ["DerivedData", "iOS DeviceSupport", "Archives"]
            .iter()
            .map(|s| format!("{}/Library/Developer/Xcode/{s}", policy::home()))
            .collect()
    }
}
impl ScanModule for XcodeModule {
    fn descriptor(&self) -> ModuleDescriptor {
        descriptor(
            "xcode",
            "Xcode Data",
            "Developer",
            "wrench.and.screwdriver",
            "Build and device-support data. Archives remain protected.",
            false,
        )
    }
    fn action_roots(&self) -> Vec<String> {
        Self::roots()
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
        for root in Self::roots() {
            if !s.is_dir(&root) || c.excludes(&root) {
                continue;
            }
            let archive = root.ends_with("/Archives");
            for e in s.children(&root, &mut warnings) {
                if !c.allows(e.path()) {
                    continue;
                }
                candidates.push(if archive {
                    Candidate::new(
                        e,
                        "Archives may contain irreplaceable release builds and dSYMs.",
                        vec![],
                        Risk::Review,
                    )
                } else {
                    Candidate::new(
                        e,
                        "Xcode regenerates this build or device-support data when needed.",
                        vec![ActionKind::Trash],
                        Risk::Rebuild,
                    )
                });
            }
        }
        flush(sink, &mut warnings);
        add_files(s, sink, "xcode", candidates, k)
    }
    fn preflight(&self, s: &Services, _: &Finding, _: ActionKind, _: &ScanControl) -> Result<()> {
        let running = s
            .apps
            .running()?
            .iter()
            .any(|a| a.bundle_id == "com.apple.dt.Xcode");
        let building = orphans::active_tools(s, None)
            .iter()
            .any(|t| t.starts_with("xcodebuild ") || t.starts_with("swift-frontend "));
        if running || building {
            return Err("Quit Xcode and stop builds before removing its data.".into());
        }
        Ok(())
    }
}

pub struct CachesModule {
    /// Display name and home-relative location of each known shared cache.
    pub locations: Vec<(&'static str, &'static str)>,
}
impl Default for CachesModule {
    fn default() -> Self {
        Self {
            locations: vec![
                ("npm", ".npm/_cacache"),
                ("Yarn", "Library/Caches/Yarn"),
                ("pnpm", "Library/pnpm/store"),
                ("pnpm cache", "Library/Caches/pnpm"),
                ("Bun", ".bun/install/cache"),
                ("Homebrew", "Library/Caches/Homebrew"),
                ("pip", "Library/Caches/pip"),
                ("uv", ".cache/uv"),
                ("Poetry", "Library/Caches/pypoetry"),
                ("SwiftPM", "Library/Caches/org.swift.swiftpm"),
                ("CocoaPods", "Library/Caches/CocoaPods"),
                ("Cargo cache", ".cargo/registry/cache"),
                ("Go build cache", "Library/Caches/go-build"),
                ("Gradle caches", ".gradle/caches"),
            ],
        }
    }
}
impl CachesModule {
    fn logs() -> String {
        policy::home() + "/Library/Logs"
    }
}
impl ScanModule for CachesModule {
    fn descriptor(&self) -> ModuleDescriptor {
        descriptor(
            "caches",
            "Caches & Logs",
            "Developer",
            "archivebox",
            "Known package caches and logs, with rebuild costs made clear.",
            false,
        )
    }
    fn action_roots(&self) -> Vec<String> {
        self.locations
            .iter()
            .map(|(_, p)| format!("{}/{p}", policy::home()))
            .chain([policy::home() + "/Library/Caches", Self::logs()])
            .collect()
    }
    fn scan(
        &self,
        s: &Services,
        c: &ScanContext,
        k: &ScanControl,
        sink: &mut dyn Sink,
    ) -> Result<()> {
        let mut candidates = vec![];
        for (title, relative) in &self.locations {
            let path = format!("{}/{relative}", policy::home());
            if !s.exists(&path) || !c.allows(&path) {
                continue;
            }
            match s.entry(&path) {
                Ok(e) => candidates.push(
                    Candidate::new(
                        e,
                        "Cached downloads may be needed for offline installs. Close the owning tools before moving this cache to Trash.",
                        vec![ActionKind::Trash],
                        Risk::Rebuild,
                    )
                    .title(*title),
                ),
                Err(e) => sink.warning(e),
            }
        }
        let logs = Self::logs();
        if s.is_dir(&logs) && !c.excludes(&logs) {
            let mut warnings = vec![];
            for e in s.children(&logs, &mut warnings) {
                if c.allows(e.path()) {
                    let title = format!("Logs · {}", e.name());
                    candidates.push(
                        Candidate::new(
                            e,
                            "Logs can help diagnose problems. Review before removal.",
                            vec![ActionKind::Trash],
                            Risk::Review,
                        )
                        .title(title),
                    );
                }
            }
            flush(sink, &mut warnings);
        }
        add_files(s, sink, "caches", candidates, k)
    }
    fn preflight(&self, s: &Services, _: &Finding, _: ActionKind, _: &ScanControl) -> Result<()> {
        let active = orphans::active_tools(s, None);
        if active.is_empty() {
            Ok(())
        } else {
            Err(format!(
                "Close active developer tools before clearing shared caches: {}",
                active[..active.len().min(3)].join(", ")
            ))
        }
    }
}
