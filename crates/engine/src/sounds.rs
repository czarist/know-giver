//! Interface sounds, played as the student asked in Settings.

use kg_audio::Cue;

use crate::Engine;

impl Engine {
    /// Plays an interface sound at the saved volume, unless sounds or its
    /// group are off, or the microphone is taking the student's words (the
    /// cue would reach the transcription).
    pub fn play_cue(&self, cue: Cue) {
        if self.settings.ui_sounds && !self.settings.muted_sound_groups.contains(&cue.group()) {
            self.play_cue_at(cue, self.settings.ui_sound_volume);
        }
    }

    /// Plays an interface sound at `volume`, e.g. to preview a volume not
    /// saved yet. Still silent while the microphone takes
    /// the student's words.
    pub fn play_cue_at(&self, cue: Cue, volume: f32) {
        if !self.mic_listening() {
            self.cues.play(cue, volume);
        }
    }

    fn mic_listening(&self) -> bool {
        self.lesson.as_ref().is_some_and(|l| l.listening())
            || self.review.as_ref().is_some_and(|r| r.listening())
    }
}
