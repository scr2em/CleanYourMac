//! macOS leftovers in the user's Library: app caches, saved window state, Mail downloads,
//! device software updates and old device backups. Caches & Logs lists them alongside the
//! developer caches.
use super::{glob, owners_closed, Candidate};
use crate::{model::*, policy, ports::*, services::Services};

/// A known kind of macOS leftover.
pub struct JunkLocation {
    pub name: &'static str,
    /// Relative to the home folder. A `*` matches one path component; the first one names
    /// the owning app's bundle identifier when `per_app` is set.
    pub path: &'static str,
    pub reason: &'static str,
    pub risk: Risk,
    /// The owning app must not be running when the item is removed.
    pub per_app: bool,
    /// The first `*` names an app's folder in Application Support rather than a bundle
    /// identifier; the app is matched by name.
    pub app_folder: bool,
}

/// Chromium caches that Electron apps (Slack, Discord, Figma, Notion and others) keep in
/// their Application Support folder: (subfolder, name).
const WEB_CACHES: &[(&str, &str)] = &[
    ("Library/Application Support/*/Cache", "Web cache"),
    ("Library/Application Support/*/Code Cache", "Script cache"),
    ("Library/Application Support/*/GPUCache", "GPU cache"),
    (
        "Library/Application Support/*/DawnGraphiteCache",
        "Graphics cache",
    ),
    (
        "Library/Application Support/*/DawnWebGPUCache",
        "Graphics cache",
    ),
    ("Library/Application Support/*/DawnCache", "Graphics cache"),
    (
        "Library/Application Support/*/Service Worker/CacheStorage",
        "Offline web cache",
    ),
];
/// A folder is an Electron or Chromium app's when it holds one of these.
const CHROMIUM_MARKERS: &[&str] = &["Code Cache", "GPUCache", "DawnGraphiteCache"];
/// Browsers, whose profiles are not app caches. Editors and other apps that AI Tools or Caches
/// & Logs list are left out through the folders those tools cover.
const WEB_CACHE_EXCEPT: &[&str] = &[
    "Google",
    "BraveSoftware",
    "Microsoft Edge",
    "Arc",
    "Vivaldi",
    "Chromium",
    "Firefox",
    "com.operasoftware.Opera",
];

/// The table of macOS leftover locations, relative to a home folder.
pub struct MacJunk {
    pub locations: Vec<JunkLocation>,
}
impl Default for MacJunk {
    fn default() -> Self {
        let mut locations = vec![
                JunkLocation {
                    name: "App cache",
                    path: "Library/Caches/*",
                    reason: "Apps recreate their caches as needed; the app may start a little slower once.",
                    risk: Risk::Rebuild,
                    per_app: true,
                    app_folder: false,
                },
                JunkLocation {
                    name: "Sandboxed app cache",
                    path: "Library/Containers/*/Data/Library/Caches",
                    reason: "Apps recreate their caches as needed; the app may start a little slower once.",
                    risk: Risk::Rebuild,
                    per_app: true,
                    app_folder: false,
                },
                JunkLocation {
                    name: "Saved window state",
                    path: "Library/Saved Application State/*",
                    reason: "Lets an app reopen its windows where you left them; it opens fresh windows instead.",
                    risk: Risk::Rebuild,
                    per_app: true,
                    app_folder: false,
                },
                JunkLocation {
                    name: "Mail downloads",
                    path: "Library/Containers/com.apple.mail/Data/Library/Mail Downloads",
                    reason: "Copies of attachments you opened in Mail; the originals stay in your mail. An attachment you edited in place is saved only here.",
                    risk: Risk::Review,
                    per_app: false,
                    app_folder: false,
                },
                JunkLocation {
                    name: "iPhone software updates",
                    path: "Library/iTunes/iPhone Software Updates",
                    reason: "Device update files, downloaded again when a device needs them.",
                    risk: Risk::Rebuild,
                    per_app: false,
                    app_folder: false,
                },
                JunkLocation {
                    name: "iPad software updates",
                    path: "Library/iTunes/iPad Software Updates",
                    reason: "Device update files, downloaded again when a device needs them.",
                    risk: Risk::Rebuild,
                    per_app: false,
                    app_folder: false,
                },
                JunkLocation {
                    name: "Device backup",
                    path: "Library/Application Support/MobileSync/Backup/*",
                    reason: "A local backup of an iPhone or iPad. Remove it only if you have a newer backup, here or in iCloud.",
                    risk: Risk::Review,
                    per_app: false,
                    app_folder: false,
                },
            ];
        locations.extend(Self::web_caches());
        Self { locations }
    }
}
impl MacJunk {
    /// The Chromium cache folders of Electron apps, one location per kind of cache.
    pub fn web_caches() -> Vec<JunkLocation> {
        WEB_CACHES
            .iter()
            .map(|(path, name)| JunkLocation {
                name,
                path,
                reason: "A Chromium cache of an Electron app. The app downloads or compiles it again; it may start a little slower once.",
                risk: Risk::Rebuild,
                per_app: true,
                app_folder: true,
            })
            .collect()
    }
    /// Existing folders matching a location, with the first `*` match.
    fn expand(
        &self,
        s: &Services,
        home: &str,
        location: &JunkLocation,
    ) -> Vec<(Entry, Option<String>)> {
        let mut current: Vec<(String, Option<String>)> = vec![(home.to_owned(), None)];
        for part in location.path.split('/') {
            let mut next = vec![];
            for (folder, capture) in current {
                if part.contains('*') {
                    for e in s.children(&folder, &mut vec![]) {
                        if e.directory && !e.symlink && glob(part, e.name()).is_some() {
                            let capture = capture.clone().or_else(|| Some(e.name().to_owned()));
                            next.push((e.identity.path, capture));
                        }
                    }
                } else {
                    next.push((format!("{folder}/{part}"), capture));
                }
            }
            current = next;
        }
        current
            .into_iter()
            .filter_map(|(path, capture)| {
                let e = s.entry(&path).ok().filter(|e| e.directory)?;
                Some((e, capture))
            })
            .collect()
    }
    /// The location and first `*` match of an existing path.
    fn locate(&self, home: &str, path: &str) -> Option<(&JunkLocation, Option<String>)> {
        let relative: Vec<&str> = policy::home_relative(path, home)?.split('/').collect();
        self.locations.iter().find_map(|l| {
            let parts: Vec<&str> = l.path.split('/').collect();
            if parts.len() != relative.len() {
                return None;
            }
            let mut capture = None;
            for (pattern, name) in parts.iter().zip(&relative) {
                glob(pattern, name)?;
                if pattern.contains('*') && capture.is_none() {
                    capture = Some((*name).to_owned());
                }
            }
            Some((l, capture))
        })
    }
    /// The folders actions may touch: each location up to its first `*`.
    pub fn action_roots(&self, home: &str) -> Vec<String> {
        self.locations
            .iter()
            .map(|l| {
                let fixed: Vec<&str> = l.path.split('/').take_while(|p| !p.contains('*')).collect();
                format!("{home}/{}", fixed.join("/"))
            })
            .collect()
    }
    /// Leftovers in `home`, leaving out folders in or around `covered` (reported elsewhere).
    pub(crate) fn candidates(
        &self,
        s: &Services,
        c: &ScanContext,
        k: &ScanControl,
        home: &str,
        covered: &[String],
    ) -> Result<Vec<Candidate>> {
        let inside = |path: &str| covered.iter().any(|c| policy::contains(path, c));
        let holds = |path: &str| covered.iter().any(|c| policy::contains(c, path));
        let mut candidates = vec![];
        for location in &self.locations {
            k.check()?;
            let mut found = vec![];
            for (e, capture) in self.expand(s, home, location) {
                if !c.allows(e.path()) || inside(e.path()) {
                    continue;
                }
                if !holds(e.path()) {
                    found.push((e, capture));
                } else if location.per_app && !location.app_folder {
                    // A vendor folder that also holds a covered cache, such as
                    // `Caches/Google` with Android Studio's: its other folders (Chrome)
                    // are listed one by one.
                    let vendor = e.name().to_owned();
                    for child in s.children(e.path(), &mut vec![]) {
                        if child.directory
                            && c.allows(child.path())
                            && !inside(child.path())
                            && !holds(child.path())
                        {
                            let name = format!("{vendor}/{}", child.name());
                            found.push((child, Some(name)));
                        }
                    }
                }
            }
            for (e, capture) in found {
                if location.app_folder {
                    let app = capture.as_deref().unwrap_or_default();
                    let folder = format!("{home}/Library/Application Support/{app}");
                    if WEB_CACHE_EXCEPT.contains(&app)
                        || !CHROMIUM_MARKERS
                            .iter()
                            .any(|m| s.exists(&format!("{folder}/{m}")))
                    {
                        continue;
                    }
                    candidates.push(
                        Candidate::new(
                            e,
                            &format!("{} Quit {app} first.", location.reason),
                            vec![ActionKind::Trash],
                            location.risk,
                        )
                        .title(format!("{} · {app}", location.name))
                        .details(vec![
                            detail("Ecosystem", "macOS"),
                            detail("Kind", location.name),
                            detail("App", app),
                        ]),
                    );
                    continue;
                }
                let owner = capture.as_deref().filter(|_| location.per_app).map(bundle);
                // Apple's own caches are left to review rather than offered as a quick fix.
                let apple = owner.is_some_and(|b| b.starts_with("com.apple."));
                let title = match (owner, &capture) {
                    (Some(b), _) => format!("{} · {}", location.name, app_name(b)),
                    (None, Some(name)) => format!("{} · {name}", location.name),
                    _ => location.name.to_owned(),
                };
                let mut details = vec![detail("Ecosystem", "macOS"), detail("Kind", location.name)];
                if let Some(b) = owner {
                    details.push(detail("App", b));
                }
                let reason = if location.per_app {
                    format!("{} Quit the app first.", location.reason)
                } else {
                    location.reason.to_owned()
                };
                let risk = if apple && location.risk == Risk::Rebuild {
                    Risk::Review
                } else {
                    location.risk
                };
                candidates.push(
                    Candidate::new(e, &reason, vec![ActionKind::Trash], risk)
                        .title(title)
                        .details(details),
                );
            }
        }
        Ok(candidates)
    }
    /// Checks before removing a leftover: `None` when the path is not one.
    pub fn preflight(&self, s: &Services, home: &str, path: &str) -> Option<Result<()>> {
        if let Some((location, capture)) = self.locate(home, path) {
            return Some(match capture.as_deref() {
                Some(app) if location.app_folder => app_closed(s, &[app]),
                Some(capture) if location.per_app => owners_closed(s, &[bundle(capture)]),
                _ => Ok(()),
            });
        }
        // A folder inside a vendor's cache folder, such as `Caches/Google/Chrome`, listed on
        // its own when the vendor folder also holds a cache another tool reports. Its app is
        // named after the folder, alone or after the vendor ("Google Chrome").
        let (vendor_path, child) = path.rsplit_once('/')?;
        let (location, vendor) = self.locate(home, vendor_path)?;
        let vendor = vendor.filter(|_| location.per_app && !location.app_folder)?;
        Some(
            owners_closed(s, &[bundle(&vendor)])
                .and_then(|()| app_closed(s, &[child, &format!("{vendor} {child}")])),
        )
    }
}
/// Fails while an app with one of `names` (its bundle's file name) is running.
fn app_closed(s: &Services, names: &[&str]) -> Result<()> {
    let running = s.apps.running()?;
    match running.iter().find_map(|a| {
        let stem = std::path::Path::new(&a.path).file_stem()?.to_str()?;
        names.iter().find(|n| n.eq_ignore_ascii_case(stem))
    }) {
        Some(name) => Err(format!("Quit {name} before removing its cache.")),
        None => Ok(()),
    }
}
/// A bundle identifier from a folder name such as `com.apple.Safari.savedState`.
fn bundle(capture: &str) -> &str {
    capture.strip_suffix(".savedState").unwrap_or(capture)
}
/// A readable app name from a bundle identifier: its last component.
fn app_name(bundle: &str) -> &str {
    bundle
        .rsplit('.')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or(bundle)
}
