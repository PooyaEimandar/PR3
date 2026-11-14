//! User-facing results come from completed operations, never the model's prose.

use super::Entry;

#[derive(Default)]
pub(super) struct Results {
    saved: usize,
    corrected: usize,
    known: usize,
    review: usize,
    proposed: usize,
    memory_failed: usize,
    queued: usize,
    opened: usize,
    mail_failed: usize,
    mail_unknown: bool,
    audit_failed: bool,
    mail_requested: bool,
    memory_requested: bool,
}

impl Results {
    pub fn record(&mut self, entry: &Entry) {
        if let Entry::Tool { name, output, .. } = entry
            && name == "send_email"
            && output.contains(crate::actions::AUDIT_FAILED)
        {
            self.audit_failed = true;
        }
        match entry {
            Entry::User { text } => {
                *self = Self::default();
                (self.mail_requested, self.memory_requested) = action_request(text);
            }
            Entry::Failed { reason } if super::unknown(reason) => self.mail_unknown = true,
            Entry::Tool { name, output, .. } if name == "send_email" && super::unknown(output) => {
                self.mail_unknown = true
            }
            Entry::Tool {
                name,
                success: true,
                output,
                ..
            } if name == "send_email" && output.starts_with(crate::device_mail::OPENED) => {
                self.opened += 1;
            }
            Entry::Tool {
                name,
                success: true,
                output,
                ..
            } if name == "remember" && output.ends_with(", already confirmed by the user.") => {
                self.known += 1
            }
            Entry::Tool { name, success, .. } => match (name.as_str(), success) {
                ("save_memory", true) => self.saved += 1,
                ("correct_memory", true) => self.corrected += 1,
                ("review_memory", true) => self.review += 1,
                ("remember", true) => self.proposed += 1,
                ("save_memory" | "correct_memory" | "remember", false) => self.memory_failed += 1,
                ("send_email", true) => self.queued += 1,
                ("send_email", false) => self.mail_failed += 1,
                _ => {}
            },
            _ => {}
        }
    }

    pub fn line(&self) -> String {
        let mut parts = Vec::new();
        if self.saved > 0 {
            parts.push(format!("{} saved", count(self.saved, "memory note")));
        }
        if self.corrected > 0 {
            parts.push(format!(
                "{} corrected",
                count(self.corrected, "memory note")
            ));
        }
        if self.known > 0 {
            parts.push(format!(
                "{} already confirmed",
                count(self.known, "memory note")
            ));
        }
        if self.review > 0 {
            parts.push("Memory update needs your review; nothing changed".to_string());
        }
        if self.proposed > 0 {
            parts.push(format!(
                "{} saved for review",
                count(self.proposed, "memory note")
            ));
        }
        if self.memory_failed > 0 {
            parts.push(format!(
                "{} failed",
                count(self.memory_failed, "memory save")
            ));
        }
        if self.saved
            + self.corrected
            + self.known
            + self.review
            + self.proposed
            + self.memory_failed
            == 0
        {
            parts.push("No new memory saved".to_string());
        }
        if self.queued > 0 {
            parts.push(format!(
                "{} added to sandbox outbox; nothing sent externally",
                count(self.queued, "email")
            ));
        } else if !self.mail_unknown && self.opened == 0 {
            parts.push("No email sent".to_string());
        }
        if self.opened > 0 {
            parts.push(format!("{} opened in your email app; review and send there. Sending and delivery unverified", count(self.opened, "email draft")));
        }
        if self.mail_unknown {
            parts.push("Email action outcome unknown; check your email app (or the sandbox outbox for a test run) before trying again".to_string());
        }
        if self.mail_failed > 0 {
            parts.push(format!(
                "{} not completed",
                count(self.mail_failed, "email action")
            ));
        }
        if self.audit_failed {
            parts.push(crate::actions::AUDIT_FAILED.to_string());
        }
        parts.join(" · ")
    }

    pub fn card_line(&self) -> String {
        if !self.has_operations() && !self.mail_requested && !self.memory_requested {
            "Reply ready".to_string()
        } else {
            self.line()
        }
    }

    pub fn needs_review(&self) -> bool {
        self.review > 0 || self.opened > 0
    }

    pub fn review_reason(&self) -> &'static str {
        if self.opened > 0 {
            "Review and send the draft in your email app; finish this task when done"
        } else {
            "Needs attention: review the memory update"
        }
    }

    pub fn incomplete(&self) -> bool {
        self.audit_failed
            || self.mail_unknown
            || self.mail_failed > 0
            || self.memory_failed > 0
            || (self.mail_requested && self.queued + self.opened == 0)
            || (self.memory_requested
                && self.saved + self.corrected + self.known + self.proposed + self.review == 0)
    }

    fn has_operations(&self) -> bool {
        self.mail_unknown
            || self.saved
                + self.corrected
                + self.known
                + self.review
                + self.proposed
                + self.memory_failed
                + self.queued
                + self.opened
                + self.mail_failed
                > 0
    }

    /// The runtime owns completion language for operation turns. A model's prose
    /// cannot turn a proposal, decline, error or sandbox write into external delivery.
    pub fn completion(&self, asked: &str, generated: &str) -> Option<String> {
        if self.has_operations() {
            return Some(self.line());
        }
        if super::mail::requested(asked) {
            let question = super::mail::clarification(generated)
                .map(|question| format!(" {question}"))
                .unwrap_or_default();
            return Some(format!(
                "No email draft was prepared · No email sent.{question}"
            ));
        }
        if action_request(asked) != (false, false) || completion_claim(generated) {
            return Some(format!(
                "No action was completed for this request. {}",
                self.line()
            ));
        }
        None
    }
}

fn action_request(text: &str) -> (bool, bool) {
    let text = text.trim().to_lowercase();
    let text = ["please ", "can you ", "could you ", "would you "]
        .iter()
        .find_map(|prefix| text.strip_prefix(prefix))
        .unwrap_or(&text);
    let email = super::mail::requested(text);
    let memory = text.starts_with("remember that ")
        || text.starts_with("remember:")
        || text.starts_with("remember my ")
        || text.starts_with("remember our ")
        || text.starts_with("save to memory")
        || text.starts_with("update memory")
        || text.starts_with("correct memory");
    (email, memory)
}

/// A bounded English fallback for models that announce an operation without calling
/// a tool. Actual operation turns use the language-independent result path above.
fn completion_claim(text: &str) -> bool {
    text.split(['\n', '.', '!', '?']).any(|line| {
        let line = line
            .trim_matches(|c: char| c.is_whitespace() || matches!(c, '*' | '_' | '#' | '"'))
            .to_lowercase();
        matches!(
            line.as_str(),
            "done" | "sent" | "saved" | "completed" | "all done"
        ) || [
            "i sent ",
            "i've sent ",
            "i have sent ",
            "i saved ",
            "i've saved ",
            "i have saved ",
            "i updated ",
            "i've updated ",
            "i have updated ",
            "i completed ",
            "i've completed ",
            "email sent",
            "saved to memory",
            "your email has been sent",
            "the email has been sent",
            "your email was sent",
            "the email was sent",
            "task completed",
            "memory updated",
            "i prepared the email",
            "i've prepared the email",
            "i have prepared the email",
            "i drafted the email",
            "i've drafted the email",
            "i have drafted the email",
            "the email is prepared",
            "your email is prepared",
            "email prepared",
            "email drafted",
            "email ready",
        ]
        .iter()
        .any(|prefix| line.starts_with(prefix))
    })
}

fn count(n: usize, noun: &str) -> String {
    format!("{n} {noun}{}", if n == 1 { "" } else { "s" })
}

#[cfg(test)]
mod tests {
    #[test]
    fn replayed_old_results_cannot_complete_a_new_draft_request_or_hide_clarification() {
        let mut results = super::Results::default();
        for entry in [
            super::Entry::User {
                text: "Remember the pickup".into(),
            },
            super::Entry::Tool {
                sources: None,
                name: "remember".into(),
                arguments: "{}".into(),
                success: true,
                output: "saved for review".into(),
            },
            super::Entry::Reply {
                text: "1 memory note saved for review · No email sent".into(),
            },
            super::Entry::User {
                text: "Prepare the pickup email now.".into(),
            },
        ] {
            results.record(&entry);
        }
        assert!(results.incomplete());
        for text in [
            "The email is prepared.",
            "1 memory note saved for review · No email sent",
        ] {
            assert_eq!(
                results
                    .completion("Prepare the pickup email now.", text)
                    .unwrap(),
                "No email draft was prepared · No email sent."
            );
        }
        let answer = results
            .completion(
                "Prepare the pickup email now.",
                super::super::mail::NEED_RECIPIENT,
            )
            .unwrap();
        assert!(answer.contains(super::super::mail::NEED_RECIPIENT));
        assert!(!answer.contains("saved for review"));
    }

    use super::*;

    #[test]
    fn now_email_requests_need_a_current_runtime_result_to_complete() {
        let asked = "Now email Alex the reference from that notice.";
        let mut results = Results::default();
        results.record(&Entry::Tool {
            sources: None,
            name: "send_email".into(),
            arguments: "{}".into(),
            success: true,
            output: String::new(),
        });
        results.record(&Entry::User { text: asked.into() });
        assert!(results.incomplete());
        let text = results
            .completion(asked, "The draft is ready for your review.")
            .unwrap();
        assert!(text.contains("No email"));
        assert!(!text.contains("draft is ready"));
        assert_ne!(results.card_line(), "Reply ready");
    }

    #[test]
    fn preparing_email_is_pending_until_a_runtime_proposal_completes() {
        for (asked, claim) in [
            (
                "Prepare the email with only the public reference now.",
                "The email is prepared and waiting for your review.",
            ),
            (
                "Prepare the pickup email to Alex now, with only the public reference.",
                "1 memory note saved for review · No email sent",
            ),
            (
                "Please draft an email to Alex about pickup.",
                "I've drafted the email. Let me know if you'd like changes.",
            ),
        ] {
            let mut results = Results::default();
            results.record(&Entry::User { text: asked.into() });
            assert!(results.incomplete(), "{asked}");
            let reply = results
                .completion(asked, claim)
                .expect("runtime owns action status");
            assert!(reply.contains("No email"), "{reply}");
            assert!(!reply.contains("1 memory note saved"));
            assert!(!reply.contains("I've drafted"));
            assert_ne!(results.card_line(), "Reply ready");
        }
    }

    #[test]
    fn an_opened_draft_still_needs_user_review_and_never_becomes_sent() {
        let mut results = Results::default();
        results.record(&Entry::User {
            text: "Email Test Person".into(),
        });
        results.record(&Entry::Tool {
            sources: None,
            name: "send_email".into(),
            arguments: "{}".into(),
            success: true,
            output: format!("{} for Test Person.", crate::device_mail::OPENED),
        });
        assert!(results.needs_review());
        assert!(
            !results.incomplete(),
            "handoff succeeded, with a user step remaining"
        );
        let completion = results
            .completion("Email Test Person", "Sent! Done!")
            .unwrap();
        assert!(completion.contains("delivery unverified"));
        assert!(!completion.contains("sandbox"));
        assert!(!completion.contains("email sent"));
        assert!(results.review_reason().contains("email app"));
    }

    #[test]
    fn completion_language_is_derived_from_results_not_assertions() {
        let mut results = Results::default();
        assert!(
            results
                .completion("What is Paris?", "Paris is a city.")
                .is_none()
        );
        for claim in [
            "Done!",
            "I've sent your email.",
            "Saved to memory.",
            "I have updated your memory.",
        ] {
            let text = results.completion("hello", claim).unwrap();
            assert!(text.starts_with("No action was completed"));
        }
        assert!(
            results
                .completion("Please email Alex", "Certainly!")
                .is_some()
        );
        results.record(&Entry::Tool {
            sources: None,
            name: "remember".into(),
            arguments: "{}".into(),
            success: true,
            output: "Saved to memory as #1, already confirmed by the user.".into(),
        });
        let text = results
            .completion("hello", "I saved a new note and sent your email.")
            .unwrap();
        assert_eq!(text, "1 memory note already confirmed · No email sent");
        results.record(&Entry::Tool {
            sources: None,
            name: "correct_memory".into(),
            arguments: "{}".into(),
            success: true,
            output: "updated".into(),
        });
        results.record(&Entry::Tool {
            sources: None,
            name: "send_email".into(),
            arguments: "{}".into(),
            success: false,
            output: "declined".into(),
        });
        assert_eq!(
            results
                .completion("hello", "Everything completed.")
                .unwrap(),
            "1 memory note corrected · 1 memory note already confirmed · No email sent · 1 email action not completed"
        );
        results.record(&Entry::User {
            text: "next".into(),
        });
        assert!(results.completion("What is 2+2?", "4").is_none());
    }

    #[test]
    fn prose_cannot_claim_execution_and_each_turn_starts_empty() {
        let mut results = Results::default();
        results.record(&Entry::Reply {
            text: "Saved, sent and completed!".into(),
        });
        assert_eq!(results.card_line(), "Reply ready");
        assert_eq!(results.line(), "No new memory saved · No email sent");
        results.record(&Entry::Tool {
            sources: None,
            name: "send_email".into(),
            arguments: "{}".into(),
            success: false,
            output: "SENT!".into(),
        });
        assert!(results.line().contains("No email sent"));
        assert!(results.line().contains("not completed"));
        results.record(&Entry::User {
            text: "next".into(),
        });
        assert_eq!(results.card_line(), "Reply ready");
    }

    #[test]
    fn confirmed_memory_proposals_and_sandbox_execution_are_distinct() {
        let mut results = Results::default();
        for name in ["save_memory", "remember", "send_email"] {
            results.record(&Entry::Tool {
                sources: None,
                name: name.into(),
                arguments: "{}".into(),
                success: true,
                output: String::new(),
            });
        }
        assert_eq!(
            results.line(),
            "1 memory note saved · 1 memory note saved for review · 1 email added to sandbox outbox; nothing sent externally"
        );
    }
}
