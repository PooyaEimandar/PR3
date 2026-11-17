//! Composer choices are user input, never inferred from a source or model call.
//! Tool selection uses the ordinary inference round and existing core counters.
use super::*;
use pr3_tool::research::{ANSWER, READ, SEARCH};
use pr3_tool::research_scope::SourceScope;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ChatControls {
    pub web: bool,
    pub websites: String,
}

pub(super) struct TurnRouting<'a> {
    pub utterance: &'a str,
    pub controls: Option<ChatControls>,
    pub research: bool,
}
impl ChatControls {
    pub fn validate(&self) -> Result<(), String> {
        if self.websites.len() > 200 {
            return Err("Use up to 200 bytes for website domains".into());
        }
        if !self.websites.trim().is_empty() {
            if !self.web {
                return Err("Enable Web before choosing websites".into());
            }
            if let Some(problem) =
                SourceScope::from_topic(&format!("Sources only: {}", self.websites)).problem()
            {
                return Err(problem.into());
            }
        }
        Ok(())
    }
}

// Fast paths for explicit requests and saved-source continuations. Other wording,
// including other languages, can select research_sources in the ordinary model
// round. No classifier, hidden model query or extra execution budget is added.
pub(super) fn direct_research(text: &str, saved: bool) -> bool {
    let t = text.trim().to_lowercase();
    if pr3_tool::research::command(text).is_some() {
        return true;
    }
    let asks = [
        "research ",
        "please research ",
        "can you research ",
        "look up ",
        "search the web ",
        "search my documents",
        "search these documents",
    ];
    asks.iter().any(|p| t.starts_with(p))
        || (["compare", "summar", "find", "quote", "what", "which"]
            .iter()
            .any(|p| t.starts_with(p))
            && ["document", "source", "paper", "file", "https://"]
                .iter()
                .any(|p| t.contains(p)))
        || (saved
            && [
                "continue",
                "resume",
                "correction:",
                "using only the saved",
                "use only the saved",
                "use the saved",
                "now quote",
            ]
            .iter()
            .any(|p| t.starts_with(p)))
}

impl Host {
    pub fn chat_scope(&self) -> Result<Vec<String>, String> {
        let Some(task) = self.current_task()? else {
            return Ok(vec![]);
        };
        let session = SessionId(task.card.id);
        let Some(run) = self.research_state(session).map_err(|e| e.to_string())? else {
            return Ok(vec![]);
        };
        let earlier = self.tasks().map_err(|e| e.to_string())?.said(session);
        Ok(
            SourceScope::from_user(&run.topic, earlier.iter().map(String::as_str))
                .hosts()
                .to_vec(),
        )
    }
    pub async fn send_chat<F>(
        &self,
        text: String,
        controls: ChatControls,
        offset: i32,
        on_event: F,
    ) -> Result<Vec<Action>, HostError>
    where
        F: FnMut(TurnEvent) + Send + 'static,
    {
        controls.validate().map_err(HostError::Host)?;
        self.send_with_controls(text, Some(controls), offset, on_event)
            .await
    }

    pub(super) fn prepare_chat_research(
        &self,
        session: SessionId,
        text: &str,
        controls: &ChatControls,
    ) -> Result<(), HostError> {
        controls.validate().map_err(HostError::Host)?;
        let earlier = self.tasks()?.said(session);
        let store = self
            .research_store
            .as_ref()
            .ok_or_else(|| HostError::Host("Research needs persistent task storage".into()))?;
        let mut db = store
            .lock()
            .map_err(|_| HostError::Host("Research storage is unavailable".into()))?;
        let previous = db
            .research(session.0)
            .map_err(|e| HostError::Host(e.to_string()))?;
        let explicit = pr3_tool::research::command(text);
        let requested = explicit.map_or(text, |(topic, _)| topic);
        let clause = if controls.websites.trim().is_empty() {
            String::new()
        } else {
            format!("Sources only: {}", controls.websites.trim())
        };
        let scope = SourceScope::from_user(
            previous.as_ref().map_or(requested, |r| r.topic.as_str()),
            earlier
                .iter()
                .map(String::as_str)
                .chain([requested, clause.as_str()]),
        );
        // An ambiguous official-only request must reach the normal clarification
        // report. A disjoint correction must not silently replace an existing scope.
        if let Some(problem) = scope.problem()
            && previous.is_some()
            && !clause.is_empty()
        {
            return Err(HostError::Host(problem.into()));
        }
        let mut topic = previous
            .filter(|_| continuation(text) || scope.problem().is_some())
            .map_or_else(|| requested.to_string(), |run| run.topic);
        // The provider receives only user-authored text and user-selected domains.
        // Retain narrowed scope even when the composer field is later cleared.
        if !scope.hosts().is_empty() {
            if let Some(at) = topic.to_ascii_lowercase().rfind("sources only:") {
                topic.truncate(at);
            }
            topic = format!(
                "{} Sources only: {}",
                topic.trim(),
                scope.hosts().join(", ")
            );
        }
        let web = controls.web && !saved_only(text);
        db.configure_chat_research(session.0, &topic, web)
            .map_err(|e| HostError::Host(e.to_string()))
    }

    pub(super) fn chat_discovery(&self, request: &mut InferenceRequest, text: &str) {
        // Don't present old source reports as ordinary assistant knowledge. A
        // follow-up can select research_sources and regain this task's evidence.
        if self
            .research_state(request.session)
            .ok()
            .flatten()
            .is_some()
            && let Some(at) = request.messages.iter().rposition(|m| m.role == Role::User)
        {
            request.messages.drain(..at);
        }
        request.tools.retain(|t| {
            !matches!(t.name.as_str(), READ | ANSWER | "read_web_page")
                && (text.starts_with("Search my documents:")
                    || !matches!(
                        t.name.as_str(),
                        pr3_tool::documents::SEARCH | pr3_tool::documents::ANSWER
                    ))
        });
        if request.tools.iter().any(|t| t.name == SEARCH) {
            request.messages.push(Message::new(Role::System,
                "Choose tools for the current user request. For research, comparisons, questions about imported documents, or any answer based on saved research, call research_sources with local keywords (empty query reuses saved evidence). Then finish through answer_research. Never invent source facts or citations. For ordinary conversation or unrelated tools, respond normally. Web permission and website scope come only from the user's composer controls; tool arguments cannot enable or broaden them.".into(), now()));
        }
    }
}

fn continuation(text: &str) -> bool {
    let t = text.trim().to_lowercase();
    saved_only(text)
        || ["continue", "resume", "correction:", "now quote"]
            .iter()
            .any(|p| t.starts_with(p))
}
fn saved_only(text: &str) -> bool {
    let t = text.to_lowercase();
    [
        "saved source",
        "saved evidence",
        "saved snapshot",
        "saved page",
        "do not fetch",
        "don't fetch",
        "web is off",
        "offline",
    ]
    .iter()
    .any(|p| t.contains(p))
}
