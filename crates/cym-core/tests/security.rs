mod common;
use common::*;
use cym_core::{adapters::fs::rename_exclusive, git::risky_key, model::*, ports::*, Engine};

#[test]
fn repository_settings_that_run_programs_are_found() {
    let config = |entries: &[&str]| entries.join("\0").into_bytes();
    assert_eq!(
        risky_key(&config(&[
            "global\tfilter.lfs.clean\ngit-lfs clean",
            "local\tcore.bare\nfalse"
        ])),
        None,
        "only the repository's own settings count"
    );
    assert_eq!(
        risky_key(&config(&[
            "local\tcore.bare\nfalse",
            "local\tfilter.x.clean\nsh -c evil"
        ]))
        .as_deref(),
        Some("filter.x.clean")
    );
    assert!(risky_key(&config(&["local\tdiff.pdf.textconv\npdftotext"])).is_some());
    assert!(risky_key(&config(&["worktree\tgpg.program\n/tmp/x"])).is_some());
    assert!(risky_key(&config(&["local\tinclude.path\n../evil"])).is_some());
    assert!(risky_key(&config(&["local\tremote.origin.url\nhttps://x"])).is_none());
}

#[test]
fn paths_must_be_plain_and_restores_come_from_the_journal() {
    let f = Fixture::new();
    let keep = f.write("home/keep/notes.txt", "keep");
    let engine = Engine::new(services(&f), builtin(&f));
    let identity = engine
        .services
        .snapshot(&keep, &ScanControl::default())
        .unwrap();
    // A finding whose path takes a detour through `..` is refused before anything moves.
    let mut detour = identity.clone();
    detour.path = f.at("home/other/../keep/notes.txt");
    let finding = file_finding("storage:x", "storage", detour);
    let result = engine
        .execute(
            &ActionRequest {
                findings: vec![finding],
                kind: ActionKind::Trash,
                context: f.context(),
                acknowledged: vec![],
                force: false,
            },
            &ScanControl::default(),
        )
        .remove(0);
    assert_eq!(result.outcome, Outcome::Failed);
    assert!(result.message.contains("plain form"), "{}", result.message);
    assert!(std::fs::metadata(&keep).is_ok());

    // A restore that is not in the journal is refused, whatever paths it names.
    let forged = ActionResult {
        id: "forged".into(),
        date: 0.0,
        title: "x".into(),
        original_path: Some(f.at("home/restored")),
        action: ActionKind::Trash,
        outcome: Outcome::Applied,
        message: String::new(),
        trash_path: Some(keep.clone()),
        finding_id: None,
        trash_identity: Some(identity),
        journal_warning: None,
        overridable: false,
    };
    assert!(engine.restore(&forged).is_err());
    assert!(std::fs::metadata(&keep).is_ok());
}

#[test]
fn restoring_never_replaces_a_new_file() {
    let f = Fixture::new();
    let from = f.write("a.txt", "trashed");
    let to = f.write("b.txt", "new");
    assert!(rename_exclusive(&from, &to).is_err());
    assert_eq!(std::fs::read_to_string(&to).unwrap(), "new");
    let free = f.at("c.txt");
    rename_exclusive(&from, &free).unwrap();
    assert_eq!(std::fs::read_to_string(&free).unwrap(), "trashed");
}
