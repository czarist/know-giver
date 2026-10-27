//! Voice lessons: listen → transcribe → tutor → speak, in a loop.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use kg_ai::{AiError, ChatMessage, TopicContext};
use kg_audio::{
    AudioClip, AudioError, ModelId, ModelStore, PiperTts, RecordOptions, Recording, WhisperModel,
    WhisperStt, split_for_speech,
};
use kg_study::{TopicId, TopicStatus};

use crate::{Engine, EngineError, Event, Notice};

/// Language code passed to Whisper.
const STT_LANGUAGE: &str = "pt";
/// Turns of conversation sent back to the tutor.
const HISTORY_TURNS: usize = 24;
/// In hands-free mode, stop waiting if the student says nothing.
const HANDS_FREE_IDLE: Duration = Duration::from_secs(12);
const SILENCE_TO_STOP: Duration = Duration::from_millis(1_400);
/// Shorter clips are treated as accidental clicks.
const MIN_CLIP_SECS: f32 = 0.4;

/// What the voice lesson is doing right now. Drives the UI's listening and
/// speaking feedback.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LessonPhase {
    #[default]
    Idle,
    Listening,
    Transcribing,
    Thinking,
    Speaking,
}

/// Who said a line in the lesson.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Speaker {
    Tutor,
    Student,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LessonTurn {
    pub speaker: Speaker,
    pub text: String,
}

/// An ongoing lesson about one topic.
#[derive(Debug)]
pub struct LessonSession {
    pub id: u64,
    pub topic_id: TopicId,
    pub topic_title: String,
    pub subject: String,
    pub turns: Vec<LessonTurn>,
    pub phase: LessonPhase,
    context: TopicContext,
    recording: Option<Recording>,
    speech_job: Option<Arc<AtomicBool>>,
}

impl LessonSession {
    /// Microphone level while listening, `0.0..=1.0`.
    pub fn input_level(&self) -> f32 {
        self.recording
            .as_ref()
            .map_or(0.0, |r| (r.level() * 6.0).min(1.0))
    }

    fn history(&self) -> Vec<ChatMessage> {
        let skip = self.turns.len().saturating_sub(HISTORY_TURNS);
        self.turns
            .iter()
            .skip(skip)
            .map(|turn| match turn.speaker {
                Speaker::Tutor => ChatMessage::assistant(turn.text.clone()),
                Speaker::Student => ChatMessage::user(turn.text.clone()),
            })
            .collect()
    }
}

pub(crate) enum LessonEvent {
    Reply {
        session: u64,
        result: Result<String, AiError>,
    },
    Transcribed {
        session: u64,
        result: Result<String, EngineError>,
    },
    SpeechFailed {
        session: u64,
        error: AudioError,
    },
}

/// Audio output and TTS, created on first use.
#[derive(Default)]
pub(crate) struct VoiceRuntime {
    speaker: Option<kg_audio::Speaker>,
    tts: Option<PiperTts>,
    /// Bumped to cancel speech that is still being synthesized.
    generation: Arc<AtomicU64>,
}

impl VoiceRuntime {
    /// Forgets the TTS so it is rebuilt with new settings or files.
    pub(crate) fn reset_tts(&mut self) {
        self.tts = None;
    }
}

impl Engine {
    pub(crate) fn start_lesson(&mut self, topic_id: TopicId) -> Result<(), EngineError> {
        if self.lesson.is_some() {
            self.end_lesson()?;
        }
        let (context, topic_title) = self.topic_context(topic_id)?;
        self.update_topic(topic_id, |t| {
            if t.status == TopicStatus::NotStarted {
                t.status = TopicStatus::Studying;
            }
        })?;
        let id = self.next_id();
        let client = self.ai_client();
        let ctx = context.clone();
        self.lesson = Some(LessonSession {
            id,
            topic_id,
            topic_title,
            subject: context.subject.clone(),
            turns: Vec::new(),
            phase: LessonPhase::Thinking,
            context,
            recording: None,
            speech_job: None,
        });
        self.spawn(move || {
            Event::Lesson(LessonEvent::Reply {
                session: id,
                result: kg_ai::start_lesson(&client, &ctx),
            })
        });
        self.preload_stt();
        Ok(())
    }

    pub(crate) fn end_lesson(&mut self) -> Result<(), EngineError> {
        self.stop_audio();
        let Some(lesson) = self.lesson.take() else {
            return Ok(());
        };
        let student_spoke = lesson.turns.iter().any(|t| t.speaker == Speaker::Student);
        if student_spoke {
            let today = Self::today();
            self.store.record_lesson(lesson.topic_id, today)?;
            self.update_topic(lesson.topic_id, |t| t.complete_lesson(today))?;
            self.notices.push(Notice::LessonSaved);
        }
        Ok(())
    }

    pub(crate) fn start_listening(&mut self) -> Result<(), EngineError> {
        self.listen(None)
    }

    fn listen(&mut self, idle_timeout: Option<Duration>) -> Result<(), EngineError> {
        let model = self.settings.whisper_model;
        if !self.stt_ready() {
            return Err(EngineError::SpeechModelMissing(ModelId::Whisper(model)));
        }
        self.stop_speaking();
        let options = RecordOptions {
            auto_stop_after: self.settings.auto_stop.then_some(SILENCE_TO_STOP),
            idle_timeout,
            ..RecordOptions::default()
        };
        let Some(lesson) = self.lesson.as_mut() else {
            return Ok(());
        };
        if matches!(
            lesson.phase,
            LessonPhase::Thinking | LessonPhase::Transcribing
        ) {
            return Ok(());
        }
        lesson.recording = Some(Recording::start(options)?);
        lesson.phase = LessonPhase::Listening;
        Ok(())
    }

    pub(crate) fn stop_listening(&mut self) {
        let Some(lesson) = self.lesson.as_mut() else {
            return;
        };
        let Some(recording) = lesson.recording.take() else {
            return;
        };
        let stopped_by_itself = recording.is_finished();
        let heard = recording.heard_speech();
        let clip = recording.finish();
        if !heard || clip.duration_secs() < MIN_CLIP_SECS {
            lesson.phase = LessonPhase::Idle;
            if !stopped_by_itself {
                self.notices.push(Notice::NothingHeard);
            }
            return;
        }
        lesson.phase = LessonPhase::Transcribing;
        let session = lesson.id;
        let stt = Arc::clone(&self.stt);
        let models = self.models.clone();
        let model = self.settings.whisper_model;
        self.spawn(move || {
            Event::Lesson(LessonEvent::Transcribed {
                session,
                result: transcribe(&stt, &models, model, &clip),
            })
        });
    }

    pub(crate) fn send_student_message(&mut self, text: String) {
        let text = text.trim().to_owned();
        if text.is_empty() {
            return;
        }
        self.stop_speaking();
        let Some(lesson) = self.lesson.as_mut() else {
            return;
        };
        if let Some(recording) = lesson.recording.take() {
            drop(recording.finish());
        }
        lesson.turns.push(LessonTurn {
            speaker: Speaker::Student,
            text,
        });
        lesson.phase = LessonPhase::Thinking;
        let session = lesson.id;
        let ctx = lesson.context.clone();
        let history = lesson.history();
        let client = self.ai_client();
        self.spawn(move || {
            Event::Lesson(LessonEvent::Reply {
                session,
                result: kg_ai::tutor_reply(&client, &ctx, &history),
            })
        });
    }

    pub(crate) fn stop_speaking(&mut self) {
        self.voice.generation.fetch_add(1, Ordering::Relaxed);
        if let Some(speaker) = &self.voice.speaker {
            speaker.stop();
        }
        if let Some(lesson) = self.lesson.as_mut()
            && lesson.phase == LessonPhase::Speaking
        {
            lesson.phase = LessonPhase::Idle;
            lesson.speech_job = None;
        }
    }

    /// Stops all lesson audio: speech and microphone.
    pub(crate) fn stop_audio(&mut self) {
        self.stop_speaking();
        if let Some(lesson) = self.lesson.as_mut()
            && let Some(recording) = lesson.recording.take()
        {
            drop(recording.finish());
            lesson.phase = LessonPhase::Idle;
        }
    }

    /// Per-frame lesson upkeep: end of recording and end of speech.
    pub(crate) fn tick_lesson(&mut self) {
        let Some(lesson) = self.lesson.as_mut() else {
            return;
        };
        match lesson.phase {
            LessonPhase::Listening
                if lesson
                    .recording
                    .as_ref()
                    .is_some_and(Recording::is_finished) =>
            {
                self.stop_listening();
            }
            LessonPhase::Speaking => {
                let synthesizing = lesson
                    .speech_job
                    .as_ref()
                    .is_some_and(|job| job.load(Ordering::Relaxed));
                let playing = self.voice.speaker.as_ref().is_some_and(|s| s.is_playing());
                if !synthesizing && !playing {
                    lesson.phase = LessonPhase::Idle;
                    lesson.speech_job = None;
                    if self.settings.hands_free
                        && let Err(err) = self.listen(Some(HANDS_FREE_IDLE))
                    {
                        self.notices.push(Notice::Error(err));
                    }
                }
            }
            _ => {}
        }
    }

    pub(crate) fn on_lesson_event(&mut self, event: LessonEvent) -> Result<(), EngineError> {
        let current = self.lesson.as_ref().map(|l| l.id);
        match event {
            LessonEvent::Reply { session, result } if Some(session) == current => {
                let text = match result {
                    Ok(text) => text,
                    Err(err) => {
                        self.set_phase(LessonPhase::Idle);
                        return Err(err.into());
                    }
                };
                if let Some(lesson) = self.lesson.as_mut() {
                    lesson.turns.push(LessonTurn {
                        speaker: Speaker::Tutor,
                        text: text.clone(),
                    });
                }
                if self.settings.voice_enabled && self.tts_ready() {
                    if let Err(err) = self.speak(text) {
                        // The answer is on screen; only the voice failed.
                        self.set_phase(LessonPhase::Idle);
                        return Err(err);
                    }
                } else {
                    self.set_phase(LessonPhase::Idle);
                }
            }
            LessonEvent::Transcribed { session, result } if Some(session) == current => {
                match result {
                    Ok(text) if !text.trim().is_empty() => self.send_student_message(text),
                    Ok(_) => {
                        self.set_phase(LessonPhase::Idle);
                        self.notices.push(Notice::NothingHeard);
                    }
                    Err(err) => {
                        self.set_phase(LessonPhase::Idle);
                        return Err(err);
                    }
                }
            }
            LessonEvent::SpeechFailed { session, error } if Some(session) == current => {
                self.stop_speaking();
                return Err(error.into());
            }
            // Results for a lesson that already ended.
            _ => {}
        }
        Ok(())
    }

    fn set_phase(&mut self, phase: LessonPhase) {
        if let Some(lesson) = self.lesson.as_mut() {
            lesson.phase = phase;
        }
    }

    /// Speaks `text` sentence by sentence, so playback starts early.
    fn speak(&mut self, text: String) -> Result<(), EngineError> {
        let tts = self.tts()?;
        if self.voice.speaker.is_none() {
            self.voice.speaker = Some(kg_audio::Speaker::open()?);
        }
        let Some(speaker) = self.voice.speaker.clone() else {
            return Ok(());
        };
        let Some(lesson) = self.lesson.as_mut() else {
            return Ok(());
        };
        let generation = self.voice.generation.fetch_add(1, Ordering::Relaxed) + 1;
        let current_generation = Arc::clone(&self.voice.generation);
        let job = Arc::new(AtomicBool::new(true));
        lesson.speech_job = Some(Arc::clone(&job));
        lesson.phase = LessonPhase::Speaking;
        let session = lesson.id;
        let (tx, waker) = self.sender();

        let spawned = std::thread::Builder::new()
            .name("kg-speech".into())
            .spawn(move || {
                for chunk in split_for_speech(&text) {
                    if current_generation.load(Ordering::Relaxed) != generation {
                        break;
                    }
                    match tts.synthesize(&chunk) {
                        Ok(clip) if current_generation.load(Ordering::Relaxed) == generation => {
                            speaker.play(clip);
                        }
                        Ok(_) => break,
                        Err(error) => {
                            let _ = tx
                                .send(Event::Lesson(LessonEvent::SpeechFailed { session, error }));
                            break;
                        }
                    }
                }
                job.store(false, Ordering::Relaxed);
                waker();
            });
        if let Err(err) = spawned {
            tracing::error!(%err, "could not spawn speech thread");
            self.set_phase(LessonPhase::Idle);
        }
        Ok(())
    }

    fn tts(&mut self) -> Result<PiperTts, EngineError> {
        if let Some(tts) = &self.voice.tts {
            return Ok(tts.clone());
        }
        let voice = self.settings.voice;
        let exe = self
            .models
            .piper_exe()
            .ok_or(EngineError::SpeechModelMissing(ModelId::PiperRuntime))?;
        let (onnx, json) = self.models.voice_files(voice);
        if !onnx.is_file() {
            return Err(EngineError::SpeechModelMissing(ModelId::Voice(voice)));
        }
        let length_scale = 1.0 / self.settings.speech_rate.max(0.1);
        let tts = PiperTts::new(exe, onnx, &json, length_scale)?;
        self.voice.tts = Some(tts.clone());
        Ok(tts)
    }

    /// Loads Whisper in the background so the first answer is not delayed.
    fn preload_stt(&self) {
        if !self.stt_ready() {
            return;
        }
        let stt = Arc::clone(&self.stt);
        let models = self.models.clone();
        let model = self.settings.whisper_model;
        let spawned = std::thread::Builder::new()
            .name("kg-stt-load".into())
            .spawn(move || {
                if let Err(err) = with_stt(&stt, &models, model, |_| Ok(())) {
                    tracing::warn!(%err, "whisper preload failed");
                }
            });
        if let Err(err) = spawned {
            tracing::warn!(%err, "could not spawn whisper preload");
        }
    }
}

fn transcribe(
    stt: &Mutex<Option<(WhisperModel, WhisperStt)>>,
    models: &ModelStore,
    model: WhisperModel,
    clip: &AudioClip,
) -> Result<String, EngineError> {
    with_stt(stt, models, model, |whisper| {
        Ok(whisper.transcribe(clip, STT_LANGUAGE)?)
    })
}

/// Runs `f` with the requested Whisper model, loading it if needed.
fn with_stt<T>(
    stt: &Mutex<Option<(WhisperModel, WhisperStt)>>,
    models: &ModelStore,
    model: WhisperModel,
    f: impl FnOnce(&mut WhisperStt) -> Result<T, EngineError>,
) -> Result<T, EngineError> {
    let mut guard = stt
        .lock()
        .map_err(|_| AudioError::Model("speech model lock poisoned".into()))?;
    if guard.as_ref().is_none_or(|(loaded, _)| *loaded != model) {
        *guard = None;
        let whisper = WhisperStt::load(&models.whisper_dir(model))?;
        *guard = Some((model, whisper));
    }
    match guard.as_mut() {
        Some((_, whisper)) => f(whisper),
        None => Err(EngineError::SpeechModelMissing(ModelId::Whisper(model))),
    }
}
