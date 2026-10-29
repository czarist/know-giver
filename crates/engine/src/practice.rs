//! Quizzes and spaced-repetition reviews.

use std::time::Duration;

use chrono::NaiveDate;
use kg_ai::{AiError, QuestionDraft, ReviewFeedback, TopicContext};
use kg_audio::{ModelId, RecordOptions, Recording};
use kg_study::{Quiz, QuizAttempt, QuizQuestion, ReviewGrade, TopicId};

/// Letters and digits only, to spot the same question twice.
fn prompt_key(prompt: &str) -> String {
    prompt
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

use crate::{Engine, EngineError, Event, Notice};

/// Where a quiz is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuizStep {
    Loading,
    Question { index: usize, chosen: Option<usize> },
    Finished { correct: u32, total: u32 },
}

#[derive(Debug)]
pub struct QuizSession {
    pub id: u64,
    pub topic_id: TopicId,
    pub topic_title: String,
    pub step: QuizStep,
    quiz: Option<Quiz>,
    correct: u32,
    /// Past exam questions that open the quiz, before generated ones.
    board: Vec<QuizQuestion>,
    /// The first questions were answered wrong in earlier quizzes.
    pub retries: usize,
    /// Only past mistakes: the result does not count as a review.
    pub mistakes_only: bool,
}

impl QuizSession {
    pub fn questions(&self) -> &[QuizQuestion] {
        self.quiz.as_ref().map_or(&[], Quiz::questions)
    }

    pub fn current(&self) -> Option<&QuizQuestion> {
        match self.step {
            QuizStep::Question { index, .. } => self.questions().get(index),
            _ => None,
        }
    }

    /// The question at `index` was answered wrong before.
    pub fn is_retry(&self, index: usize) -> bool {
        index < self.retries
    }

    pub fn correct_so_far(&self) -> u32 {
        self.correct
    }
}

/// Where a review is.
#[derive(Debug, Clone, PartialEq)]
pub enum ReviewStep {
    LoadingQuestion,
    Answering {
        question: String,
    },
    Grading {
        question: String,
        answer: String,
    },
    Graded {
        question: String,
        answer: String,
        feedback: ReviewFeedback,
        suggested: ReviewGrade,
    },
}

/// Questions in one review.
pub const REVIEW_ROUNDS: usize = 3;
/// Longest spoken answer.
const MAX_SPOKEN_ANSWER: Duration = Duration::from_secs(120);

#[derive(Debug)]
pub struct ReviewSession {
    pub id: u64,
    pub topic_id: TopicId,
    pub topic_title: String,
    pub step: ReviewStep,
    context: TopicContext,
    /// The question being asked, 0-based, of [`REVIEW_ROUNDS`].
    pub round: usize,
    /// Scores (0–5) of the answers graded so far.
    scores: Vec<u8>,
    /// Questions asked so far, so the next one is about something else.
    asked: Vec<String>,
    /// The student is answering out loud.
    mic: Option<Recording>,
    /// A spoken answer is being turned into text.
    pub transcribing: bool,
}

impl ReviewSession {
    pub fn is_loading(&self) -> bool {
        self.transcribing
            || matches!(
                self.step,
                ReviewStep::LoadingQuestion | ReviewStep::Grading { .. }
            )
    }

    pub fn rounds(&self) -> usize {
        REVIEW_ROUNDS
    }

    pub fn is_last_round(&self) -> bool {
        self.round + 1 >= REVIEW_ROUNDS
    }

    /// The microphone is open for a spoken answer.
    pub fn listening(&self) -> bool {
        self.mic.is_some()
    }

    /// Microphone level while answering out loud, `0.0..=1.0`.
    pub fn input_level(&self) -> f32 {
        self.mic
            .as_ref()
            .map_or(0.0, |r| (r.level() * 25.0).sqrt().min(1.0))
    }

    /// Average score of the answers so far, rounded, 0–5.
    fn average_score(&self) -> u8 {
        let total: u32 = self.scores.iter().map(|&s| u32::from(s)).sum();
        let count = self.scores.len().max(1) as u32;
        ((total * 2 + count) / (count * 2)).min(5) as u8
    }

    /// The grade the answers so far point to.
    fn suggested(&self) -> ReviewGrade {
        ReviewGrade::from_level(self.average_score())
    }
}

fn seed() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(1, |d| d.as_nanos() as u64)
}

/// Fisher-Yates with a small xorshift; good enough to vary question order.
fn shuffle<T>(items: &mut [T], seed: u64) {
    let mut state = seed | 1;
    for i in (1..items.len()).rev() {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        items.swap(i, (state % (i as u64 + 1)) as usize);
    }
}

pub(crate) enum PracticeEvent {
    QuizReady {
        session: u64,
        result: Result<Vec<QuestionDraft>, AiError>,
    },
    ReviewQuestion {
        session: u64,
        result: Result<String, AiError>,
    },
    ReviewGraded {
        session: u64,
        result: Result<ReviewFeedback, AiError>,
    },
    /// A spoken answer, as text; `typed` is what was written before it.
    ReviewHeard {
        session: u64,
        typed: String,
        result: Result<String, EngineError>,
    },
}

/// Quiz questions given to the tutor, and their longest length.
const QUIZ_SUBJECTS: usize = 12;
const QUIZ_SUBJECT_CHARS: usize = 200;

impl Engine {
    /// Prompts of the questions a quiz on this topic draws from (exam
    /// questions first, then the prepared ones), for the lesson to cover.
    pub(crate) fn quiz_subjects(&self, topic_id: TopicId) -> Vec<String> {
        let board = self
            .usable_board_questions(topic_id)
            .into_iter()
            .map(|q| q.prompt);
        let prepared = self.quiz_bank(topic_id).into_iter().map(|q| q.prompt);
        board
            .chain(prepared)
            .map(|p| p.split_whitespace().collect::<Vec<_>>().join(" "))
            .filter(|p| !p.is_empty())
            .map(|p| {
                if p.chars().count() > QUIZ_SUBJECT_CHARS {
                    let cut: String = p.chars().take(QUIZ_SUBJECT_CHARS - 1).collect();
                    format!("{}…", cut.trim_end())
                } else {
                    p
                }
            })
            .take(QUIZ_SUBJECTS)
            .collect()
    }

    pub(crate) fn start_quiz(&mut self, topic_id: TopicId) -> Result<(), EngineError> {
        let (ctx, topic_title) = self.topic_context(topic_id)?;
        let id = self.next_id();
        let size = self.settings.quiz_size as usize;
        // Questions missed before come first, up to half the quiz.
        let mut retry: Vec<QuizQuestion> = self
            .store
            .topic_mistakes(topic_id)?
            .into_iter()
            .map(|m| m.question)
            .take(size.div_ceil(2))
            .collect();
        for (i, q) in retry.iter_mut().enumerate() {
            q.shuffle_options(seed().wrapping_add(0x51 + i as u64));
        }
        let retries = retry.len();
        let asked: Vec<String> = retry.iter().map(|q| prompt_key(&q.prompt)).collect();
        let fresh = |q: &QuizQuestion| !asked.contains(&prompt_key(&q.prompt));
        let mut board = self.usable_board_questions(topic_id);
        board.retain(|q| fresh(q));
        shuffle(&mut board, seed());
        board.truncate(size - retries);
        for (i, q) in board.iter_mut().enumerate() {
            q.shuffle_options(seed().wrapping_add(i as u64));
        }
        board.splice(0..0, retry);
        let missing = size - board.len();
        // Questions prepared in advance come next; the model writes the rest.
        let mut prepared = self.quiz_bank(topic_id);
        prepared.retain(|q| !asked.contains(&prompt_key(&q.prompt)));
        shuffle(&mut prepared, seed());
        prepared.truncate(missing);
        let missing = missing - prepared.len();
        self.quiz = Some(QuizSession {
            id,
            topic_id,
            topic_title,
            step: QuizStep::Loading,
            quiz: None,
            correct: 0,
            board,
            retries,
            mistakes_only: false,
        });
        if missing == 0 {
            // Enough real or prepared questions: no model call needed.
            return self.on_practice_event(PracticeEvent::QuizReady {
                session: id,
                result: Ok(prepared),
            });
        }
        let client = self.ai_client();
        self.spawn(move || {
            let result = kg_ai::generate_quiz(&client, &ctx, missing as u32).map(|generated| {
                prepared.extend(generated);
                prepared
            });
            Event::Practice(PracticeEvent::QuizReady {
                session: id,
                result,
            })
        });
        Ok(())
    }

    /// A quiz made only of this topic's past mistakes.
    pub(crate) fn retry_mistakes(&mut self, topic_id: TopicId) -> Result<(), EngineError> {
        let (_, topic_title) = self.topic_context(topic_id)?;
        let mut questions: Vec<QuizQuestion> = self
            .store
            .topic_mistakes(topic_id)?
            .into_iter()
            .map(|m| m.question)
            .collect();
        shuffle(&mut questions, seed());
        for (i, q) in questions.iter_mut().enumerate() {
            q.shuffle_options(seed().wrapping_add(i as u64));
        }
        let quiz = Quiz::new(topic_id, questions)?;
        let retries = quiz.questions().len();
        self.quiz = Some(QuizSession {
            id: self.next_id(),
            topic_id,
            topic_title,
            step: QuizStep::Question {
                index: 0,
                chosen: None,
            },
            quiz: Some(quiz),
            correct: 0,
            board: Vec::new(),
            retries,
            mistakes_only: true,
        });
        Ok(())
    }

    /// Picks an option. A wrong answer goes into the mistakes notebook; a
    /// right one takes the question out of it.
    pub(crate) fn answer_quiz(&mut self, option: usize) -> Result<(), EngineError> {
        let Some(session) = self.quiz.as_mut() else {
            return Ok(());
        };
        let QuizStep::Question {
            index,
            chosen: None,
        } = session.step
        else {
            return Ok(());
        };
        let Some(question) = session.questions().get(index).cloned() else {
            return Ok(());
        };
        let is_correct = question.correct == option;
        if is_correct {
            session.correct += 1;
        }
        session.step = QuizStep::Question {
            index,
            chosen: Some(option),
        };
        let topic_id = session.topic_id;
        if is_correct {
            self.store.clear_mistake(topic_id, &question.prompt)?;
        } else {
            self.store
                .record_mistake(topic_id, &question, option, Self::today())?;
        }
        self.refresh_mistakes();
        Ok(())
    }

    pub(crate) fn next_question(&mut self) -> Result<(), EngineError> {
        let Some(session) = self.quiz.as_mut() else {
            return Ok(());
        };
        let QuizStep::Question {
            index,
            chosen: Some(_),
        } = session.step
        else {
            return Ok(());
        };
        let total = session.questions().len();
        if index + 1 < total {
            session.step = QuizStep::Question {
                index: index + 1,
                chosen: None,
            };
            return Ok(());
        }

        let (correct, total) = (session.correct, total as u32);
        session.step = QuizStep::Finished { correct, total };
        let topic_id = session.topic_id;
        let today = Self::today();
        if session.mistakes_only {
            // Going over mistakes is practice, not a review of the topic.
            self.record_activity(kg_study::ActivityKind::Quiz)?;
            self.notices.push(Notice::QuizSaved { correct, total });
            return Ok(());
        }
        self.store.record_quiz(&QuizAttempt {
            topic_id,
            taken_on: today,
            correct,
            total,
        })?;
        let grade = ReviewGrade::from_score(correct, total);
        self.update_topic(topic_id, |t| t.apply_review(grade, today))?;
        self.record_activity(kg_study::ActivityKind::Quiz)?;
        self.notices.push(Notice::QuizSaved { correct, total });
        Ok(())
    }

    pub(crate) fn start_review(&mut self, topic_id: TopicId) -> Result<(), EngineError> {
        let (context, topic_title) = self.topic_context(topic_id)?;
        let id = self.next_id();
        let client = self.ai_client();
        let ctx = context.clone();
        self.review = Some(ReviewSession {
            id,
            topic_id,
            topic_title,
            step: ReviewStep::LoadingQuestion,
            context,
            round: 0,
            scores: Vec::new(),
            asked: Vec::new(),
            mic: None,
            transcribing: false,
        });
        self.spawn(move || {
            Event::Practice(PracticeEvent::ReviewQuestion {
                session: id,
                result: kg_ai::review_question(&client, &ctx),
            })
        });
        Ok(())
    }

    /// After a graded answer: the next question of the review.
    pub(crate) fn next_review_question(&mut self) {
        let Some(review) = self.review.as_mut() else {
            return;
        };
        if !matches!(review.step, ReviewStep::Graded { .. }) || review.is_last_round() {
            return;
        }
        review.round += 1;
        review.step = ReviewStep::LoadingQuestion;
        let (id, ctx, asked) = (review.id, review.context.clone(), review.asked.clone());
        let client = self.ai_client();
        self.stop_speaking();
        self.spawn(move || {
            Event::Practice(PracticeEvent::ReviewQuestion {
                session: id,
                result: kg_ai::review_question_avoiding(&client, &ctx, &asked),
            })
        });
    }

    /// Opens the microphone for a spoken answer.
    pub(crate) fn start_review_dictation(&mut self) -> Result<(), EngineError> {
        if self.stt_job().is_none() {
            return Err(EngineError::SpeechModelMissing(ModelId::Parakeet));
        }
        let Some(review) = self.review.as_ref() else {
            return Ok(());
        };
        if !matches!(review.step, ReviewStep::Answering { .. }) || review.mic.is_some() {
            return Ok(());
        }
        self.stop_speaking();
        let mic = Recording::start(RecordOptions {
            auto_stop_after: None,
            idle_timeout: None,
            max_duration: MAX_SPOKEN_ANSWER,
            noise_floor: None,
        })?;
        if let Some(review) = self.review.as_mut() {
            review.mic = Some(mic);
        }
        Ok(())
    }

    /// Closes the microphone and sends what was said, after `typed`.
    pub(crate) fn stop_review_dictation(&mut self, typed: String) {
        let Some(stt) = self.stt_job() else {
            return;
        };
        let Some(review) = self.review.as_mut() else {
            return;
        };
        let Some(mic) = review.mic.take() else {
            return;
        };
        let ReviewStep::Answering { question } = &review.step else {
            return;
        };
        let question = question.clone();
        review.transcribing = true;
        let session = review.id;
        let clip = mic.finish();
        self.spawn(move || {
            Event::Practice(PracticeEvent::ReviewHeard {
                session,
                typed,
                result: stt.transcribe(&clip, &question),
            })
        });
    }

    /// Reads review text aloud in the tutor's voice, when the voice is on.
    fn read_aloud(&mut self, text: &str) {
        if !(self.settings.voice_enabled && self.tts_ready()) {
            return;
        }
        if let Err(err) = self.say(text) {
            tracing::warn!(%err, "could not read the review aloud");
        }
    }

    pub(crate) fn submit_review_answer(&mut self, answer: String) {
        let answer = answer.trim().to_owned();
        let Some(session) = self.review.as_mut() else {
            return;
        };
        let ReviewStep::Answering { question } = &session.step else {
            return;
        };
        if answer.is_empty() {
            return;
        }
        let question = question.clone();
        session.step = ReviewStep::Grading {
            question: question.clone(),
            answer: answer.clone(),
        };
        let id = session.id;
        let ctx = session.context.clone();
        let client = self.ai_client();
        self.spawn(move || {
            Event::Practice(PracticeEvent::ReviewGraded {
                session: id,
                result: kg_ai::evaluate_review(&client, &ctx, &question, &answer),
            })
        });
    }

    /// Records the review with `grade` and closes it.
    pub(crate) fn grade_review(&mut self, grade: ReviewGrade) -> Result<(), EngineError> {
        self.stop_speaking();
        let Some(session) = self.review.take() else {
            return Ok(());
        };
        let today = Self::today();
        let mut next_due: Option<NaiveDate> = None;
        self.update_topic(session.topic_id, |t| {
            t.apply_review(grade, today);
            next_due = t.review.due;
        })?;
        self.record_activity(kg_study::ActivityKind::Review)?;
        self.notices.push(Notice::ReviewSaved { next_due });
        Ok(())
    }

    pub(crate) fn on_practice_event(&mut self, event: PracticeEvent) -> Result<(), EngineError> {
        match event {
            PracticeEvent::QuizReady { session, result } => {
                let Some(quiz) = self.quiz.as_mut().filter(|q| q.id == session) else {
                    return Ok(());
                };
                let built = result.map_err(EngineError::from).and_then(|drafts| {
                    let seed = seed();
                    let generated = drafts.into_iter().enumerate().map(|(i, d)| {
                        let mut question = QuizQuestion {
                            prompt: d.prompt,
                            options: d.options,
                            correct: d.correct,
                            explanation: d.explanation,
                            from_board: false,
                        };
                        if question.correct < question.options.len() {
                            question.shuffle_options(seed.wrapping_add(i as u64 * 0x9E37));
                        }
                        question
                    });
                    let questions = std::mem::take(&mut quiz.board)
                        .into_iter()
                        .chain(generated)
                        .collect();
                    Ok(Quiz::new(quiz.topic_id, questions)?)
                });
                match built {
                    Ok(built) => {
                        quiz.quiz = Some(built);
                        quiz.step = QuizStep::Question {
                            index: 0,
                            chosen: None,
                        };
                    }
                    Err(err) => {
                        self.quiz = None;
                        return Err(err);
                    }
                }
            }
            PracticeEvent::ReviewQuestion { session, result } => {
                let Some(review) = self.review.as_mut().filter(|r| r.id == session) else {
                    return Ok(());
                };
                match result {
                    Ok(question) => {
                        review.asked.push(question.clone());
                        review.step = ReviewStep::Answering {
                            question: question.clone(),
                        };
                        self.read_aloud(&question);
                    }
                    Err(err) if review.round > 0 => {
                        // Keep the answers given so far: grade them now.
                        let suggested = review.suggested();
                        review.round = REVIEW_ROUNDS - 1;
                        review.step = ReviewStep::Graded {
                            question: String::new(),
                            answer: String::new(),
                            feedback: ReviewFeedback {
                                score: review.average_score(),
                                feedback: String::new(),
                            },
                            suggested,
                        };
                        return Err(err.into());
                    }
                    Err(err) => {
                        self.review = None;
                        return Err(err.into());
                    }
                }
            }
            PracticeEvent::ReviewGraded { session, result } => {
                let Some(review) = self.review.as_mut().filter(|r| r.id == session) else {
                    return Ok(());
                };
                let ReviewStep::Grading { question, answer } = review.step.clone() else {
                    return Ok(());
                };
                match result {
                    Ok(feedback) => {
                        review.scores.push(feedback.score);
                        let suggested = review.suggested();
                        let spoken = feedback.feedback.clone();
                        review.step = ReviewStep::Graded {
                            question,
                            answer,
                            feedback,
                            suggested,
                        };
                        self.read_aloud(&spoken);
                    }
                    Err(err) => {
                        // Let the student try again or grade themselves.
                        review.step = ReviewStep::Answering { question };
                        return Err(err.into());
                    }
                }
            }
            PracticeEvent::ReviewHeard {
                session,
                typed,
                result,
            } => {
                let Some(review) = self.review.as_mut().filter(|r| r.id == session) else {
                    return Ok(());
                };
                review.transcribing = false;
                let heard = result?;
                let answer = format!("{} {}", typed.trim(), heard.trim());
                self.submit_review_answer(answer);
            }
        }
        Ok(())
    }
}
