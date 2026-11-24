//! Narrow presentation check: model-written report prose follows the language
//! of the user's request. Function-word profiles only, not translation or a
//! general language identifier. It abstains whenever either side is unclear,
//! and never inspects quotations, which keep their original language.

#[derive(Clone, Copy, PartialEq)]
enum Language {
    English,
    Spanish,
    French,
    Persian,
}

const ENGLISH: &[&str] = &[
    "the", "and", "of", "is", "are", "was", "were", "not", "cannot", "from", "with", "this",
    "these", "that", "which", "for", "to", "in", "be", "has", "have", "it", "its", "by", "or",
    "but", "neither", "both",
];
// Words shared by Spanish and French ("la", "de", "que", "un", "ni", "se")
// are left out so that they cannot tie the two languages.
const SPANISH: &[&str] = &[
    "el", "los", "las", "del", "y", "no", "con", "para", "es", "son", "una", "por", "como", "pero",
    "está", "están", "esta", "estas", "este", "sus", "su", "al", "lo",
];
const FRENCH: &[&str] = &[
    "le", "les", "des", "du", "et", "ne", "pas", "est", "sont", "une", "pour", "avec", "par",
    "dans", "sur", "ce", "cette", "ces", "au", "aux", "mais",
];

/// A request is short, so two distinctive function words identify it; prose
/// needs three. Either side may abstain.
fn detect(text: &str, least: usize) -> Option<Language> {
    let persian = text
        .split_whitespace()
        .filter(|w| w.chars().any(|c| ('\u{0600}'..='\u{06FF}').contains(&c)))
        .count();
    if persian >= 3 {
        return Some(Language::Persian);
    }
    let words = text
        .split(|c: char| !c.is_alphabetic())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect::<Vec<_>>();
    let count = |list: &[&str]| words.iter().filter(|w| list.contains(&w.as_str())).count();
    let scores = [
        (Language::English, count(ENGLISH)),
        (Language::Spanish, count(SPANISH)),
        (Language::French, count(FRENCH)),
    ];
    let (best, top) = scores.iter().copied().max_by_key(|(_, n)| *n)?;
    let next = scores
        .iter()
        .filter(|(language, _)| *language != best)
        .map(|(_, n)| *n)
        .max()
        .unwrap_or(0);
    (top >= least && top >= 2 * next).then_some(best)
}

/// Reject prose clearly written in another language than a clearly identified
/// request, such as an English conclusion to a Spanish question.
pub(super) fn follows_request(prose: &str, request: &str) -> Result<(), String> {
    let (Some(asked), Some(written)) = (detect(request, 2), detect(prose, 3)) else {
        return Ok(());
    };
    if asked == written {
        return Ok(());
    }
    let name = match asked {
        Language::English => "English",
        Language::Spanish => "Spanish",
        Language::French => "French",
        Language::Persian => "Persian",
    };
    Err(format!(
        "Write the conclusion, finding text and gaps in {name}, the language of the user's request. Quotations stay exact. Keep the same citations and repair within the remaining budget."
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prose_must_follow_a_clear_request_language_and_abstains_otherwise() {
        let spanish = "¿Podemos confirmar el precio actual del pase Cobre? Compara estas dos notas y explica la incertidumbre temporal en español.";
        assert!(
            follows_request(
                "No, the current price of the Cobre pass cannot be confirmed. The two notes differ in amount and in their dates.",
                spanish
            )
            .is_err()
        );
        assert!(
            follows_request(
                "No se puede confirmar el precio actual del pase Cobre: una promoción terminó en 2022 y el catálogo no tiene fecha.",
                spanish
            )
            .is_ok()
        );
        let french = "Comparez la conservation et la condition de restauration de Jade et Agate.";
        assert!(
            follows_request(
                "The retention of Jade is 35 days and the restoration of Agate is restricted.",
                french
            )
            .is_err()
        );
        // Short, mixed or technical prose is not enough evidence of a language.
        assert!(follows_request("default-features = false", spanish).is_ok());
        assert!(follows_request("No se puede confirmar.", "Is it current?").is_ok());
        assert!(
            follows_request(
                "The current price cannot be established from these quotations.",
                "What current price can we establish for the Sable pass?"
            )
            .is_ok()
        );
    }
}
