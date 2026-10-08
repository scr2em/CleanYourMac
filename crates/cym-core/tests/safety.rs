mod common;
use common::*;
use cym_core::{git::Git, model::*, modules::storage::app_data, ports::*, Engine};

fn scan(engine: &Engine, _: &Fixture, module: &str, roots: Vec<String>) -> Vec<Finding> {
    let context = ScanContext {
        roots,
        ..Default::default()
    };
    engine
        .scan_report(&[module.into()], &context, &ScanControl::default())
        .findings
}

#[test]
fn large_files_of_apps_and_tools_are_listed_but_never_offered() {
    let home = "/Users/me";
    for path in [
        "/Users/me/Library/Containers/com.docker.docker/Data/vms/0/data/Docker.raw",
        "/Users/me/.ollama/models/blobs/sha256-abc",
        "/Users/me/.android/avd/Pixel.avd/userdata.img",
        "/Users/me/Library/Application Support/MobileSync/Backup/X/1f/1f00",
    ] {
        assert!(app_data(path, home), "{path}");
    }
    assert!(!app_data("/Users/me/Movies/trip.mov", home));
    assert!(
        !app_data("/Volumes/Disk/Library/x", home),
        "only the home folder's Library"
    );
}

#[test]
fn version_control_names_are_recognized_exactly() {
    use cym_core::policy::version_control;
    for (name, system) in [
        (".git", "Git"),
        (".HG", "Mercurial"),
        (".sl", "Sapling"),
        (".svn", "Subversion"),
        (".jj", "Jujutsu"),
        (".bzr", "Bazaar"),
        ("_darcs", "Darcs"),
        (".pijul", "Pijul"),
        (".fslckout", "Fossil"),
        ("_FOSSIL_", "Fossil"),
        ("project.fossil", "Fossil"),
        ("CVS", "CVS"),
        ("RCS", "RCS"),
        ("SCCS", "SCCS"),
        ("BitKeeper", "BitKeeper"),
        ("_MTN", "Monotone"),
        ("db.mtn", "Monotone"),
        ("{arch}", "GNU Arch"),
        (".plastic", "Plastic SCM"),
        ("$tf", "Team Foundation"),
        (".repo", "repo"),
        (".dvc", "DVC"),
    ] {
        assert_eq!(version_control(name), Some(system), "{name}");
    }
    // Plain names must match exactly: a user's own folders are not repositories.
    for name in [
        "cvs",
        "Rcs",
        "sccs",
        "bitkeeper",
        ".fossil",
        "notes",
        ".gitignore",
    ] {
        assert_eq!(version_control(name), None, "{name}");
    }
}

#[test]
fn file_scans_never_enter_version_control_folders() {
    let f = Fixture::new();
    let big = |path: &str| {
        let path = f.write(path, "");
        std::fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_len(150_000_000)
            .unwrap();
    };
    for vcs in [
        ".git",
        ".hg",
        ".sl",
        ".svn",
        ".jj",
        ".bzr",
        "_darcs",
        ".pijul",
        "CVS",
        "RCS",
        "SCCS",
        "BitKeeper",
        "_MTN",
        "{arch}",
        ".plastic",
        "$tf",
        ".repo",
        ".dvc",
    ] {
        big(&format!("repo/{vcs}/store/pack.bin"));
    }
    // A Fossil or Monotone repository is a single file, kept anywhere.
    big("repos/project.fossil");
    big("repos/project.mtn");
    big("repo/src/video.mov");
    // A user's own folder that merely shares a plain name is scanned as usual.
    big("repo/cvs/receipts.pdf");
    let engine = Engine::new(services(&f), builtin(&f));
    let paths = |module: &str| -> Vec<String> {
        let mut paths: Vec<String> = scan(&engine, &f, module, vec![f.path()])
            .iter()
            .filter_map(|r| r.resource.path().map(str::to_owned))
            .collect();
        paths.sort();
        paths
    };
    let large = paths("large");
    assert_eq!(large.len(), 2, "{large:?}");
    assert!(large[0].ends_with("repo/cvs/receipts.pdf"));
    assert!(large[1].ends_with("repo/src/video.mov"));
    // Every pack file holds the same zeros as the video, and none is a duplicate.
    let duplicates = paths("duplicates");
    assert!(
        duplicates
            .iter()
            .all(|p| p.contains("/src/") || p.contains("/cvs/")),
        "{duplicates:?}"
    );
}

#[test]
fn items_in_another_systems_checkout_are_refused() {
    let f = Fixture::new();
    let k = ScanControl::default();
    // Duplicates inside a Mercurial checkout: Mercurial is never asked, so the copy stays.
    let body = "same logo ".repeat(600);
    f.dir("site/.hg/store");
    f.write("site/logo.png", &body);
    f.write("other/logo.png", &body);
    // Dependencies inside a Subversion checkout.
    f.dir("app/.svn");
    f.write("app/package.json", "{}");
    f.write("app/node_modules/x/index.js", "x");
    let engine = Engine::new(services(&f), builtin(&f));
    let trash = |finding: &Finding| {
        engine
            .execute(
                &ActionRequest {
                    findings: vec![finding.clone()],
                    kind: ActionKind::Trash,
                    context: f.context(),
                    acknowledged: vec![],
                    force: true,
                },
                &k,
            )
            .remove(0)
    };
    let duplicates = scan(&engine, &f, "duplicates", vec![f.path()]);
    let in_checkout = duplicates
        .iter()
        .find(|r| {
            r.resource
                .path()
                .is_some_and(|p| p.ends_with("site/logo.png"))
        })
        .expect("listed");
    let result = trash(in_checkout);
    assert_eq!(result.outcome, Outcome::Failed, "{}", result.message);
    assert!(
        result.message.contains("Mercurial checkout"),
        "{}",
        result.message
    );
    assert!(std::fs::metadata(f.at("site/logo.png")).is_ok());

    let dependencies = scan(&engine, &f, "node", vec![f.path()]);
    let modules = dependencies
        .iter()
        .find(|r| {
            r.resource
                .path()
                .is_some_and(|p| p.ends_with("app/node_modules"))
        })
        .expect("listed");
    let result = trash(modules);
    assert_eq!(result.outcome, Outcome::Failed, "{}", result.message);
    assert!(
        result.message.contains("Subversion checkout"),
        "{}",
        result.message
    );
    assert!(std::fs::metadata(f.at("app/node_modules/x/index.js")).is_ok());
}

#[test]
fn an_ai_worktree_of_another_system_stays() {
    let f = Fixture::new();
    f.dir("home/.codex/worktrees/task-jj/.jj/repo");
    f.write("home/.codex/worktrees/task-jj/src/main.rs", "fn main() {}");
    let engine = Engine::new(services(&f), builtin(&f));
    let findings = scan(&engine, &f, "ai", vec![f.at("home")]);
    let worktree = findings
        .iter()
        .find(|r| {
            r.resource
                .path()
                .is_some_and(|p| p.ends_with("worktrees/task-jj"))
        })
        .expect("listed");
    assert!(
        worktree
            .blocked_reason
            .as_deref()
            .is_some_and(|r| r.contains("Jujutsu checkout")),
        "{:?}",
        worktree.blocked_reason
    );
}

#[test]
fn editor_extensions_are_never_project_dependencies() {
    let f = Fixture::new();
    // VS Code and its forks keep each extension with its own package.json and node_modules.
    let editors = [
        ".vscode",
        ".vscode-insiders",
        ".vscode-oss",
        ".cursor",
        ".windsurf",
        ".trae",
        ".kiro",
        ".positron",
        ".void",
    ];
    for editor in editors {
        let ext = format!("home/{editor}/extensions/publisher.extension-1.0.0");
        f.write(&format!("{ext}/package.json"), "{}");
        f.write(&format!("{ext}/node_modules/dep/index.js"), "x");
        f.write(&format!("{ext}/coverage/lcov.info"), "x");
    }
    // Apps keep plugins in the Library folder too.
    let plugin = "home/Library/Application Support/Code/User/globalStorage/publisher.ext";
    f.write(&format!("{plugin}/package.json"), "{}");
    f.write(&format!("{plugin}/node_modules/dep/index.js"), "x");
    // A real project beside them is still found.
    f.write("home/Projects/app/package.json", "{}");
    f.write("home/Projects/app/node_modules/dep/index.js", "x");
    f.write("home/Projects/app/coverage/lcov.info", "x");
    let engine = Engine::new(services(&f), builtin(&f));
    let paths = |module: &str, root: String| -> Vec<Finding> {
        scan(&engine, &f, module, vec![root])
            .into_iter()
            .filter(|r| r.resource.path().is_some_and(|p| !p.contains("/home/.npm")))
            .collect()
    };
    for module in ["node", "artifacts"] {
        let found = paths(module, f.at("home"));
        assert_eq!(
            found.len(),
            1,
            "{module}: {:?}",
            found.iter().map(|r| r.resource.path()).collect::<Vec<_>>()
        );
        assert!(found[0]
            .resource
            .path()
            .unwrap()
            .contains("/home/Projects/app/"));
    }
    // Chosen explicitly, an extension's dependencies are shown but can never be removed.
    let chosen = paths("node", f.at("home/.cursor/extensions"));
    assert_eq!(chosen.len(), 1);
    assert!(chosen[0].actions.is_empty());
    assert!(chosen[0]
        .blocked_reason
        .as_deref()
        .unwrap()
        .contains("editor extension"));
    let mut forced = chosen[0].clone();
    forced.actions = vec![ActionKind::Trash];
    forced.blocked_reason = None;
    let result = engine
        .execute(
            &ActionRequest {
                findings: vec![forced],
                kind: ActionKind::Trash,
                context: ScanContext {
                    roots: vec![f.at("home/.cursor/extensions")],
                    ..Default::default()
                },
                acknowledged: vec![],
                force: true,
            },
            &ScanControl::default(),
        )
        .remove(0);
    assert_eq!(result.outcome, Outcome::Failed, "{}", result.message);
    assert!(std::fs::metadata(
        f.at("home/.cursor/extensions/publisher.extension-1.0.0/node_modules")
    )
    .is_ok());
}

#[test]
fn a_large_file_that_git_tracks_stays() {
    if std::fs::metadata("/usr/bin/git").is_err() {
        return;
    }
    let f = Fixture::new();
    let repo = f.dir("repo");
    let big = f.write("repo/assets/model.bin", "");
    std::fs::File::options()
        .write(true)
        .open(&big)
        .unwrap()
        .set_len(150_000_000)
        .unwrap();
    let s = services(&f);
    let k = ScanControl::default();
    let git = Git(s.commands.as_ref());
    assert_eq!(
        git.run(&repo, &["init", "-q", "-b", "main"], &k)
            .unwrap()
            .status,
        0
    );
    // Tracked without hashing 150 MB: any blob ID marks the path as tracked.
    let blob = "100644,e69de29bb2d1d6434b8b29ae775ad8c2e48c5391,assets/model.bin";
    assert_eq!(
        git.run(&repo, &["update-index", "--add", "--cacheinfo", blob], &k)
            .unwrap()
            .status,
        0
    );
    let engine = Engine::new(services(&f), builtin(&f));
    let large = scan(&engine, &f, "large", vec![f.path()]);
    assert_eq!(large.len(), 1);
    let result = engine
        .execute(
            &ActionRequest {
                findings: vec![large[0].clone()],
                kind: ActionKind::Trash,
                context: f.context(),
                acknowledged: vec![],
                force: true,
            },
            &k,
        )
        .remove(0);
    assert_eq!(result.outcome, Outcome::Failed, "{}", result.message);
    assert!(result.message.contains("Git tracks"), "{}", result.message);
    assert!(std::fs::metadata(&big).is_ok());
}

#[test]
fn chrome_caches_are_listed_beside_android_studio_caches() {
    let f = Fixture::new();
    f.write(
        "home/Library/Caches/Google/AndroidStudio2024.1/index/data",
        "x",
    );
    f.write("home/Library/Caches/Google/Chrome/Default/Cache/data", "x");
    let engine = Engine::new(services(&f), builtin(&f));
    let found = scan(&engine, &f, "caches", vec![f.at("home")]);
    let path = |end: &str| {
        found
            .iter()
            .find(|r| r.resource.path().is_some_and(|p| p.ends_with(end)))
    };
    assert!(
        path("Caches/Google/AndroidStudio2024.1").is_some(),
        "Android Studio cache"
    );
    let chrome = path("Caches/Google/Chrome").expect("Chrome cache");
    assert_eq!(chrome.actions, vec![ActionKind::Trash]);
    // The vendor folder is never listed whole, which would count Android Studio twice.
    assert!(path("Library/Caches/Google").is_none());
}

#[test]
fn the_newest_xcode_stays_when_xcode_select_picks_the_command_line_tools() {
    let f = Fixture::new();
    // The fixture's xcode-select points at home/Applications/Xcode.app, which is absent,
    // as when the Command Line Tools are selected.
    for (app, version) in [
        ("Xcode-15.app", "15.4"),
        ("Xcode-16.app", "16.2"),
        ("Xcode-9.app", "9.0"),
    ] {
        f.write(
            &format!("home/Applications/{app}/Contents/Info.plist"),
            &format!(r#"<?xml version="1.0" encoding="UTF-8"?><plist version="1.0"><dict><key>CFBundleIdentifier</key><string>com.apple.dt.Xcode</string><key>CFBundleShortVersionString</key><string>{version}</string></dict></plist>"#),
        );
    }
    let engine = Engine::new(services(&f), builtin(&f));
    let found = scan(&engine, &f, "xcode", vec![f.at("home")]);
    let install = |name: &str| {
        found
            .iter()
            .find(|r| r.resource.path().is_some_and(|p| p.ends_with(name)))
            .unwrap_or_else(|| panic!("{name} listed"))
    };
    let newest = install("Xcode-16.app");
    assert!(newest.actions.is_empty());
    assert!(newest
        .blocked_reason
        .as_deref()
        .unwrap()
        .contains("newest one stays"));
    // Version order is numeric: 15.4 is older than 16.2 and newer than 9.0.
    assert_eq!(install("Xcode-15.app").actions, vec![ActionKind::Trash]);
    assert_eq!(install("Xcode-9.app").actions, vec![ActionKind::Trash]);
    let mut forced = newest.clone();
    forced.actions = vec![ActionKind::Trash];
    forced.blocked_reason = None;
    let result = engine
        .execute(
            &ActionRequest {
                findings: vec![forced],
                kind: ActionKind::Trash,
                context: ScanContext {
                    roots: vec![f.at("home")],
                    ..Default::default()
                },
                acknowledged: vec![],
                force: true,
            },
            &ScanControl::default(),
        )
        .remove(0);
    assert_eq!(result.outcome, Outcome::Failed, "{}", result.message);
    assert!(std::fs::metadata(f.at("home/Applications/Xcode-16.app")).is_ok());
}

#[test]
fn large_files_never_look_inside_build_or_dependency_folders() {
    let f = Fixture::new();
    let big = |path: &str| {
        let path = f.write(path, "");
        std::fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_len(150_000_000)
            .unwrap();
    };
    // Inside a project's dependencies or build output: removing one file breaks the build.
    f.write("app/package.json", "{}");
    big("app/node_modules/sharp/build/sharp.node");
    f.write("rs/Cargo.toml", "[package]");
    big("rs/target/release/server");
    f.write("py/pyproject.toml", "");
    f.write("py/.venv/pyvenv.cfg", "");
    big("py/.venv/lib/python3.12/site-packages/torch/lib/libtorch.so");
    // A folder only named like build output, with no project files, is the user's.
    big("Documents/build/keynote-render.mov");
    // A node_modules no project owns is not a dependency folder either.
    big("old/node_modules/blob.bin");
    big("Movies/trip.mov");
    let engine = Engine::new(services(&f), builtin(&f));
    let mut found: Vec<String> = scan(&engine, &f, "large", vec![f.path()])
        .iter()
        .filter_map(|r| {
            r.resource
                .path()
                .map(|p| p.strip_prefix(&f.path()).unwrap_or(p).to_owned())
        })
        .collect();
    found.sort();
    assert_eq!(
        found,
        vec![
            "/Documents/build/keynote-render.mov",
            "/Movies/trip.mov",
            "/old/node_modules/blob.bin"
        ]
    );
}

#[test]
fn dependencies_never_look_inside_build_output() {
    let f = Fixture::new();
    f.write("app/package.json", "{}");
    f.write("app/next.config.js", "module.exports = {}");
    f.write("app/node_modules/react/index.js", "x");
    // Next.js standalone output carries its own node_modules: part of the build, not a project.
    f.write("app/.next/standalone/package.json", "{}");
    f.write("app/.next/standalone/node_modules/react/index.js", "x");
    let engine = Engine::new(services(&f), builtin(&f));
    let found: Vec<String> = scan(&engine, &f, "node", vec![f.path()])
        .iter()
        .filter_map(|r| r.resource.path().map(str::to_owned))
        .filter(|p| p.starts_with(&f.path()))
        .collect();
    assert_eq!(found, vec![f.at("app/node_modules")]);
}

#[test]
fn build_output_with_a_shipped_archive_needs_confirmation() {
    let f = Fixture::new();
    f.write("app/pubspec.yaml", "name: app");
    f.write("app/build/ios/archive/Runner.xcarchive/Info.plist", "x");
    f.write("plain/pubspec.yaml", "name: plain");
    f.write("plain/build/app/intermediates/x.bin", "x");
    let engine = Engine::new(services(&f), builtin(&f));
    let findings = scan(&engine, &f, "artifacts", vec![f.path()]);
    let shipped = findings
        .iter()
        .find(|r| r.resource.path().is_some_and(|p| p.ends_with("app/build")))
        .expect("listed");
    assert!(shipped.actions.is_empty(), "never part of a one-click fix");
    assert!(shipped
        .acknowledgement
        .as_deref()
        .unwrap()
        .contains("Runner.xcarchive"));
    assert_eq!(shipped.risk, Risk::Review);
    let plain = findings
        .iter()
        .find(|r| {
            r.resource
                .path()
                .is_some_and(|p| p.ends_with("plain/build"))
        })
        .unwrap();
    assert_eq!(plain.actions, vec![ActionKind::Trash]);
}

#[test]
fn maven_and_mail_downloads_are_reviewed_not_rebuilt() {
    let f = Fixture::new();
    f.write("home/.m2/repository/com/x/1.0/x.jar", "x");
    f.write(
        "home/Library/Containers/com.apple.mail/Data/Library/Mail Downloads/A/edited.pages",
        "x",
    );
    let engine = Engine::new(services(&f), builtin(&f));
    let findings = scan(&engine, &f, "caches", vec![f.at("home")]);
    for part in [".m2/repository", "Mail Downloads"] {
        let row = findings
            .iter()
            .find(|r| r.resource.path().is_some_and(|p| p.ends_with(part)))
            .unwrap_or_else(|| panic!("{part}"));
        assert_eq!(row.risk, Risk::Review, "{part}");
    }
}

#[test]
fn an_ai_worktree_with_work_in_it_stays() {
    if std::fs::metadata("/usr/bin/git").is_err() {
        return;
    }
    let f = Fixture::new();
    let worktree = f.dir("home/.codex/worktrees/task-1");
    f.write("home/.codex/worktrees/task-2/notes.txt", "not a repository");
    // Codex keeps the repository one folder below: worktrees/<id>/<repo>.
    let nested = f.dir("home/.codex/worktrees/task-3/app");
    let s = services(&f);
    let k = ScanControl::default();
    let git = Git(s.commands.as_ref());
    for repo in [&worktree, &nested] {
        assert_eq!(
            git.run(repo, &["init", "-b", "main"], &k).unwrap().status,
            0
        );
    }
    f.write(
        "home/.codex/worktrees/task-1/new-feature.rs",
        "fn main() {}",
    );
    f.write("home/.codex/worktrees/task-3/app/wip.rs", "fn main() {}");
    // A clean repository with a submodule, and one with a repository in an ignored folder:
    // `status` sees neither's work, so both stay.
    let identity = ["-c", "user.name=t", "-c", "user.email=t@t"];
    let commit = |repo: &str| {
        let args = [&identity[..], &["commit", "-q", "--allow-empty", "-m", "x"]].concat();
        assert_eq!(git.run(repo, &args, &k).unwrap().status, 0);
    };
    let with_sub = f.dir("home/.codex/worktrees/task-4");
    let lib = f.dir("home/.codex/worktrees/task-4/lib");
    let with_ignored = f.dir("home/.codex/worktrees/task-5");
    let vendored = f.dir("home/.codex/worktrees/task-5/vendor/tool");
    for repo in [&with_sub, &lib, &with_ignored, &vendored] {
        assert_eq!(
            git.run(repo, &["init", "-q", "-b", "main"], &k)
                .unwrap()
                .status,
            0
        );
    }
    commit(&lib);
    f.write("home/.codex/worktrees/task-4/lib/wip.rs", "fn main() {}");
    let head = git.run(&lib, &["rev-parse", "HEAD"], &k).unwrap();
    let head = String::from_utf8_lossy(&head.data).trim().to_owned();
    let cacheinfo = format!("160000,{head},lib");
    assert_eq!(
        git.run(
            &with_sub,
            &["update-index", "--add", "--cacheinfo", &cacheinfo],
            &k
        )
        .unwrap()
        .status,
        0
    );
    commit(&with_sub);
    f.write("home/.codex/worktrees/task-5/.gitignore", "vendor/\n");
    f.write(
        "home/.codex/worktrees/task-5/vendor/tool/wip.rs",
        "fn main() {}",
    );
    assert_eq!(
        git.run(&with_ignored, &["add", ".gitignore"], &k)
            .unwrap()
            .status,
        0
    );
    commit(&with_ignored);
    // Agents often work on a detached HEAD: a commit no branch holds would be lost, while
    // a detached checkout of a branch's commit loses nothing.
    let detached = f.dir("home/.codex/worktrees/task-6");
    let at_branch = f.dir("home/.codex/worktrees/task-7");
    for repo in [&detached, &at_branch] {
        assert_eq!(
            git.run(repo, &["init", "-q", "-b", "main"], &k)
                .unwrap()
                .status,
            0
        );
        commit(repo);
        assert_eq!(
            git.run(repo, &["checkout", "-q", "--detach"], &k)
                .unwrap()
                .status,
            0
        );
    }
    commit(&detached);
    let engine = Engine::new(services(&f), builtin(&f));
    let findings = scan(&engine, &f, "ai", vec![f.at("home")]);
    let dirty = findings
        .iter()
        .find(|r| {
            r.resource
                .path()
                .is_some_and(|p| p.ends_with("worktrees/task-1"))
        })
        .unwrap();
    assert!(dirty
        .blocked_reason
        .as_deref()
        .unwrap()
        .contains("1 untracked item"));
    let plain = findings
        .iter()
        .find(|r| {
            r.resource
                .path()
                .is_some_and(|p| p.ends_with("worktrees/task-2"))
        })
        .unwrap();
    assert!(plain.blocked_reason.is_none());
    let nested = findings
        .iter()
        .find(|r| {
            r.resource
                .path()
                .is_some_and(|p| p.ends_with("worktrees/task-3"))
        })
        .unwrap();
    assert!(nested
        .blocked_reason
        .as_deref()
        .unwrap()
        .contains("1 untracked item"));
    let reason = |end: &str| {
        findings
            .iter()
            .find(|r| r.resource.path().is_some_and(|p| p.ends_with(end)))
            .and_then(|r| r.blocked_reason.clone())
            .unwrap_or_default()
    };
    assert!(
        reason("worktrees/task-4").contains("nested repository lib"),
        "{}",
        reason("worktrees/task-4")
    );
    assert!(
        reason("worktrees/task-5").contains("ignored folder (vendor/tool)"),
        "{}",
        reason("worktrees/task-5")
    );
    assert!(
        reason("worktrees/task-6").contains("Detached HEAD"),
        "{}",
        reason("worktrees/task-6")
    );
    let kept = findings
        .iter()
        .find(|r| {
            r.resource
                .path()
                .is_some_and(|p| p.ends_with("worktrees/task-7"))
        })
        .unwrap();
    assert_eq!(kept.blocked_reason, None);
}

/// What a cache folder holds beside known caches is not known, so it is never a one-click
/// "App cache": Poetry keeps whole environments in `pypoetry/virtualenvs`.
#[test]
fn unknown_folders_beside_known_caches_are_left_to_review() {
    let f = Fixture::new();
    f.write("home/Library/Caches/pypoetry/cache/x", "x");
    f.write(
        "home/Library/Caches/pypoetry/virtualenvs/app-py3.12/pyvenv.cfg",
        "x",
    );
    let engine = Engine::new(services(&f), builtin(&f));
    let findings = scan(&engine, &f, "caches", vec![f.at("home")]);
    let venvs = findings
        .iter()
        .find(|r| {
            r.resource
                .path()
                .is_some_and(|p| p.ends_with("pypoetry/virtualenvs"))
        })
        .expect("listed");
    assert_eq!(venvs.risk, Risk::Review, "{}", venvs.reason);
}

/// A build folder holding an app package, or too large to check, needs confirmation.
#[test]
fn shipped_apps_in_build_output_need_confirmation() {
    let f = Fixture::new();
    f.write("app/pubspec.yaml", "name: app");
    f.write("app/build/app/outputs/flutter-apk/app-release.apk", "x");
    let engine = Engine::new(services(&f), builtin(&f));
    let findings = scan(&engine, &f, "artifacts", vec![f.path()]);
    let build = findings
        .iter()
        .find(|r| r.resource.path().is_some_and(|p| p.ends_with("app/build")))
        .expect("listed");
    assert!(build.actions.is_empty());
    assert_eq!(build.acknowledged_actions, vec![ActionKind::Trash]);
    assert!(build.reason.contains("app-release.apk"), "{}", build.reason);
}

/// A one-click fix with an idle age skips items whose age is unknown or recent.
#[test]
fn idle_fixes_wait_for_a_known_old_date() {
    use cym_core::recommend::{recommendations, RULES};
    let now = 2_000_000_000.0;
    let row = |id: &str, last: Option<f64>| {
        let mut f = Finding::new(
            "xcode",
            id,
            id,
            Resource::File {
                file: FileIdentity {
                    path: format!("/x/{id}"),
                    device: 1,
                    inode: 1,
                    modified_seconds: 0,
                    modified_nanos: 0,
                    tree_signature: None,
                },
            },
            "",
        );
        f.actions = vec![ActionKind::Trash];
        f.risk = Risk::Rebuild;
        f.bytes = Some(10_000_000);
        f.allocated_bytes = Some(10_000_000);
        f.last_used_at = last;
        f
    };
    let rows = [
        row("old", Some(now - 40.0 * 86_400.0)),
        row("recent", Some(now - 86_400.0)),
        row("unknown", None),
    ];
    let fixes = recommendations(
        &|m: &str| rows.iter().filter(|r| r.module_id == m).collect(),
        RULES,
        now,
        1,
    );
    let xcode = fixes.iter().find(|r| r.id == "xcode-build-data").unwrap();
    assert_eq!(xcode.ids, vec!["xcode:old".to_string()]);
}

/// Data nothing can make again is removed only after the user confirms losing it.
#[test]
fn irreplaceable_data_needs_confirmation() {
    let f = Fixture::new();
    f.write(
        "home/Library/Application Support/MobileSync/Backup/00008030-ABC/Info.plist",
        "x",
    );
    f.write(
        "home/Library/Containers/com.apple.mail/Data/Library/Mail Downloads/A/report.docx",
        "x",
    );
    let engine = Engine::new(services(&f), builtin(&f));
    let findings = scan(&engine, &f, "caches", vec![f.at("home")]);
    for title in ["Device backup · 00008030-ABC", "Mail downloads"] {
        let row = findings
            .iter()
            .find(|r| r.title == title)
            .unwrap_or_else(|| panic!("{title}"));
        assert!(row.actions.is_empty(), "{title}");
        assert_eq!(row.acknowledged_actions, vec![ActionKind::Trash], "{title}");
        assert!(row.acknowledgement.is_some(), "{title}");
    }
}
