//! An inference-only projection, never a change to task history, provenance, loop
//! counters or stored sources. Rebuilt from this task's database on every inference.
use super::*;
use pr3_tool::research::{ANSWER, READ, SEARCH};
use serde_json::json;

pub(crate) const SAVED_EVIDENCE: &str = "pr3:saved-research-evidence";

impl Host {
    pub(super) fn research_context(
        &self,
        request: &mut InferenceRequest,
        query: &str,
    ) -> Result<(), HostError> {
        let Some(last_user) = request.messages.iter().rposition(|m| m.role == Role::User) else {
            return Ok(());
        };
        // Earlier reports/tool transcripts are not evidence for a new answer. Keep
        // the latest user message (including task corrections) and this turn's status.
        request.messages.drain(..last_user);
        // Keep remembered user wording separate from the actual current request.
        // The protected report continuation repeats only the current question,
        // rather than doubling all remembered details and research scope notes.
        if let Some(user) = request.messages.first_mut()
            && let Some(context) = user
                .content
                .strip_suffix(query)
                .filter(|s| !s.trim().is_empty())
        {
            let context = Message::new(Role::User, context.trim_end().into(), user.at);
            user.content = query.into();
            request.messages.insert(0, context);
        }
        let Some(store) = &self.research_store else {
            return Ok(());
        };
        let db = store
            .lock()
            .map_err(|_| HostError::Host("Research storage is unavailable".into()))?;
        let sources = db
            .research_sources(request.session.0)
            .map_err(|e| HostError::Host(e.to_string()))?;
        // research_source also checks that a local passage still exists.
        let sources = sources
            .into_iter()
            .filter_map(|s| match db.research_source(request.session.0, &s.source) {
                Ok(Some(s)) => Some(Ok(s)),
                Ok(None) => None,
                Err(e) => Some(Err(HostError::Host(e.to_string()))),
            })
            .collect::<Result<Vec<_>, _>>()?;
        drop(db);
        let calls = request
            .messages
            .iter()
            .flat_map(|m| &m.tool_calls)
            .map(|c| (c.id.clone(), c.name.clone()))
            .collect::<BTreeMap<_, _>>();
        let mut selected = BTreeMap::new();
        let mut rejected_reports = std::collections::BTreeSet::new();
        for message in &mut request.messages {
            if message.role != Role::Tool {
                continue;
            }
            let Some(name) = message.tool_call_id.as_ref().and_then(|id| calls.get(id)) else {
                continue;
            };
            if name == ANSWER && message.content.starts_with("error:") {
                if let Some(id) = &message.tool_call_id {
                    rejected_reports.insert(id.clone());
                }
                message.content =
                    json!({"tool":ANSWER,"status":"failed","error":message.content}).to_string();
                continue;
            }
            let Ok(mut value) = serde_json::from_str::<Value>(&message.content) else {
                continue;
            }; // retain real failures
            if name == READ {
                if let Some(id) = value["source"]
                    .as_str()
                    .filter(|id| sources.iter().any(|s| s.source == *id))
                {
                    message.content = json!({"saved_source":id,"status":"Read succeeded; evidence is in the saved-source view."}).to_string();
                    selected.insert(id.to_string(), value);
                }
            } else if name == SEARCH
                && let Some(object) = value.as_object_mut()
            {
                object.remove("passages");
                message.content = value.to_string();
            }
        }
        // A rejected report can be larger than its sources. Keep its real error,
        // but do not replay unvalidated prose/quotations into the repair attempt.
        // The core still holds the exact calls, trace and unchanged loop counters.
        for message in &mut request.messages {
            if message.role == Role::Assistant
                && message
                    .tool_calls
                    .iter()
                    .any(|c| rejected_reports.contains(&c.id))
            {
                message
                    .tool_calls
                    .retain(|c| !rejected_reports.contains(&c.id));
                if message.tool_calls.is_empty() {
                    message.content.clear();
                }
            }
        }
        request.messages.retain(|m| {
            m.role != Role::Assistant || !m.content.is_empty() || !m.tool_calls.is_empty()
        });
        if sources.is_empty() {
            request.messages.push(Message::new(Role::System,
                "No saved evidence is available for this task. Start with research_sources to discover sources, then read relevant candidates before answer_research. Never invent source IDs or quotations. The existing execution budget still applies.".into(), now()));
            return Ok(());
        }
        let source_count = sources.len();
        let packet = json!({"source_count":source_count,"sources":sources.into_iter().map(|s| {
            let mut source = json!({"source":s.source,"title":s.title,"location":s.location,"text":s.text,"captured_at":s.captured_at,"selected":selected.contains_key(&s.source)});
            if let Some(read) = selected.get(&s.source).filter(|r| r["offset"].is_u64()) {
                for key in ["text", "offset", "text_bytes", "excerpted"] { source[key] = read[key].clone(); }
            }
            source
        }).collect::<Vec<_>>()}).to_string();
        // Use the same 8 KiB ceiling as tool outputs. Token-aware shrinking happens
        // in the engine, reserving its unchanged answer budget.
        let fallback = request
            .messages
            .first()
            .filter(|m| m.role == Role::User && m.content != query)
            .map(|m| m.content.as_str())
            .unwrap_or_default();
        for count in (1..=4).rev() {
            if let Some(content) =
                pr3_llamacpp::saved_evidence_view(&packet, query, fallback, 1200, count)
                && content.len() <= Config::default().max_tool_output_bytes
            {
                let mut message = Message::new(Role::Tool, content, now());
                message.tool_call_id = Some(SAVED_EVIDENCE.into());
                request.messages.push(message);
                break;
            }
        }
        Ok(())
    }
}
