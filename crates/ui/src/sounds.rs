//! Interface sounds. Screens ask for meaningful cues (a right answer, a
//! finished quiz); clicks and hover ticks are found here, once per frame, for
//! every widget, so no screen has to remember them.

use egui::output::OutputEvent;
use egui::{Context, Id};
use kg_engine::{Cue, Engine};

/// Least time between two hover ticks, so sweeping over a list stays calm.
const HOVER_GAP: f64 = 0.07;

fn queue_id() -> Id {
    Id::new("kg_sound_queue")
}

/// Asks for `cue` at the end of this frame. A cue asked for replaces the
/// plain click of the same frame.
pub fn play(ctx: &Context, cue: Cue) {
    push(ctx, cue, None);
}

/// Like [`play`], at `volume` instead of the saved one: a preview in Settings.
pub fn preview(ctx: &Context, cue: Cue, volume: f32) {
    push(ctx, cue, Some(volume));
}

fn push(ctx: &Context, cue: Cue, volume: Option<f32>) {
    ctx.data_mut(|d| {
        d.get_temp_mut_or_default::<Vec<(Cue, Option<f32>)>>(queue_id())
            .push((cue, volume));
    });
}

/// The sound of a plain click, by the kind of widget. Sidebar and main
/// buttons ask for their own sounds where they are drawn.
fn click_cue(info: &egui::WidgetInfo) -> Cue {
    use egui::WidgetType;
    match info.typ {
        WidgetType::Checkbox => {
            if info.selected == Some(false) {
                Cue::ToggleOff
            } else {
                Cue::ToggleOn
            }
        }
        WidgetType::RadioButton
        | WidgetType::RadioGroup
        | WidgetType::SelectableLabel
        | WidgetType::ComboBox
        | WidgetType::CollapsingHeader => Cue::ToggleOn,
        _ => Cue::Click,
    }
}

/// What the pointer was on, to tick only when it reaches something new.
#[derive(Debug, Default)]
pub struct SoundState {
    hovered: Option<Id>,
    last_hover: f64,
}

impl SoundState {
    /// Plays this frame's sounds. Call once, after everything was drawn.
    pub fn end_frame(&mut self, ctx: &Context, engine: Option<&Engine>) {
        let mut cues = ctx
            .data_mut(|d| d.remove_temp::<Vec<(Cue, Option<f32>)>>(queue_id()))
            .unwrap_or_default();
        let hovered = self.clickable_under_pointer(ctx);
        let now = ctx.input(|i| i.time);
        let new_hover = hovered.is_some() && hovered != self.hovered;
        self.hovered = hovered;

        // Screenshots in tests stay silent.
        let Some(engine) = engine.filter(|_| !cfg!(test)) else {
            return;
        };
        if cues.is_empty() {
            let clicked = ctx.output(|o| {
                o.events.iter().find_map(|e| match e {
                    OutputEvent::Clicked(info) => Some(click_cue(info)),
                    _ => None,
                })
            });
            if let Some(cue) = clicked {
                cues.push((cue, None));
            } else if new_hover && now - self.last_hover > HOVER_GAP {
                self.last_hover = now;
                cues.push((Cue::Hover, None));
            }
        }
        cues.dedup();
        for (cue, volume) in cues {
            match volume {
                Some(volume) => engine.play_cue_at(cue, volume),
                None => engine.play_cue(cue),
            }
        }
    }

    /// The enabled button, link or box under the pointer. Text that can be
    /// selected and sliders also take clicks, but they also drag; those stay
    /// quiet.
    fn clickable_under_pointer(&self, ctx: &Context) -> Option<Id> {
        if ctx.input(|i| i.pointer.any_down()) {
            return self.hovered;
        }
        let hovered: Vec<Id> = ctx.interaction_snapshot(|s| s.hovered.iter().copied().collect());
        hovered.into_iter().find(|&id| {
            ctx.read_response(id)
                .is_some_and(|r| r.enabled() && r.sense.senses_click() && !r.sense.senses_drag())
        })
    }
}
