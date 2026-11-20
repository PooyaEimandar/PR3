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

pub fn terms(query: &str) -> Vec<String> {
    let lower = query.to_lowercase();
    // In a narrow quotation correction, later instructions about what NOT to
    // repeat are not positive retrieval targets. This affects ranking only;
    // the complete user request still reaches the model and permission checks.
    let focused =
        lower.contains("only") && (lower.contains("quote") || lower.contains("correction:"));
    let ranking = if focused {
        [". do not ", "; do not ", ", without ", ". web is off"]
            .iter()
            .fold(lower.as_str(), |text, suffix| {
                text.split(suffix).next().unwrap_or(text)
            })
    } else {
        &lower
    };
    let mut words: Vec<_> = ranking
        .split("sources only:")
        .next()
        .unwrap_or(ranking)
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
    best * 4 + score(text, words)
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
}
