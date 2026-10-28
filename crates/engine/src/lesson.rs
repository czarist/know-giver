//! Voice lessons: listen → transcribe → tutor → speak, in a loop.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, mpsc};
use std::time::Duration;

use kg_ai::{AiError, ChatMessage, TopicContext, TutorReply};
use kg_audio::{
    AudioClip, AudioError, ModelId, RecordOptions, Recording, SpeechOptions, SttEngine,
    Synthesizer, TtsBackend, split_for_speech,
};
use kg_study::{TopicId, TopicStatus};

use crate::stt::{
    SttJob, ends_sentence, has_new_words, is_student_speech, needs_second_opinion, same_words,
    word_count,
};
use crate::{Engine, EngineError, Event, Notice, Settings};

/// Turns of conversation sent back to the tutor.
const HISTORY_TURNS: usize = 24;
/// In hands-free mode, stop waiting if the student says nothing.
const HANDS_FREE_IDLE: Duration = Duration::from_secs(12);
const SILENCE_TO_STOP: Duration = Duration::from_millis(1_400);
/// With live captions, a finished sentence followed by this much quiet ends
/// the turn (1.4 s otherwise, so a pause mid-thought does not cut the student
/// off). If the student goes on talking while the tutor thinks, the turn
/// continues and this wait grows for the rest of the lesson.
const QUIET_AFTER_SENTENCE: Duration = Duration::from_millis(450);
/// Without a finished sentence, the caption must stay the same this long.
const STABLE_WITHOUT_SENTENCE: Duration = Duration::from_millis(1_200);
const QUIET_STEP: Duration = Duration::from_millis(250);
/// Longest monitored stretch while the tutor thinks or speaks.
const MONITOR_LIMIT: Duration = Duration::from_secs(300);
/// New sound needed before checking again whether the student is talking.
const MONITOR_STEP_SECS: f32 = 0.6;
/// Audio checked each time: enough for two or three words.
const MONITOR_WINDOW_SECS: f32 = 2.0;
/// New audio needed before the live caption is refreshed (seconds).
const PARTIAL_STEP_SECS: f32 = 0.25;
/// What the tutor says right away while the answer is being written. Spoken
/// Portuguese, like the prompts; neutral, so they fit right and wrong answers.
const FILLERS: [&str; 4] = ["Hum.", "Entendi.", "Hum, tá.", "Tá."];
/// Whiteboard points kept on screen (the newest ones).
const BOARD_POINTS: usize = 6;
/// Shorter clips are treated as accidental clicks.
const MIN_CLIP_SECS: f32 = 0.4;
/// How much of the tutor's last turn is given to Whisper as context.
const STT_CONTEXT_CHARS: usize = 300;

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
    /// What the student is saying, while they speak.
    partial: Option<Partial>,
    partial_running: bool,
    /// The tutor reply being received; sentences of older replies are ignored.
    reply: u64,
    /// Microphone kept open while the tutor thinks or speaks, to notice the
    /// student talking (continuing their turn, or interrupting).
    monitor: Option<Recording>,
    monitor_checking: bool,
    monitor_checked: usize,
    /// Where in the recording the current student turn starts.
    listen_from: usize,
    /// Beginning of a turn the student is continuing.
    carry: Option<String>,
    /// Quiet after a finished sentence that ends the turn.
    quiet_after_sentence: Duration,
    /// Room noise measured while listening, reused by later recordings.
    noise_floor: Option<f32>,
    /// When the last student turn ended, for timing logs.
    turn_ended: Option<std::time::Instant>,
}

/// A live caption and how much of the recording it covers.
#[derive(Debug, Clone)]
struct Partial {
    text: String,
    samples: usize,
    /// Recording position when the text last changed.
    changed_at: usize,
}

impl Partial {
    /// How long the caption has stayed the same, in recorded audio. Unlike
    /// loudness, this does not depend on room noise or microphone gain.
    fn stable_for(&self, sample_rate: u32) -> Duration {
        let samples = self.samples.saturating_sub(self.changed_at);
        Duration::from_secs_f64(samples as f64 / f64::from(sample_rate.max(1)))
    }
}

impl LessonSession {
    /// Microphone level while listening, `0.0..=1.0`.
    pub fn input_level(&self) -> f32 {
        self.recording
            .as_ref()
            .map_or(0.0, |r| (r.level() * 6.0).min(1.0))
    }

    /// The tutor reply being received.
    #[cfg(test)]
    pub(crate) fn reply_id(&self) -> u64 {
        self.reply
    }

    /// What the student is saying right now, while listening.
    pub fn live_caption(&self) -> Option<String> {
        if !matches!(
            self.phase,
            LessonPhase::Listening | LessonPhase::Transcribing
        ) {
            return None;
        }
        let live = self.partial.as_ref().map_or("", |p| p.text.as_str());
        let text = match &self.carry {
            Some(carry) => format!("{carry} {live}"),
            None => live.to_owned(),
        };
        let text = text.trim();
        (!text.is_empty()).then(|| text.to_owned())
    }

    /// The live caption, when it already has everything said and needs no
    /// second check.
    fn usable_partial(&self, voice_end: usize, sample_rate: u32) -> Option<String> {
        self.partial
            .as_ref()
            .filter(|p| {
                let complete = p.samples >= voice_end
                    || p.stable_for(sample_rate) >= Duration::from_millis(300);
                complete && word_count(&p.text) >= 1 && !needs_second_opinion(&p.text)
            })
            .map(|p| p.text.clone())
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
    /// Live caption of the audio recorded so far.
    Partial {
        session: u64,
        samples: usize,
        result: Result<String, EngineError>,
    },
    /// What the microphone heard while the tutor was thinking or speaking.
    Heard {
        session: u64,
        reply: u64,
        from: usize,
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
    /// "Hum." and friends in the current voice, made when a lesson starts.
    fillers: Arc<std::sync::Mutex<Vec<AudioClip>>>,
}

impl VoiceRuntime {
    /// Forgets the TTS so it is rebuilt with new settings or files.
    pub(crate) fn reset_tts(&mut self) {
        self.tts = None;
        self.fillers = Arc::default();
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
        let reply = self.next_id();
        let client = self.ai_client();
        let ctx = context.clone();
        // The board opens with a picture of the topic, when one exists.
        let opening_board = if self.settings.board_images {
            vec![format!("IMAGEM: {topic_title}")]
        } else {
            Vec::new()
        };
        self.lesson = Some(LessonSession {
            id,
            topic_id,
            topic_title,
            subject: context.subject.clone(),
            turns: Vec::new(),
            phase: LessonPhase::Thinking,
            board: opening_board,
            context,
            recording: None,
            speech_job: None,
            streaming: false,
            partial: None,
            partial_running: false,
            reply,
            monitor: None,
            monitor_checking: false,
            monitor_checked: 0,
            listen_from: 0,
            carry: None,
            quiet_after_sentence: QUIET_AFTER_SENTENCE,
            noise_floor: None,
            turn_ended: None,
        });
        if let Some(title) = self.lesson.as_ref().map(|l| l.topic_title.clone()) {
            self.request_illustration(&title);
        }
        self.warm_up_voice();
        let speech = self.open_speech_stream();
        let (tx, waker) = self.sender();
        self.spawn(move || {
            let result = kg_ai::start_lesson_stream(&client, &ctx, |sentence| {
                relay_sentence(&tx, &waker, speech.as_ref(), reply, sentence);
            });
            Event::Lesson(LessonEvent::Reply {
                session: reply,
                result,
            })
        });
        self.preload_stt();
        self.start_monitor();
        Ok(())
    }

    /// Loads the tutor's voice ahead of the first sentence.
    fn warm_up_voice(&mut self) {
        if !(self.settings.voice_enabled && self.tts_ready()) {
            return;
        }
        let Ok(tts) = self.tts() else {
            return;
        };
        let fillers = Arc::clone(&self.voice.fillers);
        let want_fillers = self.settings.fillers && fillers.lock().is_ok_and(|f| f.is_empty());
        let spawned = std::thread::Builder::new()
            .name("kg-voice-warm-up".into())
            .spawn(move || {
                tts.warm_up();
                if !want_fillers {
                    return;
                }
                let clips: Vec<AudioClip> = FILLERS
                    .iter()
                    .filter_map(|text| tts.synthesize(text, &|| false).ok())
                    .collect();
                if let Ok(mut cached) = fillers.lock() {
                    *cached = clips;
                }
            });
        if let Err(err) = spawned {
            tracing::debug!(%err, "could not warm up the voice");
        }
    }

    /// Opens the microphone while the tutor thinks or speaks, so the student
    /// can go on talking or interrupt.
    fn start_monitor(&mut self) {
        if !self.settings.barge_in || self.stt_job().is_none_or(|stt| !stt.live()) {
            return;
        }
        let Some(lesson) = self.lesson.as_mut() else {
            return;
        };
        if lesson.monitor.is_some() {
            return;
        }
        let options = RecordOptions {
            auto_stop_after: None,
            idle_timeout: None,
            max_duration: MONITOR_LIMIT,
            noise_floor: lesson.noise_floor,
        };
        match Recording::start(options) {
            Ok(recording) => {
                lesson.monitor = Some(recording);
                lesson.monitor_checking = false;
                lesson.monitor_checked = 0;
            }
            Err(err) => tracing::warn!(%err, "could not keep the microphone open"),
        }
    }

    /// While the tutor thinks or speaks: every bit of new sound, check
    /// whether it is the student talking (and not the tutor's own voice
    /// coming back through the speakers).
    fn tick_monitor(&mut self) {
        let Some(stt) = self.stt_job().filter(SttJob::live) else {
            return;
        };
        let Some(lesson) = self.lesson.as_mut() else {
            return;
        };
        let Some(monitor) = lesson.monitor.as_ref() else {
            return;
        };
        let rate = monitor.sample_rate() as f32;
        let len = monitor.len();
        let sounding = monitor
            .quiet_for()
            .is_some_and(|quiet| quiet < Duration::from_millis(250));
        if lesson.monitor_checking
            || !sounding
            || len < lesson.monitor_checked + (MONITOR_STEP_SECS * rate) as usize
        {
            return;
        }
        let from = len.saturating_sub((MONITOR_WINDOW_SECS * rate) as usize);
        let clip = monitor.snapshot_from(from);
        lesson.monitor_checking = true;
        lesson.monitor_checked = len;
        let (session, reply) = (lesson.id, lesson.reply);
        self.spawn(move || {
            Event::Lesson(LessonEvent::Heard {
                session,
                reply,
                from,
                result: stt.partial(&clip),
            })
        });
    }

    /// The student went on talking while the tutor was still thinking: drop
    /// the reply being prepared and keep listening to the same turn.
    fn continue_turn(&mut self, from: usize) {
        let reply = self.next_id();
        self.stop_speaking();
        let Some(lesson) = self.lesson.as_mut() else {
            return;
        };
        lesson.reply = reply;
        lesson.speech_job = None;
        if std::mem::take(&mut lesson.streaming)
            && lesson
                .turns
                .last()
                .is_some_and(|t| t.speaker == Speaker::Tutor)
        {
            lesson.turns.pop();
        }
        let carry = match lesson.turns.last() {
            Some(turn) if turn.speaker == Speaker::Student => lesson.turns.pop().map(|t| t.text),
            _ => None,
        };
        lesson.quiet_after_sentence =
            (lesson.quiet_after_sentence + QUIET_STEP).min(SILENCE_TO_STOP);
        tracing::info!(
            wait_ms = lesson.quiet_after_sentence.as_millis() as u64,
            "student kept talking; continuing the turn"
        );
        self.resume_listening(from, carry);
    }

    /// The student spoke over the tutor: stop talking and listen.
    fn barge_in(&mut self, from: usize) {
        tracing::info!("student interrupted the tutor");
        self.stop_speaking();
        self.resume_listening(from, None);
    }

    /// Turns the open monitor into the student's turn, starting at `from`.
    fn resume_listening(&mut self, from: usize, carry: Option<String>) {
        let Some(lesson) = self.lesson.as_mut() else {
            return;
        };
        let Some(monitor) = lesson.monitor.take() else {
            return;
        };
        lesson.recording = Some(monitor);
        lesson.listen_from = from;
        lesson.carry = carry;
        lesson.partial = None;
        lesson.partial_running = false;
        lesson.monitor_checking = false;
        lesson.phase = LessonPhase::Listening;
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

    /// How speech is recognized right now, if it can be.
    pub(crate) fn stt_job(&self) -> Option<SttJob> {
        SttJob::resolve(
            &self.stt,
            &self.models,
            self.settings.stt_engine,
            self.settings.whisper_model,
        )
    }

    fn listen(&mut self, idle_timeout: Option<Duration>) -> Result<(), EngineError> {
        if !self.stt_ready() {
            return Err(EngineError::SpeechModelMissing(
                match self.settings.stt_engine {
                    SttEngine::Parakeet => ModelId::Parakeet,
                    SttEngine::Whisper => ModelId::Whisper(self.settings.whisper_model),
                },
            ));
        }
        self.stop_speaking();
        let Some(lesson) = self.lesson.as_mut() else {
            return Ok(());
        };
        let options = RecordOptions {
            auto_stop_after: self.settings.auto_stop.then_some(SILENCE_TO_STOP),
            idle_timeout,
            noise_floor: lesson.noise_floor,
            ..RecordOptions::default()
        };
        if matches!(
            lesson.phase,
            LessonPhase::Thinking | LessonPhase::Transcribing
        ) {
            return Ok(());
        }
        lesson.monitor = None;
        lesson.recording = Some(Recording::start(options)?);
        lesson.phase = LessonPhase::Listening;
        lesson.partial = None;
        lesson.partial_running = false;
        lesson.listen_from = 0;
        lesson.carry = None;
        Ok(())
    }

    pub(crate) fn stop_listening(&mut self) {
        let stt = self.stt_job();
        let Some(lesson) = self.lesson.as_mut() else {
            return;
        };
        let Some(recording) = lesson.recording.take() else {
            return;
        };
        let stopped_by_itself = recording.is_finished();
        let heard = recording.heard_speech() && recording.voice_end() > lesson.listen_from;
        let ready_text = lesson.usable_partial(recording.voice_end(), recording.sample_rate());
        if lesson.noise_floor.is_none() {
            lesson.noise_floor = recording.noise_floor();
        }
        let mut clip = recording.finish();
        let start = lesson.listen_from.min(clip.samples.len());
        clip.samples.drain(..start);
        if !heard || clip.duration_secs() < MIN_CLIP_SECS {
            lesson.phase = LessonPhase::Idle;
            if lesson.carry.is_some() {
                // Nothing more was said: the turn ends as it was.
                self.answer_student(String::new(), true);
                return;
            }
            if !stopped_by_itself {
                self.notices.push(Notice::NothingHeard);
            }
            return;
        }
        let last_recording = self.paths.last_recording();
        if let Some(text) = ready_text {
            // The live caption already has every word: answer right away.
            tracing::info!(%text, "using the live caption as the transcript");
            save_recording(clip, last_recording);
            self.answer_student(text, true);
            return;
        }
        let Some(stt) = stt else {
            lesson.phase = LessonPhase::Idle;
            return;
        };
        lesson.phase = LessonPhase::Transcribing;
        let session = lesson.id;
        // What the tutor said last helps Whisper read short answers.
        let context = lesson
            .turns
            .iter()
            .rev()
            .find(|t| t.speaker == Speaker::Tutor)
            .map(|t| tail_chars(&t.text, STT_CONTEXT_CHARS).to_owned())
            .unwrap_or_default();
        self.spawn(move || {
            // Kept so a bad transcription can be checked against the audio.
            if let Err(error) = clip.write_wav(&last_recording) {
                tracing::debug!(%error, "could not save the last recording");
            }
            tracing::info!(
                secs = clip.duration_secs(),
                rate = clip.sample_rate,
                "transcribing recording"
            );
            Event::Lesson(LessonEvent::Transcribed {
                session,
                result: stt.transcribe(&clip, &context),
            })
        });
    }

    /// While listening with live captions: refresh the caption, and end the
    /// turn once a finished sentence is followed by a short silence.
    fn tick_live_caption(&mut self) {
        let Some(stt) = self.stt_job().filter(SttJob::live) else {
            return;
        };
        let Some(lesson) = self.lesson.as_mut() else {
            return;
        };
        let Some(recording) = lesson.recording.as_ref() else {
            return;
        };
        let voice_end = recording.voice_end();
        let rate = recording.sample_rate();
        let quiet = recording.quiet_for().unwrap_or_default();
        let spoke = voice_end > lesson.listen_from;
        let (by_caption, by_level) = match &lesson.partial {
            Some(p) if word_count(&p.text) >= 1 => {
                let stable = p.stable_for(rate);
                let sentence = ends_sentence(&p.text);
                (
                    // The caption stopped changing: robust to room noise.
                    (sentence && stable >= lesson.quiet_after_sentence)
                        || stable >= STABLE_WITHOUT_SENTENCE,
                    // Or the microphone went quiet after a finished sentence.
                    sentence && p.samples >= voice_end && quiet >= lesson.quiet_after_sentence,
                )
            }
            _ => (false, false),
        };
        // A monitor turned into a recording has no automatic stop of its own.
        let long_pause = spoke && quiet >= SILENCE_TO_STOP;
        if self.settings.auto_stop && (by_caption || by_level || long_pause) {
            let waited = lesson
                .partial
                .as_ref()
                .map_or(0, |p| p.stable_for(rate).as_millis() as u64);
            tracing::info!(
                stable_ms = waited,
                quiet_ms = quiet.as_millis() as u64,
                by_caption,
                "end of turn"
            );
            self.stop_listening();
            return;
        }
        let covered = lesson
            .partial
            .as_ref()
            .map_or(lesson.listen_from, |p| p.samples);
        let step = (PARTIAL_STEP_SECS * recording.sample_rate() as f32) as usize;
        if lesson.partial_running || !recording.heard_speech() || recording.len() < covered + step {
            return;
        }
        let from = lesson.listen_from;
        let clip = recording.snapshot_from(from);
        lesson.partial_running = true;
        let session = lesson.id;
        self.spawn(move || {
            let samples = from + clip.samples.len();
            Event::Lesson(LessonEvent::Partial {
                session,
                samples,
                result: stt.partial(&clip),
            })
        });
    }

    /// A typed message from the student.
    pub(crate) fn send_student_message(&mut self, text: String) {
        self.answer_student(text, false);
    }

    /// Sends the student's turn to the tutor. `spoken` turns get an
    /// immediate "Hum." while the answer is written.
    fn answer_student(&mut self, text: String, spoken: bool) {
        let text = text.trim().to_owned();
        let carried = self.lesson.as_ref().is_some_and(|l| l.carry.is_some());
        if text.is_empty() && !carried {
            return;
        }
        self.stop_speaking();
        let reply = self.next_id();
        let Some(lesson) = self.lesson.as_mut() else {
            return;
        };
        if let Some(recording) = lesson.recording.take() {
            drop(recording.finish());
        }
        let text = match lesson.carry.take() {
            Some(carry) => format!("{carry} {text}").trim().to_owned(),
            None => text,
        };
        lesson.turns.push(LessonTurn {
            speaker: Speaker::Student,
            text,
        });
        lesson.phase = LessonPhase::Thinking;
        lesson.reply = reply;
        lesson.listen_from = 0;
        lesson.turn_ended = Some(std::time::Instant::now());
        let turn = lesson.turns.len();
        let session = reply;
        let ctx = lesson.context.clone();
        let history = lesson.history();
        let client = self.ai_client();
        let speech = self.open_speech_stream();
        let speaking_enabled = speech.is_some();
        let (tx, waker) = self.sender();
        self.spawn(move || {
            let result = kg_ai::tutor_reply_stream(&client, &ctx, &history, |sentence| {
                relay_sentence(&tx, &waker, speech.as_ref(), session, sentence);
            });
            Event::Lesson(LessonEvent::Reply { session, result })
        });
        if spoken && speaking_enabled {
            self.play_filler(turn);
        }
        self.start_monitor();
    }

    /// Plays a short "Hum." right away, so the tutor reacts while the answer
    /// is still being written.
    fn play_filler(&mut self, turn: usize) {
        if !self.settings.fillers {
            return;
        }
        let clip = self
            .voice
            .fillers
            .lock()
            .ok()
            .and_then(|f| (!f.is_empty()).then(|| f[turn % f.len()].clone()));
        if let (Some(clip), Some(speaker)) = (clip, &self.voice.speaker) {
            speaker.play(clip);
        }
    }

    /// Test hook: applies a tutor reply to the current lesson as if the
    /// model had answered.
    #[cfg(feature = "test-support")]
    pub fn simulate_tutor_reply(&mut self, speech: &str, board: &[&str]) {
        let Some(session) = self.lesson.as_ref().map(|l| l.reply) else {
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
            if std::mem::take(&mut lesson.streaming) {
                // Interrupted while the reply was still arriving: keep what
                // was said and ignore the rest, instead of reading it all again.
                lesson.reply = 0;
            }
        }
    }

    /// Stops all lesson audio: speech and microphone.
    pub(crate) fn stop_audio(&mut self) {
        self.stop_speaking();
        if let Some(lesson) = self.lesson.as_mut() {
            lesson.monitor = None;
            if let Some(recording) = lesson.recording.take() {
                drop(recording.finish());
                lesson.phase = LessonPhase::Idle;
            }
        }
    }

    /// Per-frame lesson upkeep: end of recording and end of speech.
    pub(crate) fn tick_lesson(&mut self) {
        let Some(lesson) = self.lesson.as_mut() else {
            return;
        };
        if !matches!(lesson.phase, LessonPhase::Thinking | LessonPhase::Speaking) {
            // The monitor only listens while the tutor has the floor.
            lesson.monitor = None;
        }
        match lesson.phase {
            LessonPhase::Thinking => self.tick_monitor(),
            LessonPhase::Listening
                if lesson
                    .recording
                    .as_ref()
                    .is_some_and(Recording::is_finished) =>
            {
                self.stop_listening();
            }
            LessonPhase::Listening => self.tick_live_caption(),
            LessonPhase::Speaking => {
                self.tick_monitor();
                let Some(lesson) = self.lesson.as_mut() else {
                    return;
                };
                if lesson.phase != LessonPhase::Speaking {
                    return;
                }
                let synthesizing = lesson
                    .speech_job
                    .as_ref()
                    .is_some_and(|job| job.load(Ordering::Relaxed));
                let playing = self.voice.speaker.as_ref().is_some_and(|s| s.is_playing());
                if !synthesizing && !playing {
                    lesson.phase = LessonPhase::Idle;
                    lesson.speech_job = None;
                    lesson.monitor = None;
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
        let current_reply = self.lesson.as_ref().map(|l| l.reply);
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
            LessonEvent::Sentence { session, text } if Some(session) == current_reply => {
                if let Some(lesson) = self.lesson.as_mut() {
                    // Captions grow sentence by sentence while the reply arrives.
                    if lesson.streaming {
                        if let Some(turn) = lesson.turns.last_mut() {
                            turn.text.push(' ');
                            turn.text.push_str(&text);
                        }
                    } else {
                        if let Some(ended) = lesson.turn_ended.take() {
                            tracing::info!(
                                after_ms = ended.elapsed().as_millis() as u64,
                                "first tutor sentence"
                            );
                        }
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
            LessonEvent::Reply { session, result } if Some(session) == current_reply => {
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
                let mut pictures: Vec<String> = Vec::new();
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
                    pictures = reply
                        .board
                        .iter()
                        .filter_map(|entry| entry.strip_prefix("IMAGEM:"))
                        .map(|query| query.trim().to_owned())
                        .collect();
                    lesson.board.extend(reply.board);
                    let excess = lesson.board.len().saturating_sub(BOARD_POINTS);
                    lesson.board.drain(..excess);
                    spoken_live = lesson.speech_job.is_some();
                    if spoken_live && lesson.phase == LessonPhase::Thinking {
                        lesson.phase = LessonPhase::Speaking;
                    }
                }
                for query in &pictures {
                    self.request_illustration(query);
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
                    Ok(text) if !text.trim().is_empty() => self.answer_student(text, true),
                    Ok(_) if self.lesson.as_ref().is_some_and(|l| l.carry.is_some()) => {
                        self.answer_student(String::new(), true);
                    }
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
            LessonEvent::Partial {
                session,
                samples,
                result,
            } if Some(session) == current => {
                if let Some(lesson) = self.lesson.as_mut() {
                    lesson.partial_running = false;
                    match result {
                        Ok(text) if lesson.phase == LessonPhase::Listening => {
                            let changed_at = match &lesson.partial {
                                Some(p) if same_words(&p.text, &text) => p.changed_at,
                                _ => samples,
                            };
                            lesson.partial = Some(Partial {
                                text,
                                samples,
                                changed_at,
                            });
                        }
                        Ok(_) => {}
                        Err(err) => tracing::warn!(%err, "live caption failed"),
                    }
                }
            }
            LessonEvent::Heard {
                session,
                reply,
                from,
                result,
            } if Some(session) == current => {
                let Some(lesson) = self.lesson.as_mut() else {
                    return Ok(());
                };
                lesson.monitor_checking = false;
                if reply != lesson.reply {
                    return Ok(());
                }
                let text = match result {
                    Ok(text) => text,
                    Err(err) => {
                        tracing::warn!(%err, "could not check the microphone");
                        return Ok(());
                    }
                };
                match lesson.phase {
                    // Only the short "Hum." may be playing, so any other word
                    // is the student.
                    LessonPhase::Thinking if has_new_words(&text, &FILLERS.join(" ")) => {
                        if lesson
                            .turns
                            .last()
                            .is_some_and(|t| t.speaker == Speaker::Student)
                        {
                            self.continue_turn(from);
                        } else {
                            self.barge_in(from);
                        }
                    }
                    LessonPhase::Speaking => {
                        let tutor = lesson
                            .turns
                            .iter()
                            .rev()
                            .find(|t| t.speaker == Speaker::Tutor)
                            .map_or("", |t| t.text.as_str());
                        if is_student_speech(&text, tutor) {
                            tracing::info!(%text, "heard the student over the tutor");
                            self.barge_in(from);
                        }
                    }
                    _ => {}
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

    /// Loads speech recognition in the background so the first answer is
    /// not delayed.
    fn preload_stt(&self) {
        if let Some(stt) = self.stt_job() {
            stt.preload();
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

/// Keeps the last recording, so a bad transcription can be checked against
/// the audio.
fn save_recording(clip: AudioClip, path: std::path::PathBuf) {
    let spawned = std::thread::Builder::new()
        .name("kg-save-recording".into())
        .spawn(move || {
            if let Err(error) = clip.write_wav(&path) {
                tracing::debug!(%error, "could not save the last recording");
            }
        });
    if let Err(error) = spawned {
        tracing::debug!(%error, "could not save the last recording");
    }
}

/// The last `max` characters of `text`, starting at a word boundary.
fn tail_chars(text: &str, max: usize) -> &str {
    let count = text.chars().count();
    if count <= max {
        return text;
    }
    let start = text.char_indices().nth(count - max).map_or(0, |(i, _)| i);
    let tail = &text[start..];
    tail.split_once(' ').map_or(tail, |(_, rest)| rest)
}

#[cfg(test)]
mod stt_context_tests {
    use super::{Partial, tail_chars};
    use std::time::Duration;

    #[test]
    fn stability_is_measured_in_recorded_audio() {
        let partial = Partial {
            text: "Sim, entendi.".into(),
            samples: 48_000 * 3,
            changed_at: 48_000 * 2,
        };
        assert_eq!(partial.stable_for(48_000), Duration::from_secs(1));
        let fresh = Partial {
            changed_at: partial.samples,
            ..partial
        };
        assert_eq!(fresh.stable_for(48_000), Duration::ZERO);
    }

    #[test]
    fn tail_keeps_whole_words_from_the_end() {
        assert_eq!(tail_chars("curto", 300), "curto");
        assert_eq!(tail_chars("ficou claro até aqui", 9), "até aqui");
        assert_eq!(tail_chars("ficou claro até aqui", 7), "aqui");
        assert_eq!(tail_chars("é à crase", 5), "crase");
    }
}
