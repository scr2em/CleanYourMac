//! Where each AI tool keeps its data, and what each place costs to remove. Settings,
//! credentials, instructions, skills, agents, plugins and memory are never listed here, and
//! `NEVER` keeps them out even when a pattern would match them.
use super::{at, AiLocation, AiTool, Reader, Root, Tier};

/// Names never listed, whatever a location's pattern matches.
pub const NEVER: &[&str] = &[
    ".env",
    "AGENTS.md",
    "CLAUDE.md",
    "GEMINI.md",
    "QWEN.md",
    "agents",
    "auth.json",
    "commands",
    "config.json",
    "config.toml",
    "config.yaml",
    "globalContext.json",
    "installed_plugins.json",
    "marketplaces",
    "mcp.json",
    "memories",
    "memory",
    "oauth_creds.json",
    "secrets.json",
    "settings.json",
    "skills",
    "state.vscdb",
];

const RESUME: &str = "Removing a session ends resuming it.";

/// Every tool the module knows.
pub fn all() -> Vec<AiTool> {
    vec![
        super::claude(),
        claude_desktop(),
        codex(),
        gemini(),
        qwen(),
        copilot(),
        opencode(),
        cline_cli(),
        hermes(),
        herdr(),
        goose(),
        amp(),
        kiro_cli(),
        crush(),
        cursor(),
        editor(
            "VS Code",
            "Code",
            &["com.microsoft.VSCode"],
            &["/Visual Studio Code.app/"],
            false,
        ),
        editor(
            "Windsurf",
            "Windsurf",
            &["com.exafunction.windsurf"],
            &["/Windsurf.app/"],
            true,
        ),
        editor("Kiro", "Kiro", &["dev.kiro"], &["/Kiro.app/"], true),
        editor("Trae", "Trae", &["com.trae"], &["/Trae.app/"], true),
        editor("Void", "Void", &["com.voideditor"], &["/Void.app/"], true),
        zed(),
        continue_dev(),
        ollama(),
        hugging_face(),
        lm_studio(),
        local_models(),
    ]
}

/// The Chromium caches and logs of an Electron app's data folder.
fn electron() -> Vec<AiLocation> {
    crate::modules::electron_folders!("", "")
        .into_iter()
        // Caches come back; logs and crash reports are never made again.
        .map(|(folder, name, note, cache)| {
            at(
                folder,
                name,
                if cache { Tier::Safe } else { Tier::Review },
                note,
            )
        })
        .collect()
}

/// Per-folder state and the chats of AI extensions (Cline, Roo Code, Kilo Code) in a VS Code
/// style editor.
fn editor_state() -> Vec<AiLocation> {
    let task = "A task of an AI extension, with its messages and checkpoints. Removing it removes it from the extension's history.";
    vec![
        at(
            "User/workspaceStorage/*",
            "Workspace data",
            Tier::Review,
            "Editor state for a folder: open files, search history and extension data.",
        )
        .reads(Reader::Workspace),
        at(
            "User/workspaceStorage/*/chatSessions",
            "Chat sessions",
            Tier::Caution,
            "Copilot and other chat sessions of this folder.",
        )
        .reads(Reader::WorkspaceChats),
        at(
            "User/globalStorage/saoudrizwan.claude-dev/tasks/*",
            "Cline task",
            Tier::Caution,
            task,
        )
        .reads(Reader::ExtensionTask),
        at(
            "User/globalStorage/rooveterinaryinc.roo-cline/tasks/*",
            "Roo Code task",
            Tier::Caution,
            task,
        )
        .reads(Reader::ExtensionTask),
        at(
            "User/globalStorage/kilocode.kilo-code/tasks/*",
            "Kilo Code task",
            Tier::Caution,
            task,
        )
        .reads(Reader::ExtensionTask),
    ]
}

/// A VS Code style editor in `~/Library/Application Support/<folder>`. VS Code's caches are
/// listed by Developer Caches, so only its workspaces and chats are listed here.
fn editor(
    name: &'static str,
    folder: &'static str,
    apps: &'static [&'static str],
    bundle: &'static [&'static str],
    caches: bool,
) -> AiTool {
    let mut locations = if caches { electron() } else { vec![] };
    locations.extend(editor_state());
    AiTool {
        name,
        roots: vec![Root::Home(format!("Library/Application Support/{folder}"))],
        locations,
        process_paths: bundle,
        apps,
        ..Default::default()
    }
}

fn cursor() -> AiTool {
    let mut locations = electron();
    locations.extend(editor_state());
    locations.extend([
        at(
            "projects/*/agent-transcripts",
            "Agent transcripts",
            Tier::Caution,
            "Cursor agent chats of a project.",
        )
        .reads(Reader::Meta("*.json")),
        at("chats/*", "Chats", Tier::Caution, "Cursor CLI chats."),
        at(
            "worktrees/*",
            "Cursor worktree",
            Tier::Review,
            "A worktree Cursor made for an agent. Changes not merged elsewhere are lost.",
        ),
    ]);
    AiTool {
        name: "Cursor",
        roots: vec![
            Root::Home("Library/Application Support/Cursor".into()),
            Root::Home(".cursor".into()),
        ],
        locations,
        processes: &["cursor-agent"],
        process_paths: &["/Cursor.app/"],
        apps: &["com.todesktop.230313mzl4w4u92"],
        home_locations: vec![at(
            "Library/Application Support/Caches/cursor-updater",
            "Cursor update downloads",
            Tier::Safe,
            "Downloaded updates.",
        )],
        ..Default::default()
    }
}

fn claude_desktop() -> AiTool {
    let mut locations = electron();
    locations.extend([
        at(
            "sentry",
            "Crash reports",
            Tier::Safe,
            "Reports waiting to be sent.",
        ),
        at(
            "vm_bundles",
            "Virtual machine images",
            Tier::Review,
            "The Linux image Claude uses to run code. It is downloaded again when needed.",
        ),
        at(
            "claude-code-sessions",
            "Claude Code sessions",
            Tier::Caution,
            "Claude Code sessions started from the desktop app.",
        ),
    ]);
    AiTool {
        name: "Claude Desktop",
        roots: vec![Root::Home("Library/Application Support/Claude".into())],
        locations,
        process_paths: &["/Claude.app/"],
        apps: &["com.anthropic.claudefordesktop"],
        ..Default::default()
    }
}

fn codex() -> AiTool {
    AiTool {
        name: "Codex",
        roots: vec![Root::Home(".codex".into()), Root::Env("CODEX_HOME")],
        locations: vec![
            at("log", "Logs", Tier::Safe, "Logs of past sessions."),
            at(
                "logs_*.sqlite",
                "Log database",
                Tier::Safe,
                "Logs of past sessions.",
            ),
            at("cache", "Cache", Tier::Safe, "Rebuilt when needed."),
            at(
                ".tmp",
                "Temporary files",
                Tier::Safe,
                "Scratch files of past sessions.",
            ),
            at(
                "tmp",
                "Temporary files",
                Tier::Safe,
                "Scratch files of past sessions.",
            ),
            at(
                "shell_snapshots",
                "Shell snapshots",
                Tier::Safe,
                "Shell environment captured at session start; made again for each session.",
            ),
            at(
                "worktrees/*",
                "Codex worktree",
                Tier::Review,
                "A worktree Codex made for a task. Changes not merged elsewhere are lost.",
            ),
            at(
                "sessions/*/*/*/rollout-*.jsonl",
                "Session",
                Tier::Caution,
                RESUME,
            )
            .reads(Reader::Codex),
            at(
                "archived_sessions/rollout-*.jsonl",
                "Archived session",
                Tier::Caution,
                "A session you archived in Codex.",
            )
            .reads(Reader::Codex),
            at(
                "history.jsonl",
                "Prompt history",
                Tier::Caution,
                "Every prompt you typed, for recall.",
            ),
        ],
        processes: &["codex"],
        commands: &["@openai/codex"],
        apps: &["com.openai.codex"],
        ..Default::default()
    }
}

fn gemini() -> AiTool {
    AiTool {
        name: "Gemini CLI",
        roots: vec![Root::Home(".gemini".into()), Root::EnvJoin("GEMINI_CLI_HOME", ".gemini")],
        locations: vec![
            at("tmp/bin", "Helper downloads", Tier::Safe, "Tools Gemini CLI downloads, such as ripgrep."),
            at("tmp/*/plans", "Plans", Tier::Review, "Plans of past sessions."),
            at("tmp/*/tracker", "Task tracker", Tier::Review, "Task lists of past sessions."),
            at("tmp/*/checkpoints", "Checkpoints", Tier::Review, "Checkpoints of past sessions."),
            at("tmp/*/chats", "Chats", Tier::Caution, "The saved sessions of a project; /resume lists them.").reads(Reader::Chat),
            at("tmp/*/checkpoint-*.json", "Saved chat", Tier::Caution, "A chat saved with /chat save."),
            at("tmp/*/logs.json", "Prompt log", Tier::Caution, "Prompts of a project, for recall."),
            at("history/*", "Restore snapshots", Tier::Caution, "File snapshots that /restore uses."),
        ],
        processes: &["gemini"],
        commands: &["@google/gemini-cli", "/bin/gemini"],
        command: Some("Gemini CLI removes old sessions itself when general.sessionRetention is set in settings.json."),
        ..Default::default()
    }
}

fn qwen() -> AiTool {
    AiTool {
        name: "Qwen Code",
        roots: vec![Root::Home(".qwen".into()), Root::Env("QWEN_HOME")],
        locations: vec![
            at("debug", "Debug logs", Tier::Safe, "Logs of past sessions."),
            at(
                "bin",
                "Helper downloads",
                Tier::Safe,
                "Tools Qwen Code downloads.",
            ),
            at("tmp/*/chats", "Chats", Tier::Caution, RESUME).reads(Reader::Chat),
            at("projects/*/chats/*.jsonl", "Chat", Tier::Caution, RESUME).reads(Reader::Chat),
        ],
        processes: &["qwen"],
        commands: &["@qwen-code/qwen-code"],
        ..Default::default()
    }
}

fn copilot() -> AiTool {
    AiTool {
        name: "GitHub Copilot CLI",
        roots: vec![Root::Home(".copilot".into()), Root::Env("COPILOT_HOME")],
        locations: vec![
            at("logs", "Logs", Tier::Safe, "Logs of past sessions."),
            at("session-state/*", "Session", Tier::Caution, RESUME)
                .reads(Reader::Meta("workspace.yaml")),
        ],
        processes: &["copilot"],
        commands: &["@github/copilot"],
        ..Default::default()
    }
}

fn opencode() -> AiTool {
    AiTool {
        name: "opencode",
        roots: vec![Root::Home(".local/share/opencode".into())],
        locations: vec![
            at("log", "Logs", Tier::Safe, "Logs of past sessions."),
            at(
                "tool-output",
                "Saved tool output",
                Tier::Safe,
                "Full output of past tool calls.",
            ),
            at(
                "worktree/*",
                "opencode worktree",
                Tier::Review,
                "A worktree opencode made. Changes not merged elsewhere are lost.",
            ),
            at(
                "snapshot",
                "Undo snapshots",
                Tier::Caution,
                "Snapshots that /undo uses.",
            ),
            at(
                "storage",
                "Session storage",
                Tier::Caution,
                "Sessions and messages of older versions.",
            ),
            at(
                "opencode.db",
                "Session database",
                Tier::Caution,
                "Every session and message.",
            ),
        ],
        processes: &["opencode"],
        home_locations: vec![at(
            ".cache/opencode",
            "opencode cache",
            Tier::Safe,
            "Packages and models data downloaded again when needed.",
        )],
        ..Default::default()
    }
}

fn cline_cli() -> AiTool {
    AiTool {
        name: "Cline CLI",
        roots: vec![Root::Home(".cline".into()), Root::Env("CLINE_DIR")],
        locations: vec![
            at("data/logs", "Logs", Tier::Safe, "Logs of past sessions."),
            at(
                "worktrees/*",
                "Cline worktree",
                Tier::Review,
                "A worktree Cline made. Changes not merged elsewhere are lost.",
            ),
            at("data/sessions/*", "Session", Tier::Caution, RESUME).reads(Reader::Meta("*.json")),
            at("data/tasks/*", "Task", Tier::Caution, RESUME).reads(Reader::ExtensionTask),
        ],
        processes: &["cline"],
        ..Default::default()
    }
}

fn hermes() -> AiTool {
    AiTool {
        name: "Hermes Agent",
        roots: vec![Root::Home(".hermes".into()), Root::Env("HERMES_HOME")],
        locations: vec![
            at("logs", "Logs", Tier::Safe, "Logs of past sessions."),
            at("cache", "Cache", Tier::Safe, "Rebuilt when needed."),
            at(
                "checkpoints",
                "Checkpoints",
                Tier::Review,
                "File checkpoints of past sessions.",
            ),
            at(
                "session-exports",
                "Session exports",
                Tier::Review,
                "Sessions you exported.",
            ),
            at("sessions", "Sessions", Tier::Caution, RESUME),
            at(
                "terminal-sessions",
                "Terminal sessions",
                Tier::Caution,
                "Recorded terminal sessions.",
            ),
            at(
                "state.db",
                "Session database",
                Tier::Caution,
                "Every session and message.",
            ),
        ],
        processes: &["hermes"],
        commands: &["/hermes"],
        command: Some(
            "hermes sessions prune --dry-run shows the old sessions Hermes would remove.",
        ),
        ..Default::default()
    }
}

fn herdr() -> AiTool {
    AiTool {
        name: "herdr",
        roots: vec![Root::Home(".local/state/herdr".into())],
        locations: vec![
            at("*.log", "Log", Tier::Safe, "Logs of past sessions."),
            at("logs", "Logs", Tier::Safe, "Logs of past sessions."),
            at(
                "sessions",
                "Saved sessions",
                Tier::Caution,
                "Layouts and agents herdr restores.",
            ),
            at(
                "session-history.json",
                "Session history",
                Tier::Caution,
                "The list of past sessions.",
            ),
        ],
        processes: &["herdr"],
        home_locations: vec![at(
            ".herdr/worktrees/*",
            "herdr worktree",
            Tier::Review,
            "A worktree herdr made for an agent. Changes not merged elsewhere are lost.",
        )],
        ..Default::default()
    }
}

fn goose() -> AiTool {
    AiTool {
        name: "Goose",
        processes: &["goose", "goosed"],
        apps: &["com.block.goose"],
        home_locations: vec![
            at(
                ".local/state/goose/logs",
                "Goose logs",
                Tier::Safe,
                "Logs of past sessions.",
            ),
            at(
                "Library/Application Support/Goose/logs",
                "Goose app logs",
                Tier::Safe,
                "Logs of past sessions.",
            ),
            at(
                ".local/share/goose/data/models",
                "Goose local models",
                Tier::Review,
                "Models Goose downloaded; downloaded again when needed.",
            ),
            at(
                ".local/share/goose/sessions",
                "Goose sessions",
                Tier::Caution,
                RESUME,
            ),
        ],
        ..Default::default()
    }
}

fn amp() -> AiTool {
    AiTool {
        name: "Amp",
        processes: &["amp"],
        commands: &["@sourcegraph/amp"],
        home_locations: vec![
            at(
                ".cache/amp/logs",
                "Amp logs",
                Tier::Safe,
                "Logs of past sessions.",
            ),
            at(
                ".local/share/amp/threads",
                "Amp threads",
                Tier::Caution,
                "Local copies of your threads.",
            ),
            at(
                ".local/share/amp/history.jsonl",
                "Amp prompt history",
                Tier::Caution,
                "Every prompt you typed, for recall.",
            ),
        ],
        ..Default::default()
    }
}

fn kiro_cli() -> AiTool {
    AiTool {
        name: "Kiro CLI",
        processes: &["kiro-cli"],
        home_locations: vec![at(".kiro/sessions", "Kiro sessions", Tier::Caution, RESUME)],
        ..Default::default()
    }
}

fn crush() -> AiTool {
    AiTool {
        name: "Crush",
        processes: &["crush"],
        home_locations: vec![at(
            ".cache/crush",
            "Crush cache",
            Tier::Safe,
            "Rebuilt when needed.",
        )],
        ..Default::default()
    }
}

fn zed() -> AiTool {
    AiTool {
        name: "Zed",
        roots: vec![Root::Home("Library/Application Support/Zed".into())],
        locations: vec![
            at(
                "threads",
                "Agent threads",
                Tier::Caution,
                "Your agent conversations.",
            ),
            at(
                "conversations",
                "Assistant conversations",
                Tier::Caution,
                "Conversations of the older assistant panel.",
            ),
        ],
        process_paths: &["/Zed.app/", "/Zed Preview.app/"],
        apps: &["dev.zed.Zed"],
        ..Default::default()
    }
}

fn continue_dev() -> AiTool {
    AiTool {
        name: "Continue",
        roots: vec![Root::Home(".continue".into())],
        locations: vec![
            at(
                "index/*",
                "Code index",
                Tier::Safe,
                "Rebuilt by indexing your code again.",
            ),
            at("logs", "Logs", Tier::Safe, "Logs of past sessions."),
            at(
                ".utils",
                "Helper downloads",
                Tier::Safe,
                "Downloaded again when needed.",
            ),
            at(
                "dev_data",
                "Usage data",
                Tier::Safe,
                "Usage records of past sessions.",
            ),
            at(
                "sessions",
                "Chat sessions",
                Tier::Caution,
                "Your Continue chats.",
            ),
        ],
        ..Default::default()
    }
}

fn ollama() -> AiTool {
    AiTool {
        name: "Ollama",
        roots: vec![
            Root::Env("OLLAMA_MODELS"),
            Root::Home(".ollama/models".into()),
        ],
        locations: vec![
            at("manifests", "Model", Tier::Review, "").reads(Reader::Ollama),
            at(
                "blobs/*-partial*",
                "Unfinished download",
                Tier::Safe,
                "Part of a model download that did not finish.",
            ),
        ],
        processes: &["ollama"],
        process_paths: &["/Ollama.app/"],
        apps: &["com.electron.ollama"],
        command: Some("ollama rm <model> removes a model, the same way."),
        ..Default::default()
    }
}

fn hugging_face() -> AiTool {
    let model = "A downloaded model. Downloaded again the next time a program asks for it.";
    AiTool {
        name: "Hugging Face",
        roots: vec![
            Root::Env("HF_HUB_CACHE"),
            Root::EnvJoin("HF_HOME", "hub"),
            Root::Home(".cache/huggingface/hub".into()),
        ],
        locations: vec![
            at("models--*", "Model", Tier::Review, model).reads(Reader::HubRepo),
            at(
                "datasets--*",
                "Dataset",
                Tier::Review,
                "A downloaded dataset. Downloaded again when needed.",
            )
            .reads(Reader::HubRepo),
            at(
                "spaces--*",
                "Space",
                Tier::Review,
                "A downloaded Space. Downloaded again when needed.",
            )
            .reads(Reader::HubRepo),
        ],
        command: Some("The hf command lists and removes cached repositories too (hf cache)."),
        ..Default::default()
    }
}

fn lm_studio() -> AiTool {
    AiTool {
        name: "LM Studio",
        roots: vec![
            Root::Pointer(".lmstudio-home-pointer"),
            Root::Home(".lmstudio".into()),
            Root::Home(".cache/lm-studio".into()),
        ],
        locations: vec![
            at(
                "models/*/*",
                "Model",
                Tier::Review,
                "A downloaded model. Download it again in LM Studio to use it.",
            ),
            at(
                "server-logs",
                "Server logs",
                Tier::Safe,
                "Logs of the local server.",
            ),
        ],
        processes: &["LM Studio", "lms"],
        process_paths: &["/LM Studio.app/"],
        apps: &["ai.elementlabs.lmstudio"],
        ..Default::default()
    }
}

fn local_models() -> AiTool {
    let model = "A downloaded model. Downloaded again the next time a program asks for it.";
    AiTool {
        name: "Local models",
        home_locations: vec![
            at(".cache/whisper/*.pt", "Whisper model", Tier::Review, model),
            at(
                ".cache/torch/hub/checkpoints/*",
                "PyTorch Hub weights",
                Tier::Review,
                model,
            ),
            at(
                "Library/Application Support/nomic.ai/GPT4All/*.gguf",
                "GPT4All model",
                Tier::Review,
                "A downloaded model. Download it again in GPT4All to use it.",
            ),
            at(
                ".diffusionbee/downloads",
                "DiffusionBee models",
                Tier::Review,
                "Downloaded models. Download them again in DiffusionBee to use them.",
            ),
        ],
        ..Default::default()
    }
}
