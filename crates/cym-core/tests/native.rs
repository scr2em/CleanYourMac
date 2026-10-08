//! Opt-in macOS checks against real system services. Run with `bash scripts/test-native.sh`.
//! Each test acts only on resources it created itself.
#![cfg(target_os = "macos")]
mod common;
use common::*;
use cym_core::{
    adapters::journal::MemoryJournal, model::*, modules, orphans, ports::*, simulator::Simctl,
    Engine, Services,
};
use std::{fs, io::Read, process::Command, sync::Arc, thread, time::Duration};

fn native() -> Services {
    Services::with_journal(Arc::new(MemoryJournal::default()))
}

#[test]
#[ignore]
fn native_trash_round_trip() {
    let f = Fixture::new();
    let path = f.write(
        &format!("CleanYourMac-native-test-{}.txt", uuid::Uuid::new_v4()),
        "Disposable native Trash fixture",
    );
    let engine = Engine::new(native(), modules::builtin());
    let k = ScanControl::default();
    let finding = file_finding(
        "native",
        "large",
        engine.services.entry(&path).unwrap().identity,
    );
    let result = engine
        .execute(
            &ActionRequest {
                findings: vec![finding],
                kind: ActionKind::Trash,
                context: f.context(),
                acknowledged: vec![],
                force: false,
            },
            &k,
        )
        .remove(0);
    assert_eq!(result.outcome, Outcome::Applied, "{}", result.message);
    assert!(fs::metadata(&path).is_err());
    engine.restore(&result).unwrap();
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "Disposable native Trash fixture"
    );
}

/// Starts the C fixture, which forks a detached child and prints its PID.
fn orphan(ignore_term: bool) -> i32 {
    let executable = std::env::var("CYM_ORPHAN_FIXTURE").expect("CYM_ORPHAN_FIXTURE");
    let mut child = Command::new(executable)
        .args(if ignore_term {
            vec!["ignore-term"]
        } else {
            vec![]
        })
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut text = String::new();
    child
        .stdout
        .take()
        .unwrap()
        .read_to_string(&mut text)
        .unwrap();
    child.wait().unwrap();
    thread::sleep(Duration::from_millis(200));
    text.trim().parse().unwrap()
}

#[test]
#[ignore]
fn native_orphan_termination_and_explicit_force_quit() {
    let s = native();
    let k = ScanControl::default();
    let context = ScanContext::default();
    for ignore_term in [false, true] {
        let pid = orphan(ignore_term);
        let identity = s.processes.inspect(pid).unwrap().identity;
        let candidates = orphans::candidates(&s, &context, &k).unwrap();
        assert!(candidates.iter().any(|(p, _)| p.identity == identity));
        let outcome = orphans::signal(&s, &identity, false, &context, &k).unwrap();
        if ignore_term {
            assert_eq!(outcome, Outcome::Requested);
            assert!(orphans::signal(&s, &identity, true, &context, &k).is_err());
            thread::sleep(Duration::from_secs(2));
            assert_eq!(
                orphans::signal(&s, &identity, true, &context, &k).unwrap(),
                Outcome::Applied
            );
        } else {
            assert_eq!(outcome, Outcome::Applied);
        }
        if s.processes
            .inspect(pid)
            .is_some_and(|p| p.identity == identity)
        {
            unsafe { libc::kill(pid, libc::SIGKILL) };
            panic!("fixture process survived");
        }
    }
}

#[test]
#[ignore]
fn native_simulator_actions_use_only_a_new_disposable_device() {
    let s = native();
    let k = ScanControl::default();
    let simctl = Simctl(s.commands.as_ref());
    let inventory = simctl.inventory(&k).unwrap();
    let runtime = inventory
        .runtimes
        .iter()
        .find(|r| r.identifier.contains(".iOS-") && r.is_available == Some(true))
        .expect("an installed iOS runtime");
    let run = |args: &[&str]| {
        let mut arguments = vec!["simctl".to_string()];
        arguments.extend(args.iter().map(|a| a.to_string()));
        s.commands
            .run("/usr/bin/xcrun", &arguments, Duration::from_secs(60), &k)
            .unwrap()
    };
    let name = format!("CleanYourMac Disposable Fixture {}", uuid::Uuid::new_v4());
    let created = run(&[
        "create",
        &name,
        "com.apple.CoreSimulator.SimDeviceType.iPhone-SE-3rd-generation",
        &runtime.identifier,
    ]);
    assert_eq!(created.status, 0, "{}", created.error);
    // The UUID comes only from this create result, never from existing devices.
    let id = created.text().trim().to_owned();
    uuid::Uuid::parse_str(&id).unwrap();
    let context = ScanContext::default();
    let outcome = simctl
        .act(&id, ActionKind::ResetSimulator, &context, &k)
        .and_then(|_| simctl.act(&id, ActionKind::DeleteSimulator, &context, &k));
    if outcome.is_err() {
        run(&["delete", &id]);
    }
    outcome.unwrap();
    assert!(!simctl
        .inventory(&k)
        .unwrap()
        .devices
        .values()
        .flatten()
        .any(|d| d.udid == id));
}
