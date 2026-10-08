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

#[test]
fn old_claude_versions_and_unrelated_folders() {
    let f = Fixture::new();
    // Four native versions; the launcher points at the oldest one.
    for (i, v) in ["2.1.1", "2.1.2", "2.1.3", "2.1.4"].iter().enumerate() {
        let path = f.write(&format!("home/.local/share/claude/versions/{v}"), "binary");
        let when = std::time::SystemTime::UNIX_EPOCH
            + std::time::Duration::from_secs(1_700_000_000 + i as u64 * 1000);
        std::fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(when)
            .unwrap();
    }
    f.dir("home/.local/bin");
    std::os::unix::fs::symlink(
        "../share/claude/versions/2.1.1",
        f.at("home/.local/bin/claude"),
    )
    .unwrap();
    // A different tool that happens to start with .claude-, and a real second account.
    f.write("home/.claude-code-router/config.json", "{}");
    f.write("home/.claude-work/settings.json", "{}");
    f.write("home/.claude-work/debug/x.txt", "x");
    f.write(
        "home/Library/Caches/claude-cli-nodejs/-app/errors/e.txt",
        "x",
    );
    let findings = scan(&f, vec![]);
    let titles: Vec<&str> = findings.iter().map(|r| r.title.as_str()).collect();
    // Kept: the launcher's target (2.1.1) and the two newest (2.1.4, 2.1.3).
    assert!(
        titles.contains(&"Old Claude Code version 2.1.2"),
        "{titles:?}"
    );
    for kept in ["2.1.1", "2.1.3", "2.1.4"] {
        assert!(
            !titles.contains(&format!("Old Claude Code version {kept}").as_str()),
            "{kept}"
        );
    }
    assert!(findings.iter().all(|r| !r
        .resource
        .path()
        .unwrap_or_default()
        .contains(".claude-code-router")));
    assert!(findings.iter().any(|r| r
        .resource
        .path()
        .unwrap_or_default()
        .ends_with(".claude-work/debug")));
    assert!(titles.contains(&"MCP and error logs"));
}

#[test]
fn codex_gemini_and_cli_sessions_show_their_project() {
    let f = Fixture::new();
    let live = f.dir("home/projects/live");
    let gone = f.at("home/projects/gone");
    f.write(
        "home/.codex/sessions/2026/09/01/rollout-2026-09-01T08-00-00-a.jsonl",
        &[
            format!(r#"{{"type":"session_meta","payload":{{"id":"a","timestamp":"2026-09-01T08:00:00Z","cwd":"{gone}","git":{{"branch":"fix"}}}}}}"#),
            r#"{"type":"event_msg","payload":{"type":"user_message","message":"rename the module"}}"#.to_owned(),
        ]
        .join("\n"),
    );
    f.write("home/.codex/log/codex-tui.log", "x");
    f.write("home/.codex/config.toml", "model = \"x\"");
    f.write("home/.codex/auth.json", "{}");
    // Gemini CLI: the project is named in .project_root beside the chats.
    f.write("home/.gemini/tmp/abc123/.project_root", &live);
    f.write(
        "home/.gemini/tmp/abc123/chats/session-1.json",
        r#"{"startTime":"2026-09-02T09:00:00Z","messages":[{"type":"user","content":"write docs"}]}"#,
    );
    f.write("home/.gemini/tmp/bin/rg", "x");
    f.write("home/.gemini/oauth_creds.json", "{}");
    f.write("home/.gemini/settings.json", "{}");
    // Copilot CLI keeps a workspace.yaml in each session folder.
    f.write(
        "home/.copilot/session-state/s1/workspace.yaml",
        &format!("id: s1\ncwd: {live}\nsummary: Add login page\n"),
    );
    f.write("home/.copilot/config.json", "{}");

    let findings = scan(&f, vec![]);
    let find = |part: &str| {
        findings
            .iter()
            .find(|r| r.resource.path().is_some_and(|p| p.ends_with(part)))
    };
    let rollout = find("rollout-2026-09-01T08-00-00-a.jsonl").expect("codex session");
    assert_eq!(rollout.value("Tool"), Some("Codex"));
    assert_eq!(rollout.value("Tier"), Some("Caution"));
    assert_eq!(rollout.value("First prompt"), Some("rename the module"));
    assert_eq!(rollout.value("Branch"), Some("fix"));
    assert_eq!(rollout.value("Project folder"), Some("Missing"));
    assert!(rollout.reason.starts_with("Orphan"));
    assert!(find(".codex/log").is_some_and(|r| r.value("Tier") == Some("Safe")));
    assert_eq!(
        rollout.badge.as_deref(),
        Some("Caution"),
        "the tier is the badge"
    );

    let chats = find("tmp/abc123/chats").expect("gemini chats");
    assert_eq!(chats.value("Tool"), Some("Gemini CLI"));
    assert_eq!(chats.value("Project"), Some("~/projects/live"));
    assert_eq!(chats.value("First prompt"), Some("write docs"));
    assert_eq!(chats.value("Project folder"), None);
    assert_eq!(chats.brand.as_deref(), Some("googlegemini"));
    assert_eq!(rollout.brand.as_deref(), None, "no Codex logo");
    assert!(find("tmp/bin").is_some());

    let copilot = find("session-state/s1").expect("copilot session");
    assert_eq!(copilot.title, "Session · Add login page");
    assert_eq!(copilot.value("Project"), Some("~/projects/live"));

    for never in [
        "config.toml",
        "auth.json",
        "oauth_creds.json",
        "settings.json",
        "config.json",
    ] {
        assert!(
            findings
                .iter()
                .all(|r| !r.resource.path().unwrap_or_default().ends_with(never)),
            "{never} must never be listed"
        );
    }
}

#[test]
fn editors_list_orphan_workspaces_and_extension_tasks() {
    let f = Fixture::new();
    let live = f.dir("home/projects/live");
    let gone = f.at("home/projects/gone app");
    let cursor = "home/Library/Application Support/Cursor";
    f.write(&format!("{cursor}/Cache/data_0"), "x");
    f.write(&format!("{cursor}/User/globalStorage/state.vscdb"), "x");
    // One workspace whose folder exists, one whose folder is gone and has chats.
    f.write(
        &format!("{cursor}/User/workspaceStorage/w1/workspace.json"),
        &format!(r#"{{"folder":"file://{live}"}}"#),
    );
    f.write(
        &format!("{cursor}/User/workspaceStorage/w2/workspace.json"),
        &format!(r#"{{"folder":"file://{}"}}"#, gone.replace(' ', "%20")),
    );
    f.write(
        &format!("{cursor}/User/workspaceStorage/w2/chatSessions/c.json"),
        r#"{"requests":[{"message":{"text":"why is CI red"}}]}"#,
    );
    // A Roo Code task, with its workspace in history_item.json.
    let task = format!("{cursor}/User/globalStorage/rooveterinaryinc.roo-cline/tasks/t1");
    f.write(
        &format!("{task}/ui_messages.json"),
        r#"[{"ts":1759312800000,"type":"say","say":"task","text":"port the parser"}]"#,
    );
    f.write(
        &format!("{task}/history_item.json"),
        &format!(r#"{{"id":"t1","workspace":"{live}"}}"#),
    );

    let findings = scan(&f, vec![]);
    let find = |part: &str| {
        findings
            .iter()
            .find(|r| r.resource.path().is_some_and(|p| p.ends_with(part)))
    };
    assert!(find("Cursor/Cache").is_some_and(|r| r.value("Tier") == Some("Safe")));
    assert!(
        find("workspaceStorage/w1").is_none(),
        "the folder still exists"
    );
    let orphan = find("workspaceStorage/w2").expect("orphan workspace");
    assert_eq!(orphan.value("Tier"), Some("Caution"), "it holds chats");
    assert_eq!(orphan.value("Project"), Some("~/projects/gone app"));
    let chats = find("w2/chatSessions").expect("chat sessions");
    assert_eq!(chats.value("First prompt"), Some("why is CI red"));
    let roo = find("tasks/t1").expect("roo task");
    assert_eq!(roo.title, "Roo Code task · port the parser");
    assert_eq!(roo.value("Project"), Some("~/projects/live"));
    assert_eq!(roo.value("Started"), Some("2025-10-01T10:00:00Z"));
    assert!(findings.iter().all(|r| !r
        .resource
        .path()
        .unwrap_or_default()
        .ends_with("state.vscdb")));
}

#[test]
fn local_models_report_what_removing_them_frees() {
    let f = Fixture::new();
    let store = "home/.ollama/models";
    let manifest = |layers: &[(&str, u64)]| {
        let layers: Vec<String> = layers
            .iter()
            .map(|(d, s)| format!(r#"{{"digest":"{d}","size":{s}}}"#))
            .collect();
        format!(
            r#"{{"config":{{"digest":"sha256:cfg","size":10}},"layers":[{}]}}"#,
            layers.join(",")
        )
    };
    // Two models share a base layer; each has its own adapter. The config is shared too.
    f.write(
        &format!("{store}/manifests/registry.ollama.ai/library/llama3/latest"),
        &manifest(&[("sha256:base", 4_000_000_000), ("sha256:a", 100)]),
    );
    f.write(
        &format!("{store}/manifests/hf.co/org/model/q4"),
        &manifest(&[("sha256:base", 4_000_000_000), ("sha256:b", 300)]),
    );
    f.write(&format!("{store}/blobs/sha256-base"), "x");
    // An old unfinished download, and one that may still be running.
    let old = f.write(&format!("{store}/blobs/sha256-dead-partial-0"), "x");
    std::fs::File::options()
        .write(true)
        .open(&old)
        .unwrap()
        .set_modified(
            std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000),
        )
        .unwrap();
    f.write(&format!("{store}/blobs/sha256-new-partial"), "x");
    // Hugging Face, in the location HF_HOME names.
    f.write("hf/hub/models--meta-llama--Llama-3-8B/blobs/x", "x");
    f.write("hf/token", "secret");

    let findings = scan(&f, vec![("HF_HOME".into(), f.at("hf"))]);
    let find = |part: &str| {
        findings
            .iter()
            .find(|r| r.resource.path().is_some_and(|p| p.ends_with(part)))
    };
    let llama = find("library/llama3/latest").expect("llama3");
    assert_eq!(llama.title, "Model · llama3:latest");
    assert_eq!(llama.bytes, Some(100), "only its own layer is freed");
    assert_eq!(llama.value("Shared with other models"), Some("4.0 GB"));
    let other = find("hf.co/org/model/q4").unwrap();
    assert_eq!(other.title, "Model · hf.co/org/model:q4");
    assert!(find("sha256-dead-partial-0").is_some_and(|r| r.value("Tier") == Some("Safe")));
    assert!(
        find("sha256-new-partial").is_none(),
        "may still be downloading"
    );

    let repo = find("models--meta-llama--Llama-3-8B").expect("hub repo");
    assert_eq!(repo.title, "Model · meta-llama/Llama-3-8B");
    assert_eq!(repo.value("Tool"), Some("Hugging Face"));
    assert!(find("hf/token").is_none());
}

#[test]
fn tiers_filter_the_list_and_its_totals() {
    let f = Fixture::new();
    f.write("home/.codex/log/a.log", "12345");
    f.write("home/.codex/history.jsonl", "1234567890");
    let module = AiToolsModule {
        home: Some(f.at("home")),
        env: Some(vec![]),
        ..Default::default()
    };
    let engine = Engine::new(services(&f), builtin(&f).register(Arc::new(module)));
    let context = ScanContext {
        roots: vec![f.at("home")],
        ..Default::default()
    };
    engine.scan_to_store(
        &["ai".into()],
        &context,
        &ScanControl::default(),
        1,
        &|_| {},
    );
    let tier = |badge: &str| {
        engine.results.query(&cym_core::results::Query {
            module: Some("ai".into()),
            badge: Some(badge.into()),
            ..Default::default()
        })
    };
    let safe = tier("Safe");
    let caution = tier("Caution");
    assert_eq!(safe.len(), 1);
    assert_eq!(caution.len(), 1);
    assert_eq!(caution.summary.logical_bytes, 10);
}
