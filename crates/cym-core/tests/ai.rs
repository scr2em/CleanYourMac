mod common;
use common::*;
use cym_core::{
    model::*,
    modules::ai::{decode_project, session_info, AiToolsModule},
    ports::*,
    Engine,
};
use std::sync::Arc;

/// A transcript line as Claude Code writes it.
fn line(kind: &str, cwd: &str, extra: &str) -> String {
    format!(
        r#"{{"type":"{kind}","cwd":"{cwd}","sessionId":"s","timestamp":"2026-10-01T10:00:00Z","gitBranch":"main"{extra}}}"#
    )
}

fn scan(f: &Fixture, env: Vec<(String, String)>) -> Vec<Finding> {
    let module = AiToolsModule {
        home: Some(f.at("home")),
        env: Some(env),
        ..Default::default()
    };
    let engine = Engine::new(services(f), builtin(f).register(Arc::new(module)));
    let context = ScanContext {
        roots: vec![f.at("home")],
        ..Default::default()
    };
    engine
        .scan_report(&["ai".into()], &context, &ScanControl::default())
        .findings
}

#[test]
fn transcripts_show_their_project_prompt_and_whether_the_project_still_exists() {
    let info = session_info(&[
        line("user", "/Users/x/app", r#","isMeta":true,"message":{"content":"<command-name>/clear</command-name>"}"#),
        line("user", "/Users/x/app", r#","message":{"content":[{"type":"text","text":"  Fix the   login bug\nplease "}]}"#),
        line("user", "/Users/x/app", r#","message":{"content":"second prompt"}"#),
    ]
    .join("\n"));
    assert_eq!(info.cwd.as_deref(), Some("/Users/x/app"));
    assert_eq!(info.branch.as_deref(), Some("main"));
    assert_eq!(
        info.first_prompt.as_deref(),
        Some("Fix the login bug please")
    );
    assert_eq!(
        decode_project("-Users-x-projects-foo"),
        "/Users/x/projects/foo"
    );

    let f = Fixture::new();
    let live = f.dir("home/projects/live");
    let gone = f.at("home/projects/gone");
    let claude = "home/.claude";
    // A project that still exists, with a session that has bulky side folders.
    let p1 = format!("{claude}/projects/-live");
    f.write(
        &format!("{p1}/a.jsonl"),
        &line(
            "user",
            &live,
            r#","message":{"content":"build the parser"}"#,
        ),
    );
    f.write(&format!("{p1}/a/subagents/agent-1.jsonl"), "x");
    f.write(&format!("{p1}/a/tool-results/out.txt"), "x");
    // An orphan project, and one whose memory must stay.
    let p2 = format!("{claude}/projects/-gone");
    f.write(
        &format!("{p2}/b.jsonl"),
        &line("user", &gone, r#","message":{"content":"old work"}"#),
    );
    let p3 = format!("{claude}/projects/-gone-with-memory");
    f.write(
        &format!("{p3}/c.jsonl"),
        &line(
            "user",
            &format!("{gone}2"),
            r#","message":{"content":"remembered"}"#,
        ),
    );
    f.write(&format!("{p3}/memory/notes.md"), "keep");
    // Leftovers of a removed transcript, caches, and files never to touch.
    f.write(&format!("{p1}/z/tool-results/old.txt"), "x");
    f.write(&format!("{claude}/plugins/cache/p/x"), "x");
    f.write(&format!("{claude}/shell-snapshots/snap.sh"), "x");
    f.write(&format!("{claude}/settings.json"), "{}");
    f.write(&format!("{claude}/skills/s/SKILL.md"), "x");
    f.write(&format!("{claude}/plugins/installed_plugins.json"), "{}");
    // A background job that is blocked with open tasks.
    f.write(
        &format!("{claude}/jobs/j1/state.json"),
        r#"{"status":"blocked","todos":[{"content":"ship it","status":"pending"},{"content":"done","status":"completed"}]}"#,
    );
    f.write(&format!("{claude}/jobs/j1/tmp/shot.png"), "x");
    // A second account that is empty, and one named by CLAUDE_CONFIG_DIR.
    f.dir("home/.claude-pers");
    f.write("elsewhere/claude-work/debug/log.txt", "x");

    let findings = scan(
        &f,
        vec![("CLAUDE_CONFIG_DIR".into(), f.at("elsewhere/claude-work"))],
    );
    let tier = |r: &Finding| r.value("Tier").unwrap_or_default().to_owned();
    let find = |part: &str| {
        findings
            .iter()
            .find(|r| r.resource.path().is_some_and(|p| p.ends_with(part)))
    };

    let session = find("-live/a.jsonl").expect("session");
    assert_eq!(session.title, "build the parser");
    assert_eq!(tier(session), "Caution");
    assert_eq!(session.value("Project folder"), None);
    assert!(find("-live/a/subagents").is_some_and(|r| tier(r) == "Review"));
    assert!(find("-live/a/tool-results").is_some_and(|r| tier(r) == "Review"));
    assert!(
        find("-live/z").is_some_and(|r| tier(r) == "Safe"),
        "leftovers"
    );

    let orphan = find("-gone/b.jsonl").unwrap();
    assert_eq!(orphan.value("Project folder"), Some("Missing"));
    assert!(orphan.reason.starts_with("Orphan"));
    let project = find("projects/-gone").expect("orphan project row");
    assert!(project.blocked_reason.is_none());
    let remembered = find("projects/-gone-with-memory").unwrap();
    assert!(remembered
        .blocked_reason
        .as_deref()
        .unwrap()
        .contains("memory"));

    assert!(find("plugins/cache").is_some_and(|r| tier(r) == "Safe"));
    assert!(find("shell-snapshots").is_some());
    let job = find("jobs/j1/tmp").unwrap();
    assert_eq!(job.value("Open tasks"), Some("1"));
    assert!(job.reason.contains("blocked with 1 open task"));
    assert!(find(".claude-pers").is_some_and(|r| r.title.starts_with("Empty folder")));
    assert!(
        find("claude-work/debug").is_some(),
        "CLAUDE_CONFIG_DIR is scanned"
    );

    // Never listed: settings, skills, plugin registry, memory.
    for never in [
        "settings.json",
        "/skills",
        "installed_plugins.json",
        "/memory",
    ] {
        assert!(
            findings
                .iter()
                .all(|r| !r.resource.path().unwrap_or_default().contains(never)),
            "{never} must never be listed"
        );
    }
}

#[test]
fn a_running_session_is_never_moved() {
    let f = Fixture::new();
    let project = f.dir("home/projects/app");
    f.write(
        "home/.claude/projects/-app/live.jsonl",
        &line("user", &project, r#","message":{"content":"working"}"#),
    );
    // sessions/<pid>.json names the running session; FakeProcesses reports PID 4242 alive.
    f.write(
        "home/.claude/sessions/4242.json",
        &format!(r#"{{"pid":4242,"sessionId":"live","cwd":"{project}"}}"#),
    );
    let mut s = services(&f);
    let mut claude = process("/usr/local/bin/claude", 1, 501);
    claude.identity.pid = 4242;
    s.processes = Arc::new(FakeProcesses {
        rows: std::sync::Mutex::new(vec![claude]),
        ..Default::default()
    });
    let module = AiToolsModule {
        home: Some(f.at("home")),
        env: Some(vec![]),
        ..Default::default()
    };
    let engine = Engine::new(s, builtin(&f).register(Arc::new(module)));
    let context = ScanContext {
        roots: vec![f.at("home")],
        ..Default::default()
    };
    let findings = engine
        .scan_report(&["ai".into()], &context, &ScanControl::default())
        .findings;
    let live = findings
        .iter()
        .find(|r| r.resource.path().is_some_and(|p| p.ends_with("live.jsonl")))
        .unwrap();
    assert_eq!(
        live.blocked_reason.as_deref(),
        Some("This session is running in Claude Code.")
    );
    // Even forced, the running session's transcript stays.
    let mut unblocked = live.clone();
    unblocked.blocked_reason = None;
    unblocked.actions = vec![ActionKind::Trash];
    let result = engine
        .execute(
            &ActionRequest {
                findings: vec![unblocked],
                kind: ActionKind::Trash,
                context,
                acknowledged: vec![],
                force: true,
            },
            &ScanControl::default(),
        )
        .remove(0);
    assert_eq!(result.outcome, Outcome::Failed, "{}", result.message);
    assert!(std::fs::metadata(f.at("home/.claude/projects/-app/live.jsonl")).is_ok());
}
