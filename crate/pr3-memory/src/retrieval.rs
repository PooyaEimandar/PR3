//! Bounded lexical retrieval helpers. These rank untrusted text; they never
//! establish truth, source authority or permission. Offsets always refer to the
//! original UTF-8 text. No generated query is sent to a provider.
use alloc::{
    string::{String, ToString},
    vec::Vec,
};

fn stem(word: &str) -> String {
    let lower = word.to_lowercase();
    if !lower.is_ascii() || lower.len() < 5 {
        return lower;
    }
    let mut s = lower.as_str();
    for suffix in ["ing", "ers", "ies", "ed", "er", "ly", "s"] {
        if let Some(base) = s.strip_suffix(suffix).filter(|b| b.len() >= 3) {
            s = base;
            break;
        }
    }
    s.strip_suffix('e')
        .filter(|b| b.len() >= 4)
        .unwrap_or(s)
        .to_string()
}

/// A bounded presentation hint from the actual request, never from source text.
/// It is not a translation, semantic classifier or permission decision.
pub fn focused_request(query: &str) -> bool {
    let lower = query.to_lowercase();
    let question = ["research online:", "research locally:", "research:"]
        .iter()
        .find_map(|prefix| lower.trim().strip_prefix(prefix))
        .unwrap_or(&lower)
        .trim();
    let text = question.split("sources only:").next().unwrap_or(question);
    let correction = text.split_once(':').is_some_and(|(label, _)| {
        matches!(
            label.trim(),
            "correction" | "corrección" | "correccion" | "اصلاح"
        )
    });
    correction
        || (text.contains("only")
            && text.contains("quote")
            && !text.contains("not only")
            && !text.contains("don't only"))
        || ((text.contains("seulement") || text.contains("uniquement"))
            && (text.contains("citation") || text.contains("citez"))
            && !text.contains("pas seulement"))
        || (text.contains("فقط") && text.contains("نقل"))
}

fn ranking_query(query: &str) -> String {
    let lower = query.to_lowercase();
    // In a narrow quotation correction, later instructions about what NOT to
    // repeat are not positive retrieval targets. This affects ranking only;
    // the complete user request still reaches the model and permission checks.
    let focused = focused_request(&lower);
    let positive = [". do not ", "; do not "]
        .iter()
        .fold(lower.as_str(), |text, suffix| {
            text.split(suffix).next().unwrap_or(text)
        });
    let mut ranking = if focused {
        [
            ". do not ",
            "; do not ",
            ", without ",
            ". web is off",
            ", sans ",
            ". ne ",
            "; ne ",
        ]
        .iter()
        .fold(positive, |text, suffix| {
            text.split(suffix).next().unwrap_or(text)
        })
    } else {
        positive
    };
    // A Persian negative follow-up clause is not a second positive retrieval
    // target. Keep ordinary semicolon-separated positive conditions intact.
    if focused
        && let Some((before, after)) = ranking.split_once('؛')
        && (after.contains("نکن") || after.contains("نگو"))
    {
        ranking = before;
    }
    ranking.to_string()
}

/// Comparison guidance must follow the positive request, not a later clause
/// saying which old comparison to omit. This is only an inference hint.
pub fn comparison_request(query: &str) -> bool {
    ranking_query(query)
        .split("sources only:")
        .next()
        .unwrap_or_default()
        .split_whitespace()
        .filter(|word| !word.contains("://"))
        .flat_map(|word| word.split(|c: char| !c.is_alphabetic()))
        .any(|word| {
            matches!(
                word,
                "compare"
                    | "compares"
                    | "comparing"
                    | "comparison"
                    | "comparisons"
                    | "comparez"
                    | "comparer"
                    | "comparaison"
                    | "comparaisons"
                    | "compara"
                    | "comparar"
                    | "comparación"
                    | "comparacion"
                    | "comparaciones"
                    | "مقایسه"
            )
        })
}

pub fn terms(query: &str) -> Vec<String> {
    let ranking = ranking_query(query);
    let mut words: Vec<_> = ranking
        .split("sources only:")
        .next()
        .unwrap_or(&ranking)
        .split_whitespace()
        .filter(|w| !w.contains("://"))
        .flat_map(|w| w.split(|c: char| !c.is_alphanumeric()))
        .filter(|w| w.chars().count() > 2)
        .filter(|w| {
            !matches!(
                *w,
                "the"
                    | "and"
                    | "from"
                    | "with"
                    | "what"
                    | "this"
                    | "that"
                    | "source"
                    | "sources"
                    | "saved"
                    | "research"
                    | "using"
                    | "only"
                    | "quote"
                    | "does"
                    | "compare"
                    | "state"
                    | "use"
                    | "during"
                    | "also"
                    | "please"
                    | "now"
                    | "correction"
                    | "discuss"
                    | "one"
                    | "finding"
                    | "findings"
                    | "continue"
                    | "resume"
                    | "evidence"
                    | "snapshot"
                    | "snapshots"
                    | "answer"
                    | "question"
                    | "explain"
                    | "whether"
                    | "when"
                    | "how"
                    | "can"
                    | "relevant"
            )
        })
        .map(stem)
        .collect();
    words.sort();
    words.dedup();
    words
}

pub fn matches(text: &str, words: &[String]) -> Vec<bool> {
    let tokens: Vec<_> = text
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(stem)
        .collect();
    words
        .iter()
        .map(|w| tokens.iter().any(|t| t == w))
        .collect()
}

/// Adjacent meaningful query terms distinguish a requested concept (such as
/// "default features") from unrelated occurrences of either word on a page.
pub fn phrases(query: &str) -> Vec<(String, String)> {
    let allowed = terms(query);
    let ranking = ranking_query(query);
    let tokens = ranking
        .split_whitespace()
        .filter(|word| !word.contains("://"))
        .flat_map(|word| word.split(|c: char| !c.is_alphanumeric()))
        .map(stem)
        .filter(|word| allowed.contains(word))
        .collect::<Vec<_>>();
    let mut result = tokens
        .windows(2)
        .filter_map(|pair| match pair {
            [a, b] if a != b => Some((a.clone(), b.clone())),
            _ => None,
        })
        .collect::<Vec<_>>();
    result.sort();
    result.dedup();
    result
}

pub fn phrase_score(text: &str, phrases: &[(String, String)]) -> usize {
    let tokens = text
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| word.len() > 2)
        .map(stem)
        .collect::<Vec<_>>();
    phrases
        .iter()
        .filter(|(a, b)| {
            tokens
                .windows(2)
                .any(|pair| matches!(pair, [x,y] if x == a && y == b))
        })
        .count()
}

pub fn score(text: &str, words: &[String]) -> usize {
    matches(text, words).into_iter().filter(|m| *m).count()
}

/// Non-overlapping exact sentences/lines, including their punctuation. Empty
/// segments are omitted, and no discontiguous text is silently concatenated.
pub fn sentences(text: &str) -> Vec<(usize, &str)> {
    let mut result = Vec::new();
    let mut start = 0;
    for end in text
        .char_indices()
        .filter_map(|(i, c)| {
            let end = i + c.len_utf8();
            (matches!(c, '\n' | ';' | '؛' | '。')
                || (matches!(c, '.' | '!' | '?')
                    && text
                        .get(end..)
                        .and_then(|s| s.chars().next())
                        .is_none_or(char::is_whitespace)))
            .then_some(end)
        })
        .chain(core::iter::once(text.len()))
    {
        if let Some(raw) = text.get(start..end) {
            let trimmed = raw.trim();
            if !trimmed.is_empty() {
                result.push((start + raw.len() - raw.trim_start().len(), trimmed));
            }
        }
        start = end;
    }
    result
}

/// Favor a compact supporting passage over query words scattered throughout a
/// long part. Two neighboring sentences preserve causal/conditional context.
pub fn passage_score(text: &str, words: &[String]) -> usize {
    let parts = sentences(text);
    let best = parts
        .iter()
        .enumerate()
        .map(|(i, (start, s))| {
            let end = parts
                .get(i + 1)
                .map_or(start + s.len(), |(at, t)| at + t.len());
            score(text.get(*start..end).unwrap_or(s), words)
        })
        .max()
        .unwrap_or(0);
    // An option heading names the scope of the following rule. Without this,
    // a nearby configuration example can outrank the actual command option.
    // This is relevance only, never authority or permission to run a command.
    best * 4 + score(text, words) + option_score(text, words) * 12
}

pub fn option_score(text: &str, words: &[String]) -> usize {
    text.lines()
        .map(str::trim)
        .filter(|line| {
            line.starts_with("--") && line.len() <= 100 && !line.contains(char::is_whitespace)
        })
        .map(|line| score(line, words))
        // A generic match such as "file" in --dirstat-by-file must not
        // outrank the requested passage. Require a compound scope match.
        .filter(|matched| *matched >= 2)
        .max()
        .unwrap_or(0)
}

pub fn rule_scope_score(text: &str, words: &[String]) -> usize {
    // The default rule is a separate requested case, not covered merely by an
    // exception saying "unlike the default". Prefer the affirmative rule's scope.
    let default = sentences(text)
        .iter()
        .filter(|(_, line)| line.to_lowercase().starts_with("by default,"))
        .map(|_| score("default", words))
        .max()
        .unwrap_or(0);
    option_score(text, words).max(default)
}

/// Choose at most `limit` parts across the whole bounded page. New query-term
/// coverage wins before another repeated hit; original indices remain intact.
pub fn select_parts(parts: &[String], query: &str, limit: usize) -> Vec<usize> {
    let words = terms(query);
    let ranked: Vec<_> = parts
        .iter()
        .map(|text| (matches(text, &words), passage_score(text, &words)))
        .collect();
    let mut covered = alloc::vec![false;words.len()];
    let mut selected = Vec::new();
    while selected.len() < limit.min(parts.len()) {
        let next = ranked
            .iter()
            .enumerate()
            .filter(|(i, _)| !selected.contains(i))
            .max_by_key(|(i, (hits, rank))| {
                (
                    hits.iter()
                        .zip(&covered)
                        .filter(|(hit, seen)| **hit && !**seen)
                        .count(),
                    *rank,
                    core::cmp::Reverse(*i),
                )
            })
            .map(|(i, _)| i);
        let Some(next) = next else { break };
        if let Some((hits, _)) = ranked.get(next) {
            for (seen, hit) in covered.iter_mut().zip(hits) {
                *seen |= hit;
            }
        }
        selected.push(next);
    }
    selected
}

#[cfg(test)]
#[allow(clippy::indexing_slicing)]
mod tests {
    use super::*;
    #[test]
    fn selection_reaches_late_parts_without_expanding_the_limit() {
        let mut parts = alloc::vec!["Background navigation.".to_string(); 30];
        parts[27] = "Orchid permits readers during a commit.".into();
        parts[29] = "Orchid requires processes on the same host.".into();
        let selected = select_parts(
            &parts,
            "Orchid reader behavior during commit and host restriction",
            12,
        );
        assert_eq!(selected.len(), 12);
        assert!(selected.contains(&27) && selected.contains(&29));
        assert_eq!(
            terms("State the capacity. Sources only: portal.example.org"),
            ["capacity"]
        );
        assert_eq!(
            score(
                "A writer is writing while a reader reads.",
                &terms("writers reading")
            ),
            2
        );
    }

    #[test]
    fn quote_only_ranking_does_not_reintroduce_explicitly_excluded_subjects() {
        let positive =
            terms("Correction: quote only the Orchid host restriction from the saved sources");
        assert_eq!(
            terms(
                "Correction: quote only the Orchid host restriction from the saved sources. Do not fetch or repeat the reader comparison."
            ),
            positive
        );
        assert_eq!(
            terms(
                "Correction: quote only the trial quota, without discussing eligibility or fetching pages."
            ),
            terms("Correction: quote only the trial quota")
        );
        assert_eq!(
            terms(
                "Use only the saved evidence. Now quote the WAL host restriction in one finding."
            ),
            terms("WAL host restriction"),
            "the actual quotation request may follow an introductory sentence"
        );
        assert_eq!(
            terms(
                "Correction: discuss only the documentation exception. Do not fetch pages or repeat the public-domain finding."
            ),
            terms("documentation exception")
        );
        assert!(
            terms("Why can free accounts not export?").contains(&"export".into()),
            "ordinary questions retain their complete retrieval terms"
        );
    }

    #[test]
    fn comparison_hint_ignores_excluded_comparisons_and_source_addresses() {
        for query in [
            "Correction: quote only the host restriction. Do not repeat the reader comparison.",
            "Correction: quote only the quota, without a comparison.",
            "Correction : citez seulement la restauration, sans comparaison de conservation.",
            "اصلاح: فقط شرط بازیابی را نقل کن؛ مقایسه را تکرار نکن",
            "Quote the price. Sources only: compare.example",
            "Quote https://example.org/comparison",
        ] {
            assert!(!comparison_request(query), "{query}");
        }
        for query in [
            "Compare Lumen and Reed reader behavior. Do not repeat prices.",
            "Correction: compare only Lumen and Reed host restrictions.",
            "Correction : comparez seulement les conditions de restauration.",
            "Compara las condiciones de restauración.",
            "اصلاح: فقط شرط بازیابی را مقایسه کن؛ مدت نگهداری را تکرار نکن",
        ] {
            assert!(comparison_request(query), "{query}");
        }
    }

    #[test]
    fn focused_multilingual_corrections_keep_positive_scope_without_negative_followups() {
        for (positive, full) in [
            (
                "Correction : citez seulement la condition de restauration",
                "Correction : citez seulement la condition de restauration, sans répéter la conservation",
            ),
            (
                "اصلاح: فقط شرط بازیابی را نقل کن",
                "اصلاح: فقط شرط بازیابی را نقل کن؛ مدت نگهداری را تکرار نکن",
            ),
            (
                "Corrección: cita la condición de restauración",
                "Corrección: cita la condición de restauración",
            ),
        ] {
            assert!(focused_request(full), "{full}");
            assert_eq!(terms(full), terms(positive), "{full}");
            assert_eq!(phrases(full), phrases(positive), "{full}");
        }
        assert!(focused_request(
            "Research locally: Correction : citez la condition"
        ));
        for query in [
            "Compare retention and restoration",
            "Not only quote the condition; explain it",
            "Ne citez pas seulement la condition",
            "Sources only: https://example.org/correction:quote-only",
        ] {
            assert!(!focused_request(query), "{query}");
        }
        assert!(terms("اصلاح: فقط شرط بازیابی؛ همچنین مدت نگهداری").contains(&"نگهداری".into()));
        assert!(
            terms("Comparez la conservation et la restauration").contains(&"conservation".into())
        );
    }
}
