mod common;
use common::*;
use cym_core::{
    model::*,
    modules::{
        projects::{self, ProjectsModule},
        system::{self, SystemModule},
        toolchains::{self, ToolchainsModule},
    },
    ports::*,
    Engine, Services,
};
use std::{fs, sync::Arc, sync::Mutex, time::Duration};

fn now() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs_f64()
}
/// Runs `kind` on a finding, confirming its stated loss as the user would.
fn run(engine: &Engine, finding: &Finding, kind: ActionKind, context: ScanContext) -> ActionResult {
    let confirmed = finding.acknowledged_actions.contains(&kind);
    engine
        .execute(
            &ActionRequest {
                findings: vec![finding.clone()],
                kind,
                context,
                acknowledged: confirmed.then(|| finding.id.clone()).into_iter().collect(),
                force: false,
            },
            &ScanControl::default(),
        )
        .remove(0)
}

#[test]
fn time_machine_snapshot_names_and_dates_are_read() {
    let names = system::parse_snapshots(
        "Snapshots for disk /:\ncom.apple.TimeMachine.2026-10-02-091500.local\ncom.apple.TimeMachine.2026-09-30-101010.local\ncom.apple.os.update-ABC\n",
    );
    assert_eq!(
        names,
        [
            "com.apple.TimeMachine.2026-09-30-101010.local",
            "com.apple.TimeMachine.2026-10-02-091500.local"
        ]
    );
    assert_eq!(system::snapshot_date(&names[0]), "2026-09-30 10:10:10");
}

#[test]
fn system_data_thins_local_snapshots_and_only_describes_system_folders() {
    let f = Fixture::new();
    f.write("system/Library/Updates/061-12345/update.pkg", "update");
    let list = "Snapshots for disk /:\ncom.apple.TimeMachine.2026-09-30-101010.local\ncom.apple.TimeMachine.2026-10-02-091500.local\n";
    let runner = StubRunner::new(vec![
        output(list, 0),
        // The action lists again, thins, then lists what is left.
        output(list, 0),
        output("Thinned local snapshots:\n", 0),
        output(
            "Snapshots for disk /:\ncom.apple.TimeMachine.2026-10-02-091500.local\n",
            0,
        ),
    ]);
    let mut s = services(&f);
    s.commands = runner.clone();
    let module = Arc::new(SystemModule {
        folders: Some(vec![(
            "macos-updates".into(),
            vec![f.at("system/Library/Updates")],
        )]),
        time_machine: true,
    });
    let engine = Engine::new(s, builtin(&f).register(module));
    let report = engine.scan_report(
        &["system".into()],
        &ScanContext::default(),
        &ScanControl::default(),
    );
    let mut titles: Vec<_> = report.findings.iter().map(|r| r.title.as_str()).collect();
    titles.sort();
    assert_eq!(
        titles,
        ["Time Machine local snapshots", "macOS update downloads"],
        "{:?}",
        report.warnings
    );
    let updates = report
        .findings
        .iter()
        .find(|r| r.title == "macOS update downloads")
        .unwrap();
    assert!(updates.actions.is_empty());
    assert!(updates.bytes.unwrap() > 0);
    assert!(updates
        .blocked_reason
        .as_deref()
        .unwrap()
        .contains("Software Update"));
    let snapshots = report
        .findings
        .iter()
        .find(|r| r.title == "Time Machine local snapshots")
        .unwrap();
    // Changes made while the backup disk was away may exist only here: confirmed first.
    assert!(snapshots.actions.is_empty());
    assert_eq!(snapshots.acknowledged_actions, [ActionKind::RunCommand]);
    assert!(snapshots.acknowledgement.is_some());
    assert_eq!(snapshots.risk, Risk::Permanent);
    assert_eq!(snapshots.value("Snapshots"), Some("2"));
    assert_eq!(snapshots.value("Oldest"), Some("2026-09-30 10:10:10"));

    let result = run(
        &engine,
        snapshots,
        ActionKind::RunCommand,
        ScanContext::default(),
    );
    assert_eq!(result.outcome, Outcome::Applied, "{}", result.message);
    assert!(result.message.contains("Removed 1"), "{}", result.message);
    assert_eq!(
        runner.calls()[2],
        [
            "/usr/bin/tmutil",
            "thinlocalsnapshots",
            "/",
            "999999999999999",
            "4"
        ]
    );
    // A scan limited to chosen folders never lists system data.
    let limited = engine.scan_report(
        &["system".into()],
        &ScanContext {
            roots: vec![f.path()],
            limit_to_roots: true,
            ..Default::default()
        },
        &ScanControl::default(),
    );
    assert!(limited.findings.is_empty());
}

#[test]
fn homebrew_old_versions_are_measured_and_cleaned_by_brew() {
    assert_eq!(
        toolchains::brew_freed(
            "==> This operation would free approximately 1.5GB of disk space.\n"
        ),
        Some(1_610_612_736)
    );
    assert_eq!(toolchains::brew_freed("Nothing to clean\n"), None);
    let f = Fixture::new();
    let brew = f.write("bin/brew", "#!/bin/sh");
    let dry = "Would remove: /opt/homebrew/Cellar/node/22.1.0 (2,000 files, 70MB)\nWould remove: /opt/homebrew/Cellar/python@3.12/3.12.3 (3,000 files, 60MB)\nWould remove: /Users/me/Library/Caches/Homebrew/downloads/abc--x.bottle.tar.gz (1MB)\n==> This operation would free approximately 131MB of disk space.\n";
    let runner = StubRunner::new(vec![output(dry, 0), output("Removing: ...\n", 0)]);
    let mut s = services(&f);
    s.commands = runner.clone();
    let module = Arc::new(ToolchainsModule {
        home: Some(f.dir("home")),
        package_tools: Some(vec![brew.clone()]),
        ..Default::default()
    });
    let engine = Engine::new(s, builtin(&f).register(module));
    let findings = engine
        .scan_report(
            &["toolchains".into()],
            &f.context(),
            &ScanControl::default(),
        )
        .findings;
    assert_eq!(findings.len(), 1, "{findings:?}");
    let old = &findings[0];
    assert_eq!(old.title, "Homebrew old versions");
    assert_eq!(old.bytes, Some(131 * 1024 * 1024));
    assert_eq!(old.value("Old versions"), Some("2"));
    assert_eq!(old.actions, [ActionKind::RunCommand]);
    let result = run(&engine, old, ActionKind::RunCommand, f.context());
    assert_eq!(result.outcome, Outcome::Applied, "{}", result.message);
    assert_eq!(runner.calls()[1], [brew.as_str(), "cleanup"]);
}

#[test]
fn nix_dry_run_paths_are_read_once_each() {
    let paths = toolchains::nix_paths(
        "finding garbage collector roots...\nwould delete '/nix/store/abc-hello-2.12'\n/nix/store/def-zlib-1.3\n/nix/store/def-zlib-1.3\n0 store paths deleted\n",
    );
    assert_eq!(
        paths,
        ["/nix/store/abc-hello-2.12", "/nix/store/def-zlib-1.3"]
    );
}

#[test]
fn git_object_counts_are_read() {
    let objects = projects::parse_objects(
        "count: 6700\nsize: 204800\nin-pack: 10\npacks: 1\nsize-pack: 1000\nprune-packable: 0\ngarbage: 1\nsize-garbage: 2048\n",
    );
    assert_eq!(objects.loose_count, 6700);
    assert_eq!(objects.loose, 204_800 * 1024);
    assert_eq!(objects.garbage, 2048 * 1024);
    assert_eq!(objects.packed, 1000 * 1024);
}

/// Git answers from fixtures; `ditto` writes the archive it was asked for and `unzip`
/// passes it, so no real archiver runs.
struct ProjectRunner {
    git: Arc<StubRunner>,
    calls: Mutex<Vec<Vec<String>>>,
}
impl CommandRunner for ProjectRunner {
    fn run(
        &self,
        exe: &str,
        args: &[String],
        timeout: Duration,
        k: &ScanControl,
    ) -> Result<Output> {
        let mut call = vec![exe.to_owned()];
        call.extend(args.iter().cloned());
        self.calls.lock().unwrap().push(call);
        match exe {
            "/usr/bin/ditto" => {
                fs::write(args.last().unwrap(), "PK").unwrap();
                Ok(output("", 0))
            }
            "/usr/bin/unzip" => Ok(output("", 0)),
            _ => self.git.run(exe, args, timeout, k),
        }
    }
}

#[test]
fn idle_projects_hibernate_into_a_checked_archive_and_unpacked_repositories_get_git_gc() {
    let f = Fixture::new();
    f.write("work/old-app/.git/HEAD", "ref: refs/heads/main\n");
    f.write("work/old-app/src/main.rs", "fn main() {}");
    // A plugin a tool keeps in a hidden home folder is not a project.
    f.write(
        "home/.vim/plugged/plugin/.git/HEAD",
        "ref: refs/heads/main\n",
    );
    let commit = now() - 400.0 * 86_400.0;
    let objects = "count: 9000\nsize: 204800\nin-pack: 10\npacks: 1\nsize-pack: 1000\nprune-packable: 0\ngarbage: 0\nsize-garbage: 0\n";
    let git = StubRunner::new(vec![
        // Scan: settings and index checks, objects, then the last commit (checks again).
        output("", 0),
        output("", 0),
        output(objects, 0),
        output("", 0),
        output("", 0),
        output(&format!("{}\n", commit as u64), 0),
        // git gc: settings and index checks, objects before, gc, objects after.
        output("", 0),
        output("", 0),
        output(objects, 0),
        output("", 0),
        output("count: 0\nsize: 0\nsize-pack: 60000\nsize-garbage: 0\n", 0),
    ]);
    let runner = Arc::new(ProjectRunner {
        git: git.clone(),
        calls: Mutex::default(),
    });
    let s = Services {
        commands: runner.clone(),
        ..services(&f)
    };
    let module = Arc::new(ProjectsModule {
        home: Some(f.at("home")),
        // Files were just written, so time is moved on to make the project idle.
        now: Some(now() + 400.0 * 86_400.0),
        ..Default::default()
    });
    let engine = Engine::new(s, builtin(&f).register(module));
    let report = engine.scan_report(&["projects".into()], &f.context(), &ScanControl::default());
    let mut titles: Vec<_> = report.findings.iter().map(|r| r.title.as_str()).collect();
    titles.sort();
    assert_eq!(
        titles,
        ["Unpacked Git objects · old-app", "old-app"],
        "{:?}",
        report.warnings
    );
    let project = report
        .findings
        .iter()
        .find(|r| r.title == "old-app")
        .unwrap();
    assert_eq!(project.actions, [ActionKind::Archive]);
    assert_eq!(project.risk, Risk::Review);
    assert_eq!(
        project.value("Archive"),
        Some(format!("{}.zip", f.at("work/old-app")).as_str())
    );
    let gc = report
        .findings
        .iter()
        .find(|r| r.title.starts_with("Unpacked"))
        .unwrap();
    assert_eq!(gc.bytes, Some(204_800 * 1024));
    assert_eq!(gc.actions, [ActionKind::RunCommand]);

    let result = run(&engine, gc, ActionKind::RunCommand, f.context());
    assert_eq!(result.outcome, Outcome::Applied, "{}", result.message);
    assert!(git
        .calls()
        .iter()
        .any(|c| c.ends_with(&["gc".to_string(), "--quiet".to_string()])));

    let result = run(&engine, project, ActionKind::Archive, f.context());
    assert_eq!(result.outcome, Outcome::Applied, "{}", result.message);
    assert!(
        fs::metadata(f.at("work/old-app.zip")).is_ok(),
        "archive kept"
    );
    assert!(fs::metadata(f.at("work/old-app")).is_err(), "folder moved");
    assert!(result.trash_path.is_some(), "restorable from Activity");
    let calls = runner.calls.lock().unwrap().clone();
    let ditto = calls.iter().find(|c| c[0] == "/usr/bin/ditto").unwrap();
    assert_eq!(ditto[1..5], ["-c", "-k", "--sequesterRsrc", "--keepParent"]);
    assert_eq!(ditto[5], f.at("work/old-app"));
    assert!(calls
        .iter()
        .any(|c| c[0] == "/usr/bin/unzip" && c[1] == "-tqq"));
    // No partial archive is left behind.
    assert!(fs::read_dir(f.at("work")).unwrap().all(|e| !e
        .unwrap()
        .file_name()
        .to_string_lossy()
        .contains("hibernating")));
}

#[test]
fn a_project_is_not_hibernated_over_an_existing_archive() {
    let f = Fixture::new();
    f.write("work/old-app/.git/HEAD", "ref: refs/heads/main\n");
    f.write("work/old-app.zip", "earlier archive");
    let git = StubRunner::new(vec![
        output("", 0),
        output("", 0),
        output("count: 0\nsize: 0\nsize-pack: 0\nsize-garbage: 0\n", 0),
        output("", 0),
        output("", 0),
        output("1000\n", 0),
    ]);
    let s = Services {
        commands: git,
        ..services(&f)
    };
    let module = Arc::new(ProjectsModule {
        home: Some(f.at("home")),
        now: Some(now() + 400.0 * 86_400.0),
        ..Default::default()
    });
    let engine = Engine::new(s, builtin(&f).register(module));
    let report = engine.scan_report(&["projects".into()], &f.context(), &ScanControl::default());
    let project = report
        .findings
        .iter()
        .find(|r| r.title == "old-app")
        .unwrap();
    assert!(project.actions.is_empty());
    assert!(project
        .blocked_reason
        .as_deref()
        .unwrap()
        .contains("already exists"));
}
