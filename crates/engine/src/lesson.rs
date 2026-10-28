//! Voice lessons: listen → transcribe → tutor → speak, in a loop.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use kg_ai::{AiError, ChatMessage, TopicContext, TutorReply};
use kg_audio::{
    AudioClip, AudioError, ModelId, ModelStore, RecordOptions, Recording, SpeechOptions,
    Synthesizer, TtsBackend, WhisperModel, WhisperStt, split_for_speech,
};
use kg_study::{TopicId, TopicStatus};

use crate::{Engine, EngineError, Event, Notice, Settings};

/// Language code passed to Whisper.
const STT_LANGUAGE: &str = "pt";
/// Turns of conversation sent back to the tutor.
const HISTORY_TURNS: usize = 24;
/// In hands-free mode, stop waiting if the student says nothing.
const HANDS_FREE_IDLE: Duration = Duration::from_secs(12);
const SILENCE_TO_STOP: Duration = Duration::from_millis(1_400);
/// Whiteboard points kept on screen (the newest ones).
const BOARD_POINTS: usize = 6;
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
    /// Key points the tutor wrote on the whiteboard, oldest first.
    pub board: Vec<String>,
    context: TopicContext,
    recording: Option<Recording>,
    speech_job: Option<Arc<AtomicBool>>,
    /// The last tutor turn is still arriving, sentence by sentence.
    streaming: bool,
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
    PreviewReady {
        generation: u64,
        result: Result<AudioClip, AudioError>,
    },
    /// One complete spoken sentence of a reply that is still arriving.
    Sentence {
        session: u64,
        text: String,
    },
    Reply {
        session: u64,
        result: Result<TutorReply, AiError>,
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
    tts: Option<Synthesizer>,
    preview_job: Option<Arc<AtomicBool>>,
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
        self.stop_voice_preview();
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
            board: Vec::new(),
            context,
            recording: None,
            speech_job: None,
            streaming: false,
        });
        let speech = self.open_speech_stream();
        let (tx, waker) = self.sender();
        self.spawn(move || {
            let result = kg_ai::start_lesson_stream(&client, &ctx, |sentence| {
                relay_sentence(&tx, &waker, speech.as_ref(), id, sentence);
            });
            Event::Lesson(LessonEvent::Reply {
                session: id,
                result,
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
            self.record_activity(kg_study::ActivityKind::Lesson)?;
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
        let speech = self.open_speech_stream();
        let (tx, waker) = self.sender();
        self.spawn(move || {
            let result = kg_ai::tutor_reply_stream(&client, &ctx, &history, |sentence| {
                relay_sentence(&tx, &waker, speech.as_ref(), session, sentence);
            });
            Event::Lesson(LessonEvent::Reply { session, result })
        });
    }

    /// Test hook: applies a tutor reply to the current lesson as if the
    /// model had answered.
    #[cfg(feature = "test-support")]
    pub fn simulate_tutor_reply(&mut self, speech: &str, board: &[&str]) {
        let Some(session) = self.lesson.as_ref().map(|l| l.id) else {
            return;
        };
        let reply = TutorReply {
            speech: speech.to_owned(),
            board: board.iter().map(|p| (*p).to_owned()).collect(),
        };
        let _ = self.on_lesson_event(LessonEvent::Reply {
            session,
            result: Ok(reply),
        });
    }

    /// Loudness of the tutor's voice right now, `0.0..=1.0`, for lip-sync.
    pub fn speech_level(&self) -> f32 {
        let speaking = self
            .lesson
            .as_ref()
            .is_some_and(|l| l.phase == LessonPhase::Speaking);
        match &self.voice.speaker {
            Some(speaker) if speaking => (speaker.level() * 4.0).min(1.0),
            _ => 0.0,
        }
    }

    pub(crate) fn stop_speaking(&mut self) {
        self.voice.preview_job = None;
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
            LessonEvent::PreviewReady { generation, result }
                if self.voice.preview_job.is_some()
                    && self.voice.generation.load(Ordering::Relaxed) == generation =>
            {
                match result {
                    Ok(clip) => {
                        if let Some(speaker) = &self.voice.speaker {
                            speaker.play(clip);
                        }
                        if let Some(job) = &self.voice.preview_job {
                            job.store(false, Ordering::Relaxed);
                        }
                    }
                    Err(AudioError::Cancelled) => self.voice.preview_job = None,
                    Err(error) => {
                        self.voice.preview_job = None;
                        return Err(error.into());
                    }
                }
            }
            LessonEvent::Sentence { session, text } if Some(session) == current => {
                if let Some(lesson) = self.lesson.as_mut() {
                    // Captions grow sentence by sentence while the reply arrives.
                    if lesson.streaming {
                        if let Some(turn) = lesson.turns.last_mut() {
                            turn.text.push(' ');
                            turn.text.push_str(&text);
                        }
                    } else {
                        lesson.streaming = true;
                        lesson.turns.push(LessonTurn {
                            speaker: Speaker::Tutor,
                            text,
                        });
                    }
                    if lesson.speech_job.is_some() && lesson.phase == LessonPhase::Thinking {
                        lesson.phase = LessonPhase::Speaking;
                    }
                }
            }
            LessonEvent::Reply { session, result } if Some(session) == current => {
                let reply = match result {
                    Ok(reply) => reply,
                    Err(err) => {
                        if let Some(lesson) = self.lesson.as_mut() {
                            lesson.streaming = false;
                        }
                        self.set_phase(LessonPhase::Idle);
                        return Err(err.into());
                    }
                };
                let text = reply.speech;
                let mut spoken_live = false;
                if let Some(lesson) = self.lesson.as_mut() {
                    // The full reply replaces the streamed captions.
                    if std::mem::take(&mut lesson.streaming)
                        && let Some(turn) = lesson.turns.last_mut()
                    {
                        turn.text.clone_from(&text);
                    } else {
                        lesson.turns.push(LessonTurn {
                            speaker: Speaker::Tutor,
                            text: text.clone(),
                        });
                    }
                    lesson.board.extend(reply.board);
                    let excess = lesson.board.len().saturating_sub(BOARD_POINTS);
                    lesson.board.drain(..excess);
                    spoken_live = lesson.speech_job.is_some();
                    if spoken_live && lesson.phase == LessonPhase::Thinking {
                        lesson.phase = LessonPhase::Speaking;
                    }
                }
                if spoken_live {
                    // Sentences were already queued for speech as they arrived;
                    // `tick_lesson` ends the phase when playback finishes.
                } else if self.settings.voice_enabled && self.tts_ready() {
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

    /// Opens a speech queue for a reply that is about to stream. Each sentence
    /// sent into it is synthesized and played in order; the queue ends when
    /// the sender is dropped. `None` when the tutor should not speak.
    fn open_speech_stream(&mut self) -> Option<mpsc::Sender<String>> {
        if !(self.settings.voice_enabled && self.tts_ready()) {
            return None;
        }
        self.stop_voice_preview();
        let opened = self.tts().and_then(|tts| {
            if self.voice.speaker.is_none() {
                self.voice.speaker = Some(kg_audio::Speaker::open()?);
            }
            Ok(tts)
        });
        let tts = match opened {
            Ok(tts) => tts,
            Err(err) => {
                // The lesson goes on in text; the error is shown once.
                self.notices.push(Notice::Error(err));
                return None;
            }
        };
        let speaker = self.voice.speaker.clone()?;
        let lesson = self.lesson.as_mut()?;
        let generation = self.voice.generation.fetch_add(1, Ordering::Relaxed) + 1;
        let current_generation = Arc::clone(&self.voice.generation);
        let job = Arc::new(AtomicBool::new(true));
        lesson.speech_job = Some(Arc::clone(&job));
        let session = lesson.id;
        let (events, waker) = self.sender();
        let (queue, sentences) = mpsc::channel::<String>();

        let spawned = std::thread::Builder::new()
            .name("kg-speech-stream".into())
            .spawn(move || {
                let cancelled = || current_generation.load(Ordering::Relaxed) != generation;
                'sentences: for sentence in sentences {
                    for chunk in split_for_speech(&sentence) {
                        if cancelled() {
                            break 'sentences;
                        }
                        match tts.synthesize(&chunk, &cancelled) {
                            Ok(clip) if !cancelled() => speaker.play(clip),
                            Ok(_) | Err(AudioError::Cancelled) => break 'sentences,
                            Err(error) => {
                                let _ = events.send(Event::Lesson(LessonEvent::SpeechFailed {
                                    session,
                                    error,
                                }));
                                break 'sentences;
                            }
                        }
                    }
                }
                job.store(false, Ordering::Relaxed);
                waker();
            });
        match spawned {
            Ok(_) => Some(queue),
            Err(err) => {
                tracing::error!(%err, "could not spawn speech thread");
                if let Some(lesson) = self.lesson.as_mut() {
                    lesson.speech_job = None;
                }
                None
            }
        }
    }

    /// Speaks `text` sentence by sentence, so playback starts early.
    fn speak(&mut self, text: String) -> Result<(), EngineError> {
        self.stop_voice_preview();
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
                    match tts.synthesize(&chunk, &|| {
                        current_generation.load(Ordering::Relaxed) != generation
                    }) {
                        Ok(clip) if current_generation.load(Ordering::Relaxed) == generation => {
                            speaker.play(clip);
                        }
                        Ok(_) => break,
                        Err(AudioError::Cancelled) => break,
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

    fn tts(&mut self) -> Result<Synthesizer, EngineError> {
        if let Some(tts) = &self.voice.tts {
            return Ok(tts.clone());
        }
        let tts = self.synthesizer(&self.settings)?;
        self.voice.tts = Some(tts.clone());
        Ok(tts)
    }

    fn synthesizer(&self, settings: &Settings) -> Result<Synthesizer, EngineError> {
        let options = SpeechOptions {
            rate: settings.speech_rate,
            pitch: settings.speech_pitch,
            expressiveness: settings.speech_expressiveness,
            rhythm_variation: settings.speech_rhythm_variation,
        };
        match settings.tts_backend {
            TtsBackend::Local => {
                for id in [settings.voice.runtime_id(), settings.voice.model_id()] {
                    if !self.models.is_installed(id) {
                        return Err(EngineError::SpeechModelMissing(id));
                    }
                }
                Ok(Synthesizer::local(&self.models, settings.voice, options)?)
            }
            TtsBackend::Cloud => Ok(Synthesizer::cloud(settings.cloud_speech.clone(), options)?),
        }
    }

    /// Whether the voice sample is being generated or played.
    pub fn voice_preview_active(&self) -> bool {
        self.voice.preview_job.as_ref().is_some_and(|job| {
            job.load(Ordering::Relaxed)
                || self
                    .voice
                    .speaker
                    .as_ref()
                    .is_some_and(|speaker| speaker.is_playing())
        })
    }

    pub(crate) fn stop_voice_preview(&mut self) {
        if self.voice.preview_job.is_some() {
            self.stop_speaking();
        }
    }

    /// Samples draft settings without changing the user's saved voice.
    pub(crate) fn preview_voice(
        &mut self,
        mut settings: Settings,
        text: String,
    ) -> Result<(), EngineError> {
        self.stop_audio();
        settings.normalize();
        let tts = self.synthesizer(&settings)?;
        if self.voice.speaker.is_none() {
            self.voice.speaker = Some(kg_audio::Speaker::open()?);
        }
        let generation = self.voice.generation.fetch_add(1, Ordering::Relaxed) + 1;
        let current = Arc::clone(&self.voice.generation);
        self.voice.preview_job = Some(Arc::new(AtomicBool::new(true)));
        let (tx, waker) = self.sender();
        let spawned = std::thread::Builder::new()
            .name("kg-voice-preview".into())
            .spawn(move || {
                let text =
                    kg_audio::normalize_for_speech(&text.chars().take(500).collect::<String>());
                let result =
                    tts.synthesize(&text, &|| current.load(Ordering::Relaxed) != generation);
                let _ = tx.send(Event::Lesson(LessonEvent::PreviewReady {
                    generation,
                    result,
                }));
                waker();
            });
        if let Err(error) = spawned {
            self.voice.preview_job = None;
            return Err(AudioError::Tts(format!("could not start voice preview: {error}")).into());
        }
        Ok(())
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

/// Sends a streamed sentence to the speech queue (if any) and to the UI.
fn relay_sentence(
    events: &mpsc::Sender<Event>,
    waker: &crate::Waker,
    speech: Option<&mpsc::Sender<String>>,
    session: u64,
    sentence: &str,
) {
    if let Some(speech) = speech {
        let _ = speech.send(sentence.to_owned());
    }
    let _ = events.send(Event::Lesson(LessonEvent::Sentence {
        session,
        text: sentence.to_owned(),
    }));
    waker();
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
