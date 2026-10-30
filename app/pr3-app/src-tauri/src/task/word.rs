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

/// Words that point back at what came before - "send it", "the same to Alex", "which one
/// is cheaper", "translate the last part" - so a message with one goes on in its task.
const GOES_ON: &str = "again also another answer both change continue correct edit either else
    expand fix he her hers him his instead it its less longer more no nope ok okay one ones
    other others redo repeat rephrase rest reword rewrite same she shorten shorter simpler
    simplify summarise summarize sure that thats their them these they this those too
    translate undo which yeah yep yes";

/// How a message that goes on from the one before often starts, before [`normalize`]
/// drops some of these.
const GOES_ON_FROM: &[&str] = &["and", "but", "or", "also", "plus", "then"];

/// Words that say nothing of what a message is about, such as what it asks PR3 to do.
const SAYS_NOTHING: &str = "a about after all am an any anything are as at be been before best
    but by calculate can cant check compare convert could create define describe did didnt
    do does doesnt dont explain find for from get give go going good got great had has have
    help here how i id if im in into is isnt ive just know let lets like long look make many
    me much my need nice not now of often on or our out please pr3 really recommend right
    say see should show so some something suggest tell than thank thanks the there think to
    up us very want was way we well were what when where who why will with would write you
    your youre";

/// How a message that asks something of its own starts: a question or a request, not
/// more detail for the task.
const ASKS: &str = "add are book calculate can check compare convert could create define
    describe did do does email explain find give help how is list make plan recommend remind
    send set should show suggest tell what when where which who why will would write";

/// A message needs at least this many words of its own to look new: "Thursday works"
/// could answer anything.
const NEW_WORDS: usize = 2;

/// Whether `text`, sent in a task, seems to start something new (docs Sec. 4.8): it asks
/// something of its own - a question or a request, not more detail for the task or an
/// answer to PR3 - in words of its own, none of them in `earlier` (the task's title, its
/// opening message and its last exchange), and it points back at nothing.
pub fn starts_anew(text: &str, earlier: &[&str]) -> bool {
    let first = text
        .split(|c: char| !c.is_alphanumeric())
        .find(|w| !w.is_empty())
        .map(str::to_lowercase);
    if first.is_some_and(|w| GOES_ON_FROM.contains(&w.as_str())) {
        return false;
    }
    let said = normalize(text);
    if said.starts_with("what about ") || said.starts_with("how about ") {
        return false;
    }
    let words: Vec<&str> = said.split(' ').collect();
    let asks = text.trim_end().ends_with('?') || words.first().is_some_and(|w| among(ASKS, w));
    if !asks || words.iter().any(|w| among(GOES_ON, w)) {
        return false;
    }
    let own: Vec<&str> = words
        .iter()
        .copied()
        .filter(|w| w.chars().count() > 1 && !among(SAYS_NOTHING, w))
        .collect();
    if own.len() < NEW_WORDS {
        return false;
    }
    let before = normalize(&earlier.join(" "));
    let before: Vec<&str> = before.split(' ').collect();
    !own.iter()
        .any(|word| before.iter().any(|said| same_word(word, said)))
}

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

/// Whether `word` is one of `list`'s, which are separated by spaces.
fn among(list: &str, word: &str) -> bool {
    list.split_whitespace().any(|w| w == word)
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

    /// The task's title, opening message, last message and reply.
    const LUNCH: [&str; 4] = [
        "Email Sam that lunch is on Thursday",
        "Email Sam that lunch is on Thursday",
        "Email Sam that lunch is on Thursday",
        "Sent to Sam. Want me to suggest a time too?",
    ];

    #[test]
    fn a_question_or_request_in_words_of_its_own_starts_anew() {
        for text in [
            "What's the weather like in Lisbon tomorrow?",
            "How do I keep basil fresh?",
            // "suggest" says what to do, not what it is about.
            "Suggest a name for a grey cat.",
            "Remind me to water the plants at six",
        ] {
            assert!(starts_anew(text, &LUNCH), "{text}");
        }
    }

    #[test]
    fn a_message_that_goes_on_stays_in_its_task() {
        for text in [
            "Send it to Alex instead.",
            "Now send the same details to Alex.",
            "Make it shorter",
            "Translate to German",
            "Which one is cheaper?",
            "And tomorrow?",
            "But is Ana away next week?",
            "What about Lisbon?",
            "Thanks!",
            "Great, perfect",
            // Words the task said, in another form.
            "Is Thursday's lunch at noon?",
            "Do emails take long to arrive?",
            // More detail for the task, or an answer to PR3: they ask nothing of their own.
            "Friday works",
            "Noon at Casa Lupa",
            "The budget is 40 euros.",
            "We want a hotel in Alfama for under 120 euros a night.",
            "Let's plan a mushroom risotto as the main course.",
        ] {
            assert!(!starts_anew(text, &LUNCH), "{text}");
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
