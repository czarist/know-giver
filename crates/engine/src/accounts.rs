//! Signing in with an account the student already has (ChatGPT, Claude
//! through Claude Code, GitHub Copilot), installing Claude Code when it is
//! missing, and keeping the ChatGPT session saved.

use std::collections::HashMap;

use kg_ai::{AccountKind, AccountState, AiError, ChatGptSession, LoginEvent, LoginPages};

use crate::{Engine, Event, Notice};

/// Settings key of the saved ChatGPT session (tokens and registration).
const CHATGPT_SESSION_KEY: &str = "chatgpt_session";

/// What is running for an account service.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountAction {
    SignIn,
    Install,
}

/// Account sign-ins, installs and states, kept by the engine.
#[derive(Debug, Default)]
pub(crate) struct Accounts {
    /// The one sign-in or install running, if any.
    running: Option<(AccountKind, AccountAction)>,
    /// Lines the sign-in or installer printed, shown while it runs.
    output: String,
    /// Whether the last sign-in of each service worked.
    results: HashMap<AccountKind, bool>,
    /// The last checked state of each service.
    states: HashMap<AccountKind, AccountState>,
    checking: bool,
    /// A page the UI should open in the browser.
    open_url: Option<String>,
}

/// Events from account workers.
#[derive(Debug)]
pub(crate) enum AccountEvent {
    Output(String),
    OpenUrl(String),
    Finished {
        kind: AccountKind,
        action: AccountAction,
        result: Result<(), AiError>,
    },
    States(Vec<(AccountKind, AccountState)>),
}

impl Engine {
    /// Gives the AI layer the ChatGPT session saved earlier.
    pub(crate) fn restore_chatgpt_session(&mut self) {
        match self.store.setting(CHATGPT_SESSION_KEY) {
            Ok(Some(json)) => match serde_json::from_str::<ChatGptSession>(&json) {
                Ok(session) => kg_ai::chatgpt::restore_session(session),
                Err(err) => tracing::warn!(%err, "ignoring an unreadable ChatGPT session"),
            },
            Ok(None) => {}
            Err(err) => tracing::warn!(%err, "could not read the ChatGPT session"),
        }
    }

    /// Saves the ChatGPT session after a sign-in, a token refresh or a
    /// sign-out. Called from [`Engine::poll`].
    pub(crate) fn save_chatgpt_session(&mut self) {
        let Some(update) = kg_ai::chatgpt::take_session_update() else {
            return;
        };
        let saved = match update {
            Some(session) => serde_json::to_string(&session)
                .map_err(|err| err.to_string())
                .and_then(|json| {
                    self.store
                        .set_setting(CHATGPT_SESSION_KEY, &json)
                        .map_err(|err| err.to_string())
                }),
            None => self
                .store
                .set_setting(CHATGPT_SESSION_KEY, "")
                .map_err(|err| err.to_string()),
        };
        if let Err(err) = saved {
            tracing::warn!(%err, "could not save the ChatGPT session");
        }
    }

    /// The service whose sign-in or install is running, and which of the two.
    pub fn account_running(&self) -> Option<(AccountKind, AccountAction)> {
        self.accounts.running
    }

    /// What the running sign-in or installer printed.
    pub fn account_output(&self) -> &str {
        &self.accounts.output
    }

    /// Whether the last sign-in to `kind` worked, if one finished.
    pub fn account_result(&self, kind: AccountKind) -> Option<bool> {
        self.accounts.results.get(&kind).copied()
    }

    /// The last checked state of `kind`; `None` until a check finishes.
    pub fn account_state(&self, kind: AccountKind) -> Option<AccountState> {
        self.accounts.states.get(&kind).copied()
    }

    pub fn is_checking_accounts(&self) -> bool {
        self.accounts.checking
    }

    /// A page to open in the browser, once.
    pub fn take_account_url(&mut self) -> Option<String> {
        self.accounts.open_url.take()
    }

    /// The ChatGPT account in use, for the UI.
    pub fn chatgpt_email(&self) -> Option<String> {
        kg_ai::chatgpt::signed_in_email()
    }

    pub(crate) fn check_accounts(&mut self) {
        if self.accounts.checking {
            return;
        }
        self.accounts.checking = true;
        let started = self.spawn(|| {
            let states = AccountKind::ALL
                .into_iter()
                .map(|kind| (kind, kg_ai::account_state(kind)))
                .collect();
            Event::Account(AccountEvent::States(states))
        });
        if !started {
            self.accounts.checking = false;
        }
    }

    pub(crate) fn sign_in_account(&mut self, kind: AccountKind, pages: LoginPages) {
        self.run_account(kind, AccountAction::SignIn, move |sender, waker| {
            kg_ai::login_account(kind, &pages, |event| {
                let event = match event {
                    LoginEvent::Output(line) => AccountEvent::Output(line.to_owned()),
                    LoginEvent::OpenUrl(url) => AccountEvent::OpenUrl(url.to_owned()),
                };
                if sender.send(Event::Account(event)).is_ok() {
                    waker();
                }
            })
        });
    }

    pub(crate) fn install_account(&mut self, kind: AccountKind) {
        self.run_account(kind, AccountAction::Install, move |sender, waker| {
            kg_ai::install_account(kind, |line| {
                if sender
                    .send(Event::Account(AccountEvent::Output(line.to_owned())))
                    .is_ok()
                {
                    waker();
                }
            })
        });
    }

    pub(crate) fn sign_out_account(&mut self, kind: AccountKind) {
        self.accounts.results.remove(&kind);
        let started = self.spawn(move || {
            kg_ai::sign_out(kind);
            Event::Account(AccountEvent::States(vec![(
                kind,
                kg_ai::account_state(kind),
            )]))
        });
        if !started {
            tracing::warn!("could not start the sign-out");
        }
    }

    fn run_account(
        &mut self,
        kind: AccountKind,
        action: AccountAction,
        work: impl FnOnce(
            &std::sync::mpsc::Sender<Event>,
            &(dyn Fn() + Send + Sync),
        ) -> Result<(), AiError>
        + Send
        + 'static,
    ) {
        if self.accounts.running.is_some() {
            return;
        }
        self.accounts.running = Some((kind, action));
        self.accounts.output.clear();
        if action == AccountAction::SignIn {
            self.accounts.results.remove(&kind);
        }
        let (sender, waker) = self.sender();
        let started = self.spawn(move || {
            let result = work(&sender, &*waker);
            Event::Account(AccountEvent::Finished {
                kind,
                action,
                result,
            })
        });
        if !started {
            self.on_account_finished(
                kind,
                action,
                Err(AiError::InvalidResponse(
                    "could not start the worker".into(),
                )),
            );
        }
    }

    pub(crate) fn on_account_event(&mut self, event: AccountEvent) {
        match event {
            AccountEvent::Output(line) => {
                if self.accounts.running.is_some() {
                    self.accounts.output.push_str(&line);
                    self.accounts.output.push('\n');
                }
            }
            AccountEvent::OpenUrl(url) => {
                if self.accounts.running.is_some() {
                    self.accounts.open_url = Some(url);
                }
            }
            AccountEvent::Finished {
                kind,
                action,
                result,
            } => self.on_account_finished(kind, action, result),
            AccountEvent::States(states) => {
                self.accounts.checking = false;
                self.accounts.states.extend(states);
            }
        }
    }

    fn on_account_finished(
        &mut self,
        kind: AccountKind,
        action: AccountAction,
        result: Result<(), AiError>,
    ) {
        self.accounts.running = None;
        self.accounts.output.clear();
        self.accounts.open_url = None;
        if action == AccountAction::SignIn {
            self.accounts.results.insert(kind, result.is_ok());
        }
        match result {
            Ok(()) => {
                self.notices.push(match action {
                    AccountAction::SignIn => Notice::AccountConnected(kind),
                    AccountAction::Install => Notice::AccountInstalled(kind),
                });
                // What changed is worth checking again (signed in, installed).
                self.check_accounts();
            }
            Err(AiError::SignInCancelled) => {}
            Err(error) => self.notices.push(Notice::Error(error.into())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine() -> (Engine, tempfile::TempDir) {
        let directory = tempfile::tempdir().unwrap();
        let engine = Engine::in_memory(
            crate::AppPaths::new(directory.path()),
            crate::tests::silent_ai(),
        )
        .unwrap();
        (engine, directory)
    }

    fn send(engine: &Engine, event: AccountEvent) {
        engine.events_tx.send(Event::Account(event)).unwrap();
    }

    #[test]
    fn a_sign_in_streams_its_instructions_and_clears_them_on_success() {
        let (mut engine, _directory) = engine();
        engine.accounts.running = Some((AccountKind::Copilot, AccountAction::SignIn));
        send(
            &engine,
            AccountEvent::Output("Open https://github.com/login/device".into()),
        );
        send(&engine, AccountEvent::Output("Enter code: ABCD".into()));
        send(
            &engine,
            AccountEvent::OpenUrl("https://example.org/login".into()),
        );
        engine.poll();

        assert_eq!(
            engine.account_output(),
            "Open https://github.com/login/device\nEnter code: ABCD\n"
        );
        assert_eq!(
            engine.take_account_url().as_deref(),
            Some("https://example.org/login")
        );
        assert_eq!(engine.take_account_url(), None);

        send(
            &engine,
            AccountEvent::Finished {
                kind: AccountKind::Copilot,
                action: AccountAction::SignIn,
                result: Ok(()),
            },
        );
        engine.poll();

        assert_eq!(engine.account_running(), None);
        assert_eq!(engine.account_result(AccountKind::Copilot), Some(true));
        assert!(engine.account_output().is_empty());
        assert!(
            engine
                .take_notices()
                .iter()
                .any(|n| matches!(n, Notice::AccountConnected(AccountKind::Copilot)))
        );

        // Output arriving after the end is not shown.
        send(&engine, AccountEvent::Output("late output".into()));
        engine.poll();
        assert!(engine.account_output().is_empty());
    }

    #[test]
    fn a_failed_sign_in_reports_the_failure() {
        let (mut engine, _directory) = engine();
        engine.accounts.running = Some((AccountKind::ChatGpt, AccountAction::SignIn));
        send(
            &engine,
            AccountEvent::Finished {
                kind: AccountKind::ChatGpt,
                action: AccountAction::SignIn,
                result: Err(AiError::PlanNotIncluded),
            },
        );
        engine.poll();

        assert_eq!(engine.account_result(AccountKind::ChatGpt), Some(false));
        assert!(matches!(
            engine.take_notices().as_slice(),
            [Notice::Error(_)]
        ));
    }

    #[test]
    fn a_cancelled_sign_in_is_not_an_error() {
        let (mut engine, _directory) = engine();
        engine.accounts.running = Some((AccountKind::ChatGpt, AccountAction::SignIn));
        send(
            &engine,
            AccountEvent::Finished {
                kind: AccountKind::ChatGpt,
                action: AccountAction::SignIn,
                result: Err(AiError::SignInCancelled),
            },
        );
        engine.poll();
        assert!(engine.take_notices().is_empty());
        assert_eq!(engine.account_running(), None);
    }

    #[test]
    fn an_install_does_not_count_as_a_sign_in() {
        let (mut engine, _directory) = engine();
        engine.accounts.running = Some((AccountKind::ClaudeCode, AccountAction::Install));
        send(
            &engine,
            AccountEvent::Finished {
                kind: AccountKind::ClaudeCode,
                action: AccountAction::Install,
                result: Ok(()),
            },
        );
        engine.poll();
        assert_eq!(engine.account_result(AccountKind::ClaudeCode), None);
        assert!(
            engine
                .take_notices()
                .iter()
                .any(|n| matches!(n, Notice::AccountInstalled(AccountKind::ClaudeCode)))
        );
    }

    #[test]
    fn checked_states_are_kept_per_service() {
        let (mut engine, _directory) = engine();
        engine.accounts.checking = true;
        send(
            &engine,
            AccountEvent::States(vec![
                (AccountKind::ClaudeCode, AccountState::Missing),
                (AccountKind::ChatGpt, AccountState::SignedOut),
            ]),
        );
        engine.poll();
        assert!(!engine.is_checking_accounts());
        assert_eq!(
            engine.account_state(AccountKind::ClaudeCode),
            Some(AccountState::Missing)
        );
        assert_eq!(engine.account_state(AccountKind::Copilot), None);
    }
}
