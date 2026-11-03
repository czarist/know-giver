//! Services the student signs in to with an account they already have,
//! instead of pasting an API key: GitHub Copilot, ChatGPT and Claude (through
//! the official Claude Code program).
//!
//! Each one keeps its credentials where its owner says they belong: the
//! Copilot CLI and Claude Code store their own sign-in; ChatGPT's tokens come
//! from OpenAI's "Sign in with ChatGPT" flow and are saved by the engine
//! (see [`crate::chatgpt`]).

use std::io::{self, BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, mpsc};

use serde::{Deserialize, Serialize};

use crate::{AiError, ChatMessage};

/// A service reached through the student's own account.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AccountKind {
    ChatGpt,
    ClaudeCode,
    Copilot,
}

impl AccountKind {
    pub const ALL: [Self; 3] = [Self::ChatGpt, Self::ClaudeCode, Self::Copilot];

    /// The matching entry in [`crate::PROVIDERS`].
    pub fn provider_id(self) -> &'static str {
        match self {
            Self::ChatGpt => "chatgpt",
            Self::ClaudeCode => "claude-code",
            Self::Copilot => "copilot",
        }
    }

    pub fn from_provider_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.provider_id() == id)
    }

    /// The account service a saved base URL points at.
    pub(crate) fn for_endpoint(base_url: &str) -> Option<Self> {
        if crate::copilot::is_endpoint(base_url) {
            Some(Self::Copilot)
        } else if crate::chatgpt::is_endpoint(base_url) {
            Some(Self::ChatGpt)
        } else if crate::claude_code::is_endpoint(base_url) {
            Some(Self::ClaudeCode)
        } else {
            None
        }
    }

    /// Whether its models can read images in this app.
    pub fn sees_images(self) -> bool {
        !matches!(self, Self::Copilot)
    }
}

/// What a sign-in reports while it runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginEvent<'a> {
    /// A line the sign-in program printed (instructions, a code to type).
    Output(&'a str),
    /// A page the student must open in the browser.
    OpenUrl(&'a str),
}

/// Pages shown in the browser at the end of a sign-in the app hosts
/// (ChatGPT). Translated by the UI.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LoginPages {
    pub success: String,
    pub failure: String,
}

/// Whether a service is ready on this computer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountState {
    /// Signed in and ready to answer.
    Ready,
    /// The program is there, but nobody signed in.
    SignedOut,
    /// The program the service needs is not installed (Claude Code).
    Missing,
}

/// Signs in to `kind` through its official flow.
///
/// # Errors
///
/// Returns an error when the flow cannot start, is cancelled or is refused.
pub fn login(
    kind: AccountKind,
    pages: &LoginPages,
    mut on_event: impl FnMut(LoginEvent<'_>),
) -> Result<(), AiError> {
    match kind {
        AccountKind::Copilot => crate::copilot::login(|line| on_event(LoginEvent::Output(line))),
        AccountKind::ChatGpt => crate::chatgpt::login(pages, on_event),
        AccountKind::ClaudeCode => crate::claude_code::login(on_event),
    }
}

/// Checks whether `kind` is installed and signed in. Copilot is reported
/// ready: its CLI comes with the app and says so itself on the first request.
pub fn state(kind: AccountKind) -> AccountState {
    match kind {
        AccountKind::Copilot => AccountState::Ready,
        AccountKind::ChatGpt => crate::chatgpt::state(),
        AccountKind::ClaudeCode => crate::claude_code::state(),
    }
}

/// Installs the program `kind` needs, with its official installer, streaming
/// what the installer prints. Only Claude Code needs one.
///
/// # Errors
///
/// Returns an error when the installer cannot run or fails.
pub fn install(kind: AccountKind, on_output: impl FnMut(&str)) -> Result<(), AiError> {
    match kind {
        AccountKind::ClaudeCode => crate::claude_code::install(on_output),
        AccountKind::ChatGpt | AccountKind::Copilot => Ok(()),
    }
}

/// Signs out where the app owns the sign-in (ChatGPT). Copilot and Claude
/// Code keep theirs: signing out there would sign the student out of those
/// programs everywhere.
pub fn sign_out(kind: AccountKind) {
    if kind == AccountKind::ChatGpt {
        crate::chatgpt::sign_out();
    }
}

/// The backend behind a client whose settings point at an account service.
#[derive(Debug, Clone)]
pub(crate) enum Backend {
    Copilot(Arc<crate::copilot::CopilotBackend>),
    ChatGpt,
    ClaudeCode,
}

impl Backend {
    pub(crate) fn for_endpoint(base_url: &str) -> Option<Self> {
        AccountKind::for_endpoint(base_url).map(|kind| match kind {
            AccountKind::Copilot => Self::Copilot(Arc::default()),
            AccountKind::ChatGpt => Self::ChatGpt,
            AccountKind::ClaudeCode => Self::ClaudeCode,
        })
    }

    pub(crate) fn list_models(&self) -> Result<Vec<String>, AiError> {
        match self {
            Self::Copilot(copilot) => copilot.list_models(),
            Self::ChatGpt => crate::chatgpt::list_models(),
            Self::ClaudeCode => Ok(crate::claude_code::MODELS
                .iter()
                .map(|m| (*m).to_owned())
                .collect()),
        }
    }

    /// Sends chat messages in the OpenAI shape (text, or text and
    /// `image_url` parts) and streams the answer.
    pub(crate) fn chat(
        &self,
        model: &str,
        messages: &serde_json::Value,
        on_delta: impl FnMut(&str),
    ) -> Result<String, AiError> {
        match self {
            Self::Copilot(copilot) => {
                let messages: Vec<ChatMessage> =
                    serde_json::from_value(messages.clone()).map_err(|_| AiError::NoVisionModel)?;
                copilot.chat(model, &messages, on_delta)
            }
            Self::ChatGpt => crate::chatgpt::chat(model, messages, on_delta),
            Self::ClaudeCode => crate::claude_code::chat(model, messages, on_delta),
        }
    }
}

/// Runs a sign-in or install program, passing each line it prints (stdout
/// and stderr) to `on_line`, and fails when it exits with an error.
pub(crate) fn run_streaming(
    mut command: Command,
    what: &str,
    error: impl Fn(String) -> AiError,
    mut on_line: impl FnMut(&str),
) -> Result<(), AiError> {
    hide_window(&mut command);
    let mut child: Child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| error(format!("could not start {what}: {err}")))?;
    let (Some(stdout), Some(stderr)) = (child.stdout.take(), child.stderr.take()) else {
        let _ = child.kill();
        let _ = child.wait();
        return Err(error(format!("{what} output is unavailable")));
    };
    let (tx, rx) = mpsc::channel();
    let tx_err = tx.clone();
    let readers = [
        std::thread::Builder::new()
            .name("kg-account-out".into())
            .spawn(move || forward_lines(stdout, &tx)),
        std::thread::Builder::new()
            .name("kg-account-err".into())
            .spawn(move || forward_lines(stderr, &tx_err)),
    ];
    if readers.iter().any(Result::is_err) {
        let _ = child.kill();
        let _ = child.wait();
        return Err(error(format!("could not read {what} output")));
    }
    for line in rx {
        on_line(&line);
    }
    let status = child
        .wait()
        .map_err(|err| error(format!("could not finish {what}: {err}")))?;
    for reader in readers.into_iter().flatten() {
        let _ = reader.join();
    }
    if status.success() {
        Ok(())
    } else {
        Err(error(format!(
            "{what} exited with status {}",
            status
                .code()
                .map_or_else(|| "unknown".to_owned(), |code| code.to_string())
        )))
    }
}

pub(crate) fn forward_lines(reader: impl io::Read, sender: &mpsc::Sender<String>) {
    for line in BufReader::new(reader).lines() {
        let Ok(line) = line else { break };
        if sender.send(line).is_err() {
            break;
        }
    }
}

/// No console window flashes on Windows for the helper programs.
pub(crate) fn hide_window(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    #[cfg(not(windows))]
    let _ = command;
}

/// A data URL's media type and base64 payload.
pub(crate) fn data_url(url: &str) -> Option<(&str, &str)> {
    let rest = url.strip_prefix("data:")?;
    let (meta, data) = rest.split_once(',')?;
    let media = meta.strip_suffix(";base64")?;
    Some((media, data))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_ids_round_trip() {
        for kind in AccountKind::ALL {
            assert_eq!(
                AccountKind::from_provider_id(kind.provider_id()),
                Some(kind)
            );
            assert!(crate::provider(kind.provider_id()).is_some(), "{kind:?}");
        }
        assert_eq!(AccountKind::from_provider_id("openai"), None);
    }

    #[test]
    fn account_providers_have_their_own_endpoints() {
        for kind in AccountKind::ALL {
            let provider = crate::provider(kind.provider_id()).unwrap();
            assert_eq!(AccountKind::for_endpoint(provider.base_url), Some(kind));
        }
        // The OpenAI and Anthropic API keys keep their usual clients.
        assert_eq!(AccountKind::for_endpoint("https://api.openai.com/v1"), None);
        assert_eq!(
            AccountKind::for_endpoint("https://api.anthropic.com/v1"),
            None
        );
    }

    #[test]
    fn data_urls_split_into_type_and_payload() {
        assert_eq!(
            data_url("data:image/png;base64,AAAA"),
            Some(("image/png", "AAAA"))
        );
        assert_eq!(data_url("https://example.org/a.png"), None);
    }
}
