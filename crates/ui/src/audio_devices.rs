//! Microphone and output pickers, shared by Settings and the lesson.
//!
//! Listing devices asks the system and can take a moment, so the lists are
//! read once and again only when the student asks for it.

use egui::RichText;
use kg_engine::Command;
use rust_i18n::t;

use crate::screens::Actions;
use crate::theme;

#[derive(Debug, Clone, Default)]
struct Lists {
    inputs: Vec<String>,
    outputs: Vec<String>,
}

fn lists(ctx: &egui::Context, refresh: bool) -> Lists {
    let id = egui::Id::new("audio_device_lists");
    if !refresh && let Some(lists) = ctx.data(|d| d.get_temp::<Lists>(id)) {
        return lists;
    }
    // Tests and screenshots never show the devices of the computer they run on.
    let lists = if cfg!(test) {
        Lists::default()
    } else {
        Lists {
            inputs: kg_engine::input_devices(),
            outputs: kg_engine::output_devices(),
        }
    };
    ctx.data_mut(|d| d.insert_temp(id, lists.clone()));
    lists
}

/// Both pickers and a refresh button. A pick updates `input` or `output`
/// and is sent at once, so open recordings and the tutor's voice move to the
/// new device.
pub fn pickers(
    ui: &mut egui::Ui,
    input: &mut Option<String>,
    output: &mut Option<String>,
    actions: &mut Actions,
) {
    let refresh = ui.ctx().data(|d| {
        d.get_temp::<bool>(egui::Id::new("audio_device_refresh"))
            .unwrap_or(false)
    });
    let found = lists(ui.ctx(), refresh);
    pick_rows(ui, input, output, &found, actions);
    footer(ui);
}

/// The same pickers as plain lists, for inside a popup: a drop-down there
/// would open a popup in a popup, which egui does not show.
pub fn lists_inline(
    ui: &mut egui::Ui,
    input: &mut Option<String>,
    output: &mut Option<String>,
    actions: &mut Actions,
) {
    let refresh = ui.ctx().data(|d| {
        d.get_temp::<bool>(egui::Id::new("audio_device_refresh"))
            .unwrap_or(false)
    });
    let found = lists(ui.ctx(), refresh);
    let list = |ui: &mut egui::Ui,
                salt: &str,
                title: String,
                current: &Option<String>,
                devices: &[String]| {
        ui.label(RichText::new(title).strong());
        let mut picked = current.clone();
        egui::ScrollArea::vertical()
            .id_salt(salt)
            .max_height(150.0)
            .show(ui, |ui| {
                for (choice, label) in choices(current, devices) {
                    ui.selectable_value(&mut picked, choice, label);
                }
            });
        ui.add_space(6.0);
        (picked != *current).then_some(picked)
    };
    if let Some(pick) = list(
        ui,
        "lesson_input_devices",
        t!("audio_devices.microphone").into_owned(),
        input,
        &found.inputs,
    ) {
        *input = pick.clone();
        actions.send(Command::SetInputDevice(pick));
    }
    if let Some(pick) = list(
        ui,
        "lesson_output_devices",
        t!("audio_devices.output").into_owned(),
        output,
        &found.outputs,
    ) {
        *output = pick.clone();
        actions.send(Command::SetOutputDevice(pick));
    }
    footer(ui);
}

/// The choices of one list: the system default, a chosen device that is not
/// connected now, and the connected ones.
fn choices(current: &Option<String>, devices: &[String]) -> Vec<(Option<String>, String)> {
    let mut out = vec![(None, t!("audio_devices.system_default").into_owned())];
    if let Some(name) = current
        && !devices.contains(name)
    {
        out.push((
            Some(name.clone()),
            t!("audio_devices.not_connected", name = name).into_owned(),
        ));
    }
    out.extend(
        devices
            .iter()
            .map(|name| (Some(name.clone()), name.clone())),
    );
    out
}

fn footer(ui: &mut egui::Ui) {
    let clicked = ui.button(t!("audio_devices.refresh")).clicked();
    ui.ctx()
        .data_mut(|d| d.insert_temp(egui::Id::new("audio_device_refresh"), clicked));
    ui.label(
        RichText::new(t!("audio_devices.hint"))
            .small()
            .color(theme::palette(ui.ctx()).muted),
    );
}

fn pick_rows(
    ui: &mut egui::Ui,
    input: &mut Option<String>,
    output: &mut Option<String>,
    found: &Lists,
    actions: &mut Actions,
) {
    egui::Grid::new(ui.id().with("audio_devices"))
        .num_columns(2)
        .spacing([12.0, 8.0])
        .show(ui, |ui| {
            ui.label(t!("audio_devices.microphone"));
            if let Some(pick) = picker(ui, "audio_input", input, &found.inputs) {
                *input = pick.clone();
                actions.send(Command::SetInputDevice(pick));
            }
            ui.end_row();
            ui.label(t!("audio_devices.output"));
            if let Some(pick) = picker(ui, "audio_output", output, &found.outputs) {
                *output = pick.clone();
                actions.send(Command::SetOutputDevice(pick));
            }
            ui.end_row();
        });
}

/// One device drop-down; returns the new choice when it changed.
fn picker(
    ui: &mut egui::Ui,
    salt: &str,
    current: &Option<String>,
    devices: &[String],
) -> Option<Option<String>> {
    let choices = choices(current, devices);
    let selected = choices
        .iter()
        .find(|(choice, _)| choice == current)
        .map(|(_, label)| label.clone())
        .unwrap_or_default();
    let mut picked = current.clone();
    egui::ComboBox::from_id_salt(salt)
        .selected_text(selected)
        .width(280.0)
        .show_ui(ui, |ui| {
            for (choice, label) in choices {
                ui.selectable_value(&mut picked, choice, label);
            }
        });
    (picked != *current).then_some(picked)
}
