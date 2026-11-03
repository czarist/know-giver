//! GitHub Copilot SDK transport and its first-party CLI sign-in flow.

use std::ffi::OsStr;
use std::io::{self, BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock, mpsc};
use std::time::Duration;

use github_copilot_sdk::mode::ClientMode;
use github_copilot_sdk::session::Session;
use github_copilot_sdk::types::{MessageOptions, SessionConfig, SystemMessageConfig};
use github_copilot_sdk::{Client as CopilotSdkClient, ClientOptions};

use crate::{AiError, ChatMessage, Role};

const COPILOT_BASE_URL: &str = "https://api.githubcopilot.com";

pub(crate) fn is_endpoint(base_url: &str) -> bool {
    crate::providers::host_of(base_url) == crate::providers::host_of(COPILOT_BASE_URL)
}

/// Starts the bundled Copilot CLI browser sign-in and streams its instructions.
pub fn login(mut on_output: impl FnMut(&str)) -> Result<(), AiError> {
    let cli = github_copilot_sdk::install_bundled_cli()
        .ok_or_else(|| AiError::Copilot("bundled Copilot CLI is unavailable".into()))?;
    let mut child = web_login_command(cli)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| AiError::Copilot(format!("could not start Copilot sign-in: {err}")))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| AiError::Copilot("Copilot sign-in output is unavailable".into()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| AiError::Copilot("Copilot sign-in errors are unavailable".into()))?;
    let (output_tx, output_rx) = mpsc::channel();
    let stdout_tx = output_tx.clone();
    let stdout_reader = std::thread::Builder::new()
        .name("kg-copilot-login-out".into())
        .spawn(move || forward_output(stdout, stdout_tx))
        .map_err(|err| {
            let _ = child.kill();
            AiError::Copilot(format!("could not read Copilot sign-in output: {err}"))
        })?;
    let stderr_reader = match std::thread::Builder::new()
        .name("kg-copilot-login-err".into())
        .spawn(move || forward_output(stderr, output_tx))
    {
        Ok(reader) => reader,
        Err(err) => {
            let _ = child.kill();
            let _ = child.wait();
            let _ = stdout_reader.join();
            return Err(AiError::Copilot(format!(
                "could not read Copilot sign-in errors: {err}"
            )));
        }
    };
    for line in output_rx {
        on_output(&line);
    }
    let status = child
        .wait()
        .map_err(|err| AiError::Copilot(format!("could not finish Copilot sign-in: {err}")))?;
    for reader in [stdout_reader, stderr_reader] {
        match reader.join() {
            Ok(Ok(())) => {}
            Ok(Err(err)) => {
                return Err(AiError::Copilot(format!(
                    "could not read Copilot sign-in output: {err}"
                )));
            }
            Err(_) => {
                return Err(AiError::Copilot(
                    "Copilot sign-in output reader stopped unexpectedly".into(),
                ));
            }
        }
    }
    if status.success() {
        Ok(())
    } else {
        Err(AiError::Copilot(format!(
            "Copilot sign-in exited with status {}",
            status
                .code()
                .map_or_else(|| "unknown".to_owned(), |code| code.to_string())
        )))
    }
}

fn web_login_command(cli: impl AsRef<OsStr>) -> Command {
    let mut command = Command::new(cli);
    command.args(["login", "--web-flow"]);
    command
}

fn forward_output(reader: impl io::Read, sender: mpsc::Sender<String>) -> io::Result<()> {
    for line in BufReader::new(reader).lines() {
        if sender.send(line?).is_err() {
            break;
        }
    }
    Ok(())
}

#[derive(Default)]
pub(crate) struct CopilotBackend {
    client: Mutex<Option<CopilotSdkClient>>,
    runtime: OnceLock<Result<tokio::runtime::Runtime, String>>,
}

impl std::fmt::Debug for CopilotBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CopilotBackend")
            .field(
                "started",
                &self.client.lock().is_ok_and(|client| client.is_some()),
            )
            .finish()
    }
}

impl CopilotBackend {
    fn runtime(&self) -> Result<&tokio::runtime::Runtime, AiError> {
        self.runtime
            .get_or_init(|| {
                tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(2)
                    .thread_name("kg-copilot")
                    .enable_all()
                    .build()
                    .map_err(|err| format!("could not start Copilot runtime: {err}"))
            })
            .as_ref()
            .map_err(|message| AiError::Copilot(message.clone()))
    }

    fn client(&self) -> Result<CopilotSdkClient, AiError> {
        let runtime = self.runtime()?;
        let mut client = self
            .client
            .lock()
            .map_err(|_| AiError::Copilot("Copilot client state is unavailable".into()))?;
        if client.is_none() {
            let options = ClientOptions::new()
                .with_mode(ClientMode::Empty)
                .with_env([("COPILOT_TELEMETRY_ENABLED", "false")]);
            *client = Some(
                runtime
                    .block_on(CopilotSdkClient::start(options))
                    .map_err(|err| AiError::Copilot(err.to_string()))?,
            );
        }
        client
            .as_ref()
            .cloned()
            .ok_or_else(|| AiError::Copilot("Copilot client did not start".into()))
    }

    pub(crate) fn list_models(&self) -> Result<Vec<String>, AiError> {
        let client = self.client()?;
        self.runtime()?.block_on(async move {
            client
                .list_models()
                .await
                .map(|models| models.into_iter().map(|model| model.id).collect())
                .map_err(|err| AiError::Copilot(err.to_string()))
        })
    }

    pub(crate) fn chat(
        &self,
        model: &str,
        messages: &[ChatMessage],
        mut on_delta: impl FnMut(&str),
    ) -> Result<String, AiError> {
        let client = self.client()?;
        let (system, prompt) = prompt_parts(messages)?;
        self.runtime()?.block_on(async move {
            let session = client
                .create_session(
                    SessionConfig::default()
                        .with_model(model)
                        .with_system_message(
                            SystemMessageConfig::new()
                                .with_mode("replace")
                                .with_content(system),
                        )
                        .with_available_tools(std::iter::empty::<String>())
                        .with_streaming(true)
                        .with_enable_session_telemetry(false),
                )
                .await
                .map_err(|err| AiError::Copilot(err.to_string()))?;
            let result = stream_reply(&session, prompt, &mut on_delta).await;
            let disconnected = session.disconnect().await;
            match (result, disconnected) {
                (Ok(response), Ok(())) => Ok(response),
                (Err(err), _) => Err(err),
                (Ok(_), Err(err)) => Err(AiError::Copilot(err.to_string())),
            }
        })
    }
}

async fn stream_reply(
    session: &Session,
    prompt: String,
    on_delta: &mut impl FnMut(&str),
) -> Result<String, AiError> {
    let mut events = session.subscribe();
    let mut events_open = true;
    let mut streamed = String::new();
    let send = session
        .send_and_wait(MessageOptions::new(prompt).with_wait_timeout(Duration::from_secs(600)));
    tokio::pin!(send);
    let response = loop {
        tokio::select! {
            result = &mut send => {
                break result.map_err(|err| AiError::Copilot(err.to_string()))?;
            }
            event = events.recv(), if events_open => {
                match event {
                    Ok(event) if event.event_type == "assistant.message_delta" => {
                        if let Some(delta) = event.data.get("deltaContent").and_then(serde_json::Value::as_str)
                            && !delta.is_empty()
                        {
                            streamed.push_str(delta);
                            on_delta(delta);
                        }
                    }
                    Ok(_) => {}
                    Err(_) => events_open = false,
                }
            }
        }
    };
    let response = response
        .and_then(|event| {
            event
                .data
                .get("content")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_default()
        .trim()
        .to_owned();
    let answer = if response.is_empty() {
        streamed.trim().to_owned()
    } else {
        response
    };
    if answer.is_empty() {
        return Err(AiError::InvalidResponse(
            "Copilot returned an empty answer".into(),
        ));
    }
    if streamed.is_empty() {
        on_delta(&answer);
    }
    Ok(answer)
}

fn prompt_parts(messages: &[ChatMessage]) -> Result<(String, String), AiError> {
    let mut system = String::new();
    let mut prompt = String::new();
    for message in messages {
        match message.role {
            Role::System => {
                if !system.is_empty() {
                    system.push_str("\n\n");
                }
                system.push_str(&message.content);
            }
            Role::User | Role::Assistant => {
                if !prompt.is_empty() {
                    prompt.push_str("\n\n");
                }
                let role = match message.role {
                    Role::User => "User",
                    Role::Assistant => "Assistant",
                    Role::System => continue,
                };
                prompt.push_str(role);
                prompt.push_str(":\n");
                prompt.push_str(&message.content);
            }
        }
    }
    if prompt.is_empty() {
        return Err(AiError::InvalidResponse(
            "Copilot request has no user or assistant messages".into(),
        ));
    }
    if system.is_empty() {
        return Err(AiError::InvalidResponse(
            "Copilot request has no system instructions".into(),
        ));
    }
    Ok((system, prompt))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_copilot_api_host_uses_the_copilot_backend() {
        assert!(is_endpoint("https://api.githubcopilot.com"));
        assert!(is_endpoint(
            "https://api.githubcopilot.com/chat/completions"
        ));
        assert!(!is_endpoint("https://githubcopilot.com"));
        assert!(!is_endpoint("https://api.githubcopilot.com.example.org/v1"));
        assert!(!is_endpoint("https://api.openai.com/v1"));
    }

    #[test]
    fn login_invokes_the_cli_web_oauth_flow() {
        let command = web_login_command("copilot");

        assert_eq!(command.get_program(), OsStr::new("copilot"));
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            [OsStr::new("login"), OsStr::new("--web-flow")]
        );
    }

    #[test]
    fn login_forwards_complete_lines_from_cli_output() {
        let (sender, receiver) = mpsc::channel();

        forward_output(
            io::Cursor::new("Open https://github.com/login/device\nEnter code: ABCD\n"),
            sender,
        )
        .unwrap();

        assert_eq!(
            receiver.into_iter().collect::<Vec<_>>(),
            vec![
                "Open https://github.com/login/device".to_owned(),
                "Enter code: ABCD".to_owned()
            ]
        );
    }

    #[test]
    fn prompt_parts_preserve_system_and_conversation_order() {
        let messages = [
            ChatMessage {
                role: Role::System,
                content: "Teach clearly.".into(),
            },
            ChatMessage {
                role: Role::User,
                content: "Explain the topic.".into(),
            },
            ChatMessage {
                role: Role::Assistant,
                content: "Here is the explanation.".into(),
            },
            ChatMessage {
                role: Role::User,
                content: "Give me an example.".into(),
            },
        ];

        let (system, prompt) = prompt_parts(&messages).unwrap();

        assert_eq!(system, "Teach clearly.");
        assert_eq!(
            prompt,
            "User:\nExplain the topic.\n\nAssistant:\nHere is the explanation.\n\nUser:\nGive me an example."
        );
    }

    #[test]
    fn prompt_parts_reject_requests_without_system_or_conversation_messages() {
        assert!(matches!(
            prompt_parts(&[ChatMessage::user("Question")]),
            Err(AiError::InvalidResponse(_))
        ));
        assert!(matches!(
            prompt_parts(&[ChatMessage::system("Instructions")]),
            Err(AiError::InvalidResponse(_))
        ));
    }
}
