mod common;
use common::*;
use cym_core::{
    adapters::command::SystemRunner,
    git::{self, Git, Worktree},
    model::*,
    modules, orphans,
    ports::*,
    simulator::Simctl,
    Engine, Services,
};
use std::{
    collections::HashSet,
    fs,
    sync::Arc,
    time::{Duration, Instant},
};

#[test]
fn worktree_parser_preserves_unusual_paths() {
    let rows = git::parse(
        b"worktree /projects/main\0HEAD abc\0branch refs/heads/main\0\0worktree /projects/line\nbreak\0HEAD def\0detached\0locked reason\0\0",
    )
    .unwrap();
    assert_eq!(rows.len(), 2);
    assert!(rows[0].main);
    assert_eq!(rows[0].branch, "main");
    assert_eq!(rows[1].path, "/projects/line\nbreak");
    assert!(rows[1].locked);
    assert!(rows[1].branch.is_empty());
}

#[test]
fn ignored_files_are_listed_but_do_not_block_removal() {
    // Status, then a missing upstream.
    let runner = StubRunner::new(vec![
        output("!! .env\0!! node_modules/\0", 0),
        output("", 128),
    ]);
    let record = Worktree {
        path: "/fixture/worktree".into(),
        branch: "feature".into(),
        ..Default::default()
    };
    let safety = Git(runner.as_ref())
        .safety(&record, &ScanControl::default())
        .unwrap();
    assert!(safety.eligible, "{}", safety.reason);
    assert_eq!(safety.ignored, [".env", "node_modules"]);
    assert!(safety.reason.contains("branch feature"));
    assert!(safety.reason.contains(".env, node_modules"));
    assert_eq!(safety.upstream, "None");
    assert_eq!(runner.calls().len(), 2);
}

#[test]
fn uncommitted_and_untracked_work_blocks_removal() {
    let record = Worktree {
        path: "/fixture/worktree".into(),
        branch: "feature".into(),
        ..Default::default()
    };
    let safety = |status: &str| {
        let runner = StubRunner::new(vec![output(status, 0)]);
        Git(runner.as_ref())
            .safety(&record, &ScanControl::default())
            .unwrap()
    };
    let both = safety(" M src/a.rs\0?? new.txt\0!! target/\0");
    assert!(!both.eligible);
    assert!(both
        .reason
        .contains("1 uncommitted change and 1 untracked item"));
    // A rename's original path follows it and is not counted again.
    let renamed = safety("R  b.rs\0a\0M  c.rs\0");
    assert!(
        renamed.reason.contains("2 uncommitted changes"),
        "{}",
        renamed.reason
    );
}

fn git_available() -> bool {
    fs::metadata("/usr/bin/git").is_ok()
}
#[test]
fn real_git_worktree_rejects_late_changes_and_removes_only_an_eligible_tree() {
    if !git_available() {
        return;
    }
    let f = Fixture::new();
    let s = services(&f);
    let k = ScanControl::default();
    let repository = f.dir("main");
    f.write("main/readme.txt", "fixture");
    let git = Git(s.commands.as_ref());
    let command = |args: &[&str], path: &str| {
        let out = git.run(path, args, &k).unwrap();
        assert_eq!(out.status, 0, "{args:?}: {}", out.error);
    };
    command(&["init", "-b", "main"], &repository);
    command(&["add", "readme.txt"], &repository);
    command(
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-m",
            "fixture",
        ],
        &repository,
    );
    command(
        &["update-ref", "refs/remotes/origin/main", "HEAD"],
        &repository,
    );
    command(&["config", "remote.origin.url", &repository], &repository);
    command(
        &[
            "config",
            "remote.origin.fetch",
            "+refs/heads/*:refs/remotes/origin/*",
        ],
        &repository,
    );
    let linked = f.at("linked");
    command(
        &["worktree", "add", "-b", "fixture-feature", &linked, "main"],
        &repository,
    );
    command(
        &["branch", "--set-upstream-to=origin/main", "fixture-feature"],
        &repository,
    );
    let record = git
        .worktrees(&repository, &k)
        .unwrap()
        .into_iter()
        .find(|w| w.path == linked)
        .unwrap();
    assert!(git.safety(&record, &k).unwrap().eligible);

    // Only the linked tree is listed; the main checkout is not a cleanup candidate.
    let scan = || {
        Engine::new(s.clone(), modules::builtin()).scan_report(
            &["worktrees".into()],
            &f.context(),
            &k,
        )
    };
    let report = scan();
    assert_eq!(report.findings.len(), 1, "{:?}", report.findings);
    let linked_row = &report.findings[0];
    assert_eq!(linked_row.resource.path(), Some(linked.as_str()));
    assert_eq!(linked_row.subtitle, format!("fixture-feature · {linked}"));
    assert_eq!(linked_row.actions, vec![ActionKind::RemoveWorktree]);
    assert_eq!(linked_row.badge.as_deref(), Some("Linked"));

    // A detached linked tree is labelled and needs manual inspection.
    let detached = f.at("detached");
    command(
        &["worktree", "add", "--detach", &detached, "main"],
        &repository,
    );
    let report = scan();
    let row = report
        .findings
        .iter()
        .find(|r| r.resource.path() == Some(detached.as_str()))
        .unwrap();
    assert_eq!(row.badge.as_deref(), Some("Detached"));
    assert!(row.subtitle.starts_with("Detached at "));
    // Its commit is also on main, so removing it loses nothing.
    assert_eq!(
        row.actions,
        vec![ActionKind::RemoveWorktree],
        "{}",
        row.reason
    );
    // A commit only the detached HEAD reaches blocks removal.
    f.write("detached/readme.txt", "changed");
    command(
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-am",
            "detached work",
        ],
        &detached,
    );
    let report = scan();
    let row = report
        .findings
        .iter()
        .find(|r| r.resource.path() == Some(detached.as_str()))
        .unwrap();
    assert!(row.actions.is_empty());
    assert!(
        row.reason.contains("no branch or tag contains"),
        "{}",
        row.reason
    );
    command(&["worktree", "remove", "--force", &detached], &repository);

    // Ignored files and a branch without an upstream do not block removal.
    let unpublished = f.at("unpublished");
    command(
        &["worktree", "add", "-b", "unpublished", &unpublished, "main"],
        &repository,
    );
    fs::write(f.at("main/.git/info/exclude"), "node_modules/\n").unwrap();
    f.write("unpublished/node_modules/x/index.js", "x");
    let report = scan();
    let row = report
        .findings
        .iter()
        .find(|r| r.resource.path() == Some(unpublished.as_str()))
        .unwrap();
    assert_eq!(
        row.actions,
        vec![ActionKind::RemoveWorktree],
        "{}",
        row.reason
    );
    assert!(row
        .reason
        .contains("Ignored files are deleted too: node_modules"));
    assert_eq!(row.value("Upstream (local ref)"), Some("None"));
    command(&["worktree", "remove", &unpublished], &repository);

    let identity = s.snapshot(&linked, &k).unwrap();
    f.write("linked/late-untracked.txt", "late");
    assert!(git::remove(&s, &identity, &repository, &record.head, &f.context(), &k).is_err());
    assert!(fs::metadata(&linked).is_ok());
    fs::remove_file(f.at("linked/late-untracked.txt")).unwrap();
    let identity = s.snapshot(&linked, &k).unwrap();
    git::remove(&s, &identity, &repository, &record.head, &f.context(), &k).unwrap();
    assert!(fs::metadata(&linked).is_err());
    assert!(fs::metadata(&repository).is_ok());
    command(
        &["show-ref", "--verify", "refs/heads/fixture-feature"],
        &repository,
    );
}

#[test]
fn missing_worktree_folder_is_badged_orphaned_and_inspection_only() {
    if !git_available() {
        return;
    }
    let f = Fixture::new();
    let s = services(&f);
    let k = ScanControl::default();
    let repository = f.dir("main");
    f.write("main/readme.txt", "fixture");
    let git = Git(s.commands.as_ref());
    for args in [
        vec!["init", "-b", "main"],
        vec!["add", "readme.txt"],
        vec![
            "-c",
            "user.name=F",
            "-c",
            "user.email=f@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-m",
            "f",
        ],
    ] {
        assert_eq!(git.run(&repository, &args, &k).unwrap().status, 0);
    }
    let linked = f.at("gone");
    assert_eq!(
        git.run(&repository, &["worktree", "add", &linked], &k)
            .unwrap()
            .status,
        0
    );
    fs::remove_dir_all(&linked).unwrap();
    let report =
        Engine::new(s, modules::builtin()).scan_report(&["worktrees".into()], &f.context(), &k);
    let row = report.findings.iter().find(|r| r.title == "gone").unwrap();
    assert_eq!(row.badge.as_deref(), Some("Orphaned"));
    assert!(row.actions.is_empty());
    assert!(row.blocked_reason.is_some());
}

#[test]
fn command_runner_rejects_arbitrary_executables_and_honors_cancellation() {
    let k = ScanControl::default();
    assert!(SystemRunner
        .run(
            "/bin/sh",
            &["-c".into(), "true".into()],
            Duration::from_secs(1),
            &k
        )
        .is_err());
    k.cancel();
    assert!(SystemRunner
        .run(
            "/usr/bin/git",
            &["--version".into()],
            Duration::from_secs(2),
            &k
        )
        .is_err());
}

#[test]
fn command_deadline_holds_when_a_descendant_keeps_the_pipe_open() {
    if !git_available() {
        return;
    }
    let started = Instant::now();
    // The alias's shell exits at once while a background child inherits stdout.
    let result = SystemRunner.run(
        "/usr/bin/git",
        &[
            "-c".into(),
            "alias.hold=!sleep 10 & echo started".into(),
            "hold".into(),
        ],
        Duration::from_secs(1),
        &ScanControl::default(),
    );
    assert!(result.is_err(), "{result:?}");
    assert!(started.elapsed() < Duration::from_secs(4));
}

#[test]
fn simulator_actions_require_shutdown_and_a_specific_id() {
    let id = "35D71E75-E2F0-4EF5-A131-06B14FDB044E";
    let json = format!(
        r#"{{"devices":{{"runtime":[{{"udid":"{id}","name":"Fixture","state":"Shutdown","isAvailable":true,"dataPath":"/fixture/data"}}]}},"runtimes":[]}}"#
    );
    let k = ScanControl::default();
    let context = ScanContext::default();
    let runner = StubRunner::new(vec![output(&json, 0), output("", 0)]);
    Simctl(runner.as_ref())
        .act(id, ActionKind::ResetSimulator, &context, &k)
        .unwrap();
    assert_eq!(
        runner.calls().last().unwrap(),
        &["/usr/bin/xcrun", "simctl", "erase", id]
    );
    let booted = StubRunner::new(vec![output(&json.replace("Shutdown", "Booted"), 0)]);
    assert!(Simctl(booted.as_ref())
        .act(id, ActionKind::DeleteSimulator, &context, &k)
        .is_err());
    assert_eq!(booted.calls().len(), 1);
    let unused = StubRunner::new(vec![]);
    assert!(Simctl(unused.as_ref())
        .act("all", ActionKind::ResetSimulator, &context, &k)
        .is_err());
    assert!(unused.calls().is_empty());
    let excluded = StubRunner::new(vec![output(&json, 0)]);
    let context = ScanContext {
        exclusions: vec!["/fixture".into()],
        ..Default::default()
    };
    assert!(Simctl(excluded.as_ref())
        .act(id, ActionKind::DeleteSimulator, &context, &k)
        .is_err());
    assert_eq!(excluded.calls().len(), 1);
}

#[test]
fn orphan_classification_excludes_services_apps_and_other_users() {
    let reason = |p: Snapshot, managed: &[i32], apps: Vec<RunningApp>, ignored: &[&str]| {
        orphans::exclusion(
            &p,
            501,
            99,
            &managed.iter().copied().collect::<HashSet<_>>(),
            &apps,
            &ignored.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        )
    };
    let node = || process("/opt/homebrew/bin/node", 1, 501);
    let app = |pid: i32, path: &str| RunningApp {
        pid,
        bundle_id: "com.example.Editor".into(),
        path: path.into(),
    };
    assert_eq!(reason(node(), &[], vec![], &[]), None);
    assert!(reason(node(), &[42], vec![], &[]).is_some());
    assert!(reason(node(), &[], vec![app(42, "/x.app")], &[]).is_some());
    assert!(reason(process("/opt/homebrew/bin/node", 1, 502), &[], vec![], &[]).is_some());
    assert!(reason(process("/opt/homebrew/bin/node", 10, 501), &[], vec![], &[]).is_some());
    assert!(reason(process("/System/Library/tool", 1, 501), &[], vec![], &[]).is_some());
    assert!(reason(
        process("/Applications/Editor.app/Contents/helper", 1, 501),
        &[],
        vec![app(7, "/Applications/Editor.app")],
        &[]
    )
    .is_some());
    assert!(reason(
        process(
            "/Applications/Editor.app/Contents/service.xpc/worker",
            1,
            501
        ),
        &[],
        vec![],
        &[]
    )
    .is_some());
    assert!(reason(
        process("/opt/homebrew/bin/ssh-agent", 1, 501),
        &[],
        vec![],
        &["ssh-agent"]
    )
    .is_some());
}

fn orphan_services(
    f: &Fixture,
    launchctl: Vec<Output>,
) -> (Services, Arc<FakeProcesses>, Arc<StubRunner>) {
    let processes = Arc::new(FakeProcesses::default());
    processes
        .rows
        .lock()
        .unwrap()
        .push(process("/opt/homebrew/bin/node", 1, 501));
    let runner = StubRunner::new(launchctl);
    (
        Services {
            processes: processes.clone(),
            commands: runner.clone(),
            ..services(f)
        },
        processes,
        runner,
    )
}
fn launchctl() -> Output {
    output("PID\tStatus\tLabel\n7\t0\tcom.example.agent\n", 0)
}

#[test]
fn unknown_managed_jobs_fail_closed_and_force_requires_a_graceful_request() {
    let f = Fixture::new();
    let (s, processes, runner) = orphan_services(&f, vec![output("", 1)]);
    let k = ScanControl::default();
    assert!(orphans::candidates(&s, &ScanContext::default(), &k).is_err());
    let identity = processes.rows.lock().unwrap()[0].identity.clone();
    assert!(orphans::signal(&s, &identity, true, &ScanContext::default(), &k).is_err());
    assert_eq!(runner.calls().len(), 1);
    assert!(processes.signals.lock().unwrap().is_empty());
}

#[test]
fn orphan_module_and_termination_use_the_exact_instance() {
    let f = Fixture::new();
    let (s, processes, _) = orphan_services(&f, vec![launchctl(), launchctl(), launchctl()]);
    let k = ScanControl::default();
    let engine = Engine::new(s.clone(), modules::builtin());
    let report = engine.scan_report(&["orphans".into()], &ScanContext::default(), &k);
    assert_eq!(report.findings.len(), 1, "{:?}", report.warnings);
    let row = report.findings[0].clone();
    assert_eq!(
        row.actions,
        vec![ActionKind::Terminate, ActionKind::ForceQuit]
    );

    // A different instance now holds the PID: refuse.
    let mut stale = row.clone();
    if let Resource::Process { process } = &mut stale.resource {
        process.started_microseconds = 999;
    }
    let refused = engine.execute(
        &ActionRequest {
            findings: vec![stale],
            kind: ActionKind::Terminate,
            context: ScanContext::default(),
        },
        &k,
    );
    assert_eq!(refused[0].outcome, Outcome::Failed);
    assert!(processes.signals.lock().unwrap().is_empty());

    let applied = engine.execute(
        &ActionRequest {
            findings: vec![row],
            kind: ActionKind::Terminate,
            context: ScanContext::default(),
        },
        &k,
    );
    assert_eq!(
        applied[0].outcome,
        Outcome::Applied,
        "{}",
        applied[0].message
    );
    assert_eq!(*processes.signals.lock().unwrap(), vec![(42, false)]);
    assert!(!format!("{:?}", engine.history()).contains("fixture"));
}

#[test]
fn process_identity_includes_subsecond_start_time() {
    let a = process("/bin/tool", 1, 501).identity;
    let mut b = a.clone();
    b.started_microseconds = 3;
    assert_ne!(a, b);
    assert_ne!(a.key(), b.key());
}
