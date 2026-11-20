//! Conservative full-name disambiguation for simple English "Who is ...?"
//! requests. This is neither entity resolution nor proof that a page is truthful.
use pr3_memory_sqlite::research::Evidence;

fn words(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .map(str::to_lowercase)
        .collect()
}

pub(super) fn subject(request: &str) -> Option<String> {
    let request = request.trim();
    if !request.to_lowercase().starts_with("who is ") {
        return None;
    }
    // Preserve requested explanation/comparison rather than silently turning a
    // compound question into a quotation-only biography.
    if words(request).iter().any(|w| {
        matches!(
            w.as_str(),
            "why" | "how" | "compare" | "explain" | "analyze" | "analyse" | "interpret"
        )
    }) {
        return None;
    }
    let name = request[7..]
        .split(['?', '.', ',', '!', ';', '\n'])
        .next()?
        .trim();
    let parts = words(name);
    if !(2..=5).contains(&parts.len())
        || name.contains("://")
        || parts.iter().any(|s| {
            matches!(
                s.as_str(),
                "the"
                    | "a"
                    | "an"
                    | "of"
                    | "or"
                    | "and"
                    | "in"
                    | "from"
                    | "at"
                    | "for"
                    | "that"
                    | "with"
                    | "whose"
                    | "search"
                    | "please"
                    | "him"
                    | "her"
            )
        })
    {
        return None;
    }
    Some(parts.join(" "))
}

pub(super) fn for_request(topic: &str, earlier: &[String], current: &str) -> Option<String> {
    let continuation = |text: &str| {
        matches!(
            text.trim()
                .trim_end_matches(['.', '!'])
                .to_lowercase()
                .as_str(),
            "continue" | "continue please" | "resume" | "go on" | "retry" | "try again"
        )
    };
    subject(if continuation(current) {
        earlier
            .iter()
            .rev()
            .find(|s| !continuation(s))
            .map(String::as_str)
            .unwrap_or(topic)
    } else {
        current
    })
}

pub(super) fn mentions(subject: &str, text: &str) -> bool {
    let name = words(subject);
    !name.is_empty() && words(text).windows(name.len()).any(|w| w == name)
}

pub(super) fn substantive(subject: &str, quote: &str) -> bool {
    let name = words(subject);
    let handle = name.join("");
    !quote.trim_start().starts_with('#')
        && words(quote).iter().any(|w| {
            w.chars().any(char::is_alphabetic)
                && !name.contains(w)
                && w != &handle
                && !matches!(
                    w.as_str(),
                    "s" | "timeline"
                        | "profile"
                        | "biography"
                        | "about"
                        | "follow"
                        | "following"
                        | "followers"
                )
        })
}

pub(super) fn source_matches(subject: &str, source: &Evidence, live: &[Evidence]) -> bool {
    live.iter()
        .filter(|other| {
            if let Some(document) = source.document {
                other.document == Some(document)
            } else {
                other.document.is_none()
                    && !source.location.is_empty()
                    && other.location.split(" · part ").next()
                        == source.location.split(" · part ").next()
            }
        })
        .any(|part| mentions(subject, &part.title) || mentions(subject, &part.text))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn full_names_do_not_match_given_names_substrings_or_other_people() {
        assert_eq!(
            subject("Who is Mira Vale, search her."),
            Some("mira vale".into())
        );
        assert_eq!(subject("who is José García?"), Some("josé garcía".into()));
        for query in [
            "Who is the president of France?",
            "Who is Mira?",
            "Compare Mira Vale and Mira Stone",
            "Continue",
            "Who is at the door?",
            "Who is Mira Vale? Explain her influence.",
        ] {
            assert_eq!(subject(query), None);
        }
        assert!(mentions("mira vale", "MIRA VALE's biography"));
        assert!(!mentions("mira vale", "Mira Stone and Dana Vale"));
        assert!(!mentions("mira vale", "Mira Valeria"));
        assert!(!substantive("mira vale", "# Mira Vale's Timeline"));
        assert!(!substantive("mira vale", "Mira Vale's Biography"));
        assert!(!substantive("mira vale", "1987 — 2026"));
        assert!(!substantive("mira vale", "MiraVale Follow"));
        assert!(!substantive("mira vale", "Mira Vale 20 followers"));
        assert!(substantive(
            "mira vale",
            "Mira Vale writes about follow-up research."
        ));
        assert!(substantive("mira vale", "Mira Vale is an engineer."));
        let earlier = vec!["Who is Mira Vale?".into(), "Continue.".into()];
        assert_eq!(
            for_request("Old task", &earlier, "Continue."),
            Some("mira vale".into())
        );
        assert_eq!(
            for_request("Who is Mira Vale?", &[], "Retry"),
            Some("mira vale".into())
        );
        assert_eq!(
            for_request("Who is Mira Vale?", &earlier, "Who is Mira Stone?"),
            Some("mira stone".into())
        );
        assert_eq!(
            for_request(
                "Who is Mira Vale?",
                &earlier,
                "What do the saved pages say?"
            ),
            None
        );
        let part = |location: &str, text: &str| Evidence {
            source: "research:1:test".into(),
            title: "Biography".into(),
            location: location.into(),
            text: text.into(),
            captured_at: 1,
            document: None,
        };
        let named = part(
            "https://public.example/vale · part 1 of 2",
            "Mira Vale is an engineer.",
        );
        let continuation = part(
            "https://public.example/vale · part 2 of 2",
            "She wrote a book.",
        );
        let other = part(
            "https://public.example/stone · part 1 of 1",
            "Mira hosts a podcast.",
        );
        let live = [named, continuation.clone(), other.clone()];
        assert!(source_matches("mira vale", &continuation, &live));
        assert!(!source_matches("mira vale", &other, &live));
        assert!(!source_matches("mira vale", &continuation, &live[1..]));
    }
}
