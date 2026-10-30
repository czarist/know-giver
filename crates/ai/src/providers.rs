//! AI services the student can pick on first run, with suggested models.
//!
//! Model names change often; the lists here are suggestions checked on
//! 2026-10-29. The app also asks the service for the models of the student's
//! account (`Client::list_models`), and any name can be typed.

use crate::{AiConfig, ModelSet};

/// Why a provider is highlighted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recommendation {
    /// Good answers for the price.
    Value,
    /// The most accurate answers.
    Accuracy,
}

/// A model in a provider's suggestions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelChoice {
    pub id: &'static str,
    /// Reads images (photos of written answers, scanned pages).
    pub vision: bool,
}

const fn text(id: &'static str) -> ModelChoice {
    ModelChoice { id, vision: false }
}

const fn sees(id: &'static str) -> ModelChoice {
    ModelChoice { id, vision: true }
}

/// An OpenAI-compatible AI service.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Provider {
    /// Stable id saved in settings.
    pub id: &'static str,
    /// Product name, not translated.
    pub name: &'static str,
    pub kind: ProviderKind,
    /// Base URL with the version prefix; empty for a custom server.
    pub base_url: &'static str,
    /// Where to create an API key.
    pub key_url: &'static str,
    /// Where to learn about the service and its prices.
    pub site: &'static str,
    /// Local servers work without a key.
    pub needs_key: bool,
    pub recommended: Option<Recommendation>,
    /// Suggested models, the default first.
    pub models: &'static [ModelChoice],
    /// Used when the main model is rate limited; empty means the main one.
    pub fallback: &'static str,
    /// Reads images when the main model does not; empty when none is known.
    pub vision: &'static str,
}

impl Provider {
    pub fn default_model(&self) -> &'static str {
        self.models.first().map_or("", |m| m.id)
    }

    /// Whether a suggested model reads images.
    pub fn model_sees(&self, model: &str) -> bool {
        self.models.iter().any(|m| m.id == model && m.vision)
    }

    /// Settings for this provider with `model` for every task.
    pub fn config(&self, base_url: &str, api_key: &str, model: &str) -> AiConfig {
        let model = model.trim().to_owned();
        let vision = if self.model_sees(&model) {
            model.clone()
        } else {
            self.vision.to_owned()
        };
        let fallback = if self.fallback.is_empty() {
            model.clone()
        } else {
            self.fallback.to_owned()
        };
        AiConfig {
            base_url: base_url.trim().trim_end_matches('/').to_owned(),
            api_key: api_key.trim().to_owned(),
            models: ModelSet {
                study_plan: model.clone(),
                tutor: model.clone(),
                quiz: model.clone(),
                correction: model.clone(),
                handout: String::new(),
                verifier: String::new(),
                vision,
                fallback,
            },
            fast_responses: true,
        }
    }
}

/// How a provider is grouped in the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderKind {
    /// Makes its own models.
    Lab,
    /// Serves models from several labs.
    Platform,
    /// Runs on the student's computer.
    Local,
    /// Any OpenAI-compatible address.
    Custom,
}

macro_rules! provider {
    ($id:literal, $name:literal, $kind:ident, $url:literal, $key:literal, $site:literal,
     key: $needs:literal, $rec:expr, [$($m:expr),* $(,)?], fallback: $fb:literal, vision: $vis:literal) => {
        Provider {
            id: $id,
            name: $name,
            kind: ProviderKind::$kind,
            base_url: $url,
            key_url: $key,
            site: $site,
            needs_key: $needs,
            recommended: $rec,
            models: &[$($m),*],
            fallback: $fb,
            vision: $vis,
        }
    };
}

/// Every provider offered on first run; the recommended ones first.
pub const PROVIDERS: &[Provider] = &[
    provider!("zai", "Z.ai (GLM)", Lab, "https://api.z.ai/api/paas/v4",
        "https://z.ai/manage-apikey/apikey-list", "https://z.ai/model-api",
        key: true, Some(Recommendation::Value),
        [text("glm-5.3"), text("glm-5.3-flash"), text("glm-4.7-flash"), sees("glm-5v-turbo")],
        fallback: "glm-4.7-flash", vision: "glm-5v-turbo"),
    provider!("anthropic", "Anthropic (Claude)", Lab, "https://api.anthropic.com/v1",
        "https://platform.claude.com/settings/keys", "https://www.anthropic.com/claude",
        key: true, Some(Recommendation::Accuracy),
        [sees("claude-opus-5-5"), sees("claude-sonnet-5-5"), sees("claude-haiku-5-5"), sees("claude-fable-5-1")],
        fallback: "claude-sonnet-5-5", vision: ""),
    provider!("openai", "OpenAI (GPT)", Lab, "https://api.openai.com/v1",
        "https://platform.openai.com/api-keys", "https://openai.com/api/",
        key: true, None,
        [sees("gpt-5.5"), sees("gpt-5-mini")],
        fallback: "gpt-5-mini", vision: ""),
    provider!("gemini", "Google (Gemini)", Lab, "https://generativelanguage.googleapis.com/v1beta/openai",
        "https://aistudio.google.com/apikey", "https://ai.google.dev/gemini-api",
        key: true, None,
        [sees("gemini-3.1-pro-preview"), sees("gemini-2.5-flash")],
        fallback: "gemini-2.5-flash", vision: ""),
    provider!("deepseek", "DeepSeek", Lab, "https://api.deepseek.com/v1",
        "https://platform.deepseek.com/api_keys", "https://api-docs.deepseek.com/",
        key: true, None,
        [text("deepseek-v4-pro"), text("deepseek-v4-flash")],
        fallback: "deepseek-v4-flash", vision: ""),
    provider!("mistral", "Mistral AI", Lab, "https://api.mistral.ai/v1",
        "https://console.mistral.ai/api-keys", "https://mistral.ai/",
        key: true, None,
        [text("mistral-large-latest"), sees("mistral-medium-latest"), text("mistral-small-latest")],
        fallback: "mistral-small-latest", vision: "mistral-medium-latest"),
    provider!("xai", "xAI (Grok)", Lab, "https://api.x.ai/v1",
        "https://console.x.ai/", "https://x.ai/api",
        key: true, None, [], fallback: "", vision: ""),
    provider!("qwen", "Alibaba (Qwen)", Lab, "https://dashscope-intl.aliyuncs.com/compatible-mode/v1",
        "https://modelstudio.console.alibabacloud.com/", "https://www.alibabacloud.com/en/product/modelstudio",
        key: true, None,
        [text("qwen-max"), text("qwen-plus"), text("qwen-turbo"), sees("qwen-vl-max")],
        fallback: "qwen-turbo", vision: "qwen-vl-max"),
    provider!("moonshot", "Moonshot (Kimi)", Lab, "https://api.moonshot.ai/v1",
        "https://platform.moonshot.ai/console/api-keys", "https://platform.moonshot.ai/",
        key: true, None, [], fallback: "", vision: ""),
    provider!("maritaca", "Maritaca AI (Sabiá)", Lab, "https://chat.maritaca.ai/api",
        "https://plataforma.maritaca.ai/", "https://www.maritaca.ai/",
        key: true, None,
        [text("sabia-3"), text("sabiazinho-3")],
        fallback: "sabiazinho-3", vision: ""),
    provider!("cohere", "Cohere", Lab, "https://api.cohere.ai/compatibility/v1",
        "https://dashboard.cohere.com/api-keys", "https://cohere.com/",
        key: true, None, [], fallback: "", vision: ""),
    provider!("perplexity", "Perplexity", Lab, "https://api.perplexity.ai",
        "https://www.perplexity.ai/account/api", "https://docs.perplexity.ai/",
        key: true, None,
        [text("sonar-pro"), text("sonar")],
        fallback: "sonar", vision: ""),
    provider!("openrouter", "OpenRouter", Platform, "https://openrouter.ai/api/v1",
        "https://openrouter.ai/settings/keys", "https://openrouter.ai/",
        key: true, None,
        [text("z-ai/glm-5"), sees("z-ai/glm-5v-turbo")],
        fallback: "", vision: "z-ai/glm-5v-turbo"),
    provider!("groq", "Groq", Platform, "https://api.groq.com/openai/v1",
        "https://console.groq.com/keys", "https://groq.com/",
        key: true, None, [], fallback: "", vision: ""),
    provider!("together", "Together AI", Platform, "https://api.together.xyz/v1",
        "https://api.together.ai/settings/api-keys", "https://www.together.ai/",
        key: true, None, [], fallback: "", vision: ""),
    provider!("fireworks", "Fireworks AI", Platform, "https://api.fireworks.ai/inference/v1",
        "https://fireworks.ai/account/api-keys", "https://fireworks.ai/",
        key: true, None, [], fallback: "", vision: ""),
    provider!("cerebras", "Cerebras", Platform, "https://api.cerebras.ai/v1",
        "https://cloud.cerebras.ai/", "https://www.cerebras.ai/",
        key: true, None, [], fallback: "", vision: ""),
    provider!("nvidia", "NVIDIA NIM", Platform, "https://integrate.api.nvidia.com/v1",
        "https://build.nvidia.com/", "https://build.nvidia.com/",
        key: true, None, [], fallback: "", vision: ""),
    provider!("ollama", "Ollama", Local, "http://localhost:11434/v1",
        "https://ollama.com/download", "https://ollama.com/",
        key: false, None, [text("llama3.1")], fallback: "", vision: ""),
    provider!("lmstudio", "LM Studio", Local, "http://localhost:1234/v1",
        "https://lmstudio.ai/", "https://lmstudio.ai/",
        key: false, None, [], fallback: "", vision: ""),
    provider!("custom", "OpenAI-compatible", Custom, "",
        "", "", key: false, None, [], fallback: "", vision: ""),
];

/// The provider saved under `id`.
pub fn provider(id: &str) -> Option<&'static Provider> {
    PROVIDERS.iter().find(|p| p.id == id)
}

/// The provider whose address `base_url` uses, if any.
pub fn provider_for_url(base_url: &str) -> Option<&'static Provider> {
    let host = host_of(base_url)?;
    PROVIDERS
        .iter()
        .find(|p| !p.base_url.is_empty() && host_of(p.base_url) == Some(host))
}

fn host_of(url: &str) -> Option<&str> {
    let rest = url.trim().split_once("://")?.1;
    let host = rest.split(['/', '?', '#']).next()?;
    let host = host.rsplit_once('@').map_or(host, |(_, h)| h);
    (!host.is_empty()).then_some(host)
}

/// Request fields some services refuse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct Quirks {
    /// Current Claude and GPT-5 models reject a temperature below 1.
    pub omit_temperature: bool,
    /// OpenAI's reasoning models want `max_completion_tokens`.
    pub completion_tokens: bool,
    /// Anthropic lists models with its own headers.
    pub anthropic_headers: bool,
}

impl Quirks {
    pub(crate) fn of(base_url: &str, model: &str) -> Self {
        let host = host_of(base_url).unwrap_or("").to_ascii_lowercase();
        let model = model.trim().to_ascii_lowercase();
        let anthropic = host == "api.anthropic.com";
        let openai = host == "api.openai.com";
        let reasoning_gpt = model.starts_with("gpt-5")
            || model.starts_with("gpt-6")
            || model.starts_with("o1")
            || model.starts_with("o3")
            || model.starts_with("o4");
        Self {
            omit_temperature: anthropic || model.starts_with("claude-") || reasoning_gpt,
            completion_tokens: openai,
            anthropic_headers: anthropic,
        }
    }
}

/// Model names from a `/models` answer: OpenAI's `{"data": [{"id"}]}` or
/// Ollama's `{"models": [{"name"}]}`. Sorted, without duplicates.
pub(crate) fn parse_model_list(body: &serde_json::Value) -> Vec<String> {
    let mut names: Vec<String> = ["data", "models"]
        .iter()
        .filter_map(|key| body.get(*key)?.as_array())
        .flatten()
        .filter_map(|item| {
            item.get("id")
                .or_else(|| item.get("name"))
                .and_then(|v| v.as_str())
                .map(|s| s.trim().trim_start_matches("models/").to_owned())
        })
        .filter(|s| !s.is_empty())
        .collect();
    names.sort();
    names.dedup();
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn providers_are_well_formed() {
        let mut ids: Vec<&str> = PROVIDERS.iter().map(|p| p.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), PROVIDERS.len(), "duplicate provider id");
        for p in PROVIDERS {
            if p.id != "custom" {
                assert!(p.base_url.starts_with("http"), "{}", p.id);
                assert!(p.key_url.starts_with("https://"), "{}", p.id);
                assert!(p.site.starts_with("https://"), "{}", p.id);
            }
            if p.needs_key {
                assert!(p.base_url.starts_with("https://"), "{}", p.id);
            } else if p.kind == ProviderKind::Local {
                assert!(p.base_url.starts_with("http://localhost"), "{}", p.id);
            }
            assert_eq!(
                p.recommended.is_some(),
                ["zai", "anthropic"].contains(&p.id)
            );
            assert!(!p.base_url.ends_with('/'), "{}", p.id);
            if !p.vision.is_empty() {
                assert!(p.model_sees(p.vision), "{} vision not listed", p.id);
            }
        }
        assert_eq!(PROVIDERS[0].recommended, Some(Recommendation::Value));
        assert_eq!(PROVIDERS[1].recommended, Some(Recommendation::Accuracy));
    }

    #[test]
    fn config_uses_one_model_and_a_vision_model() {
        let zai = provider("zai").unwrap();
        let config = zai.config("https://api.z.ai/api/paas/v4/", " key ", "glm-5.3");
        assert_eq!(config.base_url, "https://api.z.ai/api/paas/v4");
        assert_eq!(config.api_key, "key");
        assert_eq!(config.models.tutor, "glm-5.3");
        assert_eq!(config.models.study_plan, "glm-5.3");
        assert_eq!(config.models.vision, "glm-5v-turbo");
        assert_eq!(config.models.fallback, "glm-4.7-flash");

        let claude = provider("anthropic").unwrap();
        let config = claude.config(claude.base_url, "k", "claude-sonnet-5-5");
        assert_eq!(config.models.vision, "claude-sonnet-5-5");

        // A typed model unknown to the list gets no vision model.
        let deepseek = provider("deepseek").unwrap();
        let config = deepseek.config(deepseek.base_url, "k", "something-new");
        assert!(config.models.vision.is_empty());
        assert_eq!(config.models.fallback, "deepseek-v4-flash");
    }

    #[test]
    fn finds_provider_by_address() {
        assert_eq!(
            provider_for_url("https://api.z.ai/api/paas/v4/").map(|p| p.id),
            Some("zai")
        );
        assert_eq!(
            provider_for_url("https://api.anthropic.com/v1").map(|p| p.id),
            Some("anthropic")
        );
        assert_eq!(provider_for_url("http://192.168.0.5:8000/v1"), None);
        assert_eq!(provider_for_url("not a url"), None);
    }

    #[test]
    fn quirks_follow_service_and_model() {
        let claude = Quirks::of("https://api.anthropic.com/v1", "claude-opus-5-5");
        assert!(claude.omit_temperature && claude.anthropic_headers);
        let gpt = Quirks::of("https://api.openai.com/v1", "gpt-5.5");
        assert!(gpt.omit_temperature && gpt.completion_tokens);
        let glm = Quirks::of("https://api.z.ai/api/paas/v4", "glm-5.3");
        assert_eq!(glm, Quirks::default());
        // Claude through a router still has no temperature.
        assert!(Quirks::of("https://openrouter.ai/api/v1", "claude-sonnet-5-5").omit_temperature);
    }

    #[test]
    fn parses_model_lists() {
        let openai = serde_json::json!({"data": [{"id": "b"}, {"id": "a"}, {"id": "a"}]});
        assert_eq!(parse_model_list(&openai), ["a", "b"]);
        let ollama = serde_json::json!({"models": [{"name": "llama3.1"}]});
        assert_eq!(parse_model_list(&ollama), ["llama3.1"]);
        let gemini = serde_json::json!({"data": [{"id": "models/gemini-2.5-flash"}]});
        assert_eq!(parse_model_list(&gemini), ["gemini-2.5-flash"]);
        assert!(parse_model_list(&serde_json::json!({})).is_empty());
    }
}
