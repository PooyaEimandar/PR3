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
    let request = super::command(request)
        .map(|(topic, _)| topic)
        .unwrap_or(request);
    if !request.to_lowercase().starts_with("who is ") {
        return None;
    }
    // Preserve requested explanation/comparison rather than silently turning a
    // compound question into a quotation-only biography.
    if words(request).iter().any(|w| {
        matches!(
            w.as_str(),
            "why" | "how" | "compare" | "explain" | "analyze" | "analyse" | "interpret" | "whether"
        )
    }) {
        return None;
    }
    // A second question can ask for identity uncertainty or another inference.
    // Do not turn that compound request into a quotation-only biography. A URL
    // query string is not a second question from the user.
    if request.split_once('?').is_some_and(|(_, rest)| {
        rest.split_whitespace()
            .filter(|word| !word.contains("://"))
            .any(|word| word.contains('?'))
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

pub(super) fn active_request<'a>(
    topic: &'a str,
    earlier: &'a [String],
    current: &'a str,
) -> &'a str {
    let continuation = |text: &str| {
        matches!(
            text.trim()
                .trim_end_matches(['.', '!'])
                .to_lowercase()
                .as_str(),
            "continue" | "continue please" | "resume" | "go on" | "retry" | "try again"
        )
    };
    if continuation(current) {
        earlier
            .iter()
            .rev()
            .find(|s| !continuation(s))
            .map(String::as_str)
            .unwrap_or(topic)
    } else {
        current
    }
}

pub(super) fn for_request(topic: &str, earlier: &[String], current: &str) -> Option<String> {
    subject(active_request(topic, earlier, current))
}

/// Intensifiers neither create nor remove an identity assertion: "are clearly
/// different people" is still categorical and "do not definitively establish"
/// is still a negation. Hedges such as "likely" or "probably" are kept.
fn assertion_words(text: &str) -> Vec<String> {
    const INTENSIFIERS: [&str; 10] = [
        "actually",
        "certainly",
        "clearly",
        "conclusively",
        "definitely",
        "definitively",
        "explicitly",
        "firmly",
        "really",
        "truly",
    ];
    words(text)
        .into_iter()
        .filter(|word| !INTENSIFIERS.contains(&word.as_str()))
        .collect()
}

#[derive(PartialEq)]
enum Relationship {
    Same,
    Different,
}

fn relationship(text: &str) -> Option<Relationship> {
    // Noun variants do not change a categorical identity assertion. Keep this
    // narrow English check consistent for claims and their cited evidence.
    let normalized = assertion_words(text)
        .into_iter()
        .map(|word| match word.as_str() {
            "individual" => "person".into(),
            "individuals" | "persons" => "people".into(),
            _ => word,
        })
        .collect::<Vec<_>>();
    let text = format!(" {} ", normalized.join(" "));
    if [
        " are different people ",
        " describe different people ",
        " refer to different people ",
        " are not the same person ",
        " two different people ",
        " distinct people ",
        " separate people ",
        " establish different people ",
        " establishes different people ",
        " prove different people ",
    ]
    .iter()
    .any(|phrase| text.contains(phrase))
    {
        Some(Relationship::Different)
    } else if [
        " are the same person ",
        " is the same person ",
        " describe the same person ",
        " refer to the same person ",
        " establish the same person ",
        " establishes the same person ",
        " prove the same person ",
        " proves the same person ",
    ]
    .iter()
    .any(|phrase| text.contains(phrase))
    {
        Some(Relationship::Same)
    } else {
        None
    }
}

fn uncertain(text: &str) -> bool {
    let text = format!(" {} ", assertion_words(text).join(" "));
    [
        " could ",
        " may ",
        " might ",
        " whether ",
        " cannot ",
        " can t ",
        " can not ",
        " possibly ",
        " uncertain ",
        " unclear ",
        " not establish ",
        " not prove ",
        " no evidence ",
        " neither ",
        " nor ",
        " not necessarily ",
        " if ",
    ]
    .iter()
    .any(|phrase| text.contains(phrase))
}

/// Narrow English report-quality check, not entity resolution or a general
/// entailment test. Separate biographical facts cannot support a categorical
/// identity link. A source must explicitly make that link; uncertainty remains
/// a valid answer. This never changes source access or execution permission.
pub(super) fn validate_relationship(
    claim: &str,
    quotes: &[String],
    request: &str,
) -> Result<(), String> {
    let request = super::command(request.trim())
        .map(|(topic, _)| topic)
        .unwrap_or(request);
    if subject(request.split('?').next().unwrap_or(request)).is_none() {
        return Ok(());
    }
    // Different roles or places under one name conflict only if one person is
    // meant, so "conflicting occupations" presupposes the unestablished link.
    // A page saying it "does not mention" a field does not say it mentions
    // only its own field.
    let phrase = |text: &str| format!(" {} ", words(text).join(" "));
    let narrowed = [
        " mentions only ",
        " mention only ",
        " only mention ",
        " only mentions ",
    ]
    .iter()
    .any(|p| phrase(claim).contains(p))
        && quotes.iter().any(|q| phrase(q).contains(" not mention "));
    if narrowed
        || (super::support::conflict_wording(claim)
            && !quotes.iter().any(|q| super::support::conflict_wording(q)))
    {
        return Err(super::IDENTITY_RELATIONSHIP_ERROR.into());
    }
    for (_, sentence) in pr3_memory::retrieval::sentences(claim) {
        let Some(relation) = relationship(sentence) else {
            continue;
        };
        if uncertain(sentence) {
            continue;
        }
        let explicit = quotes.iter().any(|quote| {
            pr3_memory::retrieval::sentences(quote)
                .iter()
                .any(|(_, sentence)| {
                    !sentence.trim_end().ends_with('?')
                        && !uncertain(sentence)
                        && relationship(sentence).as_ref() == Some(&relation)
                })
        });
        if !explicit {
            return Err(super::IDENTITY_RELATIONSHIP_ERROR.into());
        }
    }
    Ok(())
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
    fn person_noun_variants_do_not_bypass_identity_uncertainty() {
        let request = "Who is Arden Hale? Could these profiles describe different people?";
        let quotes = vec![
            "Arden Hale is a sculptor based in Bergen.".into(),
            "Arden Hale is an astronomer based in Quito.".into(),
        ];
        for claim in [
            "The sources describe two different individuals with matching names.",
            "These profiles refer to different persons.",
            "They describe the same individual.",
            "They are separate people.",
            "The pages do not establish the same person. The sources describe two different individuals.",
        ] {
            assert!(
                validate_relationship(claim, &quotes, request).is_err(),
                "{claim}"
            );
        }
        for claim in [
            "They could describe different individuals.",
            "The pages do not establish whether they describe the same individual.",
            "There is no evidence that they are separate people.",
        ] {
            assert!(
                validate_relationship(claim, &quotes, request).is_ok(),
                "{claim}"
            );
        }
        assert!(
            validate_relationship(
                "They describe the same individual.",
                &["Both profiles refer to the same person.".into()],
                request
            )
            .is_ok()
        );
        assert!(
            validate_relationship(
                "These profiles refer to different persons.",
                &["These profiles refer to different individuals.".into()],
                request
            )
            .is_ok()
        );
        assert!(
            validate_relationship(
                "They describe the same individual.",
                &["These profiles might describe the same individual.".into()],
                request
            )
            .is_err()
        );
    }
    #[test]
    fn categorical_identity_variants_do_not_turn_missing_mentions_into_proof() {
        let query = "Who is Arden Hale? Do these profiles establish one person?";
        let quotes = vec![
            "Arden Hale is a sculptor. The page does not mention astronomy.".into(),
            "Arden Hale is an astronomer. No sculpture work is mentioned.".into(),
        ];
        for claim in [
            "The profiles establish two different people.",
            "These are distinct individuals.",
            "These are separate individuals.",
            "The profiles prove the same person.",
        ] {
            assert!(
                validate_relationship(claim, &quotes, query).is_err(),
                "{claim}"
            );
        }
        for claim in [
            "The profiles do not establish two different people.",
            "They could be distinct individuals.",
            "Neither page proves the same person.",
        ] {
            assert!(
                validate_relationship(claim, &quotes, query).is_ok(),
                "{claim}"
            );
        }
        assert!(
            validate_relationship(
                "These are distinct individuals.",
                &["These are distinct individuals.".into()],
                query
            )
            .is_ok()
        );
    }
    #[test]
    fn different_profiles_are_not_conflicts_and_absent_mentions_are_not_exclusive() {
        let query = "Research online: Who is Lee Morgan? Use https://profiles.example/photographer and https://profiles.example/botanist. Do these pages establish one person or could they describe different people?";
        let quotes = vec![
            "Lee Morgan is a botanist based in Cork. This page does not mention any photography work.".into(),
            "Lee Morgan is a photographer based in Dunedin. The page does not mention any botany work.".into(),
        ];
        for claim in [
            "The pages do not establish one person; they only show conflicting occupations and locations under the same name.",
            "The profiles are contradictory, so the identity is unclear.",
            "The two profiles are incompatible, so the pages may describe different people.",
            "This does not establish one person. The sources explicitly state each page mentions only their respective field of work.",
        ] {
            assert_eq!(
                validate_relationship(claim, &quotes, query),
                Err(super::super::IDENTITY_RELATIONSHIP_ERROR.into()),
                "{claim}"
            );
        }
        for claim in [
            "The pages describe a photographer in Dunedin and a botanist in Cork; they do not establish whether this is one person.",
            "Different occupations are not contradictory; they could describe one person or two.",
            "Each page says it does not mention the other field, so the identity remains uncertain.",
        ] {
            assert!(
                validate_relationship(claim, &quotes, query).is_ok(),
                "{claim}"
            );
        }
        // A source that itself reports a conflict can be described as such.
        assert!(
            validate_relationship(
                "The profiles conflict about the birth year, so identity is unclear.",
                &["The two profiles conflict about the birth year.".into()],
                query
            )
            .is_ok()
        );
    }
    #[test]
    fn intensifiers_neither_create_nor_remove_an_identity_assertion() {
        let query = "Research online: Who is Lee Morgan? Use https://profiles.example/photographer and https://profiles.example/botanist. Do these pages establish one person or could they describe different people?";
        let quotes = vec![
            "Lee Morgan is a photographer based in Dunedin. The page does not mention any botany work.".into(),
            "Lee Morgan is a botanist based in Cork. This page does not mention any photography work.".into(),
        ];
        for claim in [
            "They are clearly different people.",
            "These are really distinct individuals.",
            "The pages definitively establish the same person.",
        ] {
            assert_eq!(
                validate_relationship(claim, &quotes, query),
                Err(super::super::IDENTITY_RELATIONSHIP_ERROR.into()),
                "{claim}"
            );
        }
        for claim in [
            // The cautious 4B answer rejected in the 2026-11-24 strict2 run.
            "The available sources establish two separate profiles for \"Lee Morgan\" with different professions and locations: one as a photographer in Dunedin, the other as a botanist in Cork. These pages do not definitively establish they describe the same person; they could be different people with shared names.",
            "The pages do not conclusively prove they are the same person.",
            "They are likely different people.",
        ] {
            assert!(
                validate_relationship(claim, &quotes, query).is_ok(),
                "{claim}"
            );
        }
        // An intensified quotation is still an explicit link, and an
        // intensified negation in a quotation is still not one.
        assert!(
            validate_relationship(
                "They are the same person.",
                &["Both profiles explicitly refer to the same person.".into()],
                query
            )
            .is_ok()
        );
        assert!(
            validate_relationship(
                "They are the same person.",
                &["The records do not conclusively prove they are the same person.".into()],
                query
            )
            .is_err()
        );
    }
    #[test]
    fn separate_biographies_do_not_prove_a_categorical_identity_relationship() {
        let query =
            "Research online: Who is Mira Vale? Do these profiles establish the same person?";
        let quotes = vec![
            "Mira Vale is a historian. No engineering work is mentioned.".into(),
            "Mira Vale is an engineer. No history work is mentioned.".into(),
        ];
        for claim in [
            "The two profiles describe different people.",
            "They are the same person.",
            "They are not the same person.",
        ] {
            assert!(
                validate_relationship(claim, &quotes, query).is_err(),
                "{claim}"
            );
        }
        for claim in [
            "The profiles could describe different people.",
            "The pages do not establish whether they are the same person.",
            "Mira Vale is described as a historian and an engineer by these separate profiles.",
        ] {
            assert!(
                validate_relationship(claim, &quotes, query).is_ok(),
                "{claim}"
            );
        }
        assert!(
            validate_relationship(
                "They are the same person.",
                &["Both profiles refer to the same person.".into()],
                query
            )
            .is_ok()
        );
        for quote in [
            "They could refer to the same person.",
            "They are the same person?",
            "If they are the same person, more evidence is needed.",
        ] {
            assert!(
                validate_relationship("They are the same person.", &[quote.into()], query).is_err(),
                "{quote}"
            );
        }
        assert!(
            validate_relationship(
                "The two profiles describe different people.",
                &["These are different people.".into()],
                query
            )
            .is_ok()
        );
        assert!(
            validate_relationship(
                "Different users are different people.",
                &[],
                "Explain this database schema"
            )
            .is_ok(),
            "unrelated questions are outside this narrow check"
        );
    }
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
            "Who is Mira Vale? Do these pages establish one person or could they describe different people?",
            "Who is Mira Vale? Is she the same person as the author?",
        ] {
            assert_eq!(subject(query), None);
        }
        assert_eq!(
            subject("Who is Mira Vale? Use https://profiles.example/vale?view=bio"),
            Some("mira vale".into())
        );
        assert_eq!(
            subject("Research online: Who is Mira Vale?"),
            Some("mira vale".into())
        );
        assert_eq!(
            subject("Research locally: Who is Mira Vale? Are these profiles the same person?"),
            None
        );
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
        let corrected = vec![
            "Who is Mira Vale?".into(),
            "Do these pages establish the same person?".into(),
            "Continue.".into(),
        ];
        assert_eq!(
            active_request("Who is Mira Vale?", &corrected, "Retry"),
            "Do these pages establish the same person?"
        );
        assert_eq!(for_request("Who is Mira Vale?", &corrected, "Retry"), None);
        assert_eq!(
            active_request("What is the current price?", &[], "Continue"),
            "What is the current price?"
        );
        assert_eq!(
            active_request(
                "What is the current price?",
                &[],
                "Correction: quote the historical price"
            ),
            "Correction: quote the historical price"
        );
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
