//! Short interface sounds: a soft tick on hover, a pop on click, chimes for
//! right and wrong answers and a little fanfare when something is finished.
//!
//! Every sound is synthesized here (bell-like sine partials with an
//! envelope), so there are no sound files to ship or license. They play on
//! their own output stream, mixed over speech instead of queued behind it.

use std::num::NonZero;
use std::sync::OnceLock;
use std::sync::mpsc;

use rodio::Source;

use crate::AudioClip;

/// Sample rate the cues are rendered at.
const CUE_RATE: u32 = 44_100;

/// A sound the interface can make.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Cue {
    /// The pointer reached something clickable.
    Hover,
    /// An ordinary button or link was clicked.
    Click,
    /// The main button of a screen was clicked.
    Confirm,
    /// A screen was picked in the sidebar.
    Navigate,
    /// A box was ticked, an option picked or a list opened.
    ToggleOn,
    /// A box was unticked.
    ToggleOff,
    /// A right answer.
    Correct,
    /// A wrong answer; low and short, never harsh.
    Wrong,
    /// A quiz, review or lesson came to its end.
    Complete,
    /// A challenge sticker was won.
    Unlock,
}

/// Cues the student can silence together in Settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CueGroup {
    Hover,
    Buttons,
    Navigation,
    Toggles,
    Answers,
    Celebrations,
}

impl CueGroup {
    pub const ALL: [CueGroup; 6] = [
        CueGroup::Hover,
        CueGroup::Buttons,
        CueGroup::Navigation,
        CueGroup::Toggles,
        CueGroup::Answers,
        CueGroup::Celebrations,
    ];

    /// A cue that shows how the group sounds.
    pub fn sample(self) -> Cue {
        match self {
            CueGroup::Hover => Cue::Hover,
            CueGroup::Buttons => Cue::Confirm,
            CueGroup::Navigation => Cue::Navigate,
            CueGroup::Toggles => Cue::ToggleOn,
            CueGroup::Answers => Cue::Correct,
            CueGroup::Celebrations => Cue::Complete,
        }
    }
}

impl Cue {
    pub const ALL: [Cue; 10] = [
        Cue::Hover,
        Cue::Click,
        Cue::Confirm,
        Cue::Navigate,
        Cue::ToggleOn,
        Cue::ToggleOff,
        Cue::Correct,
        Cue::Wrong,
        Cue::Complete,
        Cue::Unlock,
    ];

    pub fn group(self) -> CueGroup {
        match self {
            Cue::Hover => CueGroup::Hover,
            Cue::Click | Cue::Confirm => CueGroup::Buttons,
            Cue::Navigate => CueGroup::Navigation,
            Cue::ToggleOn | Cue::ToggleOff => CueGroup::Toggles,
            Cue::Correct | Cue::Wrong => CueGroup::Answers,
            Cue::Complete | Cue::Unlock => CueGroup::Celebrations,
        }
    }

    /// Loudness against the other cues: hover stays barely audible.
    fn level(self) -> f32 {
        match self {
            Cue::Hover => 0.09,
            Cue::Click | Cue::Navigate | Cue::ToggleOn | Cue::ToggleOff => 0.24,
            Cue::Confirm => 0.28,
            Cue::Correct | Cue::Wrong => 0.38,
            Cue::Complete | Cue::Unlock => 0.42,
        }
    }

    /// Renders the cue as mono audio: low, rounded tones through a soft
    /// filter, with a short echo, as on a game console's menus.
    pub fn render(self) -> AudioClip {
        let notes: &[Note] = match self {
            Cue::Hover => &[Note::new(0.0, 740.0, 0.004, 0.014, 0.05)],
            // A wooden "thock": a falling tone over a deeper body.
            Cue::Click => &[
                Note::new(0.0, 330.0, 0.002, 0.030, 0.25).glide(-0.25),
                Note::new(0.0, 165.0, 0.002, 0.040, 0.10).gain(0.7),
            ],
            // A soft rising bubble: something happens.
            Cue::Confirm => &[
                Note::new(0.000, G4, 0.003, 0.050, 0.20),
                Note::new(0.035, D5, 0.003, 0.090, 0.20),
                Note::new(0.000, 196.0, 0.003, 0.050, 0.05).gain(0.5),
            ],
            // A light swish up: moving somewhere else.
            Cue::Navigate => &[
                Note::new(0.0, C5, 0.006, 0.045, 0.10).glide(0.15),
                Note::new(0.0, 262.0, 0.006, 0.045, 0.05)
                    .glide(0.15)
                    .gain(0.6),
            ],
            Cue::ToggleOn => &[
                Note::new(0.000, A4, 0.002, 0.025, 0.15),
                Note::new(0.045, E5, 0.002, 0.035, 0.15),
            ],
            Cue::ToggleOff => &[
                Note::new(0.000, 520.0, 0.002, 0.025, 0.15),
                Note::new(0.045, 350.0, 0.002, 0.035, 0.15),
            ],
            // A rising fourth.
            Cue::Correct => &[
                Note::new(0.000, E5, 0.004, 0.090, 0.20),
                Note::new(0.085, A5, 0.004, 0.260, 0.20),
            ],
            // A falling major third, low and muted.
            Cue::Wrong => &[
                Note::new(0.000, 262.0, 0.010, 0.110, 0.06).glide(-0.03),
                Note::new(0.120, 208.0, 0.010, 0.220, 0.06).glide(-0.05),
            ],
            // An arpeggio up to the octave that rings out.
            Cue::Complete => &[
                Note::new(0.000, C5, 0.004, 0.120, 0.20),
                Note::new(0.075, E5, 0.004, 0.120, 0.20),
                Note::new(0.150, G5, 0.004, 0.140, 0.20),
                Note::new(0.225, C6, 0.004, 0.320, 0.20),
                Note::new(0.225, G5, 0.004, 0.280, 0.10).gain(0.4),
            ],
            // A fuller fanfare with a gentle sparkle on top.
            Cue::Unlock => &[
                Note::new(0.000, G4, 0.004, 0.110, 0.20),
                Note::new(0.070, C5, 0.004, 0.110, 0.20),
                Note::new(0.140, E5, 0.004, 0.110, 0.20),
                Note::new(0.210, G5, 0.004, 0.160, 0.20),
                Note::new(0.300, C6, 0.004, 0.360, 0.20),
                Note::new(0.300, E5, 0.004, 0.300, 0.10).gain(0.45),
                Note::new(0.380, 1_568.0, 0.003, 0.090, 0.05).gain(0.18),
                Note::new(0.450, 1_760.0, 0.003, 0.090, 0.05).gain(0.15),
                Note::new(0.520, 2_093.0, 0.003, 0.120, 0.05).gain(0.12),
            ],
        };
        let mut clip = mix(notes);
        soften(&mut clip.samples);
        echo(&mut clip.samples);
        fade_out(&mut clip.samples);
        normalize(&mut clip.samples, self.level());
        clip
    }
}

const G4: f32 = 392.0;
const A4: f32 = 440.0;
const C5: f32 = 523.25;
const D5: f32 = 587.33;
const E5: f32 = 659.25;
const G5: f32 = 783.99;
const A5: f32 = 880.0;
const C6: f32 = 1_046.5;

/// Room left after the last note for the echo.
const ECHO_TAIL: f32 = 0.12;

/// One struck tone: a sine with two fading overtones.
#[derive(Debug, Clone, Copy)]
struct Note {
    /// Start, in seconds from the beginning of the cue.
    at: f32,
    freq: f32,
    attack: f32,
    /// Time for the tone to fall to about a third.
    decay: f32,
    /// Strength of the overtones, `0.0..=1.0`.
    bright: f32,
    gain: f32,
    /// Pitch change over the note, as a fraction (−0.3 falls by 30%).
    glide: f32,
}

impl Note {
    const fn new(at: f32, freq: f32, attack: f32, decay: f32, bright: f32) -> Self {
        Self {
            at,
            freq,
            attack,
            decay,
            bright,
            gain: 1.0,
            glide: 0.0,
        }
    }

    const fn gain(mut self, gain: f32) -> Self {
        self.gain = gain;
        self
    }

    const fn glide(mut self, glide: f32) -> Self {
        self.glide = glide;
        self
    }

    /// Length until the tone is inaudible.
    fn length(&self) -> f32 {
        self.attack + self.decay * 4.0
    }
}

fn mix(notes: &[Note]) -> AudioClip {
    let rate = CUE_RATE as f32;
    let end = notes
        .iter()
        .map(|n| n.at + n.length())
        .fold(0.0_f32, f32::max);
    let mut samples = vec![0.0_f32; ((end + ECHO_TAIL) * rate).ceil() as usize + 1];
    for note in notes {
        let start = (note.at * rate) as usize;
        let count = (note.length() * rate) as usize;
        let mut phase = 0.0_f32;
        for i in 0..count {
            let Some(out) = samples.get_mut(start + i) else {
                break;
            };
            let t = i as f32 / rate;
            let progress = t / note.length();
            let freq = note.freq * (1.0 + note.glide * progress);
            phase += std::f32::consts::TAU * freq / rate;
            let envelope = if t < note.attack {
                t / note.attack
            } else {
                (-(t - note.attack) / note.decay).exp()
            };
            // Overtones fade faster than the fundamental, like a struck bar.
            let shine = (-(t / (note.decay * 0.5))).exp();
            let tone = phase.sin()
                + note.bright * 0.5 * shine * (2.0 * phase).sin()
                + note.bright * 0.2 * shine * (3.0 * phase).sin();
            *out += tone * envelope * note.gain;
        }
    }
    AudioClip {
        samples,
        sample_rate: CUE_RATE,
    }
}

/// A gentle low-pass, so nothing sounds sharp.
fn soften(samples: &mut [f32]) {
    let cutoff = 3_200.0_f32;
    let a = 1.0 - (-std::f32::consts::TAU * cutoff / CUE_RATE as f32).exp();
    let mut y = 0.0_f32;
    for sample in samples.iter_mut() {
        y += a * (*sample - y);
        *sample = y;
    }
}

/// Two faint repeats, like a small room.
fn echo(samples: &mut [f32]) {
    let dry = samples.to_vec();
    for (delay, gain) in [(0.055_f32, 0.22_f32), (0.110, 0.08)] {
        let offset = (delay * CUE_RATE as f32) as usize;
        for (i, value) in dry.iter().enumerate() {
            if let Some(out) = samples.get_mut(i + offset) {
                *out += value * gain;
            }
        }
    }
}

/// A few milliseconds of fade at the end, so no cue ends in a click.
fn fade_out(samples: &mut [f32]) {
    let fade = (CUE_RATE as f32 * 0.004) as usize;
    let len = samples.len();
    for (i, sample) in samples
        .iter_mut()
        .skip(len.saturating_sub(fade))
        .enumerate()
    {
        *sample *= 1.0 - i as f32 / fade.max(1) as f32;
    }
}

fn normalize(samples: &mut [f32], peak: f32) {
    let max = samples.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
    if max > 0.0 {
        let scale = peak / max;
        samples.iter_mut().for_each(|s| *s *= scale);
    }
}

/// Plays [`Cue`]s on the default output device. The device opens on the first
/// cue, on its own thread; when there is none, cues are silently dropped.
#[derive(Debug, Default)]
pub struct CuePlayer {
    tx: OnceLock<Option<mpsc::Sender<(Cue, f32)>>>,
}

impl CuePlayer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Plays `cue` at `volume` (`0.0..=1.0`) over whatever already plays.
    pub fn play(&self, cue: Cue, volume: f32) {
        let volume = volume.clamp(0.0, 1.0);
        if volume <= 0.0 {
            return;
        }
        if let Some(tx) = self.tx.get_or_init(open) {
            let _ = tx.send((cue, volume));
        }
    }
}

fn open() -> Option<mpsc::Sender<(Cue, f32)>> {
    let (tx, rx) = mpsc::channel::<(Cue, f32)>();
    let spawned = std::thread::Builder::new()
        .name("kg-cues".into())
        .spawn(move || cue_thread(rx));
    match spawned {
        Ok(_) => Some(tx),
        Err(err) => {
            tracing::warn!(%err, "interface sounds are unavailable");
            None
        }
    }
}

fn cue_thread(rx: mpsc::Receiver<(Cue, f32)>) {
    let mut sink = match rodio::DeviceSinkBuilder::open_default_sink() {
        Ok(sink) => sink,
        Err(err) => {
            tracing::warn!(%err, "no output device for interface sounds");
            return;
        }
    };
    sink.log_on_drop(false);
    let clips: Vec<(Cue, AudioClip)> = Cue::ALL.iter().map(|&c| (c, c.render())).collect();
    let (Some(channels), Some(rate)) = (NonZero::new(1u16), NonZero::new(CUE_RATE)) else {
        return;
    };
    while let Ok((cue, volume)) = rx.recv() {
        let Some((_, clip)) = clips.iter().find(|(c, _)| *c == cue) else {
            continue;
        };
        let source = rodio::buffer::SamplesBuffer::new(channels, rate, clip.samples.clone());
        sink.mixer().add(source.amplify(volume));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_cue_is_short_and_within_its_level() {
        for cue in Cue::ALL {
            let clip = cue.render();
            assert!(clip.duration_secs() > 0.01, "{cue:?} is empty");
            assert!(clip.duration_secs() < 2.0, "{cue:?} is too long");
            let peak = clip.samples.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
            assert!((peak - cue.level()).abs() < 1e-3, "{cue:?} peak {peak}");
            assert!(clip.samples.iter().all(|s| s.is_finite()));
        }
    }

    #[test]
    fn hover_is_the_quietest_and_shortest() {
        let hover = Cue::Hover.render();
        for cue in Cue::ALL.into_iter().filter(|c| *c != Cue::Hover) {
            let clip = cue.render();
            assert!(hover.duration_secs() <= clip.duration_secs());
            assert!(Cue::Hover.level() < cue.level());
        }
    }

    #[test]
    fn cues_end_in_silence() {
        for cue in Cue::ALL {
            let clip = cue.render();
            let last = clip.samples.last().copied().unwrap_or(1.0);
            assert!(last.abs() < 1e-3, "{cue:?} ends at {last}");
        }
    }

    #[test]
    fn nothing_is_shrill() {
        // Most energy sits low: the average sample-to-sample change (a rough
        // measure of high frequencies) stays small against the loudness.
        for cue in Cue::ALL {
            let clip = cue.render();
            let diff: f32 = clip.samples.windows(2).map(|w| (w[1] - w[0]).abs()).sum();
            let level: f32 = clip.samples.iter().map(|s| s.abs()).sum();
            assert!(diff / level.max(1e-6) < 0.25, "{cue:?} is too bright");
        }
    }

    #[test]
    fn every_group_has_a_sample_of_its_own() {
        for group in CueGroup::ALL {
            assert_eq!(group.sample().group(), group);
        }
    }
}
