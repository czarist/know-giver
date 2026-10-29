//! Tutor tasks: plan generation, lessons, quizzes and review grading.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::AiError;
use crate::client::{ChatMessage, ChatRequest, Client, Role};
use crate::config::Task;
use crate::prompts::{self, fill};

/// Longest tutor answer, in words. Voice answers must stay short.
const TUTOR_MAX_WORDS: &str = "120";

/// What the student told us about their preparation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanPrefs {
    pub weeks: u32,
    pub hours_per_week: u32,
    /// Role (cargo) to focus on when the edital covers several.
    pub role: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct TopicDraft {
    pub title: String,
    #[serde(default)]
    pub difficulty: String,
    #[serde(default = "first_week")]
    pub week: u32,
}

fn first_week() -> u32 {
    1
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SubjectDraft {
    pub name: String,
    #[serde(default)]
    pub topics: Vec<TopicDraft>,
}

/// Plan as returned by the model, before validation.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PlanDraft {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub exam: String,
    #[serde(default)]
    pub subjects: Vec<SubjectDraft>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct QuestionDraft {
    pub prompt: String,
    pub options: Vec<String>,
    pub correct: usize,
    #[serde(default)]
    pub explanation: String,
}

#[derive(Deserialize)]
struct QuizDraft {
    questions: Vec<QuestionDraft>,
}

#[derive(Deserialize)]
struct ReviewQuestionDraft {
    question: String,
}

/// Correction of a review answer.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ReviewFeedback {
    /// 0..=5, see `prompts/review_eval.md`.
    pub score: u8,
    pub feedback: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Default)]
pub struct SectionDraft {
    #[serde(default)]
    pub heading: String,
    #[serde(default)]
    pub paragraphs: Vec<String>,
    #[serde(default)]
    pub key_points: Vec<String>,
    /// A Wikipedia article whose picture helps; may be empty.
    #[serde(default)]
    pub image: String,
}

/// A written lesson (apostila) for one topic, before validation.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Default)]
pub struct HandoutDraft {
    #[serde(default)]
    pub introduction: String,
    #[serde(default)]
    pub sections: Vec<SectionDraft>,
    #[serde(default)]
    pub examples: Vec<String>,
    #[serde(default)]
    pub pitfalls: Vec<String>,
    #[serde(default)]
    pub summary: Vec<String>,
    #[serde(default)]
    pub exercises: Vec<QuestionDraft>,
}

/// Markers of whiteboard lines at the end of a tutor answer. The first ones
/// hold several points separated by `|`; matched in any case.
const BOARD_MARKERS: [&str; 3] = ["QUADRO:", "FORMULA:", "DIAGRAMA:"];
/// Markers whose whole line is one board entry (tables, timelines...).
/// Matched in capitals only, so speech such as "Exemplo: imagine..." is
/// never taken for the board.
const BOARD_BLOCK_MARKERS: [&str; 6] = [
    "IMAGEM:",
    "TABELA:",
    "LINHA DO TEMPO:",
    "MAPA:",
    "DESTAQUE:",
    "EXEMPLO:",
];
/// Longest whiteboard point kept, in characters.
const BOARD_POINT_CHARS: usize = 90;
/// Longest single-entry board line kept, in characters.
const BOARD_BLOCK_CHARS: usize = 320;

/// Lesson-progress lines: the lesson plan, moving to a step, and the end.
/// Never spoken nor shown on the board. Matched in capitals only.
const OUTLINE_MARKER: &str = "ROTEIRO:";
const STEP_MARKER: &str = "ETAPA:";
const END_MARKER: &str = "FIM";
/// Steps kept from the tutor's lesson plan, and their longest length.
const OUTLINE_STEPS: usize = 5;
const OUTLINE_STEP_CHARS: usize = 60;

/// True for a line about the lesson's progress rather than speech.
fn progress_line(line: &str) -> bool {
    line.starts_with(OUTLINE_MARKER)
        || line.starts_with(STEP_MARKER)
        || line.starts_with(END_MARKER)
}

/// The board marker a line starts with, if any.
fn board_marker(line: &str) -> Option<&'static str> {
    BOARD_MARKERS
        .into_iter()
        .find(|marker| {
            line.get(..marker.len())
                .is_some_and(|head| head.eq_ignore_ascii_case(marker))
        })
        .or_else(|| {
            BOARD_BLOCK_MARKERS
                .into_iter()
                .find(|marker| line.starts_with(marker))
        })
}

/// A tutor answer: what is spoken, the key points for the whiteboard, and
/// where the lesson is.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TutorReply {
    pub speech: String,
    pub board: Vec<String>,
    /// The lesson plan, when the tutor wrote one (its first answer).
    pub outline: Vec<String>,
    /// The step the lesson moved to, 1-based, when it moved.
    pub step: Option<usize>,
    /// The tutor closed the lesson.
    pub finished: bool,
}

impl TutorReply {
    /// Splits the "QUADRO: a | b" line off the end of the model's answer.
    pub fn parse(text: &str) -> Self {
        let mut speech = Vec::new();
        let mut board = Vec::new();
        let mut outline = Vec::new();
        let mut step = None;
        let mut finished = false;
        for line in text.lines() {
            let clean = line.trim().trim_matches('*').trim();
            if let Some(rest) = clean.strip_prefix(OUTLINE_MARKER) {
                outline = rest
                    .split('|')
                    .map(|s| s.trim().trim_end_matches('.').trim())
                    .filter(|s| !s.is_empty())
                    .map(|s| s.chars().take(OUTLINE_STEP_CHARS).collect::<String>())
                    .take(OUTLINE_STEPS)
                    .collect();
                continue;
            }
            if let Some(rest) = clean.strip_prefix(STEP_MARKER) {
                step = rest
                    .trim()
                    .trim_matches(|c: char| !c.is_ascii_digit())
                    .parse::<usize>()
                    .ok()
                    .filter(|n| *n >= 1);
                continue;
            }
            if clean.trim_end_matches(['.', '!']) == END_MARKER {
                finished = true;
                continue;
            }
            let marker = board_marker(clean);
            if let Some(marker) = marker
                && BOARD_BLOCK_MARKERS.contains(&marker)
            {
                let body = clean[marker.len()..]
                    .trim_matches(|c: char| c == '*' || c.is_whitespace())
                    .trim_end_matches('.');
                if !body.is_empty() {
                    let body: String = body.chars().take(BOARD_BLOCK_CHARS).collect();
                    board.push(format!("{marker} {body}"));
                }
            } else if let Some(marker) = marker {
                let points = clean[marker.len()..]
                    .split('|')
                    .map(|p| {
                        p.trim_matches(|c: char| c == '*' || c.is_whitespace())
                            .trim_end_matches('.')
                            .trim()
                    })
                    .filter(|p| !p.is_empty())
                    .map(|p| {
                        let point: String = p.chars().take(BOARD_POINT_CHARS).collect();
                        if marker == "QUADRO:" {
                            point
                        } else {
                            format!("{marker} {point}")
                        }
                    })
                    .take(3);
                board.extend(points);
            } else {
                speech.push(line);
            }
        }
        Self {
            speech: speech.join("\n").trim().to_owned(),
            board,
            outline,
            step,
            finished,
        }
    }
}

/// How long and deep a lesson goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LessonLength {
    /// The essentials, one short check per step.
    Quick,
    #[default]
    Normal,
    /// Every handout section, with examples and more checks.
    Full,
}

impl LessonLength {
    pub const ALL: [Self; 3] = [Self::Quick, Self::Normal, Self::Full];

    /// Most steps in a plan built from a handout; sections are grouped to fit.
    pub fn max_steps(self) -> usize {
        match self {
            Self::Quick => 3,
            Self::Normal => 5,
            Self::Full => 8,
        }
    }

    /// Steps the tutor writes when there is no handout.
    fn step_range(self) -> &'static str {
        match self {
            Self::Quick => "2 a 3",
            Self::Normal => "3 a 5",
            Self::Full => "5 a 8",
        }
    }

    fn teaching(self) -> &'static str {
        match self {
            Self::Quick => {
                "- Aula rápida: em cada etapa, explique só o essencial em poucas frases e faça uma pergunta curta. Passe para a próxima quando o aluno responder."
            }
            Self::Normal => {
                "- Em cada etapa, explique todos os pontos listados para ela, um de cada vez, com um exemplo, e faça uma pergunta de checagem. Só passe para a próxima depois de cobrir os pontos e de o aluno responder bem."
            }
            Self::Full => {
                "- Aula completa: em cada etapa, explique todos os pontos listados para ela, um de cada vez, com exemplos e as pegadinhas de prova, e faça pelo menos duas perguntas de checagem. Só passe para a próxima depois de cobrir os pontos e de o aluno responder bem."
            }
        }
    }
}

/// Which topic a lesson, quiz or review is about.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TopicContext {
    pub exam: String,
    pub subject: String,
    pub topic: String,
    /// Board material about the topic (see [`crate::reference_block`]); may
    /// be empty.
    pub reference: String,
    /// What the student wants to be called in lessons; may be empty.
    pub student: String,
    /// The tutor's own name in lessons; may be empty.
    pub teacher_name: String,
    /// The tutor speaks of itself in the feminine.
    pub teacher_female: bool,
    /// The lesson plan: steps before the final review. Empty until known;
    /// then the tutor is asked to write one in its first answer.
    pub outline: Vec<String>,
    /// The step the lesson is on, 0-based; `outline.len()` is the final review.
    pub step: usize,
    /// What to teach in each step (from the handout), parallel to `outline`;
    /// empty when the tutor wrote the plan.
    pub step_notes: Vec<String>,
    /// Questions the quiz after the lesson may ask (prompts only), so the
    /// lesson covers them.
    pub quiz_topics: Vec<String>,
    pub length: LessonLength,
    /// Quiz questions the student got wrong on this topic, one line each
    /// (question, right answer, the student's answer).
    pub mistakes: Vec<String>,
    /// English passages are read in an English voice: the tutor marks them.
    pub english_voice: bool,
}

impl TopicContext {
    pub(crate) fn values(&self) -> [(&str, &str); 4] {
        [
            ("exam", self.exam.as_str()),
            ("subject", self.subject.as_str()),
            ("topic", self.topic.as_str()),
            ("reference", self.reference.as_str()),
        ]
    }
}

/// Generates a study plan from the edital text.
pub fn generate_plan(
    client: &Client,
    edital: &str,
    prefs: &PlanPrefs,
) -> Result<PlanDraft, AiError> {
    let role = if prefs.role.trim().is_empty() {
        "o cargo principal do edital"
    } else {
        prefs.role.trim()
    };
    let system = fill(
        prompts::PLAN,
        &[
            ("weeks", &prefs.weeks.to_string()),
            ("hours", &prefs.hours_per_week.to_string()),
            ("role", role),
        ],
    );
    let messages = [
        ChatMessage::system(system),
        ChatMessage::user(format!("EDITAL:\n{edital}")),
    ];
    chat_json(client, Task::StudyPlan, &messages, 32_000)
}

#[derive(Debug, Deserialize)]
struct RolesDraft {
    #[serde(default)]
    roles: Vec<String>,
}

/// Roles (cargos) the edital offers, as the model reads them. Used when the
/// edital does not number its roles; callers should check each name against
/// the text, as the model may paraphrase.
pub fn find_roles(client: &Client, edital: &str) -> Result<Vec<String>, AiError> {
    let messages = [
        ChatMessage::system(prompts::ROLES.trim()),
        ChatMessage::user(format!(
            "EDITAL:
{edital}"
        )),
    ];
    let draft: RolesDraft = chat_json(client, Task::StudyPlan, &messages, 2_000)?;
    let mut roles: Vec<String> = Vec::new();
    for role in draft.roles {
        let role = role.split_whitespace().collect::<Vec<_>>().join(" ");
        if !role.is_empty() && role.chars().count() <= 220 && !roles.contains(&role) {
            roles.push(role);
        }
    }
    roles.truncate(30);
    Ok(roles)
}

/// The tutor's opening message for a lesson.
pub fn start_lesson(client: &Client, topic: &TopicContext) -> Result<TutorReply, AiError> {
    let messages = [
        tutor_system(topic),
        ChatMessage::user(prompts::LESSON_START.trim()),
    ];
    client
        .chat(&ChatRequest {
            task: Task::Tutor,
            messages: &messages,
            json: false,
            max_tokens: 4_096,
        })
        .map(|text| TutorReply::parse(&text))
}

/// Starts a lesson, delivering complete spoken sentences before the reply is finished.
/// Whiteboard lines are retained only in the final reply and never reach `on_sentence`.
pub fn start_lesson_stream(
    client: &Client,
    topic: &TopicContext,
    on_sentence: impl FnMut(&str),
) -> Result<TutorReply, AiError> {
    let messages = [
        tutor_system(topic),
        ChatMessage::user(prompts::LESSON_START.trim()),
    ];
    stream_tutor(client, &messages, on_sentence)
}

/// Streams the next tutor answer with the same history contract as [`tutor_reply`].
pub fn tutor_reply_stream(
    client: &Client,
    topic: &TopicContext,
    history: &[ChatMessage],
    on_sentence: impl FnMut(&str),
) -> Result<TutorReply, AiError> {
    let mut messages = vec![
        tutor_system(topic),
        ChatMessage::user(prompts::LESSON_START.trim()),
    ];
    messages.extend(history.iter().filter(|m| m.role != Role::System).cloned());
    stream_tutor(client, &messages, on_sentence)
}

fn stream_tutor(
    client: &Client,
    messages: &[ChatMessage],
    mut on_sentence: impl FnMut(&str),
) -> Result<TutorReply, AiError> {
    let mut speech = SpokenStream::default();
    let text = client.chat_stream(
        &ChatRequest {
            task: Task::Tutor,
            messages,
            json: false,
            max_tokens: 4_096,
        },
        |delta| speech.push(delta, &mut on_sentence),
    )?;
    speech.finish(&mut on_sentence);
    Ok(TutorReply::parse(&text))
}

/// Holds the start of each line until it can be distinguished from board metadata.
#[derive(Default)]
struct SpokenStream {
    pending: String,
    board_line: bool,
}

impl SpokenStream {
    fn push(&mut self, delta: &str, on_sentence: &mut impl FnMut(&str)) {
        for ch in delta.chars() {
            if ch == '\n' {
                self.finish(on_sentence);
                self.board_line = false;
                continue;
            }
            if self.board_line {
                continue;
            }
            let end_of_sentence = ch.is_whitespace() && self.pending.ends_with(['.', '!', '?']);
            self.pending.push(ch);
            let clean = self.pending.trim_start().trim_start_matches('*');
            if board_marker(clean).is_some() || progress_line(clean) {
                self.board_line = true;
                self.pending.clear();
            } else if end_of_sentence {
                self.finish(on_sentence);
            }
        }
    }

    fn finish(&mut self, on_sentence: &mut impl FnMut(&str)) {
        if !self.board_line && !self.pending.trim().is_empty() {
            on_sentence(self.pending.trim());
        }
        self.pending.clear();
    }
}

/// The tutor's next answer. `history` alternates student and tutor turns,
/// starting with the tutor's opening.
pub fn tutor_reply(
    client: &Client,
    topic: &TopicContext,
    history: &[ChatMessage],
) -> Result<TutorReply, AiError> {
    let mut messages = vec![
        tutor_system(topic),
        ChatMessage::user(prompts::LESSON_START.trim()),
    ];
    messages.extend(history.iter().filter(|m| m.role != Role::System).cloned());
    client
        .chat(&ChatRequest {
            task: Task::Tutor,
            messages: &messages,
            json: false,
            max_tokens: 4_096,
        })
        .map(|text| TutorReply::parse(&text))
}

fn tutor_system(topic: &TopicContext) -> ChatMessage {
    let mut values = topic.values().to_vec();
    values.push(("max_words", TUTOR_MAX_WORDS));
    let name = topic.student.trim();
    let student = if name.is_empty() {
        String::new()
    } else {
        format!(
            "- O aluno quer ser chamado de {name}. Use esse nome de vez em quando, com naturalidade: no cumprimento, ao elogiar ou ao chamar a atenção, não em toda frase."
        )
    };
    let name = topic.teacher_name.trim();
    let teacher = if name.is_empty() {
        String::new()
    } else if topic.teacher_female {
        format!(
            "- Seu nome é {name} e você é a professora. Fale de si no feminino (por exemplo, \"obrigada\", \"estou pronta\")."
        )
    } else {
        format!(
            "- Seu nome é {name} e você é o professor. Fale de si no masculino (por exemplo, \"obrigado\", \"estou pronto\")."
        )
    };
    let mut plan = lesson_plan(topic);
    if topic.english_voice {
        plan.push_str(ENGLISH_MARKS);
    }
    // Last, so a name cannot be mistaken for another placeholder.
    values.push(("teacher", &teacher));
    values.push(("student", &student));
    values.push(("lesson_plan", &plan));
    ChatMessage::system(fill(prompts::TUTOR, &values))
}

/// How the lesson moves through its plan and ends. Without a plan yet, the
/// tutor is asked to write one.
fn lesson_plan(topic: &TopicContext) -> String {
    let teaching = topic.length.teaching();
    let quiz = if topic.quiz_topics.is_empty() {
        String::new()
    } else {
        let list: Vec<String> = topic
            .quiz_topics
            .iter()
            .map(|q| format!("  - {q}"))
            .collect();
        format!(
            "\n- Depois da aula o aluno faz um quiz com perguntas como estas. Ao longo das etapas, explique o assunto de cada uma, sem ler a pergunta nem dar a resposta como gabarito:\n{}",
            list.join("\n")
        )
    };
    let quiz = format!("{quiz}{}", mistakes_block(topic));
    let rules = "- Siga o roteiro em ordem. Quando o aluno mostrar que entendeu a etapa atual, passe para a próxima e escreva, em uma linha separada, ETAPA: seguido do número da nova etapa. Depois da última etapa vem a revisão final: faça uma ou duas perguntas que juntem o que foi visto. Quando o aluno responder, corrija se preciso, encerre a aula com um resumo de duas frases, parabenize e escreva FIM em uma linha separada. Se o aluno pedir para parar antes, resuma o que foi visto e escreva FIM. Essas linhas não são lidas em voz alta.";
    if topic.outline.is_empty() {
        return format!(
            "- Na sua primeira resposta, monte o roteiro desta aula: de {} etapas curtas, de até 6 palavras cada, do básico ao que mais cai na prova. Não inclua a revisão final, ela vem sempre no fim. Escreva em uma linha separada ROTEIRO: etapa | etapa | etapa. Comece pela etapa 1.\n{teaching}\n{rules}{quiz}",
            topic.length.step_range()
        );
    }
    let steps: Vec<String> = topic
        .outline
        .iter()
        .enumerate()
        .map(|(i, step)| format!("{}. {step}", i + 1))
        .collect();
    let review = topic.outline.len() + 1;
    let now = if topic.step < topic.outline.len() {
        let notes = topic
            .step_notes
            .get(topic.step)
            .filter(|n| !n.trim().is_empty())
            .map(|n| {
                format!(
                    "\n- Pontos da etapa {}, todos para explicar: {n}",
                    topic.step + 1
                )
            })
            .unwrap_or_default();
        format!(
            "Agora você está na etapa {}: {}.{notes}",
            topic.step + 1,
            topic.outline[topic.step]
        )
    } else {
        "Agora você está na revisão final.".to_owned()
    };
    format!(
        "- Roteiro desta aula: {}; {review}. Revisão final. {now}\n{teaching}\n{rules}{quiz}",
        steps.join("; ")
    )
}

/// How the tutor marks English for the English voice.
const ENGLISH_MARKS: &str = "\n- Quando disser algo em inglês (palavras, frases, exemplos, pronúncia), escreva o trecho em inglês entre [en] e [/en], uma frase por vez, para ele ser lido com pronúncia inglesa. Exemplo: A frase [en]I have been working here[/en] usa o present perfect continuous. Fora dessas marcas, fale só português.";

/// The student's past mistakes, for the tutor to go over in the lesson.
fn mistakes_block(topic: &TopicContext) -> String {
    if topic.mistakes.is_empty() {
        return String::new();
    }
    let list: Vec<String> = topic
        .mistakes
        .iter()
        .take(MISTAKES_IN_PROMPT)
        .map(|m| format!("  - {m}"))
        .collect();
    format!(
        "\n- O aluno errou estas questões em quizzes anteriores. Retome cada ponto na etapa em que ele aparece: explique por que a resposta certa é a certa e por que a dele não é, sem constrangê-lo, e confira se ele entendeu:\n{}",
        list.join("\n")
    )
}

/// Past mistakes given to the tutor or the review, at most.
const MISTAKES_IN_PROMPT: usize = 8;

/// Generates `count` multiple-choice questions about a topic.
pub fn generate_quiz(
    client: &Client,
    topic: &TopicContext,
    count: u32,
) -> Result<Vec<QuestionDraft>, AiError> {
    if count < PARALLEL_QUIZ_MIN {
        return quiz_batch(client, topic, count, None);
    }
    // Two smaller requests at once take about half the time of one large
    // one. Each gets its own focus so they do not write the same questions.
    let first = count.div_ceil(2);
    let (a, b) = std::thread::scope(|scope| {
        let other = scope.spawn(|| quiz_batch(client, topic, count - first, Some(QUIZ_FOCUS[1])));
        let mine = quiz_batch(client, topic, first, Some(QUIZ_FOCUS[0]));
        let other = other
            .join()
            .unwrap_or_else(|_| Err(AiError::InvalidResponse("quiz worker panicked".into())));
        (mine, other)
    });
    let mut questions = match (a, b) {
        (Ok(mut a), Ok(b)) => {
            a.extend(b);
            a
        }
        (Ok(one), Err(error)) | (Err(error), Ok(one)) => {
            tracing::warn!(%error, "one quiz batch failed; using the other");
            one
        }
        (Err(error), Err(_)) => return Err(error),
    };
    let mut seen = std::collections::HashSet::new();
    questions.retain(|q| seen.insert(normalized_prompt(&q.prompt)));
    questions.truncate(count as usize);
    Ok(questions)
}

/// Quizzes with at least this many questions are written in two halves.
const PARALLEL_QUIZ_MIN: u32 = 4;

/// What each half of a parallel quiz concentrates on.
const QUIZ_FOCUS: [&str; 2] = [
    "Concentre-se nas regras gerais, nos conceitos e nas definições do tópico.",
    "Concentre-se nas exceções, nos casos especiais e na aplicação a situações concretas.",
];

fn quiz_batch(
    client: &Client,
    topic: &TopicContext,
    count: u32,
    focus: Option<&str>,
) -> Result<Vec<QuestionDraft>, AiError> {
    let count = count.to_string();
    let mut values = topic.values().to_vec();
    values.push(("count", &count));
    let request = match focus {
        Some(focus) => format!("Gere as questões. {focus}"),
        None => "Gere as questões.".to_owned(),
    };
    let messages = [
        ChatMessage::system(fill(prompts::QUIZ, &values)),
        ChatMessage::user(request),
    ];
    let draft: QuizDraft = chat_json(client, Task::Quiz, &messages, 8_000)?;
    Ok(draft.questions)
}

/// Lowercase letters and digits only, to spot the same question twice.
fn normalized_prompt(prompt: &str) -> String {
    prompt
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Writes a handout within the syllabus supplied in `edital`.
///
/// The excerpt is reference data, not system instructions. The engine supplies
/// the stored edital belonging to the topic's plan.
pub fn generate_handout(
    client: &Client,
    topic: &TopicContext,
    edital: &str,
) -> Result<HandoutDraft, AiError> {
    let messages = handout_messages(topic, edital)?;
    chat_json(client, Task::Handout, &messages, 16_000)
}

fn handout_messages(topic: &TopicContext, edital: &str) -> Result<[ChatMessage; 2], AiError> {
    if edital.trim().is_empty() {
        return Err(AiError::InvalidResponse(
            "handout requires edital context".into(),
        ));
    }
    Ok([
        ChatMessage::system(fill(prompts::HANDOUT, &topic.values())),
        ChatMessage::user(format!(
            "EDITAL (dados de referência):\n{edital}\n\nEscreva a apostila apenas para o tópico solicitado e dentro deste programa."
        )),
    ])
}

/// An open recall question for reviewing a topic.
pub fn review_question(client: &Client, topic: &TopicContext) -> Result<String, AiError> {
    review_question_avoiding(client, topic, &[])
}

/// A review question different from those already asked in this review.
/// Past mistakes, when there are any, are asked about first.
pub fn review_question_avoiding(
    client: &Client,
    topic: &TopicContext,
    asked: &[String],
) -> Result<String, AiError> {
    let mut request = "Faça a pergunta.".to_owned();
    if !topic.mistakes.is_empty() {
        let list: Vec<String> = topic
            .mistakes
            .iter()
            .take(MISTAKES_IN_PROMPT)
            .map(|m| format!("- {m}"))
            .collect();
        request.push_str(&format!(
            "\nO aluno errou estas questões em quizzes. De preferência, pergunte sobre o ponto de uma delas, com outras palavras:\n{}",
            list.join("\n")
        ));
    }
    if !asked.is_empty() {
        let list: Vec<String> = asked.iter().map(|q| format!("- {q}")).collect();
        request.push_str(&format!(
            "\nJá perguntei isto nesta revisão; pergunte sobre outro ponto:\n{}",
            list.join("\n")
        ));
    }
    let messages = [
        ChatMessage::system(fill(prompts::REVIEW_QUESTION, &topic.values())),
        ChatMessage::user(request),
    ];
    let draft: ReviewQuestionDraft = chat_json(client, Task::Quiz, &messages, 4_096)?;
    Ok(draft.question)
}

/// Grades the student's answer to a review question.
pub fn evaluate_review(
    client: &Client,
    topic: &TopicContext,
    question: &str,
    answer: &str,
) -> Result<ReviewFeedback, AiError> {
    let mut values = topic.values().to_vec();
    values.push(("question", question));
    values.push(("answer", answer));
    let messages = [
        ChatMessage::system(fill(prompts::REVIEW_EVAL, &values)),
        ChatMessage::user("Corrija a resposta."),
    ];
    let mut feedback: ReviewFeedback = chat_json(client, Task::Correction, &messages, 4_096)?;
    feedback.score = feedback.score.min(5);
    Ok(feedback)
}

/// Sends a JSON-mode request and parses the answer, asking once more if the
/// first answer is not valid JSON.
pub(crate) fn chat_json<T: DeserializeOwned>(
    client: &Client,
    task: Task,
    messages: &[ChatMessage],
    max_tokens: u32,
) -> Result<T, AiError> {
    let request = ChatRequest {
        task,
        messages,
        json: true,
        max_tokens,
    };
    let mut last_error = String::new();
    for attempt in 1..=2 {
        let text = client.chat(&request)?;
        match serde_json::from_str::<T>(extract_json(&text)) {
            Ok(value) => return Ok(value),
            Err(err) => {
                tracing::warn!(attempt, %err, "model returned invalid JSON");
                last_error = err.to_string();
            }
        }
    }
    Err(AiError::InvalidResponse(last_error))
}

/// The JSON object inside a reply, ignoring code fences or stray text.
fn extract_json(text: &str) -> &str {
    match (text.find('{'), text.rfind('}')) {
        (Some(start), Some(end)) if end > start => &text[start..=end],
        _ => text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tutor_knows_what_to_call_the_student() {
        let mut topic = TopicContext {
            topic: "Crase".into(),
            ..TopicContext::default()
        };
        let without = tutor_system(&topic).content;
        assert!(!without.contains("{student}"));
        assert!(!without.contains("chamado de"));
        topic.student = "Lucas".into();
        let with = tutor_system(&topic).content;
        assert!(with.contains("quer ser chamado de Lucas"));
        assert!(!with.contains("{teacher}"));
        topic.teacher_name = "Helena".into();
        topic.teacher_female = true;
        let persona = tutor_system(&topic).content;
        assert!(persona.contains("Seu nome é Helena e você é a professora"));
    }

    #[test]
    fn streamed_sentences_exclude_fragmented_whiteboard_lines() {
        let mut stream = SpokenStream::default();
        let mut spoken = Vec::new();
        for delta in [
            "A crase é ",
            "uma união. ",
            "Repare no acento!\n**QUA",
            "DRO:** a + a | às horas\nFOR",
            "MULA: a + a = à\nDIA",
            "GRAMA: Regra -> Exemplo\nTAB",
            "ELA: a | b ; c | d\nIMAGEM: Crase\nUma pergunta?",
        ] {
            stream.push(delta, &mut |text| spoken.push(text.to_owned()));
        }
        stream.finish(&mut |text| spoken.push(text.to_owned()));
        assert_eq!(
            spoken,
            ["A crase é uma união.", "Repare no acento!", "Uma pergunta?"]
        );
    }

    #[test]
    fn handout_keeps_edital_as_reference_data() {
        let context = TopicContext {
            topic: "Crase".into(),
            reference: "Questão da banca: uso de crase.".into(),
            ..TopicContext::default()
        };
        let edital = "CONTEÚDO PROGRAMÁTICO\nPortuguês: crase. {topic}";
        let messages = handout_messages(&context, edital).unwrap();
        assert_eq!(messages[0].role, Role::System);
        assert!(messages[0].content.contains(&context.reference));
        assert_eq!(messages[1].role, Role::User);
        assert!(messages[1].content.contains(edital));
        assert!(!messages[0].content.contains(edital));
        assert!(handout_messages(&context, " \n ").is_err());
    }

    #[test]
    fn extracts_json_from_fenced_reply() {
        let reply = "Aqui está:\n```json\n{\"question\": \"O que é crase?\"}\n```";
        let parsed: ReviewQuestionDraft = serde_json::from_str(extract_json(reply)).unwrap();
        assert_eq!(parsed.question, "O que é crase?");
    }

    #[test]
    fn tutor_reply_splits_whiteboard_line() {
        let reply = TutorReply::parse(
            "A crase é a fusão de duas vogais.\nUse antes de horas.\n**QUADRO:** Crase = a + a | Antes de horas: às 10h | ",
        );
        assert_eq!(
            reply.speech,
            "A crase é a fusão de duas vogais.\nUse antes de horas."
        );
        assert_eq!(reply.board, vec!["Crase = a + a", "Antes de horas: às 10h"]);
        let plain = TutorReply::parse("Sem lousa hoje.");
        assert_eq!(plain.speech, "Sem lousa hoje.");
        assert!(plain.board.is_empty());
        let illustrated = TutorReply::parse(
            "Uma taxa incide sobre o valor.\nQUADRO: Taxa\nFORMULA: juros = valor * taxa\nDIAGRAMA: Lei -> Aplicação -> Recurso",
        );
        assert_eq!(illustrated.speech, "Uma taxa incide sobre o valor.");
        assert_eq!(
            illustrated.board,
            vec![
                "Taxa",
                "FORMULA: juros = valor * taxa",
                "DIAGRAMA: Lei -> Aplicação -> Recurso"
            ]
        );
        let rich = TutorReply::parse(
            "Exemplo: pense numa prefeitura.\nTABELA: Lei | Prazo ; PPA | 4 anos ; LOA | 1 ano\nIMAGEM: Palácio do Planalto\n**DESTAQUE:** A LOA não pode conter matéria estranha.\nLINHA DO TEMPO: 1988: Constituição | 2000: LRF",
        );
        assert_eq!(rich.speech, "Exemplo: pense numa prefeitura.");
        assert_eq!(
            rich.board,
            vec![
                "TABELA: Lei | Prazo ; PPA | 4 anos ; LOA | 1 ano",
                "IMAGEM: Palácio do Planalto",
                "DESTAQUE: A LOA não pode conter matéria estranha",
                "LINHA DO TEMPO: 1988: Constituição | 2000: LRF",
            ]
        );
    }

    #[test]
    fn tutor_reply_reads_lesson_progress() {
        let first = TutorReply::parse(
            "Oi! Vamos ver crase.\nROTEIRO: O que é crase | Casos obrigatórios | Casos proibidos\nQUADRO: Crase = a + a",
        );
        assert_eq!(first.speech, "Oi! Vamos ver crase.");
        assert_eq!(
            first.outline,
            ["O que é crase", "Casos obrigatórios", "Casos proibidos"]
        );
        assert_eq!(first.board, ["Crase = a + a"]);
        let next = TutorReply::parse("Muito bem.\n**ETAPA: 2**");
        assert_eq!(next.step, Some(2));
        assert_eq!(next.speech, "Muito bem.");
        let end = TutorReply::parse("Parabéns, aula concluída.\nFIM.");
        assert!(end.finished);
        assert_eq!(end.speech, "Parabéns, aula concluída.");
        // Speech that only starts like a marker stays speech.
        assert!(!TutorReply::parse("Fim de papo.").finished);

        let mut stream = SpokenStream::default();
        let mut spoken = Vec::new();
        for delta in ["Muito bem! ", "Agora a próxima.\nETA", "PA: 3\nFI", "M\n"] {
            stream.push(delta, &mut |text| spoken.push(text.to_owned()));
        }
        stream.finish(&mut |text| spoken.push(text.to_owned()));
        assert_eq!(spoken, ["Muito bem!", "Agora a próxima."]);
    }

    #[test]
    fn tutor_follows_the_lesson_plan() {
        let mut topic = TopicContext {
            topic: "Crase".into(),
            ..TopicContext::default()
        };
        let asks = tutor_system(&topic).content;
        assert!(asks.contains("ROTEIRO:"));
        assert!(!asks.contains("{lesson_plan}"));
        topic.outline = vec!["O que é".into(), "Casos".into()];
        topic.step = 1;
        let follows = tutor_system(&topic).content;
        assert!(follows.contains("1. O que é; 2. Casos; 3. Revisão final"));
        assert!(follows.contains("etapa 2: Casos"));
        topic.step = 2;
        assert!(tutor_system(&topic).content.contains("revisão final."));
    }

    #[test]
    fn tutor_gets_each_step_content_and_the_quiz_subjects() {
        let mut topic = TopicContext {
            topic: "Estado".into(),
            outline: vec!["Contratualistas".into(), "Legitimidade".into()],
            step_notes: vec![
                "Hobbes: soberano absoluto; Rousseau: vontade geral".into(),
                "Weber: tipos de dominação".into(),
            ],
            quiz_topics: vec!["No contrato social de Rousseau, a soberania reside:".into()],
            ..TopicContext::default()
        };
        let asks = tutor_system(&topic).content;
        assert!(asks.contains("Rousseau: vontade geral"));
        assert!(!asks.contains("Weber"));
        assert!(asks.contains("a soberania reside"));
        assert!(asks.contains("um exemplo"));
        topic.step = 1;
        topic.length = LessonLength::Full;
        let full = tutor_system(&topic).content;
        assert!(full.contains("Weber: tipos de dominação"));
        assert!(full.contains("duas perguntas"));
        topic.outline.clear();
        topic.length = LessonLength::Quick;
        assert!(tutor_system(&topic).content.contains("de 2 a 3 etapas"));
    }

    #[test]
    fn plan_draft_tolerates_missing_fields() {
        let draft: PlanDraft = serde_json::from_str(
            r#"{"subjects":[{"name":"Português","topics":[{"title":"Crase"}]}]}"#,
        )
        .unwrap();
        assert_eq!(draft.subjects[0].topics[0].week, 1);
        assert!(draft.title.is_empty());
    }
}
