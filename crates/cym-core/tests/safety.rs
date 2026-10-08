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
}
