mod common;
use common::*;
use cym_core::{model::*, ports::*, Engine};

fn plist(id: &str, version: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><plist version="1.0"><dict><key>CFBundleIdentifier</key><string>{id}</string><key>CFBundleShortVersionString</key><string>{version}</string></dict></plist>"#
    )
}
fn scan(engine: &Engine, f: &Fixture, module: &str) -> Vec<Finding> {
    let context = ScanContext {
        roots: vec![f.at("home")],
        ..Default::default()
    };
    engine
        .scan_report(&[module.into()], &context, &ScanControl::default())
        .findings
}
fn run(engine: &Engine, f: &Fixture, finding: &Finding, kind: ActionKind) -> ActionResult {
    engine
        .execute(
            &ActionRequest {
                findings: vec![finding.clone()],
                kind,
                context: ScanContext {
                    roots: vec![f.at("home")],
                    ..Default::default()
                },
                acknowledged: vec![],
                force: false,
            },
            &ScanControl::default(),
        )
        .remove(0)
}

#[test]
fn containers_list_their_disks_and_offer_only_their_own_cleanup() {
    let f = Fixture::new();
    let disk = f.write(
        "home/Library/Containers/com.docker.docker/Data/vms/0/data/Docker.raw",
        "sparse",
    );
    f.write("home/.docker/bin/docker", "#!/bin/sh");
    f.write("home/Library/Caches/lima/download/img", "x");
    let df = [
        r#"{"Active":"2","Reclaimable":"1.2GB (50%)","Size":"2.4GB","TotalCount":"5","Type":"Images"}"#,
        r#"{"Active":"1","Reclaimable":"0B","Size":"10MB","TotalCount":"1","Type":"Containers"}"#,
        r#"{"Active":"1","Reclaimable":"800MB","Size":"1GB","TotalCount":"3","Type":"Local Volumes"}"#,
        r#"{"Active":"0","Reclaimable":"3.5GB","Size":"3.5GB","TotalCount":"40","Type":"Build Cache"}"#,
    ]
    .join("\n");
    let socket = "unix:///Users/me/.docker/run/docker.sock\n";
    let runner = StubRunner::new(vec![
        output("desktop-linux\n", 0),
        output(socket, 0),
        output(&df, 0),
        // The action checks the context and its endpoint again, measures again, then prunes.
        output("desktop-linux\n", 0),
        output(socket, 0),
        output(&df, 0),
        output(
            "Deleted build cache objects:\nabc\n\nTotal reclaimed space: 3.5GB\n",
            0,
        ),
    ]);
    let mut s = services(&f);
    s.commands = runner.clone();
    let engine = Engine::new(s, builtin(&f));
    let findings = scan(&engine, &f, "containers");
    let titled = |t: &str| {
        findings.iter().find(|r| r.title == t).unwrap_or_else(|| {
            panic!(
                "{t}: {:?}",
                findings.iter().map(|r| &r.title).collect::<Vec<_>>()
            )
        })
    };

    let vm = titled("Docker Desktop virtual disk");
    assert!(vm
        .blocked_reason
        .as_deref()
        .unwrap()
        .contains("every image"));
    assert!(vm.actions.is_empty());
    let cache = titled("Docker build cache");
    assert_eq!(cache.bytes, Some(3_500_000_000));
    assert_eq!(cache.actions, vec![ActionKind::RunCommand]);
    assert_eq!(
        cache.value("Data path"),
        Some(format!("{disk}/builder-prune").as_str())
    );
    assert_eq!(cache.brand.as_deref(), Some("docker"));
    assert!(titled("Docker unused images")
        .actions
        .contains(&ActionKind::RunCommand));
    let volumes = titled("Docker unused volumes");
    assert!(volumes.actions.is_empty(), "volumes are never removed");
    assert!(
        findings
            .iter()
            .all(|r| r.title != "Docker stopped containers"),
        "nothing to free"
    );
    assert!(titled("Lima downloads")
        .actions
        .contains(&ActionKind::Trash));

    // The disk counts once: the cleanup items sit inside it.
    let totals = cym_core::analytics::analytics(&findings);
    let vm_and_lima = vm.disk_bytes().unwrap() + titled("Lima downloads").disk_bytes().unwrap();
    assert_eq!(totals.disk_bytes, vm_and_lima);

    let result = run(&engine, &f, cache, ActionKind::RunCommand);
    assert_eq!(result.outcome, Outcome::Applied, "{}", result.message);
    assert!(result.message.contains("3.5GB"));
    let docker = f.at("home/.docker/bin/docker");
    assert!(runner.calls().contains(&vec![
        docker.clone(),
        "builder".into(),
        "prune".into(),
        "--force".into()
    ]));
    // A disk is never moved, even when asked.
    let mut forced = vm.clone();
    forced.blocked_reason = None;
    forced.actions = vec![ActionKind::Trash];
    assert_eq!(
        run(&engine, &f, &forced, ActionKind::Trash).outcome,
        Outcome::Failed
    );
    assert!(std::fs::metadata(&disk).is_ok());
}

#[test]
fn docker_cleanup_stops_when_the_context_changed() {
    let f = Fixture::new();
    f.write("home/.docker/bin/docker", "#!/bin/sh");
    let mut s = services(&f);
    s.commands = StubRunner::new(vec![
        output("colima\n", 0),
        output("unix:///Users/me/.colima/default/docker.sock\n", 0),
    ]);
    let engine = Engine::new(s, builtin(&f));
    let mut finding = Finding::new(
        "containers",
        "docker:desktop-linux:image-prune",
        "Docker unused images",
        Resource::Command {
            tool: "docker".into(),
            task: "image-prune".into(),
        },
        "fixture",
    );
    finding.actions = vec![ActionKind::RunCommand];
    finding.details = vec![
        detail("Context", "desktop-linux"),
        detail("Endpoint", "unix:///Users/me/.docker/run/docker.sock"),
    ];
    let result = run(&engine, &f, &finding, ActionKind::RunCommand);
    assert_eq!(result.outcome, Outcome::Failed);
    assert!(
        result.message.contains("different engine"),
        "{}",
        result.message
    );
    // A task outside the fixed list never runs.
    finding.resource = Resource::Command {
        tool: "docker".into(),
        task: "volumes".into(),
    };
    finding.details.clear();
    let result = run(&engine, &f, &finding, ActionKind::RunCommand);
    assert_eq!(result.outcome, Outcome::Failed);
}

#[test]
fn xcode_previews_device_support_and_extra_installs() {
    let f = Fixture::new();
    f.write(
        "home/Library/Developer/Xcode/UserData/Previews/Simulator Devices/x",
        "data",
    );
    f.write(
        "home/Library/Developer/Xcode/watchOS DeviceSupport/Watch7,1 11.2/x",
        "x",
    );
    f.write("home/Library/Developer/Xcode/DocumentationCache/v1/x", "x");
    f.write("home/Library/Developer/CoreSimulator/Caches/dyld/x", "x");
    f.write(
        "home/Library/Developer/CoreSimulator/Devices/ABCDEF12-0000/data/Library/Caches/com.apple.containermanagerd/Dead/temp.1/x",
        "x",
    );
    // Two Xcode installs; the fixture's xcode-select points at Xcode.app.
    f.write(
        "home/Applications/Xcode.app/Contents/Info.plist",
        &plist("com.apple.dt.Xcode", "16.2"),
    );
    f.write(
        "home/Applications/Xcode-beta.app/Contents/Info.plist",
        &plist("com.apple.dt.Xcode", "26.0"),
    );
    f.write(
        "home/Applications/Other.app/Contents/Info.plist",
        &plist("com.example.other", "1"),
    );
    let runner = StubRunner::new(vec![output("", 0)]);
    let mut s = services(&f);
    s.commands = runner.clone();
    let engine = Engine::new(s, builtin(&f));
    let findings = scan(&engine, &f, "xcode");
    let titled = |t: &str| {
        findings.iter().find(|r| r.title == t).unwrap_or_else(|| {
            panic!(
                "{t}: {:?}",
                findings.iter().map(|r| &r.title).collect::<Vec<_>>()
            )
        })
    };

    let previews = titled("SwiftUI preview simulators");
    assert_eq!(previews.actions, vec![ActionKind::RunCommand]);
    assert!(previews.bytes.unwrap() > 0);
    assert!(titled("Watch7,1 11.2").actions.contains(&ActionKind::Trash));
    assert!(titled("Documentation cache")
        .actions
        .contains(&ActionKind::Trash));
    assert!(titled("Simulator caches")
        .actions
        .contains(&ActionKind::Trash));
    assert_eq!(
        titled("Removed app data · simulator ABCDEF12").risk,
        Risk::Review
    );
    assert!(
        titled("Xcode 16.2").blocked_reason.is_some(),
        "the selected Xcode stays"
    );
    assert!(titled("Xcode 26.0").actions.contains(&ActionKind::Trash));
    assert!(findings.iter().all(|r| !r.title.contains("Other")));

    let result = run(&engine, &f, previews, ActionKind::RunCommand);
    assert_eq!(result.outcome, Outcome::Applied, "{}", result.message);
    assert_eq!(
        runner.calls()[0],
        [
            "/usr/bin/xcrun",
            "simctl",
            "--set",
            "previews",
            "delete",
            "all"
        ]
    );
}

#[test]
fn simulator_runtimes_are_deleted_with_simctl() {
    let f = Fixture::new();
    let list = r#"{"devices":{"com.apple.CoreSimulator.SimRuntime.iOS-17-5":[{"udid":"6A6F4D10-0000-4000-8000-000000000001","name":"iPhone 15","state":"Shutdown","isAvailable":true}],"com.apple.CoreSimulator.SimRuntime.iOS-16-4":[{"udid":"6A6F4D10-0000-4000-8000-000000000002","name":"iPhone 14","state":"Shutdown","isAvailable":false}]},"runtimes":[{"identifier":"com.apple.CoreSimulator.SimRuntime.iOS-17-5","name":"iOS 17.5","version":"17.5"}]}"#;
    let images = r#"{"0F2E6A4C-1111-4222-8333-444455556666":{"identifier":"0F2E6A4C-1111-4222-8333-444455556666","runtimeIdentifier":"com.apple.CoreSimulator.SimRuntime.iOS-17-5","version":"17.5","build":"21F79","sizeBytes":7800000000,"deletable":true,"kind":"Disk Image","lastUsedAt":"2026-03-01T10:00:00Z","path":"/Library/Developer/CoreSimulator/Volumes/iOS_21F79"}}"#;
    let runner = StubRunner::new(vec![
        output(list, 0),
        output(images, 0),
        // The action: a fresh image list, a fresh inventory, then the delete.
        output(images, 0),
        output(list, 0),
        output("", 0),
    ]);
    let mut s = services(&f);
    s.commands = runner.clone();
    let engine = Engine::new(s, builtin(&f));
    let findings = scan(&engine, &f, "simulators");
    let runtime = findings
        .iter()
        .find(|r| r.title == "iOS 17.5")
        .expect("runtime");
    assert_eq!(runtime.bytes, Some(7_800_000_000));
    assert_eq!(runtime.actions, vec![ActionKind::RunCommand]);
    assert_eq!(
        findings.iter().filter(|r| r.title == "iOS 17.5").count(),
        1,
        "listed once"
    );
    let stale = findings.iter().find(|r| r.title == "iPhone 14").unwrap();
    assert_eq!(stale.badge.as_deref(), Some("Unavailable"));

    let result = run(&engine, &f, runtime, ActionKind::RunCommand);
    assert_eq!(result.outcome, Outcome::Applied, "{}", result.message);
    assert_eq!(
        runner.calls().last().unwrap(),
        &[
            "/usr/bin/xcrun",
            "simctl",
            "runtime",
            "delete",
            "0F2E6A4C-1111-4222-8333-444455556666"
        ]
    );
}

#[test]
fn installers_and_electron_caches() {
    let f = Fixture::new();
    f.write("home/Downloads/Figma-125.dmg", "x");
    f.write("home/Downloads/tools/node-v20.pkg", "x");
    f.write("home/Downloads/notes.txt", "x");
    f.write("home/Desktop/Xcode_16.xip", "x");
    f.write(
        "home/Applications/Install macOS Sequoia.app/Contents/Info.plist",
        &plist("com.apple.InstallAssistant.macOSSequoia", "15.4"),
    );
    // Slack is an Electron app; Notes keeps an unrelated folder named Cache.
    f.write(
        "home/Library/Application Support/Slack/Cache/Cache_Data/x",
        "x",
    );
    f.write(
        "home/Library/Application Support/Slack/Code Cache/js/x",
        "x",
    );
    f.write(
        "home/Library/Application Support/Slack/Local Storage/leveldb/x",
        "x",
    );
    f.write("home/Library/Application Support/Notes/Cache/x", "x");
    f.write("home/Library/Application Support/Cursor/Code Cache/x", "x");
    let engine = Engine::new(services(&f), builtin(&f));

    let installers = scan(&engine, &f, "installers");
    let names: Vec<&str> = installers.iter().map(|r| r.title.as_str()).collect();
    for name in [
        "Figma-125.dmg",
        "node-v20.pkg",
        "Xcode_16.xip",
        "Install macOS Sequoia",
    ] {
        assert!(names.contains(&name), "{name}: {names:?}");
    }
    assert!(!names.contains(&"notes.txt"));

    let caches = scan(&engine, &f, "caches");
    let titles: Vec<&str> = caches.iter().map(|r| r.title.as_str()).collect();
    assert!(titles.contains(&"Web cache · Slack"), "{titles:?}");
    assert!(titles.contains(&"Script cache · Slack"));
    assert!(
        !titles.iter().any(|t| t.ends_with("· Notes")),
        "no Chromium evidence"
    );
    assert!(
        !titles.iter().any(|t| t.ends_with("· Cursor")),
        "listed by AI Tools"
    );
    assert!(caches.iter().all(|r| !r
        .resource
        .path()
        .unwrap_or_default()
        .contains("Local Storage")));
}

#[test]
fn a_remote_docker_engine_is_never_cleaned() {
    let f = Fixture::new();
    f.write("home/.docker/bin/docker", "#!/bin/sh");
    let runner = StubRunner::new(vec![
        output("build-server\n", 0),
        output("ssh://ci@build.example.com\n", 0),
    ]);
    let mut s = services(&f);
    s.commands = runner.clone();
    let engine = Engine::new(s, builtin(&f));
    let context = ScanContext {
        roots: vec![f.at("home")],
        ..Default::default()
    };
    let report = engine.scan_report(&["containers".into()], &context, &ScanControl::default());
    assert!(report.findings.is_empty());
    assert!(report
        .warnings
        .iter()
        .any(|w| w.contains("not an engine on this Mac")));
    assert_eq!(runner.calls().len(), 2, "system df never ran");
}

#[test]
fn docker_cleanup_stops_when_it_would_free_more_than_reviewed() {
    let f = Fixture::new();
    f.write("home/.docker/bin/docker", "#!/bin/sh");
    let socket = "unix:///Users/me/.docker/run/docker.sock\n";
    let grown = r#"{"Active":"0","Reclaimable":"9.5GB","Size":"9.5GB","TotalCount":"80","Type":"Build Cache"}"#;
    let runner = StubRunner::new(vec![
        output("desktop-linux\n", 0),
        output(socket, 0),
        output(grown, 0),
    ]);
    let mut s = services(&f);
    s.commands = runner.clone();
    let engine = Engine::new(s, builtin(&f));
    let mut finding = Finding::new(
        "containers",
        "docker:desktop-linux:builder-prune",
        "Docker build cache",
        Resource::Command {
            tool: "docker".into(),
            task: "builder-prune".into(),
        },
        "fixture",
    );
    finding.bytes = Some(3_500_000_000);
    finding.actions = vec![ActionKind::RunCommand];
    finding.details = vec![
        detail("Context", "desktop-linux"),
        detail("Endpoint", "unix:///Users/me/.docker/run/docker.sock"),
    ];
    let result = run(&engine, &f, &finding, ActionKind::RunCommand);
    assert_eq!(result.outcome, Outcome::Failed);
    assert!(result.message.contains("more than"), "{}", result.message);
    assert_eq!(runner.calls().len(), 3, "prune never ran");
}
