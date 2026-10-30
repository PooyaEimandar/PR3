//! Task words (docs Sec. 4.8): what the user says or types about their tasks - "what is
//! still open?", "carry on with the email to Alex", "what happened with the dentist?",
//! "export the trip plan" - answered from the task list by fixed rules, never by the
//! model. Only a whole message counts, and words that name no kept task are an ordinary
//! message for the model, so "continue the story" still reaches it.

/// What a message asks of the task list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Words {
    /// Which tasks wait for the user, or did not finish.
    StillOpen,
    /// Go back to the task these words name.
    CarryOn(Vec<String>),
    /// How the task these words name stands.
    WhatHappened(Vec<String>),
    /// Hand the task these words name to the share sheet.
    Export(Vec<String>),
}

/// Whole messages asking what is open, as [`normalize`] leaves them.
const STILL_OPEN: &[&str] = &[
    "what is still open",
    "what is open",
    "what is still left",
    "what is left",
    "what is left to do",
    "what is still to do",
    "what is still pending",
    "what is pending",
    "what is still outstanding",
    "what is outstanding",
    "anything open",
    "anything still open",
    "is anything open",
    "is anything still open",
    "is there anything open",
    "is there anything still open",
    "what am i waiting on",
    "what am i waiting for",
    "what do i have open",
    "what do i still have open",
    "what tasks are open",
    "which tasks are open",
    "what are my open tasks",
    "open tasks",
];

/// How a message going back to a task starts; longer forms first.
const CARRY_ON: &[&str] = &[
    "lets carry on with",
    "lets continue with",
    "lets get back to",
    "lets go back to",
    "carry on with",
    "carry on",
    "continue with",
    "continue",
    "go on with",
    "get back to",
    "go back to",
    "back to",
    "resume",
    "pick up",
];

/// How a message asking after a task starts; "how did X go" is matched apart.
const WHAT_HAPPENED: &[&str] = &[
    "whatever happened to",
    "what happened with",
    "what happened to",
    "what is happening with",
    "any news on",
    "any news about",
];

/// How a message exporting a task starts.
const EXPORT: &[&str] = &["export", "share"];

/// Words that name no task: left out of a task's name before it is looked up.
const NOT_A_NAME: &[&str] = &[
    "a", "about", "again", "an", "and", "at", "for", "from", "i", "in", "it", "its", "me", "my",
    "now", "of", "on", "one", "our", "please", "that", "the", "thing", "this", "to", "task",
    "with", "your",
];

/// Reads a whole message as words about the task list, if it is one.
pub fn parse(text: &str) -> Option<Words> {
    let said = normalize(text);
    if STILL_OPEN.contains(&said.as_str()) {
        return Some(Words::StillOpen);
    }
    if let Some(name) = said
        .strip_prefix("how did ")
        .and_then(|rest| rest.strip_suffix(" go"))
    {
        return named(name).map(Words::WhatHappened);
    }
    if let Some(name) = WHAT_HAPPENED.iter().find_map(|start| after(&said, start)) {
        return named(name).map(Words::WhatHappened);
    }
    if let Some(name) = EXPORT.iter().find_map(|start| after(&said, start)) {
        return named(name).map(Words::Export);
    }
    CARRY_ON
        .iter()
        .find_map(|start| after(&said, start))
        .and_then(named)
        .map(Words::CarryOn)
}

/// Lowercase words, without punctuation or a leading "hey PR3", "ok" or "so"; "what's"
/// reads "what is" and "let's" reads "lets".
pub fn normalize(text: &str) -> String {
    let mut plain = String::with_capacity(text.len());
    for c in text.chars().flat_map(char::to_lowercase) {
        if c.is_alphanumeric() {
            plain.push(c);
        } else if c != '\'' && c != '’' {
            plain.push(' ');
        }
    }
    let mut words: Vec<&str> = plain.split_whitespace().collect();
    loop {
        match words.as_slice() {
            [
                "hey" | "hi" | "ok" | "okay" | "so" | "and" | "please" | "pr3",
                ..,
            ] => {
                words.remove(0);
            }
            [.., "please"] => {
                words.pop();
            }
            _ => break,
        }
    }
    words
        .iter()
        .map(|&w| if w == "whats" { "what is" } else { w })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Whether `words` hold every word of `name`, allowing another ending of the same word
/// ("emails" for "email", "reminder" for "remind").
pub fn covers(words: &[&str], name: &[String]) -> bool {
    name.iter()
        .all(|wanted| words.iter().any(|word| same_word(word, wanted)))
}

fn same_word(a: &str, b: &str) -> bool {
    let (short, long) = if a.len() <= b.len() { (a, b) } else { (b, a) };
    short == long || (short.chars().count() >= 4 && long.starts_with(short))
}

fn after<'a>(said: &'a str, start: &str) -> Option<&'a str> {
    said.strip_prefix(start)?.strip_prefix(' ')
}

/// The words of a task's name worth looking up, if any.
fn named(name: &str) -> Option<Vec<String>> {
    let words: Vec<String> = name
        .split_whitespace()
        .filter(|w| !NOT_A_NAME.contains(w))
        .map(str::to_string)
        .collect();
    (!words.is_empty()).then_some(words)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(words: &[&str]) -> Vec<String> {
        words.iter().map(|w| w.to_string()).collect()
    }

    #[test]
    fn whole_messages_about_tasks_are_read_however_they_are_said() {
        for open in [
            "What's still open?",
            "what is still open",
            "Hey PR3, what's left?",
            "OK so what am I waiting on",
            "Anything still open?",
        ] {
            assert_eq!(parse(open), Some(Words::StillOpen), "{open}");
        }
        assert_eq!(
            parse("Carry on with the email to Alex, please."),
            Some(Words::CarryOn(name(&["email", "alex"])))
        );
        assert_eq!(
            parse("Let's get back to the trip plan"),
            Some(Words::CarryOn(name(&["trip", "plan"])))
        );
        assert_eq!(
            parse("What happened with the dentist?"),
            Some(Words::WhatHappened(name(&["dentist"])))
        );
        assert_eq!(
            parse("How did the email to Alex go?"),
            Some(Words::WhatHappened(name(&["email", "alex"])))
        );
        assert_eq!(
            parse("Export the trip plan"),
            Some(Words::Export(name(&["trip", "plan"])))
        );
    }

    #[test]
    fn other_messages_are_not_task_words() {
        for other in [
            "continue",
            "Carry on.",
            "What is still open in the Louvre on Mondays?",
            "Is the shop open?",
            "Remind me to continue the report",
            "What happened?",
            "Share it",
        ] {
            assert_eq!(parse(other), None, "{other}");
        }
    }

    #[test]
    fn a_name_matches_other_endings_of_its_words() {
        let words = ["email", "alex", "about", "reminders"];
        assert!(covers(&words, &name(&["emails", "alex"])));
        assert!(covers(&words, &name(&["remind"])));
        assert!(!covers(&words, &name(&["email", "sam"])));
        // Short words must match exactly.
        assert!(!covers(&["alexandra"], &name(&["al"])));
    }
}
