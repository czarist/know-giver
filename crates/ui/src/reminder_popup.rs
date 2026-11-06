//! The study reminder: a small Know Giver window in the corner of the
//! screen, in the app's look, with the engraving of Thoth on a jade panel.

use std::time::{Duration, Instant};

use egui::{Color32, RichText, Sense, Stroke, vec2};
use rust_i18n::t;

use crate::reminders::Progress;
use crate::{i18n, theme, widgets};

const WIDTH: f32 = 470.0;
const TILE: f32 = 150.0;
const PADDING: f32 = 14.0;
const NAME_WIDTH: f32 = 130.0;
const BAR_WIDTH: f32 = 90.0;
/// The jade of the app icon.
const JADE: Color32 = Color32::from_rgb(0x2F, 0x6F, 0x68);
const MARGIN: f32 = 20.0;
/// Clears a typical taskbar at the bottom of the screen.
const BOTTOM_MARGIN: f32 = 64.0;
/// The popup leaves by itself after this.
const LIFETIME: Duration = Duration::from_secs(120);

/// Opens the reminder popup and runs until it closes. `args` are what
/// follows `--reminder`: the UI language, then the plan's progress.
///
/// # Errors
///
/// Returns an error if eframe cannot open the window.
pub fn run_reminder(args: &[String]) -> eframe::Result {
    let (locale, rest) = args
        .split_first()
        .map_or(("pt-BR", &[][..]), |(locale, rest)| (locale.as_str(), rest));
    i18n::set(locale);
    let progress = Progress::from_args(rest);
    let height = 240.0;
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(t!("reminder.title"))
            .with_app_id("know-giver-reminder")
            .with_icon(crate::icon::app_icon())
            .with_inner_size([WIDTH, height])
            .with_resizable(false)
            .with_decorations(false)
            .with_always_on_top()
            .with_taskbar(false)
            .with_active(false)
            // Off screen until the first frame knows where the corner is.
            .with_position([-10_000.0, -10_000.0]),
        persist_window: false,
        ..Default::default()
    };
    eframe::run_native(
        "know-giver-reminder",
        options,
        Box::new(move |cc| {
            egui_extras::install_image_loaders(&cc.egui_ctx);
            theme::apply(&cc.egui_ctx);
            Ok(Box::new(ReminderPopup {
                progress,
                opened: Instant::now(),
                height: None,
                placed: false,
            }))
        }),
    )
}

struct ReminderPopup {
    progress: Progress,
    opened: Instant,
    /// The height the content asked for, once measured.
    height: Option<f32>,
    placed: bool,
}

impl eframe::App for ReminderPopup {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        // Placed in the corner once the window has its final height.
        if !self.placed
            && self.height.is_some()
            && let Some(monitor) = ctx.input(|input| input.viewport().monitor_size)
        {
            let size = ctx.content_rect().size();
            let corner = monitor - size - vec2(MARGIN, BOTTOM_MARGIN);
            ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(corner.to_pos2()));
            self.placed = true;
        }
        if self.opened.elapsed() >= LIFETIME {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        ctx.request_repaint_after(Duration::from_secs(1));

        let p = theme::palette(&ctx);
        let frame = egui::Frame::new()
            .fill(p.surface)
            .stroke(Stroke::new(1.5, p.ink));
        egui::CentralPanel::no_frame().frame(frame).show(ui, |ui| {
            let full = ui.max_rect();
            // Drag the popup anywhere.
            let drag = ui.interact(full, ui.id().with("drag"), Sense::drag());
            if drag.drag_started() {
                ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }

            // The engraving on the icon's jade, in both themes; the bust
            // rests on the bottom edge.
            let tile = egui::Rect::from_min_size(full.min, vec2(TILE, full.height()));
            ui.painter().rect_filled(tile, 0.0, JADE);
            let side = TILE.min(tile.height());
            let art = egui::Rect::from_min_size(
                egui::pos2(tile.center().x - side / 2.0, tile.bottom() - side),
                vec2(side, side),
            );
            egui::Image::new(egui::include_image!("../assets/thoth-engraving.png"))
                .paint_at(ui, art);
            ui.painter()
                .vline(tile.right(), tile.y_range(), Stroke::new(1.5, p.ink));

            let content = egui::Rect::from_min_max(
                egui::pos2(tile.right() + 18.0, full.top() + PADDING),
                egui::pos2(full.right() - 40.0, f32::INFINITY),
            );
            // Close, in the corner.
            let close = egui::Rect::from_min_size(
                egui::pos2(full.right() - 34.0, full.top() + 6.0),
                vec2(28.0, 28.0),
            );
            let close = ui
                .put(
                    close,
                    egui::Button::new(RichText::new("×").size(18.0).color(p.muted)).frame(false),
                )
                .on_hover_text(t!("reminder.close"));
            if close.clicked() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }

            let used = ui
                .scope_builder(egui::UiBuilder::new().max_rect(content), |ui| {
                    self.content(ui, &p);
                })
                .response
                .rect;
            let wanted = (used.bottom() - full.top() + PADDING).max(TILE);
            if self
                .height
                .is_none_or(|height| (height - wanted).abs() > 1.0)
            {
                self.height = Some(wanted);
                self.placed = false;
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(vec2(WIDTH, wanted)));
            }
        });
    }
}

impl ReminderPopup {
    fn content(&self, ui: &mut egui::Ui, p: &theme::Palette) {
        let ctx = ui.ctx().clone();
        ui.spacing_mut().item_spacing.y = 6.0;
        widgets::kicker(ui, &t!("app.name"), p.accent);
        ui.label(
            RichText::new(t!("reminder.title"))
                .family(theme::heading_family())
                .size(19.0)
                .color(p.text),
        );
        ui.label(RichText::new(t!("reminder.lead")).color(p.muted));
        for (name, percent) in &self.progress.subjects {
            ui.horizontal(|ui| {
                ui.allocate_ui_with_layout(
                    vec2(NAME_WIDTH, 18.0),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        ui.set_width(NAME_WIDTH);
                        ui.add(
                            egui::Label::new(RichText::new(name).small().strong().color(p.text))
                                .truncate(),
                        );
                    },
                );
                widgets::progress_line(
                    ui,
                    BAR_WIDTH,
                    *percent as f32 / 100.0,
                    &format!("{percent}%"),
                    p.accent,
                );
            });
        }
        if self.progress.more > 0 {
            ui.label(
                RichText::new(t!("reminder.more_subjects", count = self.progress.more))
                    .small()
                    .color(p.muted),
            );
        }
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if widgets::primary_button(ui, &t!("reminder.study_now")).clicked() {
                crate::instance::show_running();
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            let later = egui::Button::new(RichText::new(t!("reminder.later")).color(p.text))
                .fill(Color32::TRANSPARENT)
                .stroke(Stroke::new(1.2, p.ink));
            if ui.add(later).clicked() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        });
    }
}
