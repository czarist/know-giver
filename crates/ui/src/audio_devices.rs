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
    let clicked = ui.button(t!("audio_devices.refresh")).clicked();
    ui.ctx()
        .data_mut(|d| d.insert_temp(egui::Id::new("audio_device_refresh"), clicked));
    ui.label(
        RichText::new(t!("audio_devices.hint"))
            .small()
            .color(theme::palette(ui.ctx()).muted),
    );
}

/// One device list; returns the new choice when it changed.
fn picker(
    ui: &mut egui::Ui,
    salt: &str,
    current: &Option<String>,
    devices: &[String],
) -> Option<Option<String>> {
    let label = |name: &Option<String>| match name {
        None => t!("audio_devices.system_default").to_string(),
        Some(name) if devices.contains(name) => name.clone(),
        Some(name) => t!("audio_devices.not_connected", name = name).to_string(),
    };
    let mut picked = current.clone();
    egui::ComboBox::from_id_salt(salt)
        .selected_text(label(current))
        .width(280.0)
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut picked, None, label(&None));
            if let Some(name) = current
                && !devices.contains(name)
            {
                ui.selectable_value(&mut picked, Some(name.clone()), label(current));
            }
            for name in devices {
                ui.selectable_value(&mut picked, Some(name.clone()), name.as_str());
            }
        });
    (picked != *current).then_some(picked)
}
