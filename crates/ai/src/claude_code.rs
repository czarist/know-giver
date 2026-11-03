//! Claude through the official Claude Code program, signed in with the
//! student's own Claude subscription.
//!
//! Anthropic lets products run the unmodified Claude Code binary when each
//! end user signs in with their own credentials through Claude Code's flow;
//! the app never sees or stores them. Each request runs `claude -p` with the
//! app's prompt, no tools, no MCP servers and none of the user's Claude Code
//! settings, from an empty folder.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::{Value, json};

use crate::AiError;
use crate::account::{AccountState, LoginEvent, data_url, hide_window, run_streaming};

/// Marker base URL saved in the settings; never contacted over HTTP.
pub(crate) const BASE_URL: &str = "account://claude-code";

/// Model aliases Claude Code accepts; it maps them to the newest model of
/// each family the subscription allows.
pub(crate) const MODELS: [&str; 3] = ["sonnet", "opus", "haiku"];

/// Variables that would make Claude Code bill an API key or talk to another
/// server instead of the student's subscription.
const OVERRIDES: [&str; 6] = [
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_AUTH_TOKEN",
    "ANTHROPIC_BASE_URL",
    "CLAUDE_CODE_USE_BEDROCK",
    "CLAUDE_CODE_USE_VERTEX",
    "CLAUDECODE",
];

pub(crate) fn is_endpoint(base_url: &str) -> bool {
    base_url.trim().trim_end_matches('/') == BASE_URL
}

fn error(message: String) -> AiError {
    AiError::ClaudeCode(message)
}

/// Where Claude Code is installed: on the PATH, from its native installer,
/// or inside an npm install (the `.cmd` shim is skipped for the real binary).
pub fn executable() -> Option<PathBuf> {
    let name = if cfg!(windows) {
        "claude.exe"
    } else {
        "claude"
    };
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            candidates.push(dir.join(name));
            candidates.push(npm_binary(&dir));
        }
    }
    if let Some(home) = home() {
        candidates.push(home.join(".local").join("bin").join(name));
        candidates.push(home.join(".claude").join("local").join(name));
    }
    if let Some(appdata) = std::env::var_os("APPDATA") {
        candidates.push(npm_binary(&PathBuf::from(appdata).join("npm")));
    }
    candidates.into_iter().find(|path| path.is_file())
}

fn npm_binary(dir: &Path) -> PathBuf {
    let name = if cfg!(windows) {
        "claude.exe"
    } else {
        "claude"
    };
    dir.join("node_modules")
        .join("@anthropic-ai")
        .join("claude-code")
        .join("bin")
        .join(name)
}

fn home() -> Option<PathBuf> {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from)
}

fn command(exe: &Path) -> Command {
    let mut command = Command::new(exe);
    for name in OVERRIDES {
        command.env_remove(name);
    }
    hide_window(&mut command);
    command
}

pub(crate) fn state() -> AccountState {
    let Some(exe) = executable() else {
        return AccountState::Missing;
    };
    let output = command(&exe)
        .args(["auth", "status"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output();
    match output {
        Ok(output) => parse_status(&String::from_utf8_lossy(&output.stdout)),
        Err(err) => {
            tracing::warn!(%err, "could not ask Claude Code for its sign-in");
            AccountState::SignedOut
        }
    }
}

fn parse_status(json: &str) -> AccountState {
    let status: Value = serde_json::from_str(json).unwrap_or_default();
    let signed_in = status["loggedIn"].as_bool().unwrap_or(false)
        && status["authMethod"].as_str().is_some_and(|m| m != "none");
    if signed_in {
        AccountState::Ready
    } else {
        AccountState::SignedOut
    }
}

/// Claude Code's own browser sign-in with a Claude subscription.
pub(crate) fn login(mut on_event: impl FnMut(LoginEvent<'_>)) -> Result<(), AiError> {
    let exe = executable().ok_or(AiError::ClaudeCodeMissing)?;
    let mut login = command(&exe);
    login.args(["auth", "login"]);
    run_streaming(login, "Claude Code sign-in", error, |line| {
        on_event(LoginEvent::Output(line));
    })
}

/// Runs Anthropic's official installer for this system.
pub(crate) fn install(on_output: impl FnMut(&str)) -> Result<(), AiError> {
    let installer = if cfg!(windows) {
        let mut c = Command::new("powershell");
        c.args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            "irm https://claude.ai/install.ps1 | iex",
        ]);
        c
    } else {
        let mut c = Command::new("bash");
        c.args(["-c", "curl -fsSL https://claude.ai/install.sh | bash"]);
        c
    };
    run_streaming(installer, "the Claude Code installer", error, on_output)?;
    if executable().is_some() {
        Ok(())
    } else {
        Err(AiError::ClaudeCodeMissing)
    }
}

/// Arguments for one answer: no tools, no MCP, no saved session, none of the
/// user's settings, one turn, streamed as JSON lines.
fn args(model: &str, system_file: &Path) -> Vec<std::ffi::OsString> {
    let mut args: Vec<std::ffi::OsString> = [
        "-p",
        "--input-format",
        "stream-json",
        "--output-format",
        "stream-json",
        "--verbose",
        "--include-partial-messages",
        "--tools",
        "",
        "--strict-mcp-config",
        "--setting-sources",
        "",
        "--disable-slash-commands",
        "--no-session-persistence",
        "--max-turns",
        "1",
        "--model",
    ]
    .into_iter()
    .map(Into::into)
    .collect();
    args.push(model.into());
    args.push("--system-prompt-file".into());
    args.push(system_file.into());
    args
}

/// The system prompt and one user message: earlier turns become a labelled
/// transcript, images become base64 image blocks.
fn request(messages: &Value) -> Result<(String, Value), AiError> {
    let mut system = String::new();
    let mut transcript = String::new();
    let mut images = Vec::new();
    let list = messages
        .as_array()
        .ok_or_else(|| AiError::InvalidResponse("missing chat messages".into()))?;
    let conversation = list.iter().filter(|m| m["role"] != "system").count();
    for message in list {
        let role = message["role"].as_str().unwrap_or("user");
        let mut text = String::new();
        match &message["content"] {
            Value::String(s) => text.push_str(s),
            Value::Array(parts) => {
                for part in parts {
                    match part["type"].as_str() {
                        Some("text") => {
                            if !text.is_empty() {
                                text.push('\n');
                            }
                            text.push_str(part["text"].as_str().unwrap_or_default());
                        }
                        Some("image_url") => {
                            let url = part["image_url"]["url"].as_str().unwrap_or_default();
                            let (media, data) = data_url(url).ok_or_else(|| {
                                AiError::InvalidResponse("image is not a data URL".into())
                            })?;
                            images.push(json!({
                                "type": "image",
                                "source": {"type": "base64", "media_type": media, "data": data}
                            }));
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        if role == "system" {
            if !system.is_empty() {
                system.push_str("\n\n");
            }
            system.push_str(&text);
            continue;
        }
        if conversation > 1 {
            if !transcript.is_empty() {
                transcript.push_str("\n\n");
            }
            transcript.push_str(if role == "assistant" {
                "Assistant:\n"
            } else {
                "User:\n"
            });
        }
        transcript.push_str(&text);
    }
    if transcript.trim().is_empty() && images.is_empty() {
        return Err(AiError::InvalidResponse(
            "request has no user message".into(),
        ));
    }
    let mut content = vec![json!({"type": "text", "text": transcript})];
    content.extend(images);
    let line = json!({
        "type": "user",
        "message": {"role": "user", "content": content}
    });
    Ok((system, line))
}

pub(crate) fn chat(
    model: &str,
    messages: &Value,
    mut on_delta: impl FnMut(&str),
) -> Result<String, AiError> {
    let exe = executable().ok_or(AiError::ClaudeCodeMissing)?;
    let (system, line) = request(messages)?;
    // An empty folder, so no project instructions are picked up.
    let dir = tempfile::tempdir().map_err(|err| error(format!("no temporary folder: {err}")))?;
    let system_file = dir.path().join("system.md");
    std::fs::write(&system_file, &system)
        .map_err(|err| error(format!("could not write the instructions: {err}")))?;
    let mut child = command(&exe)
        .args(args(model, &system_file))
        .current_dir(dir.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| error(format!("could not start Claude Code: {err}")))?;
    let guard = KillOnDrop(&mut child);
    if let Some(mut stdin) = guard.0.stdin.take() {
        writeln!(stdin, "{line}")
            .map_err(|err| error(format!("could not send the request: {err}")))?;
        // Closing stdin ends the conversation after this turn.
    }
    let stdout = guard
        .0
        .stdout
        .take()
        .ok_or_else(|| error("Claude Code output is unavailable".into()))?;
    let stderr = guard.0.stderr.take();
    let errors = std::thread::spawn(move || {
        let mut text = String::new();
        if let Some(stderr) = stderr {
            for line in BufReader::new(stderr)
                .lines()
                .map_while(Result::ok)
                .take(20)
            {
                text.push_str(&line);
                text.push('\n');
            }
        }
        text
    });
    let mut reader = Reader::default();
    for line in BufReader::new(stdout).lines() {
        let line = line.map_err(|err| error(format!("could not read Claude Code: {err}")))?;
        if let Some(done) = reader.line(&line, &mut on_delta) {
            return done;
        }
    }
    drop(guard);
    let stderr = errors.join().unwrap_or_default();
    Err(error(format!(
        "Claude Code ended without an answer: {}",
        stderr.trim()
    )))
}

/// Stops Claude Code if the answer is abandoned midway.
struct KillOnDrop<'a>(&'a mut std::process::Child);

impl Drop for KillOnDrop<'_> {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Reads Claude Code's JSON lines: text deltas while it writes, then the
/// final result.
#[derive(Default)]
struct Reader {
    streamed: String,
}

impl Reader {
    fn line(
        &mut self,
        line: &str,
        on_delta: &mut impl FnMut(&str),
    ) -> Option<Result<String, AiError>> {
        let event: Value = serde_json::from_str(line).ok()?;
        match event["type"].as_str()? {
            "stream_event" => {
                let delta = &event["event"]["delta"];
                if event["event"]["type"] == "content_block_delta"
                    && delta["type"] == "text_delta"
                    && let Some(text) = delta["text"].as_str()
                    && !text.is_empty()
                {
                    self.streamed.push_str(text);
                    on_delta(text);
                }
                None
            }
            "result" => Some(self.finish(&event, on_delta)),
            _ => None,
        }
    }

    fn finish(&self, event: &Value, on_delta: &mut impl FnMut(&str)) -> Result<String, AiError> {
        let result = event["result"]
            .as_str()
            .unwrap_or_default()
            .trim()
            .to_owned();
        if event["is_error"].as_bool().unwrap_or(false) {
            let lower = result.to_lowercase();
            if lower.contains("limit") {
                return Err(AiError::PlanLimit);
            }
            if lower.contains("log in") || lower.contains("login") || lower.contains("auth") {
                return Err(AiError::Auth { status: 401 });
            }
            return Err(error(format!(
                "{}: {result}",
                event["subtype"].as_str().unwrap_or("error")
            )));
        }
        let answer = if result.is_empty() {
            self.streamed.trim().to_owned()
        } else {
            result
        };
        if answer.is_empty() {
            return Err(AiError::InvalidResponse(
                "Claude Code returned an empty answer".into(),
            ));
        }
        if self.streamed.is_empty() {
            on_delta(&answer);
        }
        Ok(answer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_marker_url_uses_claude_code() {
        assert!(is_endpoint("account://claude-code"));
        assert!(!is_endpoint("https://api.anthropic.com/v1"));
    }

    #[test]
    fn requests_have_no_tools_settings_or_saved_session() {
        let args: Vec<String> = args("sonnet", Path::new("sys.md"))
            .into_iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        let after = |flag: &str| {
            args.iter()
                .position(|a| a == flag)
                .map(|i| args[i + 1].as_str())
        };
        assert_eq!(after("--tools"), Some(""));
        assert_eq!(after("--setting-sources"), Some(""));
        assert_eq!(after("--max-turns"), Some("1"));
        assert_eq!(after("--model"), Some("sonnet"));
        assert_eq!(after("--system-prompt-file"), Some("sys.md"));
        for flag in ["--strict-mcp-config", "--no-session-persistence", "-p"] {
            assert!(args.iter().any(|a| a == flag), "{flag}");
        }
    }

    #[test]
    fn the_api_key_never_reaches_claude_code() {
        let command = command(Path::new("claude"));
        let removed: Vec<_> = command
            .get_envs()
            .filter(|(_, value)| value.is_none())
            .map(|(key, _)| key.to_string_lossy().into_owned())
            .collect();
        for name in OVERRIDES {
            assert!(removed.iter().any(|k| k == name), "{name}");
        }
    }

    #[test]
    fn system_text_and_turns_become_one_request() {
        let messages = json!([
            {"role": "system", "content": "Teach."},
            {"role": "user", "content": "Explain."},
            {"role": "assistant", "content": "Here."},
            {"role": "user", "content": [
                {"type": "text", "text": "Read this page."},
                {"type": "image_url", "image_url": {"url": "data:image/png;base64,AAAA"}}
            ]}
        ]);
        let (system, line) = request(&messages).unwrap();
        assert_eq!(system, "Teach.");
        let content = &line["message"]["content"];
        assert_eq!(
            content[0]["text"],
            "User:\nExplain.\n\nAssistant:\nHere.\n\nUser:\nRead this page."
        );
        assert_eq!(content[1]["source"]["media_type"], "image/png");
        assert_eq!(content[1]["source"]["data"], "AAAA");
    }

    #[test]
    fn a_single_question_is_sent_without_labels() {
        let messages = json!([
            {"role": "system", "content": "Teach."},
            {"role": "user", "content": "Explain."}
        ]);
        let (_, line) = request(&messages).unwrap();
        assert_eq!(line["message"]["content"][0]["text"], "Explain.");
    }

    #[test]
    fn deltas_stream_and_the_result_ends_the_answer() {
        let mut reader = Reader::default();
        let mut seen = String::new();
        let lines = [
            r#"{"type":"system","subtype":"init"}"#,
            r#"{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"Bra"}}}"#,
            r#"{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"sília"}}}"#,
            r#"{"type":"result","subtype":"success","is_error":false,"result":"Brasília"}"#,
        ];
        let mut done = None;
        for line in lines {
            if let Some(result) = reader.line(line, &mut |d: &str| seen.push_str(d)) {
                done = Some(result);
            }
        }
        assert_eq!(seen, "Brasília");
        assert_eq!(done.unwrap().unwrap(), "Brasília");
    }

    #[test]
    fn a_reached_limit_is_reported_as_such() {
        let mut reader = Reader::default();
        let line = r#"{"type":"result","subtype":"success","is_error":true,"result":"Claude usage limit reached. Your limit will reset at 5pm."}"#;
        let result = reader.line(line, &mut |_: &str| {}).unwrap();
        assert!(matches!(result, Err(AiError::PlanLimit)));
    }

    #[test]
    fn status_says_whether_someone_signed_in() {
        assert_eq!(
            parse_status(r#"{"loggedIn": true, "authMethod": "claude.ai"}"#),
            AccountState::Ready
        );
        assert_eq!(
            parse_status(r#"{"loggedIn": false, "authMethod": "none"}"#),
            AccountState::SignedOut
        );
        assert_eq!(parse_status("not json"), AccountState::SignedOut);
    }
}
