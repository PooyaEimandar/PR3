//! User-facing status is derived from runtime results, never model assertions.
use super::Entry;

#[derive(Default)]
pub(super) struct Results {
    saved: usize,
    corrected: usize,
    known: usize,
    review: usize,
    proposed: usize,
    failed: usize,
    requested: bool,
    unknown: bool,
}
impl Results {
    pub fn record(&mut self, entry: &Entry) {
        match entry {
            Entry::User { text } => {
                *self = Self::default();
                self.requested = memory_request(text);
            }
            Entry::Failed { reason } if super::unknown(reason) => self.unknown = true,
            Entry::Tool { output, .. } if super::unknown(output) => self.unknown = true,
            Entry::Tool {
                name,
                success,
                output,
                ..
            } => match (name.as_str(), success) {
                ("remember", true) if output.ends_with(", already confirmed by the user.") => {
                    self.known += 1
                }
                ("save_memory", true) => self.saved += 1,
                ("correct_memory", true) => self.corrected += 1,
                ("review_memory", true) => self.review += 1,
                ("remember", true) => self.proposed += 1,
                ("save_memory" | "correct_memory" | "remember" | "review_memory", false) => {
                    self.failed += 1
                }
                _ => {}
            },
            _ => {}
        }
    }
    pub fn line(&self) -> String {
        let mut parts = Vec::new();
        for (n, suffix) in [
            (self.saved, "saved"),
            (self.corrected, "corrected"),
            (self.known, "already confirmed"),
            (self.proposed, "saved for review"),
        ] {
            if n > 0 {
                parts.push(format!(
                    "{n} memory note{} {suffix}",
                    if n == 1 { "" } else { "s" }
                ));
            }
        }
        if self.review > 0 {
            parts.push("Memory update needs your review; nothing changed".into());
        }
        if self.failed > 0 {
            parts.push(format!(
                "{} memory save{} failed",
                self.failed,
                if self.failed == 1 { "" } else { "s" }
            ));
        }
        if parts.is_empty() {
            parts.push("No new memory saved".into());
        }
        if self.unknown {
            parts.push("Action outcome unknown; check its destination before retrying".into());
        }
        parts.join(" · ")
    }
    fn operations(&self) -> usize {
        self.saved + self.corrected + self.known + self.proposed + self.review + self.failed
    }
    pub fn card_line(&self) -> String {
        if self.operations() == 0 && !self.requested && !self.unknown {
            "Reply ready".into()
        } else {
            self.line()
        }
    }
    pub fn needs_review(&self) -> bool {
        self.review > 0
    }
    pub fn review_reason(&self) -> &'static str {
        "Needs attention: review the memory update"
    }
    pub fn incomplete(&self) -> bool {
        self.unknown || self.failed > 0 || (self.requested && self.operations() == 0)
    }
    pub fn completion(&self, asked: &str, generated: &str) -> Option<String> {
        if self.operations() > 0 {
            return Some(self.line());
        }
        if memory_request(asked) || completion_claim(generated) {
            return Some(format!(
                "No action was completed for this request. {}",
                self.line()
            ));
        }
        None
    }
}
fn memory_request(text: &str) -> bool {
    let text = text.trim().to_lowercase();
    let text = ["please ", "can you ", "could you ", "would you "]
        .iter()
        .find_map(|p| text.strip_prefix(p))
        .unwrap_or(&text);
    [
        "remember that ",
        "remember:",
        "remember my ",
        "remember our ",
        "save to memory",
        "update memory",
        "correct memory",
    ]
    .iter()
    .any(|p| text.starts_with(p))
}
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn status_uses_current_results_and_never_mentions_removed_connectors() {
        let mut r = Results::default();
        for claim in [
            "Your email has been sent.",
            "I have updated your memory.",
            "Email drafted.",
        ] {
            assert!(
                r.completion("hello", claim)
                    .unwrap()
                    .starts_with("No action")
            );
        }
        assert_eq!(r.line(), "No new memory saved");
        assert!(
            r.completion("hello", "I've sent everything.")
                .unwrap()
                .starts_with("No action")
        );
        r.record(&Entry::Tool {
            name: "remember".into(),
            arguments: "{}".into(),
            success: true,
            output: "review".into(),
            sources: None,
        });
        assert_eq!(
            r.completion("hello", "Done!").unwrap(),
            "1 memory note saved for review"
        );
        r.record(&Entry::User {
            text: "Remember my deadline".into(),
        });
        assert!(r.incomplete());
        assert!(!r.line().contains("email"));
    }
}
