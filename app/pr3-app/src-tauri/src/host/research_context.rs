//! An inference-only projection, never a change to task history, provenance, loop
//! counters or stored sources. Rebuilt from this task's database on every inference.
use super::*;
use pr3_memory_sqlite::research::source_reference;
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
        // This exact System message is emitted from the core's actual counter.
        // User/source text cannot reserve a round by imitating its wording.
        let limit = Config::default().max_tool_rounds;
        let final_round =
            pr3_core::tool_round_budget(limit.saturating_sub(1), limit).is_some_and(|budget| {
                request
                    .messages
                    .iter()
                    .any(|m| m.role == Role::System && m.content == budget)
            });
        if final_round {
            request.tools.retain(|tool| tool.name == ANSWER);
            request.messages.push(Message::new(Role::System,
                "The final research round is reserved for answer_research. Use the saved evidence now. Put any unanswered part in gaps; no search or source read is available in this round.".into(), now()));
        }
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
        let earlier = self
            .tasks
            .lock()
            .map(|tasks| tasks.said(request.session))
            .unwrap_or_default();
        let db = store
            .lock()
            .map_err(|_| HostError::Host("Research storage is unavailable".into()))?;
        let scope = db
            .research(request.session.0)
            .map_err(|e| HostError::Host(e.to_string()))?
            .map(|r| {
                pr3_tool::research_scope::SourceScope::from_user(
                    &r.topic,
                    earlier
                        .iter()
                        .map(String::as_str)
                        .chain(std::iter::once(query)),
                )
            })
            .unwrap_or_default();
        let sources = db
            .research_sources(request.session.0)
            .map_err(|e| HostError::Host(e.to_string()))?;
        // research_source also checks that a local passage still exists.
        let sources = sources
            .into_iter()
            .filter(|s| scope.allows_evidence(s))
            .filter_map(|s| match db.research_source(request.session.0, &s.source) {
                Ok(Some(s)) => Some(Ok(s)),
                Ok(None) => None,
                Err(e) => Some(Err(HostError::Host(e.to_string()))),
            })
            .collect::<Result<Vec<_>, _>>()?;
        drop(db);
        if let Some(problem) = scope.problem() {
            request.tools.retain(|tool| tool.name == ANSWER);
            request.messages.push(Message::new(Role::System, format!("Website scope needs user clarification: {problem} No source may be used until this is resolved. Call answer_research now with findings: [] and explain the required clarification in gaps. No search is needed."), now()));
            return Ok(());
        }
        let calls = request
            .messages
            .iter()
            .flat_map(|m| &m.tool_calls)
            .map(|c| (c.id.clone(), (c.name.clone(), c.arguments.clone())))
            .collect::<BTreeMap<_, _>>();
        let mut selected = BTreeMap::new();
        let mut rejected_reports = std::collections::BTreeSet::new();
        let mut completed_retrieval = std::collections::BTreeSet::new();
        for message in &mut request.messages {
            if message.role != Role::Tool {
                continue;
            }
            let Some((name, arguments)) =
                message.tool_call_id.as_ref().and_then(|id| calls.get(id))
            else {
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
            if final_round
                && matches!(name.as_str(), READ | SEARCH)
                && let Some(id) = &message.tool_call_id
            {
                completed_retrieval.insert(id.clone());
            }
            if name == READ {
                if let Some(id) = value["source"].as_str().and_then(|id| {
                    sources.iter().find(|s| {
                        s.source == id || source_reference(request.session.0, &s.source) == id
                    })
                }) {
                    message.content = json!({"saved_source":source_reference(request.session.0,&id.source),"status":"Read succeeded; evidence is in the saved-source view."}).to_string();
                    // Fetching a candidate saves all its parts. Do not pin the
                    // default first part (often navigation) above relevant later
                    // passages. Exact-source reads and explicit later parts/byte
                    // ranges still express a request for that particular evidence.
                    let args: Value = serde_json::from_str(arguments).unwrap_or(Value::Null);
                    if pin_read(&args) {
                        selected.insert(id.source.clone(), value);
                    }
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
        // In the final round, completed retrieval calls are obsolete planning
        // examples and compete with the actual evidence for context space. Keep
        // their saved evidence and any failures; only this inference copy changes.
        // The core still holds the exact calls, trace and unchanged loop counters.
        for message in &mut request.messages {
            if message.role == Role::Assistant
                && message.tool_calls.iter().any(|c| {
                    rejected_reports.contains(&c.id) || completed_retrieval.contains(&c.id)
                })
            {
                message.tool_calls.retain(|c| {
                    !rejected_reports.contains(&c.id) && !completed_retrieval.contains(&c.id)
                });
                if message.tool_calls.is_empty() {
                    message.content.clear();
                }
            }
        }
        request.messages.retain(|m| {
            (m.role != Role::Assistant || !m.content.is_empty() || !m.tool_calls.is_empty())
                && !m
                    .tool_call_id
                    .as_ref()
                    .is_some_and(|id| completed_retrieval.contains(id))
        });
        if sources.is_empty() {
            request.messages.push(Message::new(Role::System,
                if final_round {
                    "No saved evidence is available. Call answer_research with no findings and explain the missing evidence in gaps. Never invent a source or quotation."
                } else {
                    "No saved evidence is available for this task. Start with research_sources to discover sources, then read relevant candidates before answer_research. Never invent source IDs or quotations. The existing execution budget still applies."
                }.into(), now()));
            return Ok(());
        }
        let source_count = sources.len();
        let packet = json!({"source_count":source_count,"sources":sources.into_iter().map(|s| {
            let origin = s.document.map(|id| format!("document:{id}")).unwrap_or_else(|| s.location.split(" · part ").next().unwrap_or(&s.source).to_string());
            let origin = pr3_core::trusted::Hash::of(&[origin.as_bytes()]).to_hex()[..16].to_string();
            let split_quotes = !pr3_tool::research::whole_quote_suitable(&s.text, "", &earlier, query);
            let mut source = json!({"source":source_reference(request.session.0,&s.source),"origin":origin,"title":s.title,"location":s.location,"text":s.text,"captured_at":s.captured_at,"selected":selected.contains_key(&s.source),"split_quotes":split_quotes});
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

fn pin_read(args: &Value) -> bool {
    args["offset"].is_u64()
        || args["part"].as_u64().is_some_and(|p| p > 1)
        || args["source"].as_str().is_some_and(|s| {
            s.starts_with("s:") || s.starts_with("research:") || s.starts_with("document:")
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_whole_page_fetch_does_not_pin_its_navigation_over_later_evidence() {
        for args in [
            json!({"source":"c:0123456789abcdef"}),
            json!({"source":"c:0123456789abcdef","part":1}),
            json!({"source":"candidate:1:hash","part":1}),
        ] {
            assert!(!pin_read(&args));
        }
        for args in [
            json!({"source":"c:0123456789abcdef","part":2}),
            json!({"source":"s:0123456789abcdef"}),
            json!({"source":"research:1:hash:1:hash"}),
            json!({"source":"s:0123456789abcdef","offset":123}),
        ] {
            assert!(pin_read(&args));
        }
    }
}
