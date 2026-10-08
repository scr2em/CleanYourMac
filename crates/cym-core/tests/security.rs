mod common;
use common::*;
use cym_core::{
    adapters::fs::rename_exclusive,
    git::{gitlink, risky_key, Git},
    model::*,
    ports::*,
    Engine,
};

#[test]
fn repository_settings_that_run_programs_are_found() {
    // `git config --list --show-scope -z`: the scope, NUL, then `key\nvalue`, NUL.
    let config = |entries: &[(&str, &str)]| {
        entries
            .iter()
            .flat_map(|(scope, entry)| [scope.as_bytes(), b"\0", entry.as_bytes(), b"\0"].concat())
            .collect::<Vec<u8>>()
    };
    assert_eq!(
        risky_key(&config(&[
            ("global", "filter.lfs.clean\ngit-lfs clean"),
            ("local", "core.bare\nfalse")
        ])),
        None,
        "only the repository's own settings count"
    );
    assert_eq!(
        risky_key(&config(&[
            ("local", "core.bare\nfalse"),
            ("local", "filter.x.clean\nsh -c evil")
        ]))
        .as_deref(),
        Some("filter.x.clean")
    );
    assert!(risky_key(&config(&[("local", "diff.pdf.textconv\npdftotext")])).is_some());
    assert!(risky_key(&config(&[("worktree", "gpg.program\n/tmp/x")])).is_some());
    assert!(risky_key(&config(&[("local", "include.path\n../evil")])).is_some());
    assert!(risky_key(&config(&[("local", "remote.origin.url\nhttps://x")])).is_none());
    assert_eq!(
        gitlink(
            &[
                &b"100644 abc 0\treadme.md"[..],
                b"\0",
                b"160000 def 0\tsub",
                b"\0"
            ]
            .concat()
        )
        .as_deref(),
        Some("sub")
    );
}

#[test]
fn real_git_settings_and_nested_repositories_are_caught() {
    if std::fs::metadata("/usr/bin/git").is_err() {
        return;
    }
    let f = Fixture::new();
    let s = services(&f);
    let k = ScanControl::default();
    let git = Git(s.commands.as_ref());
    let ok = |path: &str, args: &[&str]| {
        assert_eq!(git.run(path, args, &k).unwrap().status, 0, "{args:?}")
    };
    let clean = f.dir("clean");
    ok(&clean, &["init", "-q", "-b", "main"]);
    assert_eq!(git.risky_config(&clean, &k).unwrap(), None);
    let filtered = f.dir("filtered");
    ok(&filtered, &["init", "-q", "-b", "main"]);
    ok(&filtered, &["config", "filter.evil.clean", "touch marker"]);
    assert_eq!(
        git.risky_config(&filtered, &k).unwrap().as_deref(),
        Some("filter.evil.clean")
    );
    // A nested repository recorded as a gitlink.
    let outer = f.dir("outer");
    ok(&outer, &["init", "-q", "-b", "main"]);
    let inner = f.dir("outer/sub");
    ok(&inner, &["init", "-q", "-b", "main"]);
    f.write("outer/sub/a.txt", "a");
    ok(&inner, &["add", "a.txt"]);
    ok(
        &inner,
        &[
            "-c",
            "user.name=F",
            "-c",
            "user.email=f@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-q",
            "-m",
            "a",
        ],
    );
    ok(&outer, &["add", "sub"]);
    assert_eq!(
        git.risky_config(&outer, &k).unwrap().as_deref(),
        Some("nested repository sub")
    );
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

#[test]
fn example_results_are_never_acted_on() {
    let f = Fixture::new();
    let keep = f.write("keep.txt", "keep");
    let engine = Engine::new(services(&f), builtin(&f));
    let identity = engine
        .services
        .snapshot(&keep, &ScanControl::default())
        .unwrap();
    engine
        .results
        .insert(vec![file_finding("storage:keep", "storage", identity)]);
    engine
        .example
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let result = engine
        .execute_ids(
            &["storage:keep".into()],
            ActionKind::Trash,
            &f.context(),
            &ScanControl::default(),
        )
        .remove(0);
    assert_eq!(result.outcome, Outcome::Failed);
    assert!(result.message.contains("example"), "{}", result.message);
    assert!(std::fs::metadata(&keep).is_ok());
}
