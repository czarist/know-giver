//! Required first-run setup (language, name, teacher, AI service, terms) and
//! the guided tour that follows it. Study plans and progress are never
//! touched here.

use kg_ai::{AiConfig, AiError};

use crate::settings::{SETUP_VERSION, TERMS_VERSION, TermsAcceptance};
use crate::{Engine, EngineError, Event, Settings};

/// Models a service offered for one address and key.
#[derive(Debug)]
pub struct ModelList {
    pub base_url: String,
    pub api_key: String,
    pub result: Result<Vec<String>, AiError>,
}

impl ModelList {
    /// Whether this list belongs to `config`'s address and key.
    pub fn is_for(&self, config: &AiConfig) -> bool {
        self.base_url == config.base_url && self.api_key == config.api_key
    }
}

impl Engine {
    /// The required setup has to run before anything else.
    pub fn needs_setup(&self) -> bool {
        self.settings.needs_setup()
    }

    /// The guided tour comes once, right after the setup.
    pub fn needs_tour(&self) -> bool {
        !self.needs_setup() && !self.settings.tour_done
    }

    pub fn is_listing_models(&self) -> bool {
        self.listing_models
    }

    /// The last model list fetched, whatever it was for.
    pub fn model_list(&self) -> Option<&ModelList> {
        self.model_list.as_ref()
    }

    /// Result of the last connection test of exactly `config`.
    pub fn connection_result(&self, config: &AiConfig) -> Option<bool> {
        self.last_test
            .as_ref()
            .filter(|(tested, _)| tested == config)
            .map(|(_, ok)| *ok)
    }

    /// Saves the choices of the setup and the agreement to the terms.
    pub(crate) fn complete_setup(&mut self, mut settings: Settings) -> Result<(), EngineError> {
        let ai = &settings.ai;
        if settings.student_name.trim().is_empty()
            || settings.teacher.is_none()
            || settings.region.is_none()
            || ai.base_url.trim().is_empty()
            || ai.models.tutor.trim().is_empty()
        {
            return Err(EngineError::SetupIncomplete);
        }
        settings.student_name = settings.student_name.trim().to_owned();
        settings.setup_version = SETUP_VERSION;
        settings.terms_accepted = Some(TermsAcceptance {
            version: TERMS_VERSION.to_owned(),
            accepted_at: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            region: settings.region,
        });
        self.save_settings(settings)
    }

    pub(crate) fn finish_tour(&mut self) -> Result<(), EngineError> {
        if self.settings.tour_done {
            return Ok(());
        }
        self.settings.tour_done = true;
        self.settings.save(&mut self.store, &self.env_ai)?;
        Ok(())
    }

    /// Asks the service which models the key can use.
    pub(crate) fn list_models(&mut self, config: AiConfig) {
        if self.listing_models {
            return;
        }
        self.listing_models = true;
        self.spawn(move || {
            let result = kg_ai::Client::new(config.clone()).list_models();
            Event::ModelsListed(ModelList {
                base_url: config.base_url,
                api_key: config.api_key,
                result,
            })
        });
    }

    pub(crate) fn on_models_listed(&mut self, list: ModelList) {
        self.listing_models = false;
        if let Err(err) = &list.result {
            tracing::warn!(%err, url = %list.base_url, "could not list models");
        }
        self.model_list = Some(list);
    }
}
