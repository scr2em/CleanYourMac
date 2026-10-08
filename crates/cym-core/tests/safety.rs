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
        .contains("uncommitted or untracked"));
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
        .contains("uncommitted or untracked"));
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
}
