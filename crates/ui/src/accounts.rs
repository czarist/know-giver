//! Signing in with an account the student already has: ChatGPT, Claude
//! (through Claude Code) and GitHub Copilot. The cards open the AI step for
//! people who pay for a chat plan but have never seen an API key; the panel
//! shows where a sign-in stands and what to do next, in setup and Settings.

use egui::RichText;
use kg_engine::{AccountAction, AccountKind, AccountState, Command, Engine, LoginPages};
use rust_i18n::t;

use crate::screens::Actions;
use crate::{theme, widgets};

/// Where the last sign-in page is kept, to open it again.
fn url_id() -> egui::Id {
    egui::Id::new("account_sign_in_url")
}

/// Keeps the page a sign-in asked for, so "Open the page again" works.
pub(crate) fn remember_url(ctx: &egui::Context, url: String) {
    ctx.data_mut(|d| d.insert_temp(url_id(), url));
}

/// The pages the browser shows when a ChatGPT sign-in ends.
pub(crate) fn pages() -> LoginPages {
    LoginPages {
        success: t!("account.page_success").into_owned(),
        failure: t!("account.page_failure").into_owned(),
    }
}

/// Whether `kind` is signed in and ready on this computer.
pub(crate) fn connected(engine: &Engine, kind: AccountKind) -> bool {
    match kind {
        // The Copilot CLI only says so through a sign-in in this session.
        AccountKind::Copilot => engine.account_result(kind) == Some(true),
        AccountKind::ChatGpt | AccountKind::ClaudeCode => {
            engine.account_result(kind) == Some(true)
                || engine.account_state(kind) == Some(AccountState::Ready)
        }
    }
}

/// Asks once which services are installed and signed in.
pub(crate) fn check_once(engine: &Engine, actions: &mut Actions) {
    let unknown = AccountKind::ALL
        .into_iter()
        .any(|kind| kind != AccountKind::Copilot && engine.account_state(kind).is_none());
    // Tests never start the real programs.
    if unknown
        && !cfg!(test)
        && !engine.is_checking_accounts()
        && engine.account_running().is_none()
    {
        actions.send(Command::CheckAccounts);
    }
}

fn name(kind: AccountKind) -> String {
    match kind {
        AccountKind::ChatGpt => t!("account.chatgpt.name"),
        AccountKind::ClaudeCode => t!("account.claude.name"),
        AccountKind::Copilot => t!("account.copilot.name"),
    }
    .into_owned()
}

fn pitch(kind: AccountKind) -> String {
    match kind {
        AccountKind::ChatGpt => t!("account.chatgpt.pitch"),
        AccountKind::ClaudeCode => t!("account.claude.pitch"),
        AccountKind::Copilot => t!("account.copilot.pitch"),
    }
    .into_owned()
}

/// One card per account service, stacked; returns the one the student
/// clicked. The card holds the next step (sign in, install) right there.
pub(crate) fn cards(
    ui: &mut egui::Ui,
    engine: &Engine,
    selected: Option<&str>,
    mainland: bool,
    actions: &mut Actions,
) -> Option<&'static kg_engine::Provider> {
    let p = theme::palette(ui.ctx());
    check_once(engine, actions);
    ui.label(RichText::new(t!("account.intro_title")).strong().size(16.0));
    widgets::hint(ui, &t!("account.intro"));
    ui.add_space(8.0);
    let mut picked = None;
    for kind in AccountKind::ALL {
        let Some(provider) = kg_engine::ai_provider(kind.provider_id()) else {
            continue;
        };
        if !provider.offered(mainland) {
            continue;
        }
        let is_selected = selected == Some(provider.id);
        let response = ui
            .scope_builder(egui::UiBuilder::new().sense(egui::Sense::click()), |ui| {
                egui::Frame::new()
                    .fill(if is_selected {
                        p.accent.gamma_multiply(0.12)
                    } else {
                        p.surface
                    })
                    .stroke(egui::Stroke::new(
                        if is_selected { 2.0 } else { 1.0 },
                        if is_selected { p.accent } else { p.border },
                    ))
                    .corner_radius(6.0)
                    .inner_margin(egui::Margin::same(16))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                            ui.spacing_mut().item_spacing.y = 6.0;
                            ui.horizontal(|ui| {
                                crate::provider_logo::show(ui, provider, 26.0);
                                ui.label(
                                    RichText::new(name(kind))
                                        .family(theme::strong_family())
                                        .size(17.0),
                                );
                                if connected(engine, kind) {
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            ui.add(
                                                crate::icons::Icon::Check.image(18.0, p.success),
                                            );
                                        },
                                    );
                                }
                            });
                            ui.label(RichText::new(pitch(kind)).color(p.muted));
                            if is_selected {
                                ui.add_space(4.0);
                                panel(ui, engine, kind, actions);
                            }
                        });
                    });
            })
            .response
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        if response.clicked() && !is_selected {
            picked = Some(provider);
        }
        ui.add_space(8.0);
    }
    picked
}

/// Where the sign-in to `kind` stands and the one action that comes next.
pub(crate) fn panel(ui: &mut egui::Ui, engine: &Engine, kind: AccountKind, actions: &mut Actions) {
    let p = theme::palette(ui.ctx());
    check_once(engine, actions);
    let running = engine.account_running();
    let busy = running.is_some();
    if let Some((running_kind, action)) = running
        && running_kind == kind
    {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(match action {
                AccountAction::SignIn => t!("account.waiting"),
                AccountAction::Install => t!("account.claude.installing"),
            });
        });
        if action == AccountAction::SignIn
            && let Some(url) = ui.ctx().data(|d| d.get_temp::<String>(url_id()))
        {
            widgets::link(ui, &t!("account.open_again"), &url);
        }
        let output = engine.account_output();
        if !output.is_empty() {
            ui.add(
                egui::Label::new(RichText::new(output).monospace().small())
                    .selectable(true)
                    .wrap(),
            );
        }
        return;
    }

    let sign_in = |ui: &mut egui::Ui, label: String, actions: &mut Actions| {
        if ui
            .add_enabled_ui(!busy, |ui| widgets::primary_button(ui, &label))
            .inner
            .clicked()
        {
            actions.send(Command::SignInAccount(kind, pages()));
        }
    };
    match kind {
        AccountKind::ChatGpt => {
            if connected(engine, kind) {
                connected_line(
                    ui,
                    &match engine.chatgpt_email() {
                        Some(email) if !email.is_empty() => {
                            t!("account.chatgpt.connected_as", email = email)
                        }
                        _ => t!("account.connected"),
                    },
                );
                widgets::hint(ui, &t!("account.chatgpt.using_plan"));
                ui.horizontal(|ui| {
                    widgets::link(
                        ui,
                        &t!("account.chatgpt.manage"),
                        "https://chatgpt.com/#settings",
                    );
                    ui.label(RichText::new("·").color(p.muted));
                    if ui
                        .add_enabled(!busy, egui::Button::new(t!("account.sign_out")).small())
                        .clicked()
                    {
                        actions.send(Command::SignOutAccount(kind));
                    }
                });
            } else {
                sign_in(ui, t!("account.chatgpt.sign_in").into_owned(), actions);
                widgets::hint(ui, &t!("account.chatgpt.how"));
            }
        }
        AccountKind::ClaudeCode => match engine.account_state(kind) {
            None => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(t!("account.checking"));
                });
            }
            Some(AccountState::Missing) => {
                widgets::hint(ui, &t!("account.claude.missing"));
                if ui
                    .add_enabled_ui(!busy, |ui| {
                        widgets::primary_button(ui, &t!("account.claude.install"))
                    })
                    .inner
                    .clicked()
                {
                    actions.send(Command::InstallAccount(kind));
                }
                widgets::hint(ui, &t!("account.claude.install_how"));
                widgets::link(
                    ui,
                    &t!("account.claude.docs"),
                    "https://code.claude.com/docs/en/setup",
                );
            }
            Some(AccountState::SignedOut) if !connected(engine, kind) => {
                sign_in(ui, t!("account.claude.sign_in").into_owned(), actions);
                widgets::hint(ui, &t!("account.claude.how"));
            }
            Some(_) => {
                connected_line(ui, &t!("account.claude.connected"));
                widgets::hint(ui, &t!("account.claude.using_plan"));
            }
        },
        AccountKind::Copilot => {
            if connected(engine, kind) {
                connected_line(ui, &t!("account.connected"));
            } else {
                sign_in(ui, t!("copilot.sign_in").into_owned(), actions);
                widgets::hint(ui, &t!("copilot.instructions"));
            }
            ui.horizontal(|ui| {
                widgets::link(ui, &t!("copilot.sign_in_docs"), "https://docs.github.com/en/copilot/how-tos/copilot-cli/set-up-copilot-cli/authenticate-copilot-cli");
                ui.label(RichText::new("·").color(p.muted));
                widgets::link(ui, &t!("copilot.plans"), "https://github.com/features/copilot/plans");
            });
        }
    }
    if engine.account_result(kind) == Some(false) {
        ui.label(RichText::new(t!("account.failed")).color(p.danger));
    }
}

fn connected_line(ui: &mut egui::Ui, text: &str) {
    let p = theme::palette(ui.ctx());
    ui.horizontal(|ui| {
        ui.add(crate::icons::Icon::Check.image(16.0, p.success));
        ui.label(RichText::new(text).color(p.success));
    });
}
