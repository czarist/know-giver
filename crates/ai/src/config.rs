//! Endpoint and per-task model settings.

use serde::{Deserialize, Serialize};

/// What a request is for. Each task can use a different model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Task {
    StudyPlan,
    Tutor,
    Quiz,
    Correction,
    Handout,
    /// Solves handout exercises blind to check their answer keys.
    Verify,
    Vision,
}

/// Language the prompts are written in and the model is asked to answer in.
/// It follows the app's language.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum PromptLanguage {
    #[default]
    Portuguese,
    English,
    Japanese,
    /// Simplified Chinese (mainland, Singapore).
    ChineseSimplified,
    /// Traditional Chinese (Taiwan, Hong Kong).
    ChineseTraditional,
}

impl PromptLanguage {
    /// From a locale such as "pt-BR", "en", "ja", "zh-Hans" or "zh-Hant";
    /// anything else is Portuguese.
    pub fn from_locale(locale: &str) -> Self {
        let locale = locale.trim().to_ascii_lowercase().replace('_', "-");
        if locale.starts_with("en") {
            Self::English
        } else if locale.starts_with("ja") {
            Self::Japanese
        } else if ["zh-hant", "zh-tw", "zh-hk", "zh-mo"]
            .iter()
            .any(|t| locale.starts_with(t))
        {
            Self::ChineseTraditional
        } else if locale.starts_with("zh") {
            Self::ChineseSimplified
        } else {
            Self::Portuguese
        }
    }

    /// From `KG_PROMPT_LANG` ("pt", "en", "ja", "zh-Hans", "zh-Hant"), for the examples.
    pub fn from_env() -> Self {
        Self::from_locale(&std::env::var("KG_PROMPT_LANG").unwrap_or_default())
    }

    pub fn code(self) -> &'static str {
        match self {
            Self::Portuguese => "pt",
            Self::English => "en",
            Self::Japanese => "ja",
            Self::ChineseSimplified => "zh-Hans",
            Self::ChineseTraditional => "zh-Hant",
        }
    }

    /// The language short instructions written in code fall back to:
    /// Portuguese, or English for every other language. The prompt files
    /// themselves exist in every language and set the reply language.
    pub fn base(self) -> Self {
        match self {
            Self::Portuguese => Self::Portuguese,
            _ => Self::English,
        }
    }

    /// Chinese or Japanese: no spaces between words.
    pub fn is_cjk(self) -> bool {
        matches!(
            self,
            Self::Japanese | Self::ChineseSimplified | Self::ChineseTraditional
        )
    }

    /// The text in this language: Portuguese, or English for the others
    /// (see [`Self::base`]).
    pub fn pick<'a>(self, portuguese: &'a str, english: &'a str) -> &'a str {
        match self.base() {
            Self::Portuguese => portuguese,
            _ => english,
        }
    }
}

/// Model names per task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ModelSet {
    /// Heavy task: study plan generation.
    pub study_plan: String,
    /// Fast model for lessons.
    pub tutor: String,
    pub quiz: String,
    /// Grades review answers.
    pub correction: String,
    /// Writes study handouts. Empty means "same as the study plan model",
    /// which keeps settings saved before this field existed working.
    pub handout: String,
    /// Solves handout exercises without the key. Empty means "same as the
    /// correction model"; a different model family catches more wrong keys.
    pub verifier: String,
    /// Optional OCR model. Empty disables sending scanned PDF pages to the endpoint.
    pub vision: String,
    /// Used when the task's model is rate limited or unavailable.
    pub fallback: String,
}

impl Default for ModelSet {
    /// Local-first default: one Ollama model for everything.
    fn default() -> Self {
        let model = "llama3.1".to_owned();
        Self {
            study_plan: model.clone(),
            tutor: model.clone(),
            quiz: model.clone(),
            correction: model.clone(),
            handout: String::new(),
            verifier: String::new(),
            vision: String::new(),
            fallback: model,
        }
    }
}

impl ModelSet {
    pub fn for_task(&self, task: Task) -> &str {
        match task {
            Task::StudyPlan => &self.study_plan,
            Task::Tutor => &self.tutor,
            Task::Quiz => &self.quiz,
            Task::Correction => &self.correction,
            Task::Handout if self.handout.trim().is_empty() => &self.study_plan,
            Task::Handout => &self.handout,
            Task::Verify if self.verifier.trim().is_empty() => &self.correction,
            Task::Verify => &self.verifier,
            Task::Vision => &self.vision,
        }
    }
}

/// Connection settings for an OpenAI-compatible chat endpoint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AiConfig {
    /// Base URL including the version prefix (e.g. `/v1`).
    pub base_url: String,
    /// Optional: local servers like Ollama do not need one.
    pub api_key: String,
    pub models: ModelSet,
    /// Ask reasoning models for minimal reasoning, which cuts latency a lot
    /// and keeps long outputs (study plans) within the token budget.
    pub fast_responses: bool,
}

impl Default for AiConfig {
    /// Local-first default: Ollama on its standard port.
    fn default() -> Self {
        Self {
            base_url: "http://localhost:11434/v1".into(),
            api_key: String::new(),
            models: ModelSet::default(),
            fast_responses: true,
        }
    }
}

impl AiConfig {
    /// Builds a config from environment-style variables (`OPENAI_API_BASE`,
    /// `OPENAI_API_KEY`, `MODEL_*`), keeping defaults for anything missing.
    pub fn from_env(get: impl Fn(&str) -> Option<String>) -> Self {
        let get = |key: &str| {
            get(key)
                .map(|v| v.trim().to_owned())
                .filter(|v| !v.is_empty())
        };
        let mut config = Self::default();
        if let Some(base) = get("OPENAI_API_BASE") {
            config.base_url = base;
        }
        if let Some(key) = get("OPENAI_API_KEY") {
            config.api_key = key;
        }
        let m = &mut config.models;
        // A single tutor model is the most sensible default for the others.
        if let Some(tutor) = get("MODEL_TUTOR") {
            m.study_plan = tutor.clone();
            m.quiz = tutor.clone();
            m.correction = tutor.clone();
            m.fallback = tutor.clone();
            m.tutor = tutor;
        }
        for (key, slot) in [
            ("MODEL_STUDY_PLAN", &mut m.study_plan),
            ("MODEL_QUIZ", &mut m.quiz),
            ("MODEL_CORRECTION", &mut m.correction),
            ("MODEL_FALLBACK", &mut m.fallback),
            ("MODEL_HANDOUT", &mut m.handout),
            ("MODEL_VERIFIER", &mut m.verifier),
            ("MODEL_VISION", &mut m.vision),
        ] {
            if let Some(value) = get(key) {
                *slot = value;
            }
        }
        config
    }

    pub(crate) fn chat_url(&self) -> String {
        format!("{}/chat/completions", self.base_url.trim_end_matches('/'))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    #[test]
    fn reads_env_with_per_task_models() {
        let env: HashMap<&str, &str> = [
            ("OPENAI_API_BASE", "https://api.z.ai/api/paas/v4/"),
            ("OPENAI_API_KEY", " secret "),
            ("MODEL_STUDY_PLAN", "glm-big"),
            ("MODEL_TUTOR", "glm-flash"),
            ("MODEL_FALLBACK", ""),
        ]
        .into();
        let config = AiConfig::from_env(|k| env.get(k).map(|v| (*v).to_owned()));
        assert_eq!(config.api_key, "secret");
        assert_eq!(config.models.study_plan, "glm-big");
        assert_eq!(config.models.quiz, "glm-flash");
        assert_eq!(config.models.fallback, "glm-flash");
        assert_eq!(
            config.chat_url(),
            "https://api.z.ai/api/paas/v4/chat/completions"
        );
    }

    #[test]
    fn handout_model_falls_back_to_plan_model() {
        let mut models = ModelSet {
            study_plan: "big".into(),
            ..ModelSet::default()
        };
        assert_eq!(models.for_task(Task::Handout), "big");
        models.handout = "writer".into();
        assert_eq!(models.for_task(Task::Handout), "writer");
    }

    #[test]
    fn empty_env_keeps_local_defaults() {
        let config = AiConfig::from_env(|_| None);
        assert_eq!(config, AiConfig::default());
    }

    #[test]
    fn vision_is_opt_in_and_loads_from_env() {
        assert!(AiConfig::default().models.vision.is_empty());
        let config =
            AiConfig::from_env(|key| (key == "MODEL_VISION").then(|| "vision-model".into()));
        assert_eq!(config.models.for_task(Task::Vision), "vision-model");
        let legacy: ModelSet = serde_json::from_str("{}").unwrap();
        assert!(legacy.vision.is_empty());
    }
}
