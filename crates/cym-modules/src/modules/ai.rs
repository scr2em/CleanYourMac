//! AI developer tools: Claude Code, Codex, Gemini CLI, editors and local model stores.
//!
//! Every tool is a table of locations inside its data folders, each with a risk tier:
//! - **Safe**: caches, logs, telemetry and downloaded binaries the tool makes again.
//! - **Review**: scratch output and large side files worth a look first.
//! - **Caution**: session transcripts and history. Removing one ends `--resume` and similar
//!   for that session.
//!
//! Settings, credentials, instructions, skills, agents, commands, plugins and memory are never
//! listed. Session transcripts are read to show their project, first prompt and dates, and to
//! flag sessions whose project folder no longer exists.
use super::{
    add_files, descriptor, flush, glob, owners_closed, wildcard, Candidate, LastUsed, ScanModule,
};
use crate::{model::*, policy, ports::*, services::Services};
use std::{collections::HashSet, path::Path};

pub mod catalog;
pub mod readers;
use readers::Read;
pub use readers::Reader;

/// How much removing an item costs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tier {
    /// The tool makes it again, or downloads it again, when needed.
    Safe,
    /// Worth a look before removal.
    Review,
    /// History you cannot get back; removing it ends resume for those sessions.
    Caution,
}
impl Tier {
    fn risk(self) -> Risk {
        match self {
            Tier::Safe => Risk::Rebuild,
            Tier::Review | Tier::Caution => Risk::Review,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Tier::Safe => "Safe",
            Tier::Review => "Review",
            Tier::Caution => "Caution",
        }
    }
}

/// A place inside a tool's data folder, relative to it. A `*` in a component lists one item
/// per match.
#[derive(Clone, Debug)]
pub struct AiLocation {
    pub path: &'static str,
    pub title: &'static str,
    pub tier: Tier,
    /// What removing it costs.
    pub note: &'static str,
    /// How to read each item, for sessions and models.
    pub read: Option<Reader>,
}
pub fn at(path: &'static str, title: &'static str, tier: Tier, note: &'static str) -> AiLocation {
    AiLocation {
        path,
        title,
        tier,
        note,
        read: None,
    }
}
impl AiLocation {
    pub fn reads(mut self, reader: Reader) -> Self {
        self.read = Some(reader);
        self
    }
}

/// Where a tool keeps its data.
#[derive(Clone, Debug)]
pub enum Root {
    /// Relative to the home folder; a `*` in the name matches several folders, as with
    /// `.claude-*` for extra accounts.
    Home(String),
    /// A folder named by an environment variable, when the user set one.
    Env(&'static str),
    /// A folder inside the one an environment variable names, such as `$HF_HOME/hub`.
    EnvJoin(&'static str, &'static str),
    /// A folder named in a file in the home folder, such as LM Studio's
    /// `~/.lmstudio-home-pointer`.
    Pointer(&'static str),
}

/// The session store a tool keeps, read to show and group transcripts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sessions {
    /// `projects/<encoded cwd>/<id>.jsonl` with `<id>/subagents` and `<id>/tool-results`.
    Claude,
}

#[derive(Clone, Debug, Default)]
pub struct AiTool {
    pub name: &'static str,
    pub roots: Vec<Root>,
    pub locations: Vec<AiLocation>,
    pub sessions: Option<Sessions>,
    /// Process names that mean the tool is running.
    pub processes: &'static [&'static str],
    /// Parts of an executable's path that mean the tool is running under another name, such
    /// as Claude Code's native binary named after its version.
    pub process_paths: &'static [&'static str],
    /// Parts of a command line that mean the tool is running inside an interpreter, such as
    /// Node.js for Gemini CLI.
    pub commands: &'static [&'static str],
    /// Bundle-identifier prefixes of its apps.
    pub apps: &'static [&'static str],
    /// The tool's own cleanup command or setting, shown with every item.
    pub command: Option<&'static str>,
    /// A folder matched by a pattern root counts only if it holds one of these (or is empty),
    /// so `~/.claude-*` finds accounts but not unrelated tools such as `~/.claude-code-router`.
    pub markers: &'static [&'static str],
    /// Locations relative to the home folder rather than a data folder.
    pub home_locations: Vec<AiLocation>,
}

pub struct AiToolsModule {
    pub tools: Vec<AiTool>,
    /// The home folder roots are relative to; the user's when `None`.
    pub home: Option<String>,
    /// Environment variables; the process environment when `None`.
    pub env: Option<Vec<(String, String)>>,
}
impl Default for AiToolsModule {
    fn default() -> Self {
        Self {
            tools: catalog::all(),
            home: None,
            env: None,
        }
    }
}

const RESUME: &str = "Removing a transcript ends --resume and --continue for that session.";
const MODEL_TOOLS: &[&str] = &["Ollama", "Hugging Face", "LM Studio", "Local models"];

/// Claude Code: `~/.claude`, extra accounts in `~/.claude-*`, and `CLAUDE_CONFIG_DIR`.
pub fn claude() -> AiTool {
    AiTool {
        name: "Claude Code",
        roots: vec![Root::Home(".claude".into()), Root::Home(".claude-*".into()), Root::Env("CLAUDE_CONFIG_DIR")],
        // Claude Code's own cleanup treats these as removable (code.claude.com/docs/en/claude-directory).
        locations: vec![
            at("downloads", "Installer downloads", Tier::Safe, "Downloaded installers of the claude binary."),
            at("local", "Old local installation", Tier::Safe, "An earlier per-user npm installation of Claude Code; reinstall it if you still run it."),
            at("cache", "Cache", Tier::Safe, "Rebuilt when needed."),
            at("plugins/cache", "Plugin cache", Tier::Safe, "Plugins are downloaded again from their marketplaces."),
            at("plugins/.trash", "Removed plugins", Tier::Safe, "Plugins you already uninstalled."),
            at("skills/.trash", "Removed skills", Tier::Safe, "Skills you already deleted."),
            at("paste-cache", "Paste cache", Tier::Safe, "Large pastes; recalled prompts lose their pasted text."),
            at("image-cache", "Image cache", Tier::Safe, "Pasted images of older versions."),
            at("uploads", "Uploads", Tier::Safe, "Files attached to past sessions."),
            at("backups", "Settings backups", Tier::Review, "Automatic backups of .claude.json, which can hold MCP server tokens; the current file stays."),
            at("shell-snapshots", "Shell snapshots", Tier::Safe, "Shell environment captured at session start; made again for each session."),
            at("session-env", "Session environments", Tier::Safe, "Per-session environment of past sessions."),
            at("usage-data", "Usage data", Tier::Safe, "Usage records of past sessions."),
            at("feedback-bundles", "Feedback bundles", Tier::Safe, "Bundles prepared for feedback reports."),
            at("statsig", "Feature-flag cache", Tier::Safe, "No longer written by current versions."),
            at("todos", "Old todo lists", Tier::Safe, "No longer written by current versions."),
            at("logs", "Old logs", Tier::Safe, "No longer written by current versions."),
            at("telemetry", "Telemetry", Tier::Safe, "Usage events waiting to be sent again."),
            at("debug", "Debug logs", Tier::Safe, "Logs of past sessions."),
            at("plans", "Plans", Tier::Review, "Plans written in plan mode."),
            at("tasks", "Task lists", Tier::Review, "Task lists a resumed session picks up again."),
            at("file-history", "Edit checkpoints", Tier::Caution, "Earlier versions of files Claude edited. Removing them ends /rewind file restores for those sessions."),
            at("history.jsonl", "Prompt history", Tier::Caution, "Every prompt you typed, for up-arrow and Ctrl-R recall."),
        ],
        sessions: Some(Sessions::Claude),
        processes: &["claude"],
        process_paths: &["/.local/share/claude/versions/"],
        apps: &["com.anthropic.claudefordesktop"],
        command: Some("claude purge --dry-run shows what Claude Code would remove for a project; cleanupPeriodDays (default 30) removes old transcripts automatically."),
        markers: &["projects", ".claude.json", "settings.json", "sessions", "history.jsonl"],
        commands: &["@anthropic-ai/claude-code"],
        home_locations: vec![
            at("Library/Caches/claude-cli-nodejs", "MCP and error logs", Tier::Safe, "Logs Claude Code writes per project."),
            at(".cache/claude/staging", "Installer staging", Tier::Safe, "Files left by the native installer."),
        ],
    }
}

impl AiToolsModule {
    fn home(&self) -> String {
        self.home.clone().unwrap_or_else(policy::home)
    }
    fn var(&self, name: &str) -> Option<String> {
        match &self.env {
            Some(env) => env.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone()),
            None => std::env::var(name).ok(),
        }
        .filter(|v| !v.trim().is_empty())
    }
    /// The tool's existing data folders, each once.
    pub fn roots(&self, s: &Services, tool: &AiTool) -> Vec<String> {
        let home = self.home();
        let mut found: Vec<String> = vec![];
        for root in &tool.roots {
            let paths: Vec<String> = match root {
                Root::Home(name) if name.contains('*') => s
                    .children(&home, &mut vec![])
                    .into_iter()
                    .filter(|e| e.directory && glob(name, e.name()).is_some())
                    .filter(|e| {
                        empty(s, e.path())
                            || tool
                                .markers
                                .iter()
                                .any(|m| s.exists(&format!("{}/{m}", e.path())))
                    })
                    .map(|e| e.path().to_owned())
                    .collect(),
                other => self.root_path(other, &home).into_iter().collect(),
            };
            for path in paths {
                if s.is_dir(&path) && !found.contains(&path) {
                    found.push(path);
                }
            }
        }
        found
    }
    /// A root that names one folder.
    fn root_path(&self, root: &Root, home: &str) -> Option<String> {
        match root {
            Root::Home(name) => Some(format!("{home}/{name}")),
            Root::Env(var) => self.var(var).map(|v| policy::canonical(&v)),
            Root::EnvJoin(var, sub) => self
                .var(var)
                .map(|v| policy::canonical(&format!("{}/{sub}", v.trim_end_matches('/')))),
            // A small file naming a folder; never a protected one such as the home folder.
            Root::Pointer(file) => {
                let path = format!("{home}/{file}");
                read_small(&path)
                    .map(|t| t.trim().to_owned())
                    .filter(|t| t.starts_with('/'))
                    .map(|t| policy::canonical(&t))
                    .filter(|t| !policy::protected(t) && !policy::system_excluded(t))
            }
        }
    }
    /// The tool a path belongs to, with the data folder holding it.
    fn owner(&self, s: &Services, path: &str) -> Option<(&AiTool, String)> {
        let home = self.home();
        self.tools.iter().find_map(|tool| {
            self.roots(s, tool)
                .into_iter()
                .chain(self.home_roots(tool, &home))
                .find(|root| policy::contains(path, root))
                .map(|root| (tool, root))
        })
    }
    /// The home-relative folders a tool's home locations and binaries live in.
    fn home_roots(&self, tool: &AiTool, home: &str) -> Vec<String> {
        let mut roots: Vec<String> = tool
            .home_locations
            .iter()
            .map(|l| format!("{home}/{}", fixed_prefix(l.path)))
            .collect();
        if tool.sessions == Some(Sessions::Claude) {
            roots.push(format!("{home}/.local/share/claude/versions"));
        }
        roots
    }
}

/// A small text file read without following a link or blocking on a FIFO.
fn read_small(path: &str) -> Option<String> {
    use std::{io::Read, os::unix::fs::OpenOptionsExt};
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .ok()?;
    if !file.metadata().ok()?.is_file() {
        return None;
    }
    let mut text = String::new();
    file.take(4097).read_to_string(&mut text).ok()?;
    (text.len() <= 4096).then_some(text)
}

/// A folder holding nothing but Finder metadata.
fn empty(s: &Services, path: &str) -> bool {
    s.children(path, &mut vec![])
        .iter()
        .all(|e| e.name() == ".DS_Store")
}

fn now() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64())
}

/// The part of a location's path before its first pattern component.
fn fixed_prefix(path: &str) -> String {
    path.split('/')
        .take_while(|c| !c.contains('*'))
        .collect::<Vec<_>>()
        .join("/")
}

/// Existing items for a location, with the names each `*` component matched. Names in
/// `catalog::NEVER` are never returned.
fn expand(s: &Services, root: &str, location: &AiLocation) -> Vec<(Entry, Vec<String>)> {
    let parts: Vec<&str> = location.path.split('/').collect();
    let mut current: Vec<(String, Vec<String>)> = vec![(root.to_owned(), vec![])];
    for (i, part) in parts.iter().enumerate() {
        let last = i + 1 == parts.len();
        current = if part.contains('*') {
            current
                .into_iter()
                .flat_map(|(dir, matched)| {
                    s.children(&dir, &mut vec![])
                        .into_iter()
                        .filter(|e| {
                            wildcard(part, e.name())
                                && !catalog::NEVER.contains(&e.name())
                                && (last || e.directory)
                        })
                        .map(move |e| {
                            let mut m = matched.clone();
                            m.push(e.name().to_owned());
                            (e.path().to_owned(), m)
                        })
                        .collect::<Vec<_>>()
                })
                .collect()
        } else {
            current
                .into_iter()
                .map(|(dir, m)| (format!("{dir}/{part}"), m))
                .collect()
        };
    }
    current
        .into_iter()
        .filter(|(path, _)| !catalog::NEVER.contains(&path.rsplit('/').next().unwrap_or_default()))
        .filter_map(|(path, m)| s.entry(&path).ok().map(|e| (e, m)))
        .collect()
}

/// What a Claude Code transcript says about its session.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SessionInfo {
    pub cwd: Option<String>,
    pub branch: Option<String>,
    pub started: Option<String>,
    pub first_prompt: Option<String>,
    pub title: Option<String>,
}
/// Reads the start of a transcript: the working folder, branch, start time, the first prompt
/// the user typed, and a title if the session has one.
pub fn session_info(head: &str) -> SessionInfo {
    let mut info = SessionInfo::default();
    for line in head.lines() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let text = |key: &str| v.get(key).and_then(|x| x.as_str()).map(str::to_owned);
        if info.cwd.is_none() {
            info.cwd = text("cwd");
        }
        if info.branch.is_none() {
            info.branch = text("gitBranch").filter(|b| !b.is_empty() && b != "HEAD");
        }
        if info.started.is_none() {
            info.started = text("timestamp");
        }
        match v.get("type").and_then(|t| t.as_str()) {
            Some("summary") if info.title.is_none() => info.title = text("summary"),
            Some("ai-title") if info.title.is_none() => info.title = text("aiTitle"),
            // A name the user gave with /rename wins over any other title.
            Some("custom-title") => info.title = text("customTitle").or(info.title.take()),
            Some("user")
                if info.first_prompt.is_none()
                    && !v["isMeta"].as_bool().unwrap_or(false)
                    && !v["isSidechain"].as_bool().unwrap_or(false) =>
            {
                info.first_prompt = prompt_text(&v["message"]["content"]);
            }
            _ => {}
        }
    }
    info
}
/// The text the user typed, ignoring slash-command wrappers, tool results and system notes.
pub(crate) fn prompt_text(content: &serde_json::Value) -> Option<String> {
    let raw = match content {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(parts) => parts
            .iter()
            .filter(|p| matches!(p["type"].as_str(), Some("text" | "input_text") | None))
            .filter_map(|p| p["text"].as_str())
            .collect::<Vec<_>>()
            .join(" "),
        _ => return None,
    };
    let text = raw.trim();
    if text.is_empty() || text.starts_with('<') || text.starts_with("Caveat:") {
        return None;
    }
    let line: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    Some(if line.chars().count() > 120 {
        line.chars().take(119).collect::<String>() + "…"
    } else {
        line
    })
}
/// The working folder Claude Code encoded as a project folder name: every character other than
/// a letter or digit becomes `-`. Lossy, so transcripts' recorded `cwd` is preferred; this is
/// the best guess when a project has no readable transcript.
pub fn decode_project(name: &str) -> String {
    name.replace('-', "/")
}
/// Whether a project folder belongs to a worktree that Claude Code made.
fn worktree(name: &str, cwd: Option<&str>) -> bool {
    name.contains("--claude-worktrees-") || cwd.is_some_and(|c| c.contains("/.claude/worktrees/"))
}
fn short(path: &str, home: &str) -> String {
    policy::abbreviate_home(path, home)
}

/// Live Claude Code sessions recorded in `sessions/<pid>.json`.
#[derive(Default)]
struct Live {
    sessions: HashSet<String>,
    names: Vec<String>,
}
fn live_sessions(s: &Services, root: &str) -> Live {
    let mut live = Live::default();
    for e in s.children(&format!("{root}/sessions"), &mut vec![]) {
        if !e.name().ends_with(".json") {
            continue;
        }
        let Some(text) = s.read_text(e.path()) else {
            continue;
        };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        let Some(pid) = v["pid"].as_i64().and_then(|p| i32::try_from(p).ok()) else {
            continue;
        };
        if let Some(process) = s.processes.inspect(pid) {
            if let Some(id) = v["sessionId"].as_str() {
                live.sessions.insert(id.to_owned());
            }
            let cwd = v["cwd"].as_str().unwrap_or_default();
            live.names
                .push(format!("{} (PID {pid}) in {cwd}", process.name));
        }
    }
    live
}

/// Running processes of a tool, described for the user.
fn running(s: &Services, tool: &AiTool) -> Vec<String> {
    s.processes
        .pids()
        .into_iter()
        .filter_map(|pid| s.processes.inspect(pid))
        .filter(|p| {
            tool.processes
                .iter()
                .any(|name| p.name.eq_ignore_ascii_case(name))
                || tool
                    .process_paths
                    .iter()
                    .any(|part| p.identity.executable.contains(part))
                || tool.commands.iter().any(|part| p.command.contains(part))
        })
        .map(|p| {
            format!(
                "{} (PID {}) · {}",
                p.name, p.identity.pid, p.identity.executable
            )
        })
        .collect()
}

impl AiToolsModule {
    /// Old native Claude Code binaries in `~/.local/share/claude/versions`. The installer keeps
    /// the launcher's target and the two newest; so does this, and never one that is running.
    fn claude_versions(&self, s: &Services, out: &mut Vec<Candidate>) {
        let home = self.home();
        let folder = format!("{home}/.local/share/claude/versions");
        let mut versions = s.children(&folder, &mut vec![]);
        if versions.len() <= 2 {
            return;
        }
        let launcher = std::fs::read_link(format!("{home}/.local/bin/claude"))
            .ok()
            .map(|t| {
                policy::canonical(
                    &Path::new(&format!("{home}/.local/bin"))
                        .join(t)
                        .to_string_lossy(),
                )
            });
        versions.sort_by(|a, b| b.modified().total_cmp(&a.modified()));
        let used: Vec<String> = s
            .processes
            .pids()
            .into_iter()
            .filter_map(|pid| s.processes.inspect(pid))
            .map(|p| p.identity.executable)
            .collect();
        for (i, v) in versions.into_iter().enumerate() {
            let linked = launcher
                .as_deref()
                .is_some_and(|t| policy::contains(t, v.path()));
            if i < 2 || linked {
                continue;
            }
            let running = used.iter().any(|exe| policy::contains(exe, v.path()));
            out.push(
                Candidate::new(v.clone(), "An older native Claude Code binary. The installer keeps the launcher's version and the two newest; this one is neither.", vec![ActionKind::Trash], Tier::Safe.risk())
                    .title(format!("Old Claude Code version {}", v.name()))
                    .details(vec![detail("Tool", "Claude Code"), detail("Tier", Tier::Safe.label()), detail("Version", v.name())])
                    .last_used(LastUsed::At(v.modified()))
                    .blocked(running.then_some("A running Claude Code uses this version.")),
            );
        }
    }
    fn claude_sessions(
        &self,
        s: &Services,
        root: &str,
        k: &ScanControl,
        out: &mut Vec<Candidate>,
    ) -> Result<()> {
        let home = self.home();
        let live = live_sessions(s, root);
        for project in s.children(&format!("{root}/projects"), &mut vec![]) {
            k.check()?;
            if !project.directory {
                continue;
            }
            let entries = s.children(project.path(), &mut vec![]);
            let mut project_cwd: Option<String> = None;
            let mut sessions = 0;
            for e in &entries {
                if e.regular
                    && (e.name().contains(".orphaned-") || e.name().contains(".superseded-"))
                {
                    out.push(
                        Candidate::new(e.clone(), "A set-aside copy of a transcript; Claude Code does not show it and removes it after cleanupPeriodDays.", vec![ActionKind::Trash], Tier::Safe.risk())
                            .title(format!("Set-aside transcript · {}", e.name()))
                            .details(vec![detail("Tool", "Claude Code"), detail("Tier", Tier::Safe.label())])
                            .last_used(LastUsed::At(e.modified())),
                    );
                    continue;
                }
                let Some(id) = e.name().strip_suffix(".jsonl").filter(|_| e.regular) else {
                    continue;
                };
                sessions += 1;
                let head =
                    s.fs.read(e.path(), 512 * 1024)
                        .map(|b| String::from_utf8_lossy(&b).into_owned())
                        .unwrap_or_default();
                let info = session_info(&head);
                let cwd = info
                    .cwd
                    .clone()
                    .unwrap_or_else(|| decode_project(project.name()));
                project_cwd.get_or_insert_with(|| cwd.clone());
                let gone = s.missing(&cwd);
                let dead_worktree = gone && worktree(project.name(), Some(&cwd));
                let running = live.sessions.contains(id);
                let title = info
                    .title
                    .clone()
                    .or(info.first_prompt.clone())
                    .unwrap_or_else(|| format!("Session {id}"));
                let mut details = vec![
                    detail("Tool", "Claude Code"),
                    detail("Tier", Tier::Caution.label()),
                    detail("Project", short(&cwd, &home)),
                    detail("Session", id),
                ];
                if let Some(prompt) = &info.first_prompt {
                    details.push(detail("First prompt", prompt.clone()));
                }
                if let Some(branch) = &info.branch {
                    details.push(detail("Branch", branch.clone()));
                }
                if let Some(started) = &info.started {
                    details.push(detail("Started", started.clone()));
                }
                let state = if dead_worktree {
                    "The worktree this session ran in no longer exists. "
                } else if gone {
                    "Orphan: the project folder no longer exists. "
                } else {
                    ""
                };
                if gone {
                    details.push(detail("Project folder", "Missing"));
                }
                let reason = format!("{state}Claude Code session transcript. {RESUME}");
                let mut candidate = Candidate::new(
                    e.clone(),
                    &reason,
                    vec![ActionKind::Trash],
                    Tier::Caution.risk(),
                )
                .title(title)
                .details(details)
                .last_used(LastUsed::At(e.modified()));
                if running {
                    candidate = candidate.blocked(Some("This session is running in Claude Code."));
                }
                out.push(candidate);
                // Subagent transcripts and saved tool output: large, and not needed to resume.
                let side = format!("{}/{id}", project.path());
                for (part, what) in [
                    ("subagents", "Subagent transcripts"),
                    ("tool-results", "Saved tool output"),
                ] {
                    let Ok(entry) = s.entry(&format!("{side}/{part}")) else {
                        continue;
                    };
                    let reason = format!(
                        "{what} of this session. The session still resumes without them; Claude only loses the full text of these outputs, which the transcript refers to by path."
                    );
                    let mut c = Candidate::new(
                        entry,
                        &reason,
                        vec![ActionKind::Trash],
                        Tier::Review.risk(),
                    )
                    .title(format!(
                        "{what} · {}",
                        info.title
                            .clone()
                            .or(info.first_prompt.clone())
                            .unwrap_or_else(|| id.to_owned())
                    ))
                    .details(vec![
                        detail("Tool", "Claude Code"),
                        detail("Tier", Tier::Review.label()),
                        detail("Project", short(&cwd, &home)),
                        detail("Session", id),
                        detail("Keeps", "The main transcript, so --resume still works"),
                    ])
                    .last_used(LastUsed::At(e.modified()));
                    if running {
                        c = c.blocked(Some("This session is running in Claude Code."));
                    }
                    out.push(c);
                }
            }
            // Side folders left behind after their transcript was removed.
            for e in &entries {
                if e.directory
                    && e.name() != "memory"
                    && !entries
                        .iter()
                        .any(|t| t.name() == format!("{}.jsonl", e.name()))
                {
                    out.push(
                        Candidate::new(
                            e.clone(),
                            "Files of a session whose transcript is gone; nothing refers to them.",
                            vec![ActionKind::Trash],
                            Tier::Safe.risk(),
                        )
                        .title(format!("Leftovers of session {}", e.name()))
                        .details(vec![
                            detail("Tool", "Claude Code"),
                            detail("Tier", Tier::Safe.label()),
                        ]),
                    );
                }
            }
            // A whole project whose folder is gone, unless Claude keeps memory for it there.
            let cwd = project_cwd.unwrap_or_else(|| decode_project(project.name()));
            if sessions > 0 && s.missing(&cwd) {
                let memory = entries.iter().any(|e| e.name() == "memory");
                let kind = if worktree(project.name(), Some(&cwd)) {
                    "worktree"
                } else {
                    "project"
                };
                let mut c = Candidate::new(
                    project.clone(),
                    &format!("All {sessions} transcript(s) of a {kind} whose folder no longer exists: {}. {RESUME}", short(&cwd, &home)),
                    vec![ActionKind::Trash],
                    Tier::Caution.risk(),
                )
                .title(format!("Orphan {kind} · {}", short(&cwd, &home)))
                .details(vec![
                    detail("Tool", "Claude Code"),
                    detail("Tier", Tier::Caution.label()),
                    detail("Project", short(&cwd, &home)),
                    detail("Sessions", sessions.to_string()),
                    detail("Project folder", "Missing"),
                ])
                .last_used(LastUsed::At(project.modified()));
                if memory {
                    c = c.blocked(Some("Claude keeps memory for this project here. Remove its sessions one by one instead."));
                } else if entries
                    .iter()
                    .any(|e| live.sessions.contains(e.name().trim_end_matches(".jsonl")))
                {
                    c = c.blocked(Some("A session of this project is running in Claude Code."));
                }
                out.push(c);
            }
        }
        Ok(())
    }

    /// Background jobs: `jobs/<id>/state.json` and their scratch `tmp/`.
    fn claude_jobs(&self, s: &Services, root: &str, out: &mut Vec<Candidate>) {
        for job in s.children(&format!("{root}/jobs"), &mut vec![]) {
            let Ok(tmp) = s.entry(&format!("{}/tmp", job.path())) else {
                continue;
            };
            let state = s
                .read_text(&format!("{}/state.json", job.path()))
                .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
                .unwrap_or_default();
            let status = state["status"].as_str().unwrap_or("unknown").to_owned();
            let open: Vec<String> = state["todos"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|t| t["status"].as_str() != Some("completed"))
                .filter_map(|t| t["content"].as_str().map(str::to_owned))
                .collect();
            let mut details = vec![
                detail("Tool", "Claude Code"),
                detail("Tier", Tier::Review.label()),
                detail("Job", job.name()),
                detail("Status", status.clone()),
                detail("Open tasks", open.len().to_string()),
            ];
            for (i, todo) in open.iter().take(5).enumerate() {
                details.push(detail(&format!("Open task {}", i + 1), todo.clone()));
            }
            let warning = if status == "blocked" && !open.is_empty() {
                format!(" Warning: this job is blocked with {} open task(s); its scratch files may still be needed.", open.len())
            } else {
                String::new()
            };
            out.push(
                Candidate::new(
                    tmp,
                    &format!(
                        "Screenshots and scratch files of a background job ({status}).{warning}"
                    ),
                    vec![ActionKind::Trash],
                    Tier::Review.risk(),
                )
                .title(format!("Job scratch files · {}", job.name()))
                .details(details)
                .last_used(LastUsed::At(job.modified())),
            );
        }
    }
}

impl ScanModule for AiToolsModule {
    fn descriptor(&self) -> ModuleDescriptor {
        descriptor(
            "ai",
            "AI Tools",
            "Developer",
            "sparkles",
            "Caches, old versions and session transcripts of Claude Code, Codex, Gemini CLI and other AI tools, sorted by risk.",
            false,
        )
    }
    fn action_roots(&self) -> Vec<String> {
        // Lexical, without services: the home folder's matching names and the variables.
        let home = self.home();
        let names: Vec<String> = std::fs::read_dir(&home)
            .map(|d| {
                d.flatten()
                    .filter_map(|e| e.file_name().into_string().ok())
                    .collect()
            })
            .unwrap_or_default();
        let mut roots = vec![];
        for tool in &self.tools {
            for root in &tool.roots {
                match root {
                    Root::Home(name) if name.contains('*') => roots.extend(
                        names
                            .iter()
                            .filter(|n| glob(name, n).is_some())
                            .map(|n| format!("{home}/{n}")),
                    ),
                    other => roots.extend(self.root_path(other, &home)),
                }
            }
            roots.extend(self.home_roots(tool, &home));
        }
        roots
    }
    fn scan(
        &self,
        s: &Services,
        c: &ScanContext,
        k: &ScanControl,
        sink: &mut dyn Sink,
    ) -> Result<()> {
        let home = self.home();
        let mut candidates = vec![];
        let mut warnings = vec![];
        for tool in &self.tools {
            k.check()?;
            for location in &tool.home_locations {
                for (e, matched) in expand(s, &home, location) {
                    if !c.allows(e.path()) {
                        continue;
                    }
                    let title = titled(location.title, &matched);
                    let blocker = worktree_blocker(s, location, e.path(), k);
                    candidates.push(
                        Candidate::new(
                            e,
                            &format!("{} {}", location.note, tier_note(location.tier)),
                            vec![ActionKind::Trash],
                            location.tier.risk(),
                        )
                        .title(title)
                        .details(vec![
                            detail("Tool", tool.name),
                            detail("Tier", location.tier.label()),
                        ])
                        .blocked(blocker.as_deref()),
                    );
                }
            }
            if tool.sessions == Some(Sessions::Claude) {
                self.claude_versions(s, &mut candidates);
            }
            let roots = self.roots(s, tool);
            let several = roots.len() > 1;
            for root in roots {
                k.check()?;
                if !c.allows(&root) {
                    continue;
                }
                sink.progress(format!("Reading {}", short(&root, &home)));
                if empty(s, &root) {
                    if let Ok(e) = s.entry(&root) {
                        candidates.push(
                            Candidate::new(e, &format!("An empty {} data folder, for example from an account you no longer use.", tool.name), vec![ActionKind::Trash], Tier::Safe.risk())
                                .title(format!("Empty folder · {}", short(&root, &home)))
                                .details(self.base(tool, &root, Tier::Safe)),
                        );
                    }
                    continue;
                }
                for location in &tool.locations {
                    if location.read == Some(Reader::Ollama) {
                        self.ollama(s, tool, &root, &mut candidates);
                        continue;
                    }
                    for (e, matched) in expand(s, &root, location) {
                        // A download that may still be running is left alone for an hour.
                        let recent = now() - e.modified() < 3600.0;
                        if !c.allows(e.path()) || (location.path.contains("-partial") && recent) {
                            continue;
                        }
                        let mut title = titled(location.title, &matched);
                        if several {
                            title = format!("{title} · {}", short(&root, &home));
                        }
                        let read = location
                            .read
                            .map(|r| readers::read(s, r, e.path(), e.directory));
                        let candidate = match read {
                            None => {
                                let blocker = worktree_blocker(s, location, e.path(), k);
                                Some(
                                    self.plain(tool, &root, location, e, title)
                                        .blocked(blocker.as_deref()),
                                )
                            }
                            Some(Read::Skip) => None,
                            Some(Read::Named(name)) => {
                                let mut c = self.plain(
                                    tool,
                                    &root,
                                    location,
                                    e,
                                    format!("{} · {name}", location.title),
                                );
                                c.details.push(detail("Repository", name));
                                Some(c)
                            }
                            Some(Read::Session(info)) => {
                                let gone = info.cwd.as_deref().is_some_and(|cwd| s.missing(cwd));
                                Some(self.session(tool, &root, location, e, title, info, gone))
                            }
                            Some(Read::OrphanWorkspace { folder, chats }) => {
                                Some(self.orphan_workspace(tool, &root, e, &folder, chats))
                            }
                        };
                        candidates.extend(candidate);
                    }
                }
                match tool.sessions {
                    Some(Sessions::Claude) => {
                        self.claude_sessions(s, &root, k, &mut candidates)?;
                        self.claude_jobs(s, &root, &mut candidates);
                    }
                    None => {}
                }
            }
        }
        flush(sink, &mut warnings);
        let candidates = candidates
            .into_iter()
            .filter(|cand| c.allows(cand.entry.path()))
            .collect();
        add_files(s, &mut Tiered(sink), "ai", candidates, k)
    }
    fn in_use(&self, s: &Services, f: &Finding, _: ActionKind) -> Option<String> {
        let path = f.resource.path()?;
        let (tool, root) = self.owner(s, path)?;
        if let Err(reason) = owners_closed(s, tool.apps) {
            return Some(reason);
        }
        if tool.sessions == Some(Sessions::Claude) {
            let live = live_sessions(s, &root);
            if !live.names.is_empty() {
                return Some(format!(
                    "{} is running with this data folder:\n{}",
                    tool.name,
                    live.names
                        .iter()
                        .map(|n| format!("• {n}"))
                        .collect::<Vec<_>>()
                        .join("\n")
                ));
            }
        }
        let running: Vec<String> = running(s, tool)
            .into_iter()
            .map(|r| format!("• {r}"))
            .collect();
        (!running.is_empty()).then(|| format!("{} is running:\n{}", tool.name, running.join("\n")))
    }
    fn preflight(&self, s: &Services, f: &Finding, _: ActionKind, k: &ScanControl) -> Result<()> {
        let path = f.resource.path().ok_or("Missing path")?;
        // A worktree's state is checked again: work may have started since the scan.
        if let Some(reason) = git_blocker(s, path, k) {
            return Err(reason);
        }
        let (tool, root) = self
            .owner(s, path)
            .ok_or("This item is no longer inside an AI tool's data folder.")?;
        // A running session's own files are never moved, whatever the user confirms.
        if tool.sessions == Some(Sessions::Claude) {
            let live = live_sessions(s, &root);
            if live.sessions.iter().any(|id| path.contains(id.as_str())) {
                return Err("This session is running in Claude Code.".into());
            }
        }
        Ok(())
    }
}

/// Shows each finding's tier as its badge, so the list can be filtered and totalled by tier.
struct Tiered<'a>(&'a mut dyn Sink);
impl Sink for Tiered<'_> {
    fn finding(&mut self, mut finding: Finding) {
        if let Some(tier) = finding.value("Tier") {
            finding.badge = Some(tier.to_owned());
        }
        self.0.finding(finding);
    }
    fn warning(&mut self, warning: String) {
        self.0.warning(warning);
    }
    fn progress(&mut self, message: String) {
        self.0.progress(message);
    }
}

/// Why a worktree an AI tool made must stay, for locations that hold worktrees.
fn worktree_blocker(
    s: &Services,
    location: &AiLocation,
    path: &str,
    k: &ScanControl,
) -> Option<String> {
    location
        .path
        .contains("worktree")
        .then(|| git_blocker(s, path, k))
        .flatten()
}
/// Why a Git worktree must stay: anything `Git::inspect` finds, a checkout of another
/// system, or a state Git cannot report. `None` for a folder that is not a worktree.
fn git_blocker(s: &Services, path: &str, k: &ScanControl) -> Option<String> {
    // A tool may keep the repository a folder or two below the matched one (Codex uses
    // `worktrees/<id>/<repo>`), so every repository down to that depth is checked; a folder
    // that cannot be listed, or too many to check, blocks instead.
    const UNCHECKED: &str =
        "Some folders in this worktree could not be checked. Check it yourself.";
    let mut level = vec![path.to_owned()];
    let mut repos = vec![];
    for depth in 0..3 {
        let mut next = vec![];
        for dir in level {
            match super::checkout(s, &dir) {
                Some("Git") => repos.push(dir),
                Some(system) => {
                    return Some(format!(
                        "This worktree holds a {system} checkout, which CleanYourMac cannot inspect. Check it yourself."
                    ))
                }
                None if depth < 2 => {
                let mut warnings = vec![];
                let children = s.children(&dir, &mut warnings);
                if !warnings.is_empty() {
                    return Some(UNCHECKED.into());
                }
                    next.extend(
                        children
                            .into_iter()
                            .filter(|e| e.directory)
                            .map(|e| e.path().to_owned()),
                    );
                }
                None => {}
            }
        }
        if next.len() + repos.len() > 256 {
            return Some(UNCHECKED.into());
        }
        level = next;
    }
    repos.iter().find_map(|repo| repo_blocker(s, repo, k))
}
/// Why one Git repository must stay: the same check Git Worktrees makes, so a detached
/// commit no branch holds, submodules or a repository in an ignored folder keep it too.
fn repo_blocker(s: &Services, path: &str, k: &ScanControl) -> Option<String> {
    match crate::git::Git(s.commands.as_ref()).inspect(s, path, None, k) {
        Ok(crate::git::Inspection::Clean { .. }) => None,
        Ok(crate::git::Inspection::Keep { reason, .. }) => Some(reason),
        Err(_) => Some(UNREADABLE.into()),
    }
}
const UNREADABLE: &str = "Git could not inspect this worktree. Check it yourself.";

/// A location's title, with the names its patterns matched.
fn titled(title: &str, matched: &[String]) -> String {
    if matched.is_empty() {
        title.to_owned()
    } else {
        format!("{title} · {}", matched.join(" / "))
    }
}

fn gigabytes(bytes: u64) -> String {
    format!("{:.1} GB", bytes as f64 / 1e9)
}

impl AiToolsModule {
    /// The details every row of a data folder carries.
    fn base(&self, tool: &AiTool, root: &str, tier: Tier) -> Vec<Detail> {
        let mut d = vec![
            detail("Tool", tool.name),
            detail("Tier", tier.label()),
            detail("Data folder", short(root, &self.home())),
        ];
        if let Some(command) = tool.command {
            d.push(detail("Official cleanup", command));
        }
        d
    }
    fn plain(
        &self,
        tool: &AiTool,
        root: &str,
        location: &AiLocation,
        e: Entry,
        title: String,
    ) -> Candidate {
        let what = if MODEL_TOOLS.contains(&tool.name) {
            ""
        } else {
            tier_note(location.tier)
        };
        Candidate::new(
            e,
            format!("{} {what}", location.note).trim(),
            vec![ActionKind::Trash],
            location.tier.risk(),
        )
        .title(title)
        .details(self.base(tool, root, location.tier))
    }
    /// A session of a tool other than Claude Code, with its project and first prompt.
    #[allow(clippy::too_many_arguments)]
    fn session(
        &self,
        tool: &AiTool,
        root: &str,
        location: &AiLocation,
        e: Entry,
        fallback: String,
        info: SessionInfo,
        gone: bool,
    ) -> Candidate {
        let home = self.home();
        let mut details = self.base(tool, root, location.tier);
        // Marks the row as a session, so the history never keeps its title (which can quote
        // a prompt or a summary of the conversation).
        details.push(detail("Session", e.name()));
        if let Some(cwd) = &info.cwd {
            details.push(detail("Project", short(cwd, &home)));
        }
        if let Some(prompt) = &info.first_prompt {
            details.push(detail("First prompt", prompt.clone()));
        }
        if let Some(branch) = &info.branch {
            details.push(detail("Branch", branch.clone()));
        }
        if let Some(started) = &info.started {
            details.push(detail("Started", started.clone()));
        }
        let state = if gone {
            details.push(detail("Project folder", "Missing"));
            "Orphan: the project folder no longer exists. "
        } else {
            ""
        };
        let title = match (&info.title, &info.first_prompt) {
            (Some(t), _) | (None, Some(t)) => format!("{} · {t}", location.title),
            _ => fallback,
        };
        let modified = e.modified();
        Candidate::new(
            e,
            &format!("{state}{} {}", location.note, tier_note(location.tier)),
            vec![ActionKind::Trash],
            location.tier.risk(),
        )
        .title(title)
        .details(details)
        .last_used(LastUsed::At(modified))
    }
    /// Editor state kept for a folder that no longer exists.
    fn orphan_workspace(
        &self,
        tool: &AiTool,
        root: &str,
        e: Entry,
        folder: &str,
        chats: bool,
    ) -> Candidate {
        let home = self.home();
        let tier = if chats { Tier::Caution } else { Tier::Review };
        let mut details = self.base(tool, root, tier);
        details.push(detail("Project", short(folder, &home)));
        details.push(detail("Project folder", "Missing"));
        let chats = if chats {
            " It also holds the chat sessions of that folder."
        } else {
            ""
        };
        let modified = e.modified();
        Candidate::new(
            e,
            &format!("Orphan: {} keeps open files, search history and extension data for {}, which no longer exists.{chats} {}", tool.name, short(folder, &home), tier_note(tier)),
            vec![ActionKind::Trash],
            tier.risk(),
        )
        .title(format!("Workspace data · {}", short(folder, &home)))
        .details(details)
        .last_used(LastUsed::At(modified))
    }
    /// Ollama models. Removing a model's manifest is what `ollama rm` does; Ollama then frees
    /// the files no other model uses the next time it starts.
    fn ollama(&self, s: &Services, tool: &AiTool, root: &str, out: &mut Vec<Candidate>) {
        for m in readers::ollama_models(s, root) {
            let Ok(e) = s.entry(&m.manifest) else {
                continue;
            };
            let mut details = self.base(tool, root, Tier::Review);
            details.push(detail("Model", m.name.clone()));
            details.push(detail("Shared with other models", gigabytes(m.shared)));
            let modified = e.modified();
            out.push(
                Candidate::new(
                    e,
                    &format!("A local model. Ollama frees its files ({}) the next time it starts; ollama pull {} downloads it again. Files other models use stay.", gigabytes(m.unique), m.name),
                    vec![ActionKind::Trash],
                    Tier::Review.risk(),
                )
                .title(format!("Model · {}", m.name))
                .details(details)
                .frees(m.unique)
                .last_used(LastUsed::At(modified)),
            );
        }
    }
}

fn tier_note(tier: Tier) -> &'static str {
    match tier {
        Tier::Safe => "The tool makes it again when needed.",
        Tier::Review => "Look before you remove it.",
        Tier::Caution => "History you cannot get back.",
    }
}
