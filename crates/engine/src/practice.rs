//! Quizzes and spaced-repetition reviews.

use chrono::NaiveDate;
use kg_ai::{AiError, QuestionDraft, ReviewFeedback, TopicContext};
use kg_study::{Quiz, QuizAttempt, QuizQuestion, ReviewGrade, TopicId};

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

#[derive(Debug)]
pub struct ReviewSession {
    pub id: u64,
    pub topic_id: TopicId,
    pub topic_title: String,
    pub step: ReviewStep,
    context: TopicContext,
}

impl ReviewSession {
    pub fn is_loading(&self) -> bool {
        matches!(
            self.step,
            ReviewStep::LoadingQuestion | ReviewStep::Grading { .. }
        )
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
        let mut board = self.usable_board_questions(topic_id);
        shuffle(&mut board, seed());
        board.truncate(size);
        for (i, q) in board.iter_mut().enumerate() {
            q.shuffle_options(seed().wrapping_add(i as u64));
        }
        let missing = size - board.len();
        // Questions prepared in advance come next; the model writes the rest.
        let mut prepared = self.quiz_bank(topic_id);
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

    pub(crate) fn answer_quiz(&mut self, option: usize) {
        let Some(session) = self.quiz.as_mut() else {
            return;
        };
        let QuizStep::Question {
            index,
            chosen: None,
        } = session.step
        else {
            return;
        };
        let is_correct = session
            .questions()
            .get(index)
            .is_some_and(|q| q.correct == option);
        if is_correct {
            session.correct += 1;
        }
        session.step = QuizStep::Question {
            index,
            chosen: Some(option),
        };
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
        });
        self.spawn(move || {
            Event::Practice(PracticeEvent::ReviewQuestion {
                session: id,
                result: kg_ai::review_question(&client, &ctx),
            })
        });
        Ok(())
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
                    Ok(question) => review.step = ReviewStep::Answering { question },
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
                        let suggested = ReviewGrade::from_level(feedback.score);
                        review.step = ReviewStep::Graded {
                            question,
                            answer,
                            feedback,
                            suggested,
                        };
                    }
                    Err(err) => {
                        // Let the student try again or grade themselves.
                        review.step = ReviewStep::Answering { question };
                        return Err(err.into());
                    }
                }
            }
        }
        Ok(())
    }
}
