//! Readers for the session stores of AI tools other than Claude Code. Each one returns what
//! a session says about itself: its project folder, first prompt, title and start time.
use super::{prompt_text, session_info, SessionInfo};
use crate::services::Services;
use serde_json::Value;

/// How to read an item a location matches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reader {
    /// A Codex rollout: `sessions/YYYY/MM/DD/rollout-*.jsonl`. The first line holds the
    /// session's `cwd`; user messages follow.
    Codex,
    /// A Gemini CLI or Qwen Code chat: a `chats` folder whose project is named in
    /// `.project_root` beside it, or a single JSON Lines chat file.
    Chat,
    /// A Cline, Roo Code or Kilo Code task folder in an editor's extension storage.
    ExtensionTask,
    /// An item with a metadata file inside (JSON, or `key: value` lines). A `*` in the name
    /// takes the first matching file.
    Meta(&'static str),
    /// An editor's per-folder state in `workspaceStorage/<hash>`, listed only when the folder
    /// it belongs to no longer exists.
    Workspace,
    /// The chat sessions of an editor workspace, `workspaceStorage/<hash>/chatSessions`.
    WorkspaceChats,
    /// A Hugging Face hub repository, `models--org--name`.
    HubRepo,
    /// The models in an Ollama store, read from `manifests/`.
    Ollama,
}

/// What a reader found for one item.
pub enum Read {
    /// A session, with what it says about itself.
    Session(SessionInfo),
    /// Editor state for a folder that no longer exists.
    OrphanWorkspace { folder: String, chats: bool },
    /// A named item, such as a model repository.
    Named(String),
    /// Nothing to list.
    Skip,
}

pub fn read(s: &Services, reader: Reader, path: &str, directory: bool) -> Read {
    match reader {
        Reader::Codex => Read::Session(codex(&head(s, path))),
        Reader::Chat if directory => Read::Session(chat_folder(s, path)),
        Reader::Chat => Read::Session(chat_lines(&head(s, path))),
        Reader::ExtensionTask => Read::Session(extension_task(s, path)),
        Reader::Meta(file) => Read::Session(meta(s, path, directory, file)),
        Reader::Workspace => match workspace_folder(s, &format!("{path}/workspace.json")) {
            Some(folder) if s.missing(&folder) => Read::OrphanWorkspace {
                chats: s.is_dir(&format!("{path}/chatSessions")),
                folder,
            },
            _ => Read::Skip,
        },
        Reader::WorkspaceChats => {
            let parent = parent(path);
            let mut info = first_file(s, path, ".json")
                .and_then(|f| s.fs.read(&f, 2 * 1024 * 1024).ok())
                .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
                .map(|v| SessionInfo {
                    first_prompt: v["requests"]
                        .as_array()
                        .and_then(|r| r.first())
                        .and_then(|r| prompt_text(&r["message"]["text"])),
                    title: v["customTitle"].as_str().map(str::to_owned),
                    ..Default::default()
                })
                .unwrap_or_default();
            info.cwd = workspace_folder(s, &format!("{parent}/workspace.json"));
            if info.cwd.is_none() {
                return Read::Skip;
            }
            Read::Session(info)
        }
        Reader::HubRepo => Read::Named(hub_repo(name(path))),
        Reader::Ollama => Read::Skip,
    }
}

fn name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}
fn parent(path: &str) -> &str {
    path.rsplit_once('/').map_or(path, |(p, _)| p)
}
/// The first 512 KB of a file, enough for a session's opening lines.
fn head(s: &Services, path: &str) -> String {
    s.fs.read(path, 512 * 1024)
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default()
}
/// The first file in a folder, by name, ending with `suffix`.
fn first_file(s: &Services, folder: &str, suffix: &str) -> Option<String> {
    let mut files: Vec<_> = s
        .children(folder, &mut vec![])
        .into_iter()
        .filter(|e| e.regular && e.name().ends_with(suffix))
        .map(|e| e.path().to_owned())
        .collect();
    files.sort();
    files.into_iter().next()
}

/// A Codex rollout. The `session_meta` line has the working folder and branch; the first
/// user message is an `event_msg` or a `response_item`, after the instructions Codex adds
/// in angle-bracket tags.
pub fn codex(head: &str) -> SessionInfo {
    let mut info = SessionInfo::default();
    for line in head.lines() {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let p = &v["payload"];
        match (v["type"].as_str(), p["type"].as_str()) {
            (Some("session_meta"), _) => {
                info.cwd = info.cwd.or(p["cwd"].as_str().map(str::to_owned));
                info.started = info.started.or(p["timestamp"].as_str().map(str::to_owned));
                info.branch = info
                    .branch
                    .or(p["git"]["branch"].as_str().map(str::to_owned));
            }
            (Some("event_msg"), Some("user_message")) if info.first_prompt.is_none() => {
                info.first_prompt = prompt_text(&p["message"]);
            }
            (Some("response_item"), Some("message"))
                if info.first_prompt.is_none() && p["role"] == "user" =>
            {
                info.first_prompt = prompt_text(&p["content"]);
            }
            _ => {}
        }
        if info.cwd.is_some() && info.first_prompt.is_some() {
            break;
        }
    }
    info
}

/// A Gemini CLI `chats` folder: the project from `.project_root` beside it, the first prompt
/// and start time from its oldest chat.
fn chat_folder(s: &Services, path: &str) -> SessionInfo {
    let mut info = first_file(s, path, ".json")
        .and_then(|f| s.fs.read(&f, 2 * 1024 * 1024).ok())
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .map(|v| gemini_chat(&v))
        .unwrap_or_default();
    if let Some(root) = s.read_text(&format!("{}/.project_root", parent(path))) {
        let root = root.trim();
        if !root.is_empty() {
            info.cwd = Some(root.to_owned());
        }
    }
    info
}
/// A saved Gemini chat: `{ startTime, messages: [{ type: "user", content }] }`.
pub fn gemini_chat(v: &Value) -> SessionInfo {
    SessionInfo {
        started: v["startTime"].as_str().map(str::to_owned),
        first_prompt: v["messages"].as_array().and_then(|m| {
            m.iter()
                .filter(|m| m["type"] == "user")
                .find_map(|m| prompt_text(&m["content"]))
        }),
        ..Default::default()
    }
}
/// A JSON Lines chat in the Claude Code style, with Gemini-style `parts` also read.
fn chat_lines(head: &str) -> SessionInfo {
    let mut info = session_info(head);
    if info.first_prompt.is_none() {
        info.first_prompt = head
            .lines()
            .filter_map(|l| serde_json::from_str::<Value>(l).ok())
            .filter(|v| v["type"] == "user" || v["role"] == "user")
            .find_map(|v| prompt_text(&v["message"]["parts"]).or_else(|| prompt_text(&v["parts"])));
    }
    info
}

/// A Cline-family task: the first prompt is the `say: "task"` message in `ui_messages.json`;
/// the folder is in `history_item.json` (Roo Code, Kilo Code) or in the extension's
/// `state/taskHistory.json` (Cline).
pub fn extension_task(s: &Services, path: &str) -> SessionInfo {
    let mut info = SessionInfo::default();
    if let Ok(bytes) =
        s.fs.read(&format!("{path}/ui_messages.json"), 4 * 1024 * 1024)
    {
        let text = String::from_utf8_lossy(&bytes);
        let task = serde_json::from_str::<Value>(&text)
            .ok()
            .and_then(|v| v.as_array()?.iter().find(|m| m["say"] == "task").cloned());
        match task {
            Some(m) => {
                info.first_prompt = prompt_text(&m["text"]);
                info.started = m["ts"].as_f64().map(|ms| iso(ms / 1000.0));
            }
            // A file too large to read whole: take the task message from its start.
            None => info.first_prompt = first_task(&text),
        }
    }
    let item = s
        .read_text(&format!("{path}/history_item.json"))
        .and_then(|t| serde_json::from_str::<Value>(&t).ok());
    info.cwd = item.as_ref().and_then(folder_hint);
    if info.cwd.is_none() {
        let id = name(path);
        let history = format!("{}/state/taskHistory.json", parent(parent(path)));
        info.cwd =
            s.fs.read(&history, 8 * 1024 * 1024)
                .ok()
                .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
                .and_then(|v| {
                    v.as_array()?
                        .iter()
                        .find(|t| t["id"] == id)
                        .and_then(folder_hint)
                });
    }
    info
}
fn first_task(text: &str) -> Option<String> {
    let at = text.find(r#""say":"task""#)?;
    let rest = &text[at..];
    let start = rest.find(r#""text":"#)? + 7;
    let value: String = serde_json::Deserializer::from_str(&rest[start..])
        .into_iter::<String>()
        .next()?
        .ok()?;
    prompt_text(&Value::String(value))
}

/// A metadata file inside a session item, or the item itself when it is a file.
fn meta(s: &Services, path: &str, directory: bool, file: &str) -> SessionInfo {
    let target = if !directory {
        Some(path.to_owned())
    } else if let Some((prefix, suffix)) = file.split_once('*') {
        let mut files: Vec<_> = s
            .children(path, &mut vec![])
            .into_iter()
            .filter(|e| e.regular && e.name().starts_with(prefix) && e.name().ends_with(suffix))
            .map(|e| e.path().to_owned())
            .collect();
        files.sort();
        files.into_iter().next()
    } else {
        Some(format!("{path}/{file}"))
    };
    let Some(text) = target.and_then(|t| s.read_text(&t)) else {
        return SessionInfo::default();
    };
    let v = serde_json::from_str::<Value>(&text).unwrap_or_else(|_| key_values(&text));
    SessionInfo {
        cwd: folder_hint(&v),
        title: ["customTitle", "title", "summary"].iter().find_map(|k| {
            v[k].as_str()
                .and_then(|t| prompt_text(&Value::String(t.into())))
        }),
        first_prompt: ["firstPrompt", "initialPrompt", "prompt", "task"]
            .iter()
            .find_map(|k| prompt_text(&v[k])),
        started: [
            "createdAt",
            "created_at",
            "startTime",
            "started",
            "timestamp",
        ]
        .iter()
        .find_map(|k| v[k].as_str().map(str::to_owned)),
        branch: v["branch"].as_str().map(str::to_owned),
    }
}
/// Simple `key: value` lines, as in a YAML file with no nesting.
fn key_values(text: &str) -> Value {
    let mut map = serde_json::Map::new();
    for line in text.lines() {
        if line.starts_with([' ', '\t', '-', '#']) {
            continue;
        }
        if let Some((k, v)) = line.split_once(':') {
            let v = v.trim().trim_matches(|c| c == '"' || c == '\'');
            if !v.is_empty() {
                map.insert(k.trim().to_owned(), Value::String(v.to_owned()));
            }
        }
    }
    Value::Object(map)
}
/// The working folder a record names, under the keys AI tools use for it.
fn folder_hint(v: &Value) -> Option<String> {
    [
        "cwd",
        "cwdOnTaskInitialization",
        "workspace",
        "workspaceFolder",
        "workingDirectory",
        "projectRoot",
        "projectPath",
        "git_root",
    ]
    .iter()
    .filter_map(|k| v[k].as_str())
    .find_map(file_path)
}
/// A local path, from a plain path or a `file://` URI. Other URIs (remote folders) give
/// `None`, since their existence cannot be checked here.
pub fn file_path(value: &str) -> Option<String> {
    let value = value.trim();
    if value.starts_with('/') {
        return Some(value.to_owned());
    }
    let rest = value.strip_prefix("file://")?;
    let bytes = rest.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
            if let Some(b) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    let path = String::from_utf8(out).ok()?;
    path.starts_with('/').then_some(path)
}
/// The folder an editor's `workspace.json` belongs to: `folder`, or the `.code-workspace`
/// file of a multi-root workspace.
fn workspace_folder(s: &Services, file: &str) -> Option<String> {
    let v: Value = serde_json::from_str(&s.read_text(file)?).ok()?;
    ["folder", "workspace", "configuration"]
        .iter()
        .filter_map(|k| v[k].as_str())
        .find_map(file_path)
}

/// `models--org--name` as `org/name`.
pub fn hub_repo(folder: &str) -> String {
    folder
        .split_once("--")
        .map_or(folder, |(_, rest)| rest)
        .replace("--", "/")
}

/// Seconds since 1970 as an ISO 8601 UTC time.
fn iso(seconds: f64) -> String {
    let secs = seconds as i64;
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// One model in an Ollama store.
pub struct OllamaModel {
    /// The manifest file; removing it is how `ollama rm` removes a model.
    pub manifest: String,
    /// `llama3:latest`, or `host/namespace/model:tag` outside the default library.
    pub name: String,
    /// Bytes of the files only this model uses, which Ollama frees once the manifest is gone.
    pub unique: u64,
    /// Bytes it shares with other models, which stay.
    pub shared: u64,
}
/// The models in an Ollama store (`~/.ollama/models` or `OLLAMA_MODELS`), from
/// `manifests/<host>/<namespace>/<model>/<tag>`. Each manifest lists the blobs, by digest,
/// that make up a model; blobs several models use are counted as shared.
pub fn ollama_models(s: &Services, root: &str) -> Vec<OllamaModel> {
    let base = format!("{root}/manifests");
    let mut files = vec![];
    let mut pending = vec![(base.clone(), 0)];
    while let Some((dir, depth)) = pending.pop() {
        for e in s.children(&dir, &mut vec![]) {
            if e.directory && depth < 6 {
                pending.push((e.path().to_owned(), depth + 1));
            } else if !e.directory {
                files.push(e.path().to_owned());
            }
        }
    }
    files.sort();
    let layers: Vec<(String, Vec<(String, u64)>)> = files
        .into_iter()
        .filter_map(|f| {
            let v: Value = serde_json::from_str(&s.read_text(&f)?).ok()?;
            let mut blobs: Vec<(String, u64)> = v["layers"]
                .as_array()?
                .iter()
                .chain(std::iter::once(&v["config"]))
                .filter_map(|l| Some((l["digest"].as_str()?.to_owned(), l["size"].as_u64()?)))
                .collect();
            blobs.sort();
            blobs.dedup();
            Some((f, blobs))
        })
        .collect();
    let mut uses: std::collections::HashMap<&str, usize> = Default::default();
    for (_, blobs) in &layers {
        for (digest, _) in blobs {
            *uses.entry(digest).or_default() += 1;
        }
    }
    layers
        .iter()
        .map(|(file, blobs)| {
            let (unique, shared) = blobs.iter().fold((0u64, 0u64), |(u, sh), (d, size)| {
                if uses[d.as_str()] == 1 {
                    (u.saturating_add(*size), sh)
                } else {
                    (u, sh.saturating_add(*size))
                }
            });
            let parts: Vec<&str> = file
                .strip_prefix(&base)
                .unwrap_or(file)
                .trim_start_matches('/')
                .split('/')
                .collect();
            let name = match parts.as_slice() {
                ["registry.ollama.ai", "library", model, tag] => format!("{model}:{tag}"),
                [rest @ .., tag] if !rest.is_empty() => format!("{}:{tag}", rest.join("/")),
                _ => name(file).to_owned(),
            };
            OllamaModel {
                manifest: file.clone(),
                name,
                unique,
                shared,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_rollouts_and_paths() {
        let info = codex(
            [
                r#"{"type":"session_meta","payload":{"id":"x","timestamp":"2026-09-01T08:00:00Z","cwd":"/Users/x/app","git":{"branch":"main"}}}"#,
                r#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"<environment_context>...</environment_context>"}]}}"#,
                r#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"add a test"}]}}"#,
            ]
            .join("\n")
            .as_str(),
        );
        assert_eq!(info.cwd.as_deref(), Some("/Users/x/app"));
        assert_eq!(info.branch.as_deref(), Some("main"));
        assert_eq!(info.first_prompt.as_deref(), Some("add a test"));
        assert_eq!(
            file_path("file:///Users/x/My%20App").as_deref(),
            Some("/Users/x/My App")
        );
        assert_eq!(file_path("vscode-remote://ssh/x"), None);
        // A percent sign before a multi-byte character is kept, not a panic.
        assert_eq!(
            file_path("file:///Users/x/%€").as_deref(),
            Some("/Users/x/%€")
        );
        assert_eq!(
            hub_repo("models--meta-llama--Llama-3-8B"),
            "meta-llama/Llama-3-8B"
        );
        assert_eq!(iso(1_759_312_800.0), "2025-10-01T10:00:00Z");
        assert_eq!(
            first_task(r#"[{"ts":1,"type":"say","say":"task","text":"fix \"it\"","x":1"#)
                .as_deref(),
            Some("fix \"it\"")
        );
    }
}
