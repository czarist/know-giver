use std::sync::Arc;
use std::time::{Duration, Instant};

use kg_engine::{AiConfig, AppPaths, Command, Engine, PlanId, PlanPrefs, Settings};
use rust_i18n::t;

use crate::icons::{self, Icon};
use crate::{i18n, screens, theme, widgets};

/// Top-level screens, in sidebar order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Screen {
    Today,
    /// Required first run (language, name, teacher, AI, terms); no sidebar.
    Setup,
    /// First-run steps; not in the sidebar.
    Welcome,
    #[default]
    Edital,
    Plan,
    /// Generated handouts, read inside the app.
    Reader,
    Board,
    Lesson,
    Quiz,
    Review,
    /// Written answers: essays and discursive questions.
    Discursive,
    Progress,
    Settings,
}

impl Screen {
    const ALL: [Self; 11] = [
        Self::Today,
        Self::Edital,
        Self::Plan,
        Self::Reader,
        Self::Board,
        Self::Lesson,
        Self::Quiz,
        Self::Review,
        Self::Discursive,
        Self::Progress,
        Self::Settings,
    ];

    fn icon(self) -> Icon {
        match self {
            Self::Today => Icon::Today,
            Self::Setup | Self::Welcome => Icon::Sparkles,
            Self::Edital => Icon::Edital,
            Self::Plan => Icon::Plan,
            Self::Reader => Icon::Handout,
            Self::Board => Icon::Board,
            Self::Lesson => Icon::Lesson,
            Self::Quiz => Icon::Quiz,
            Self::Review => Icon::Review,
            Self::Discursive => Icon::Writing,
            Self::Progress => Icon::Progress,
            Self::Settings => Icon::Settings,
        }
    }

    fn label(self) -> String {
        match self {
            Self::Today => t!("nav.today"),
            Self::Setup => t!("nav.setup"),
            Self::Welcome => t!("nav.welcome"),
            Self::Edital => t!("nav.edital"),
            Self::Plan => t!("nav.plan"),
            Self::Reader => t!("nav.reader"),
            Self::Board => t!("nav.board"),
            Self::Lesson => t!("nav.lesson"),
            Self::Quiz => t!("nav.quiz"),
            Self::Review => t!("nav.review"),
            Self::Discursive => t!("nav.discursive"),
            Self::Progress => t!("nav.progress"),
            Self::Settings => t!("nav.settings"),
        }
        .into_owned()
    }
}

/// How long confirmations stay on screen. Errors stay until closed.
const TOAST_TTL: Duration = Duration::from_secs(6);
/// Seconds a toast takes to fade in or out.
const TOAST_FADE: f32 = 0.25;
/// Seconds a screen takes to fade in.
const SCREEN_FADE: f64 = 0.18;

#[derive(Debug)]
struct Toast {
    text: String,
    is_error: bool,
    shown_at: Instant,
}

/// Plan list grouping on the Plan screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlanView {
    #[default]
    ByWeek,
    BySubject,
}

/// UI-only state: form contents and view choices.
#[derive(Debug)]
pub struct UiState {
    pub screen: Screen,
    pub prefs: PlanPrefs,
    pub settings_draft: Option<Settings>,
    pub lesson_input: String,
    /// Captions over the teacher in the lesson.
    pub lesson_captions: bool,
    /// Transcript / chat panel open in the lesson.
    pub lesson_panel: bool,
    pub review_answer: String,
    pub confirm_delete: Option<PlanId>,
    pub confirm_delete_doc: Option<kg_engine::DocumentId>,
    /// Topic picked for a set-aside board item, before "use anyway".
    pub board_choice: std::collections::HashMap<kg_engine::BoardItem, kg_engine::TopicId>,
    pub plan_view: PlanView,
    /// Prepare handouts and quizzes right after the plan is created.
    pub prepare_all: bool,
    /// Choices of the required first run, before they are saved.
    pub setup: crate::screens::setup::SetupUi,
    /// Step of the guided tour on screen.
    pub tour_step: usize,
    pub reader: crate::screens::reader::ReaderUi,
    pub discursive: crate::screens::discursive::DiscursiveUi,
}

/// The eframe application.
pub struct KnowGiverApp {
    pub(crate) engine: Result<Engine, String>,
    pub(crate) ui: UiState,
    toasts: Vec<Toast>,
    /// Screen shown last frame and when it appeared, for the fade-in.
    shown_screen: Screen,
    shown_since: f64,
    /// Where each sidebar entry was drawn last frame, for the tour.
    pub(crate) nav_rects: Vec<(Screen, egui::Rect)>,
}

impl std::fmt::Debug for KnowGiverApp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KnowGiverApp")
            .field("screen", &self.ui.screen)
            .finish_non_exhaustive()
    }
}

impl KnowGiverApp {
    pub fn new(cc: &eframe::CreationContext<'_>, env_ai: AiConfig) -> Self {
        Self::with_paths(cc, AppPaths::detect(), env_ai)
    }

    /// Like [`Self::new`], with an explicit data folder.
    pub fn with_paths(cc: &eframe::CreationContext<'_>, paths: AppPaths, env_ai: AiConfig) -> Self {
        egui_extras::install_image_loaders(&cc.egui_ctx);
        theme::apply(&cc.egui_ctx);
        let engine = match Engine::new(paths, env_ai) {
            Ok(mut engine) => {
                let ctx = cc.egui_ctx.clone();
                engine.set_waker(Arc::new(move || ctx.request_repaint()));
                i18n::set(&engine.settings().locale);
                Ok(engine)
            }
            Err(err) => {
                tracing::error!(%err, "engine failed to start");
                Err(i18n::error_message(&err))
            }
        };
        let screen = match &engine {
            Ok(e) if e.needs_setup() => Screen::Setup,
            Ok(e) if e.needs_tour() => Screen::Today,
            Ok(e) if e.needs_onboarding() => Screen::Welcome,
            Ok(e) if e.active_plan().is_some() => Screen::Today,
            _ => Screen::Edital,
        };
        Self {
            engine,
            ui: UiState {
                screen,
                prefs: PlanPrefs {
                    weeks: 12,
                    hours_per_week: 15,
                    role: String::new(),
                },
                settings_draft: None,
                lesson_input: String::new(),
                lesson_captions: true,
                lesson_panel: false,
                review_answer: String::new(),
                confirm_delete: None,
                confirm_delete_doc: None,
                board_choice: std::collections::HashMap::new(),
                plan_view: PlanView::default(),
                prepare_all: true,
                setup: Default::default(),
                tour_step: 0,
                reader: Default::default(),
                discursive: Default::default(),
            },
            toasts: Vec::new(),
            shown_screen: screen,
            shown_since: 0.0,
            nav_rects: Vec::new(),
        }
    }

    /// Sends a command to the engine; errors become toasts.
    pub(crate) fn dispatch(&mut self, command: Command) {
        let Ok(engine) = self.engine.as_mut() else {
            return;
        };
        if let Err(err) = engine.handle(command) {
            tracing::warn!(%err, "command failed");
            self.push_toast(i18n::error_message(&err), true);
        }
    }

    pub(crate) fn go_to(&mut self, screen: Screen) {
        if self.ui.screen == Screen::Settings && screen != Screen::Settings {
            self.dispatch(Command::StopVoicePreview);
        }
        self.ui.screen = screen;
    }

    /// Drops the messages on screen, for screenshots.
    #[cfg(test)]
    pub(crate) fn clear_toasts(&mut self) {
        self.toasts.clear();
    }

    fn push_toast(&mut self, text: String, is_error: bool) {
        self.toasts.push(Toast {
            text,
            is_error,
            shown_at: Instant::now(),
        });
        if self.toasts.len() > 4 {
            self.toasts.remove(0);
        }
    }

    fn sidebar(&mut self, ui: &mut egui::Ui, engine_badges: &Badges) {
        let p = theme::palette(ui.ctx());
        let frame = egui::Frame::new()
            .fill(p.sidebar)
            .inner_margin(egui::Margin::symmetric(14, 20));

        egui::Panel::left("nav")
            .resizable(false)
            .exact_size(230.0)
            .frame(frame)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.add(icons::logo(40.0));
                    ui.vertical(|ui| {
                        ui.add_space(2.0);
                        ui.label(
                            egui::RichText::new(t!("app.name").to_uppercase())
                                .family(theme::heading_family())
                                .size(18.0)
                                .color(p.text),
                        );
                        ui.label(
                            egui::RichText::new(t!(
                                "app.version",
                                version = env!("CARGO_PKG_VERSION")
                            ))
                            .family(theme::mono_family())
                            .size(10.5)
                            .color(p.muted),
                        );
                        widgets::kicker(ui, &t!("app.tagline"), p.muted);
                    });
                });
                ui.add_space(6.0);
                widgets::rule(ui, 3.0);
                ui.add_space(12.0);

                self.nav_rects.clear();
                for screen in Screen::ALL {
                    // Written answers only when the notice describes a written test.
                    if screen == Screen::Discursive
                        && !engine_badges.writing
                        && self.ui.screen != screen
                    {
                        continue;
                    }
                    let selected = self.ui.screen == screen;
                    let color = if selected { p.accent } else { p.muted };
                    let mut text = egui::RichText::new(screen.label());
                    if selected {
                        text = text.family(theme::strong_family()).color(p.text);
                    }
                    let mut button =
                        egui::Button::image_and_text(screen.icon().image(18.0, color), text)
                            .selected(selected)
                            .frame(selected)
                            .min_size(egui::vec2(ui.available_width(), 36.0));
                    if let Some(badge) = engine_badges.for_screen(screen) {
                        button = button
                            .right_text(egui::RichText::new(badge).small().color(p.highlight));
                    }
                    let response = ui.add(button);
                    self.nav_rects.push((screen, response.rect));
                    if response.clicked() {
                        self.go_to(screen);
                    }
                }

                ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                    if let Some(plan) = &engine_badges.plan_title {
                        ui.label(egui::RichText::new(plan).small().color(p.muted));
                        widgets::kicker(ui, &t!("nav.active_plan"), p.muted);
                        ui.add_space(4.0);
                    }
                });
            });
    }

    fn toasts(&mut self, ui: &mut egui::Ui) {
        self.toasts
            .retain(|toast| toast.is_error || toast.shown_at.elapsed() < TOAST_TTL);
        if self.toasts.is_empty() {
            return;
        }
        ui.ctx().request_repaint_after(Duration::from_millis(500));
        let p = theme::palette(ui.ctx());
        let mut close = None;
        egui::Panel::bottom("toasts")
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(p.background)
                    .inner_margin(egui::Margin::symmetric(16, 8)),
            )
            .show(ui, |ui| {
                for (i, toast) in self.toasts.iter().enumerate() {
                    let color = if toast.is_error { p.danger } else { p.success };
                    // Fade in, and fade out before a confirmation goes away.
                    let age = toast.shown_at.elapsed().as_secs_f32();
                    let left = TOAST_TTL.as_secs_f32() - age;
                    let mut opacity = (age / TOAST_FADE).min(1.0);
                    if !toast.is_error {
                        opacity = opacity.min(left / TOAST_FADE);
                    }
                    if opacity < 1.0 {
                        ui.ctx().request_repaint();
                    }
                    ui.set_opacity(opacity.clamp(0.0, 1.0));
                    widgets::callout(ui, color, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(&toast.text).color(color));
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui
                                        .add(
                                            egui::Button::image(Icon::Close.image(14.0, p.muted))
                                                .frame(false),
                                        )
                                        .on_hover_text(t!("common.close"))
                                        .clicked()
                                    {
                                        close = Some(i);
                                    }
                                },
                            );
                        });
                    });
                }
            });
        if let Some(i) = close {
            self.toasts.remove(i);
        }
    }
}

/// Opens a file with the system's default app (e.g. a PDF viewer).
pub(crate) fn open_file(path: &std::path::Path) {
    let program = if cfg!(windows) {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    if let Err(err) = std::process::Command::new(program).arg(path).spawn() {
        tracing::warn!(%err, path = %path.display(), "could not open file");
    }
}

/// Opens a web page in the system's browser. Only http(s) addresses.
pub(crate) fn open_url(url: &str) {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return;
    }
    let mut command = if cfg!(windows) {
        let mut c = std::process::Command::new("rundll32");
        c.arg("url.dll,FileProtocolHandler");
        c
    } else if cfg!(target_os = "macos") {
        std::process::Command::new("open")
    } else {
        std::process::Command::new("xdg-open")
    };
    if let Err(err) = command.arg(url).spawn() {
        tracing::warn!(%err, url, "could not open link");
    }
}

/// Counts shown next to sidebar entries.
#[derive(Debug, Default)]
struct Badges {
    due_reviews: usize,
    quiz_active: bool,
    lesson_active: bool,
    /// The active plan's notice describes a written test.
    writing: bool,
    plan_title: Option<String>,
    /// Topics prepared so far and in total, while preparing the plan.
    preparing: Option<(usize, usize)>,
}

impl Badges {
    fn from_engine(engine: &Engine) -> Self {
        let today = Engine::today();
        Self {
            due_reviews: engine
                .active_plan()
                .map_or(0, |p| p.due_topics(today).len()),
            quiz_active: engine.quiz().is_some(),
            lesson_active: engine.lesson().is_some(),
            writing: engine.writing_rules().has_written_test(),
            plan_title: engine.active_plan().map(|p| p.title.clone()),
            preparing: engine.prepare_job().map(|job| (job.done(), job.total)),
        }
    }

    fn for_screen(&self, screen: Screen) -> Option<String> {
        match screen {
            Screen::Review if self.due_reviews > 0 => Some(self.due_reviews.to_string()),
            Screen::Quiz if self.quiz_active => Some(t!("nav.in_progress").into_owned()),
            Screen::Lesson if self.lesson_active => Some(t!("nav.in_progress").into_owned()),
            Screen::Plan => self
                .preparing
                .map(|(done, total)| t!("nav.preparing", done = done, total = total).into_owned()),
            _ => None,
        }
    }
}

impl eframe::App for KnowGiverApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let badges = match self.engine.as_mut() {
            Ok(engine) => {
                let study_surface = match self.ui.screen {
                    Screen::Plan | Screen::Board => engine.active_plan().is_some(),
                    Screen::Reader => engine.reader().is_some(),
                    Screen::Lesson => engine.lesson().is_some(),
                    Screen::Quiz => engine.quiz().is_some(),
                    Screen::Review => engine.review().is_some(),
                    _ => false,
                };
                let presence = ui.input(|input| {
                    if !input.focused || !study_surface {
                        kg_engine::StudyPresence::Inactive
                    } else if input.pointer.any_down()
                        || input.pointer.delta() != egui::Vec2::ZERO
                        || input
                            .events
                            .iter()
                            .any(|event| matches!(event, egui::Event::MouseWheel { .. }))
                        || !input.keys_down.is_empty()
                        || input
                            .events
                            .iter()
                            .any(|event| matches!(event, egui::Event::Text(_)))
                    {
                        kg_engine::StudyPresence::Interacting
                    } else {
                        kg_engine::StudyPresence::Visible
                    }
                });
                engine.set_study_presence(presence);
                if study_surface {
                    ui.ctx().request_repaint_after(Duration::from_secs(1));
                }
                engine.poll();
                let notices = engine.take_notices();
                let mut go_to_reader = false;
                let busy = engine.is_busy();
                let badges = Badges::from_engine(engine);
                for notice in &notices {
                    // A finished handout opens in the reader.
                    if let kg_engine::Notice::HandoutSaved(_) = notice {
                        go_to_reader = true;
                    }
                    let (text, is_error) = i18n::notice(notice);
                    self.push_toast(text, is_error);
                }
                if busy {
                    ui.ctx().request_repaint_after(Duration::from_millis(40));
                }
                if go_to_reader {
                    self.ui.screen = Screen::Reader;
                }
                badges
            }
            Err(_) => Badges::default(),
        };

        // Nothing else opens until the required first run is done.
        let (needs_setup, needs_tour) = match &self.engine {
            Ok(engine) => (engine.needs_setup(), engine.needs_tour()),
            Err(_) => (false, false),
        };
        if needs_setup {
            self.ui.screen = Screen::Setup;
        } else if self.ui.screen == Screen::Setup {
            self.ui.setup = Default::default();
            self.ui.screen = Screen::Today;
        }
        if !needs_setup {
            self.sidebar(ui, &badges);
        }
        self.toasts(ui);

        let content = egui::Frame::new()
            .fill(ui.visuals().panel_fill)
            .inner_margin(egui::Margin::symmetric(36, 28));
        egui::CentralPanel::no_frame()
            .frame(content)
            .show(ui, |ui| {
                if let Err(message) = &self.engine {
                    widgets::header(ui, &t!("fatal.title"), &t!("fatal.subtitle"));
                    let color = theme::palette(ui.ctx()).danger;
                    widgets::callout(ui, color, |ui| {
                        ui.label(message);
                    });
                    return;
                }
                // A short fade when the screen changes.
                let now = ui.input(|i| i.time);
                if self.shown_screen != self.ui.screen {
                    self.shown_screen = self.ui.screen;
                    self.shown_since = now;
                }
                let fade = ((now - self.shown_since) / SCREEN_FADE).min(1.0) as f32;
                if fade < 1.0 {
                    ui.ctx().request_repaint();
                }
                ui.set_opacity(fade);
                // Keep line length readable on wide monitors.
                ui.set_max_width(860.0);
                match self.ui.screen {
                    Screen::Today => screens::today::show(self, ui),
                    Screen::Setup => screens::setup::show(self, ui),
                    Screen::Welcome => screens::welcome::show(self, ui),
                    Screen::Edital => screens::edital::show(self, ui),
                    Screen::Plan => screens::plan::show(self, ui),
                    Screen::Reader => screens::reader::show(self, ui),
                    Screen::Board => screens::board::show(self, ui),
                    Screen::Lesson => screens::lesson::show(self, ui),
                    Screen::Quiz => screens::quiz::show(self, ui),
                    Screen::Review => screens::review::show(self, ui),
                    Screen::Discursive => screens::discursive::show(self, ui),
                    Screen::Progress => screens::progress::show(self, ui),
                    Screen::Settings => screens::settings::show(self, ui),
                }
            });
        if needs_tour {
            crate::tour::show(self, ui.ctx(), badges.writing);
        }
    }
}
