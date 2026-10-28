//! Builds handout export requests: asks where to save and fills in the
//! translated words of the PDF.

use kg_engine::{
    Engine, HandoutLabels, HandoutRequest, HandoutScope, PrepareRequest, StudyPlan, slug,
};
use rust_i18n::t;

use crate::i18n;

/// Asks where to save, then returns the request. `None` if cancelled.
pub fn request(plan: &StudyPlan, scope: HandoutScope, regenerate: bool) -> Option<HandoutRequest> {
    let (subtitle, file_part) = match scope {
        HandoutScope::Topic(id) => {
            let (_, topic) = plan.find_topic(id)?;
            (topic.title.clone(), topic.title.clone())
        }
        HandoutScope::Subject(id) => {
            let subject = plan.subjects.iter().find(|s| s.id == id)?;
            (subject.name.clone(), subject.name.clone())
        }
        HandoutScope::Plan => {
            let subtitle = if plan.exam.is_empty() {
                t!("handout.whole_plan").into_owned()
            } else {
                plan.exam.clone()
            };
            (subtitle, plan.title.clone())
        }
    };
    let file_name = format!("{}-{}.pdf", t!("handout.file_prefix"), slug(&file_part));
    let path = rfd::FileDialog::new()
        .set_title(t!("handout.save_title"))
        .set_file_name(file_name)
        .add_filter(t!("handout.pdf_filter"), &["pdf"])
        .save_file()?;
    Some(HandoutRequest {
        scope,
        path,
        title: plan.title.clone(),
        subtitle,
        generated: t!("handout.generated", date = i18n::date(Engine::today())).into_owned(),
        lang: i18n::current().split('-').next().unwrap_or("pt").to_owned(),
        labels: labels(),
        regenerate,
    })
}

/// Translated words for preparing the whole plan at once.
pub fn prepare_request() -> PrepareRequest {
    PrepareRequest {
        file_prefix: t!("handout.file_prefix").into_owned(),
        whole_plan: t!("handout.whole_plan").into_owned(),
        generated: t!("handout.generated", date = i18n::date(Engine::today())).into_owned(),
        lang: i18n::current().split('-').next().unwrap_or("pt").to_owned(),
        labels: labels(),
    }
}

fn labels() -> HandoutLabels {
    HandoutLabels {
        handout: t!("handout.label.handout").into_owned(),
        contents: t!("handout.label.contents").into_owned(),
        key_points: t!("handout.label.key_points").into_owned(),
        examples: t!("handout.label.examples").into_owned(),
        pitfalls: t!("handout.label.pitfalls").into_owned(),
        summary: t!("handout.label.summary").into_owned(),
        exercises: t!("handout.label.exercises").into_owned(),
        answers: t!("handout.label.answers").into_owned(),
    }
}
