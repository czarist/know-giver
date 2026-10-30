//! Required first run: language, name, teacher, AI service and terms. Nothing
//! else opens until it is done; study plans already saved are kept.

use egui::{Color32, RichText};
use kg_engine::{AiConfig, Command, Engine, Settings};
use rust_i18n::t;

use super::Actions;
use crate::app::KnowGiverApp;
use crate::legal::{self, LegalDoc};
use crate::{i18n, icons, theme, widgets};

const STEPS: usize = 5;
const FIELD_WIDTH: f32 = 420.0;
/// Same height for both recommendation cards, whatever their text.
const RECOMMENDATION_HEIGHT: f32 = 150.0;

/// What the student picked so far, before it is saved.
#[derive(Debug, Default)]
pub struct SetupUi {
    pub step: usize,
    /// Starts from the saved settings with the setup's own choices cleared.
    pub draft: Option<Settings>,
    pub language_chosen: bool,
    pub provider: Option<&'static kg_engine::Provider>,
    pub model: String,
    /// The model name is typed instead of picked from the menu.
    pub typing_model: bool,
    pub api_key: String,
    pub base_url: String,
    pub agreed: bool,
    pub reading: Option<LegalDoc>,
}

impl SetupUi {
    /// The AI settings these choices make, when a provider is picked.
    fn ai_config(&self) -> Option<AiConfig> {
        let provider = self.provider?;
        Some(provider.config(&self.base_url, &self.api_key, &self.model))
    }

    fn ai_ready(&self) -> bool {
        let Some(provider) = self.provider else {
            return false;
        };
        !self.model.trim().is_empty()
            && self.base_url.trim().starts_with("http")
            && (!provider.needs_key || !self.api_key.trim().is_empty())
    }

    fn step_done(&self, step: usize) -> bool {
        let Some(draft) = &self.draft else {
            return false;
        };
        match step {
            0 => self.language_chosen,
            1 => !draft.student_name.trim().is_empty(),
            2 => draft.teacher.is_some(),
            3 => self.ai_ready(),
            _ => self.agreed,
        }
    }

    fn pick_provider(&mut self, provider: &'static kg_engine::Provider) {
        if self.provider.map(|p| p.id) == Some(provider.id) {
            return;
        }
        self.provider = Some(provider);
        self.model = provider.default_model().to_owned();
        self.typing_model = false;
        self.base_url = provider.base_url.to_owned();
    }
}

pub fn show(app: &mut KnowGiverApp, ui: &mut egui::Ui) {
    let mut actions = Actions::default();
    let Ok(engine) = &app.engine else {
        return;
    };
    let setup = &mut app.ui.setup;
    if setup.draft.is_none() {
        let mut draft = engine.settings().clone();
        draft.student_name.clear();
        draft.teacher = None;
        draft.ai_provider.clear();
        setup.draft = Some(draft);
    }

    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.vertical_centered_justified(|ui| {
            ui.set_max_width(720.0);
            heading(ui, setup.step);
            ui.add_space(12.0);
            widgets::card(ui, |ui| {
                ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                    match setup.step {
                        0 => language_step(ui, setup),
                        1 => name_step(ui, setup),
                        2 => teacher_step(ui, engine, setup),
                        3 => ai_step(ui, engine, setup, &mut actions),
                        _ => terms_step(ui, setup),
                    }
                });
            });
            ui.add_space(12.0);
            navigation(ui, setup, &mut actions);
        });
    });
    actions.run(app);
}

fn heading(ui: &mut egui::Ui, step: usize) {
    let p = theme::palette(ui.ctx());
    ui.horizontal(|ui| {
        ui.add(icons::logo(44.0));
        ui.vertical(|ui| {
            ui.label(
                RichText::new(t!("setup.title"))
                    .family(theme::heading_family())
                    .size(24.0),
            );
            ui.label(RichText::new(t!("setup.subtitle")).color(p.muted));
        });
    });
    ui.add_space(10.0);
    let names = [
        t!("setup.steps.language"),
        t!("setup.steps.name"),
        t!("setup.steps.teacher"),
        t!("setup.steps.ai"),
        t!("setup.steps.terms"),
    ];
    ui.horizontal_wrapped(|ui| {
        for (i, name) in names.iter().enumerate() {
            let color = if i == step {
                p.accent
            } else if i < step {
                p.success
            } else {
                p.muted
            };
            let mut text = RichText::new(format!("{}. {name}", i + 1)).color(color);
            if i == step {
                text = text.strong();
            }
            ui.label(text);
            if i + 1 < names.len() {
                ui.label(RichText::new("·").color(p.muted));
            }
        }
    });
    ui.add(
        egui::ProgressBar::new((step + 1) as f32 / STEPS as f32)
            .desired_height(6.0)
            .fill(p.accent),
    );
}

fn step_title(ui: &mut egui::Ui, title: &str, hint: &str) {
    ui.label(
        RichText::new(title)
            .family(theme::strong_family())
            .size(18.0),
    );
    widgets::hint(ui, hint);
    ui.add_space(8.0);
}

fn language_step(ui: &mut egui::Ui, setup: &mut SetupUi) {
    // Both languages, so anyone can read the question.
    step_title(ui, &t!("setup.language.title"), &t!("setup.language.hint"));
    let current = i18n::current();
    ui.horizontal_wrapped(|ui| {
        for locale in i18n::LOCALES {
            let selected = setup.language_chosen && current == locale;
            let label = RichText::new(i18n::language_name(locale)).size(16.0);
            if ui
                .add(
                    egui::Button::new(label)
                        .selected(selected)
                        .min_size(egui::vec2(200.0, 44.0)),
                )
                .clicked()
            {
                i18n::set(locale);
                setup.language_chosen = true;
                if let Some(draft) = &mut setup.draft {
                    draft.locale = locale.to_owned();
                }
            }
        }
    });
}

fn name_step(ui: &mut egui::Ui, setup: &mut SetupUi) {
    step_title(ui, &t!("setup.name.title"), &t!("setup.name.hint"));
    if let Some(draft) = &mut setup.draft {
        super::settings::name_field(ui, &mut draft.student_name).request_focus_once(ui);
    }
}

/// Focus helper: the name field gets the cursor when the step opens.
trait FocusOnce {
    fn request_focus_once(self, ui: &egui::Ui);
}

impl FocusOnce for egui::Response {
    fn request_focus_once(self, ui: &egui::Ui) {
        let id = egui::Id::new("setup_name_focused");
        let done = ui.ctx().data(|d| d.get_temp::<bool>(id)).unwrap_or(false);
        if !done {
            self.request_focus();
            ui.ctx().data_mut(|d| d.insert_temp(id, true));
        }
    }
}

fn teacher_step(ui: &mut egui::Ui, engine: &Engine, setup: &mut SetupUi) {
    step_title(ui, &t!("setup.teacher.title"), &t!("setup.teacher.hint"));
    if let Some(draft) = &mut setup.draft {
        super::settings::teacher_picker(ui, engine, draft, true);
    }
}

fn ai_step(ui: &mut egui::Ui, engine: &Engine, setup: &mut SetupUi, actions: &mut Actions) {
    let p = theme::palette(ui.ctx());
    step_title(ui, &t!("setup.ai.title"), &t!("setup.ai.hint"));

    // The two recommendations, side by side in equal columns.
    let recommended: Vec<&'static kg_engine::Provider> = kg_engine::PROVIDERS
        .iter()
        .filter(|p| p.recommended.is_some())
        .collect();
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.x = 14.0;
        ui.columns(recommended.len().max(1), |columns| {
            for (ui, provider) in columns.iter_mut().zip(recommended) {
                recommendation_card(ui, setup, provider);
            }
        });
    });
    ui.add_space(16.0);

    field(ui, &t!("setup.ai.provider"), |ui| {
        provider_select(ui, setup)
    });
    let Some(provider) = setup.provider else {
        return;
    };
    if provider.base_url.is_empty() || !provider.needs_key {
        field(ui, &t!("setup.ai.address"), |ui| {
            ui.add(
                egui::TextEdit::singleline(&mut setup.base_url)
                    .hint_text("http://localhost:8000/v1")
                    .desired_width(FIELD_WIDTH),
            );
        });
    }
    field(ui, &t!("setup.ai.key"), |ui| {
        ui.add(
            egui::TextEdit::singleline(&mut setup.api_key)
                .password(true)
                .hint_text(if provider.needs_key {
                    t!("setup.ai.key_placeholder")
                } else {
                    t!("setup.ai.key_optional")
                })
                .desired_width(FIELD_WIDTH),
        );
        links(ui, provider);
    });
    field(ui, &t!("setup.ai.model"), |ui| {
        model_select(ui, engine, setup);
    });

    let Some(config) = setup.ai_config() else {
        return;
    };
    if config.models.vision.is_empty() {
        widgets::hint(ui, &t!("setup.ai.no_vision"));
    } else {
        widgets::hint(
            ui,
            &t!("setup.ai.vision", model = config.models.vision.as_str()),
        );
    }
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        let can_list = setup.base_url.trim().starts_with("http")
            && (!provider.needs_key || !setup.api_key.trim().is_empty());
        if ui
            .add_enabled(
                can_list && !engine.is_listing_models(),
                egui::Button::new(t!("setup.ai.load_models")),
            )
            .on_hover_text(t!("setup.ai.load_models_hint"))
            .clicked()
        {
            actions.send(Command::ListModels(Box::new(config.clone())));
        }
        let testing = engine.is_testing_connection();
        if ui
            .add_enabled(
                setup.ai_ready() && !testing,
                egui::Button::new(t!("settings.ai.test")),
            )
            .clicked()
        {
            actions.send(Command::TestAiConfig(Box::new(config.clone())));
        }
        if testing || engine.is_listing_models() {
            ui.spinner();
        }
        match engine.connection_result(&config) {
            Some(true) => {
                ui.add(crate::icons::Icon::Check.image(18.0, p.success));
                ui.label(RichText::new(t!("setup.ai.connected")).color(p.success));
            }
            Some(false) => {
                ui.label(RichText::new(t!("setup.ai.failed")).color(p.danger));
            }
            None => {}
        }
    });
    if let Some(list) = engine.model_list().filter(|list| list.is_for(&config)) {
        let text = match &list.result {
            Ok(models) => t!("setup.ai.models_loaded", count = models.len()),
            Err(_) => t!("setup.ai.models_failed"),
        };
        widgets::hint(ui, &text);
    }
}

/// A label above its field, with room after.
fn field(ui: &mut egui::Ui, label: &str, add: impl FnOnce(&mut egui::Ui)) {
    ui.label(RichText::new(label).strong());
    ui.add_space(2.0);
    add(ui);
    ui.add_space(12.0);
}

fn links(ui: &mut egui::Ui, provider: &kg_engine::Provider) {
    let p = theme::palette(ui.ctx());
    if provider.key_url.is_empty() && provider.site.is_empty() {
        return;
    }
    ui.horizontal(|ui| {
        if !provider.key_url.is_empty() {
            widgets::link(ui, &t!("setup.ai.get_key"), provider.key_url);
        }
        if !provider.key_url.is_empty() && !provider.site.is_empty() {
            ui.label(RichText::new("·").color(p.muted));
        }
        if !provider.site.is_empty() {
            widgets::link(ui, &t!("setup.ai.prices"), provider.site);
        }
    });
}

/// The provider list, with logos and groups.
pub(crate) fn provider_select(ui: &mut egui::Ui, setup: &mut SetupUi) {
    ui.horizontal(|ui| {
        if let Some(provider) = setup.provider {
            crate::provider_logo::show(ui, provider, 22.0);
        }
        let selected = setup
            .provider
            .map_or_else(|| t!("setup.ai.choose").into_owned(), provider_name);
        egui::ComboBox::from_id_salt("setup_provider")
            .selected_text(selected)
            .width(FIELD_WIDTH - 30.0)
            .height(460.0)
            .show_ui(ui, |ui| {
                if let Some(provider) = provider_list(ui, setup.provider.map(|p| p.id)) {
                    setup.pick_provider(provider);
                }
            });
    });
}

/// Draws the grouped provider rows; returns the one clicked.
pub(crate) fn provider_list(
    ui: &mut egui::Ui,
    current: Option<&str>,
) -> Option<&'static kg_engine::Provider> {
    let p = theme::palette(ui.ctx());
    ui.set_min_width(FIELD_WIDTH);
    let groups = [
        (t!("setup.ai.group_recommended"), None),
        (
            t!("setup.ai.group_labs"),
            Some(kg_engine::ProviderKind::Lab),
        ),
        (
            t!("setup.ai.group_platforms"),
            Some(kg_engine::ProviderKind::Platform),
        ),
        (
            t!("setup.ai.group_local"),
            Some(kg_engine::ProviderKind::Local),
        ),
        (
            t!("setup.ai.group_other"),
            Some(kg_engine::ProviderKind::Custom),
        ),
    ];
    let mut picked = None;
    for (title, kind) in groups {
        let members: Vec<&'static kg_engine::Provider> = kg_engine::PROVIDERS
            .iter()
            .filter(|provider| match kind {
                None => provider.recommended.is_some(),
                Some(kind) => provider.recommended.is_none() && provider.kind == kind,
            })
            .collect();
        if members.is_empty() {
            continue;
        }
        ui.add_space(4.0);
        ui.label(RichText::new(title.to_uppercase()).small().color(p.muted));
        for provider in members {
            let note = provider.recommended.map(recommendation);
            let selected = current == Some(provider.id);
            if crate::provider_logo::row(
                ui,
                provider,
                &provider_name(provider),
                note.as_deref(),
                selected,
            )
            .clicked()
            {
                picked = Some(provider);
            }
        }
    }
    picked
}

fn recommendation_card(
    ui: &mut egui::Ui,
    setup: &mut SetupUi,
    provider: &'static kg_engine::Provider,
) {
    let p = theme::palette(ui.ctx());
    let selected = setup.provider.map(|x| x.id) == Some(provider.id);
    let Some(why) = provider.recommended else {
        return;
    };
    let text = match why {
        kg_engine::Recommendation::Value => t!("setup.ai.value_text"),
        kg_engine::Recommendation::Accuracy => t!("setup.ai.accuracy_text"),
    };
    // The click area sits under the card's content, so its links still work.
    let response = ui
        .scope_builder(egui::UiBuilder::new().sense(egui::Sense::click()), |ui| {
            egui::Frame::new()
                .fill(if selected {
                    p.accent.gamma_multiply(0.12)
                } else {
                    p.surface
                })
                .stroke(egui::Stroke::new(
                    if selected { 2.0 } else { 1.0 },
                    if selected { p.accent } else { p.border },
                ))
                .corner_radius(6.0)
                .inner_margin(egui::Margin::same(16))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.set_min_height(RECOMMENDATION_HEIGHT);
                    ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                        ui.spacing_mut().item_spacing.y = 6.0;
                        ui.horizontal(|ui| {
                            widgets::chip(ui, &recommendation(why), p.highlight);
                            if selected {
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.add(crate::icons::Icon::Check.image(18.0, p.accent));
                                    },
                                );
                            }
                        });
                        ui.add_space(2.0);
                        ui.label(
                            RichText::new(provider.name)
                                .family(theme::strong_family())
                                .size(17.0),
                        );
                        ui.label(RichText::new(text).color(p.muted));
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            widgets::link(ui, &t!("setup.ai.get_key"), provider.key_url);
                            ui.label(RichText::new("·").color(p.muted));
                            widgets::link(ui, &t!("setup.ai.prices"), provider.site);
                        });
                    });
                });
        })
        .response
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    if response.clicked() {
        setup.pick_provider(provider);
    }
}

/// Suggested models and the account's own list in one menu; "Other model"
/// opens a field to type any name.
fn model_select(ui: &mut egui::Ui, engine: &Engine, setup: &mut SetupUi) {
    let Some(provider) = setup.provider else {
        return;
    };
    let config = setup.ai_config();
    let listed: Vec<String> = engine
        .model_list()
        .filter(|list| config.as_ref().is_some_and(|c| list.is_for(c)))
        .and_then(|list| list.result.as_ref().ok())
        .cloned()
        .unwrap_or_default();
    let mut options: Vec<String> = provider.models.iter().map(|m| m.id.to_owned()).collect();
    for model in &listed {
        if !options.contains(model) {
            options.push(model.clone());
        }
    }
    if !setup.model.trim().is_empty() && !options.contains(&setup.model) {
        setup.typing_model = true;
    }
    let p = theme::palette(ui.ctx());
    let selected = if setup.typing_model {
        t!("setup.ai.other_model").into_owned()
    } else if setup.model.is_empty() {
        t!("setup.ai.choose").into_owned()
    } else {
        setup.model.clone()
    };
    egui::ComboBox::from_id_salt("setup_model")
        .selected_text(selected)
        .width(FIELD_WIDTH)
        .height(360.0)
        .show_ui(ui, |ui| {
            ui.set_min_width(FIELD_WIDTH);
            if options.is_empty() {
                ui.label(
                    RichText::new(t!("setup.ai.no_suggestions"))
                        .small()
                        .color(p.muted),
                );
            }
            for option in &options {
                let mut text = RichText::new(option.as_str());
                if provider.model_sees(option) {
                    text = RichText::new(format!("{option}   · {}", t!("setup.ai.reads_images")));
                }
                let picked = !setup.typing_model && &setup.model == option;
                if ui.selectable_label(picked, text).clicked() {
                    setup.model = option.clone();
                    setup.typing_model = false;
                }
            }
            ui.separator();
            if ui
                .selectable_label(setup.typing_model, t!("setup.ai.other_model"))
                .clicked()
            {
                setup.typing_model = true;
            }
        });
    if setup.typing_model {
        ui.add_space(4.0);
        ui.add(
            egui::TextEdit::singleline(&mut setup.model)
                .hint_text(t!("setup.ai.model_placeholder"))
                .desired_width(FIELD_WIDTH),
        );
    }
}

fn terms_step(ui: &mut egui::Ui, setup: &mut SetupUi) {
    let p = theme::palette(ui.ctx());
    step_title(ui, &t!("setup.terms.title"), &t!("setup.terms.hint"));
    for point in [
        t!("setup.terms.point_ai"),
        t!("setup.terms.point_quality"),
        t!("setup.terms.point_data"),
        t!("setup.terms.point_costs"),
    ] {
        bullet(ui, &point);
    }
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        for doc in LegalDoc::ALL {
            let label = match doc {
                LegalDoc::Terms => t!("setup.terms.read_terms"),
                LegalDoc::Privacy => t!("setup.terms.read_privacy"),
            };
            if ui
                .add(egui::Button::new(label).selected(setup.reading == Some(doc)))
                .clicked()
            {
                setup.reading = if setup.reading == Some(doc) {
                    None
                } else {
                    Some(doc)
                };
            }
        }
    });
    if let Some(doc) = setup.reading {
        ui.add_space(6.0);
        egui::Frame::new()
            .fill(p.surface)
            .stroke(egui::Stroke::new(1.0, p.border))
            .inner_margin(egui::Margin::same(12))
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt(("legal", doc as u8))
                    .max_height(320.0)
                    .show(ui, |ui| legal::show(ui, doc.text(&i18n::current())));
            });
    }
    ui.add_space(8.0);
    ui.checkbox(&mut setup.agreed, t!("setup.terms.agree"));
}

fn navigation(ui: &mut egui::Ui, setup: &mut SetupUi, actions: &mut Actions) {
    let done = setup.step_done(setup.step);
    ui.horizontal(|ui| {
        if setup.step > 0 && ui.button(t!("setup.back")).clicked() {
            setup.step -= 1;
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let last = setup.step + 1 == STEPS;
            let label = if last {
                t!("setup.finish")
            } else {
                t!("setup.next")
            };
            let button = egui::Button::new(RichText::new(label).color(Color32::WHITE))
                .fill(theme::palette(ui.ctx()).accent)
                .min_size(egui::vec2(140.0, 34.0));
            if ui.add_enabled(done, button).clicked() {
                if last {
                    if let Some(settings) = finished(setup) {
                        actions.send(Command::CompleteSetup(Box::new(settings)));
                    }
                } else {
                    setup.step += 1;
                }
            }
            if !done {
                ui.label(
                    RichText::new(missing(setup.step))
                        .small()
                        .color(theme::palette(ui.ctx()).muted),
                );
            }
        });
    });
}

/// A list item whose wrapped lines stay right of the bullet.
pub(crate) fn bullet(ui: &mut egui::Ui, text: &str) {
    let p = theme::palette(ui.ctx());
    ui.horizontal_top(|ui| {
        ui.label(RichText::new("•").color(p.accent));
        ui.add(egui::Label::new(text).wrap());
    });
}

/// What the current step still needs, next to the disabled button.
fn missing(step: usize) -> String {
    match step {
        0 => t!("setup.missing.language"),
        1 => t!("setup.missing.name"),
        2 => t!("setup.missing.teacher"),
        3 => t!("setup.missing.ai"),
        _ => t!("setup.missing.terms"),
    }
    .into_owned()
}

/// The settings to save, once every step is done.
fn finished(setup: &SetupUi) -> Option<Settings> {
    if !(0..STEPS).all(|step| setup.step_done(step)) {
        return None;
    }
    let mut settings = setup.draft.clone()?;
    let provider = setup.provider?;
    settings.ai = setup.ai_config()?;
    settings.ai_provider = provider.id.to_owned();
    settings.locale = i18n::current();
    Some(settings)
}

pub(crate) fn provider_name(provider: &kg_engine::Provider) -> String {
    match provider.id {
        "custom" => t!("setup.ai.custom").into_owned(),
        "ollama" => t!("setup.ai.ollama").into_owned(),
        _ => provider.name.to_owned(),
    }
}

pub(crate) fn recommendation(why: kg_engine::Recommendation) -> String {
    match why {
        kg_engine::Recommendation::Value => t!("setup.ai.value"),
        kg_engine::Recommendation::Accuracy => t!("setup.ai.accuracy"),
    }
    .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ready() -> SetupUi {
        let mut setup = SetupUi {
            draft: Some(Settings::default()),
            language_chosen: true,
            agreed: true,
            ..SetupUi::default()
        };
        if let Some(d) = &mut setup.draft {
            d.student_name = "Ana".into();
            d.teacher = Some(kg_engine::Teacher::ALL[0]);
        }
        setup.pick_provider(kg_engine::ai_provider("zai").unwrap());
        setup.api_key = "key".into();
        setup
    }

    #[test]
    fn every_step_must_be_done() {
        let setup = ready();
        let settings = finished(&setup).unwrap();
        assert_eq!(settings.ai.models.tutor, "glm-5.3");
        assert_eq!(settings.ai_provider, "zai");

        let mut no_key = ready();
        no_key.api_key = " ".into();
        assert!(!no_key.step_done(3));
        assert!(finished(&no_key).is_none());

        let mut not_agreed = ready();
        not_agreed.agreed = false;
        assert!(finished(&not_agreed).is_none());

        let mut no_teacher = ready();
        if let Some(d) = &mut no_teacher.draft {
            d.teacher = None;
        }
        assert!(finished(&no_teacher).is_none());
    }

    #[test]
    fn local_servers_need_no_key() {
        let mut setup = ready();
        setup.pick_provider(kg_engine::ai_provider("ollama").unwrap());
        setup.api_key.clear();
        assert!(setup.step_done(3));
        setup.base_url.clear();
        assert!(!setup.step_done(3));
    }

    #[test]
    fn changing_provider_resets_model_and_address() {
        let mut setup = ready();
        setup.model = "typed".into();
        setup.pick_provider(kg_engine::ai_provider("anthropic").unwrap());
        assert_eq!(setup.model, "claude-opus-5-5");
        assert_eq!(setup.base_url, "https://api.anthropic.com/v1");
        // Picking the same one keeps what was typed.
        setup.model = "claude-haiku-5-5".into();
        setup.pick_provider(kg_engine::ai_provider("anthropic").unwrap());
        assert_eq!(setup.model, "claude-haiku-5-5");
    }
}
