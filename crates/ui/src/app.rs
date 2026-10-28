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
    /// First-run steps; not in the sidebar.
    Welcome,
    #[default]
    Edital,
    Plan,
    Board,
    Lesson,
    Quiz,
    Review,
    Progress,
    Settings,
}

impl Screen {
    const ALL: [Self; 9] = [
        Self::Today,
        Self::Edital,
        Self::Plan,
        Self::Board,
        Self::Lesson,
        Self::Quiz,
        Self::Review,
        Self::Progress,
        Self::Settings,
    ];

    fn icon(self) -> Icon {
        match self {
            Self::Today => Icon::Today,
            Self::Welcome => Icon::Sparkles,
            Self::Edital => Icon::Edital,
            Self::Plan => Icon::Plan,
            Self::Board => Icon::Board,
            Self::Lesson => Icon::Lesson,
            Self::Quiz => Icon::Quiz,
            Self::Review => Icon::Review,
            Self::Progress => Icon::Progress,
            Self::Settings => Icon::Settings,
        }
    }

    fn label(self) -> String {
        match self {
            Self::Today => t!("nav.today"),
            Self::Welcome => t!("nav.welcome"),
            Self::Edital => t!("nav.edital"),
            Self::Plan => t!("nav.plan"),
            Self::Board => t!("nav.board"),
            Self::Lesson => t!("nav.lesson"),
            Self::Quiz => t!("nav.quiz"),
            Self::Review => t!("nav.review"),
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
}

/// The eframe application.
pub struct KnowGiverApp {
    pub(crate) engine: Result<Engine, String>,
    pub(crate) ui: UiState,
    toasts: Vec<Toast>,
    /// Screen shown last frame and when it appeared, for the fade-in.
    shown_screen: Screen,
    shown_since: f64,
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
            },
            toasts: Vec::new(),
            shown_screen: screen,
            shown_since: 0.0,
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
        self.ui.screen = screen;
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
                    ui.add(icons::logo(38.0));
                    ui.vertical(|ui| {
                        ui.add_space(1.0);
                        ui.label(
                            egui::RichText::new(t!("app.name"))
                                .family(theme::heading_family())
                                .size(21.0)
                                .color(p.accent),
                        );
                        ui.label(
                            egui::RichText::new(t!("app.tagline"))
                                .small()
                                .color(p.muted),
                        );
                    });
                });
                ui.add_space(18.0);

                for screen in Screen::ALL {
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
                    if ui.add(button).clicked() {
                        self.ui.screen = screen;
                    }
                }

                if let Some(plan) = &engine_badges.plan_title {
                    ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                        ui.label(egui::RichText::new(plan).small().color(p.muted));
                        ui.label(
                            egui::RichText::new(t!("nav.active_plan"))
                                .small()
                                .strong()
                                .color(p.muted),
                        );
                    });
                }
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
fn open_file(path: &std::path::Path) {
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

/// Counts shown next to sidebar entries.
#[derive(Debug, Default)]
struct Badges {
    due_reviews: usize,
    quiz_active: bool,
    lesson_active: bool,
    plan_title: Option<String>,
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
            plan_title: engine.active_plan().map(|p| p.title.clone()),
        }
    }

    fn for_screen(&self, screen: Screen) -> Option<String> {
        match screen {
            Screen::Review if self.due_reviews > 0 => Some(self.due_reviews.to_string()),
            Screen::Quiz if self.quiz_active => Some(t!("nav.in_progress").into_owned()),
            Screen::Lesson if self.lesson_active => Some(t!("nav.in_progress").into_owned()),
            _ => None,
        }
    }
}

impl eframe::App for KnowGiverApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let badges = match self.engine.as_mut() {
            Ok(engine) => {
                engine.poll();
                let notices = engine.take_notices();
                let busy = engine.is_busy();
                let badges = Badges::from_engine(engine);
                for notice in &notices {
                    if let kg_engine::Notice::HandoutSaved(path) = notice {
                        open_file(path);
                    }
                    let (text, is_error) = i18n::notice(notice);
                    self.push_toast(text, is_error);
                }
                if busy {
                    ui.ctx().request_repaint_after(Duration::from_millis(40));
                }
                badges
            }
            Err(_) => Badges::default(),
        };

        self.sidebar(ui, &badges);
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
                    Screen::Welcome => screens::welcome::show(self, ui),
                    Screen::Edital => screens::edital::show(self, ui),
                    Screen::Plan => screens::plan::show(self, ui),
                    Screen::Board => screens::board::show(self, ui),
                    Screen::Lesson => screens::lesson::show(self, ui),
                    Screen::Quiz => screens::quiz::show(self, ui),
                    Screen::Review => screens::review::show(self, ui),
                    Screen::Progress => screens::progress::show(self, ui),
                    Screen::Settings => screens::settings::show(self, ui),
                }
            });
    }
}
