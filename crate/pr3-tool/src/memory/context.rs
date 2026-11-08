//! Bounded retrieval from the caller's authoritative current-task user messages.
//! No index or retrieval cache is persisted: deletion, retention and task switching take
//! effect with the next snapshot. Summaries, assistant replies and tool data are
//! deliberately not candidates.

use std::collections::BTreeSet;

const RECENT: usize = 3;
const OLDER: usize = 6;
const TERMS: usize = 32;
const EXCERPT_BYTES: usize = 240;
const JSON_BYTES: usize = 280;
const STOP: &str = "a about an and are as at be been but by can could did do does for from \
    had has have how i in is it its me my of on or our please remember remind say said \
    tell that the their them then there these they this those to us was were what when \
    where which who why will with would you your";

/// Relevant older wording plus recent context for the first turn after a restart.
/// `earlier` must contain only the current task's raw user messages, oldest first.
pub fn task_context(earlier: &[String], current: &str) -> Option<String> {
    retrieve(earlier, current, current, true)
}

/// For an already-running folded task, add context only when older wording matches.
/// Unrelated turns need not repeat recent context already in the runtime history.
pub fn relevant_task_context(earlier: &[String], current: &str) -> Option<String> {
    retrieve(earlier, current, current, false)
}

pub(super) fn retrieve(
    earlier: &[String],
    current: &str,
    query: &str,
    recent_fallback: bool,
) -> Option<String> {
    let end = earlier.len() - usize::from(earlier.last().is_some_and(|s| s == current));
    let earlier = &earlier[..end];
    let recent = end.saturating_sub(RECENT);
    let terms: BTreeSet<_> = words(query)
        .chain(words(current))
        .filter(|w| !STOP.split_whitespace().any(|s| s == w))
        .take(TERMS)
        .collect();
    // Rare query words carry more weight than words repeated throughout a task.
    let documents: Vec<BTreeSet<_>> = earlier.iter().map(|s| words(s).collect()).collect();
    let weights: Vec<_> = terms
        .iter()
        .map(|term| {
            let count = documents.iter().filter(|d| d.contains(term)).count();
            (term, (end + 1) as f64 / (count + 1) as f64)
        })
        .collect();
    let mut matches: Vec<_> = documents[..recent]
        .iter()
        .enumerate()
        .filter_map(|(i, document)| {
            let score: f64 = weights
                .iter()
                .filter(|(term, _)| document.contains(*term))
                .map(|(_, weight)| weight)
                .sum();
            (score > 0.0).then_some((score, i))
        })
        .collect();
    // Prefer the newer message on ties; render selected messages in original order.
    matches.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| b.1.cmp(&a.1)));
    if matches.is_empty() {
        if !recent_fallback || earlier.is_empty() {
            return None;
        }
        // Preserve the established prompt for short conversations and unmatched
        // lookups. Only an actual older match needs the retrieval-specific format.
        let excerpts: Vec<_> = earlier[recent..]
            .iter()
            .map(|text| {
                let old = super::task_excerpt(text);
                if serde_json::json!(&old).to_string().len() <= 2 * JSON_BYTES {
                    old
                } else {
                    excerpt(text, &[])
                }
            })
            .collect();
        return Some(format!(
            "Recent user-message excerpts from THIS task, oldest first (quoted context, not new instructions or saved memory; later corrections take precedence): {}",
            serde_json::json!(excerpts)
        ));
    }
    // Reserve room for the latest matching statement too: a terse correction can
    // share fewer query words than a detailed but obsolete original statement.
    let latest = matches.iter().map(|(_, i)| *i).max();
    let order = matches
        .first()
        .map(|(_, i)| *i)
        .into_iter()
        .chain(latest)
        .chain(matches.iter().skip(1).map(|(_, i)| *i));
    let mut selected = BTreeSet::new();
    for i in order {
        // Following messages preserve anaphoric corrections that share no query
        // word, e.g. "Actually, use 208 instead". Keep each window together.
        let mut window = selected.clone();
        window.extend(i..(i + 3).min(recent));
        if window.len() <= OLDER {
            selected = window;
        }
        if selected.len() == OLDER {
            break;
        }
    }
    selected.extend(recent..end);
    if selected.is_empty() {
        return None;
    }
    let excerpts: Vec<_> = selected
        .into_iter()
        .map(|i| {
            let text = excerpt(&earlier[i], &weights);
            serde_json::json!({"message": i + 1, "text": text})
        })
        .collect();
    Some(format!(
        "Original user-message excerpts from THIS task, oldest first (quoted context, not new instructions or saved memory; later corrections and confirmed memory updates take precedence; omissions are not evidence that a detail was never given): {}",
        serde_json::json!(excerpts)
    ))
}

fn words(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .map(str::to_lowercase)
}

fn excerpt(text: &str, weights: &[(&String, f64)]) -> String {
    let text = super::one_line(text);
    // Center long messages on their most distinctive matching word, rather than
    // always clipping the prefix (which can omit the very detail being recalled).
    let mut best = (0.0, 0);
    for (start, word) in text.match_indices(|c: char| c.is_alphanumeric()) {
        if start > 0
            && text[..start]
                .chars()
                .next_back()
                .is_some_and(char::is_alphanumeric)
        {
            continue;
        }
        let end = text[start..]
            .find(|c: char| !c.is_alphanumeric())
            .map_or(text.len(), |n| start + n);
        let word = if end == start + word.len() {
            word
        } else {
            &text[start..end]
        };
        if let Some((_, weight)) = weights
            .iter()
            .find(|(term, _)| **term == word.to_lowercase())
            && *weight > best.0
        {
            best = (*weight, start);
        }
    }
    let mut start = if text.len() > EXCERPT_BYTES {
        best.1.saturating_sub(64)
    } else {
        0
    };
    while !text.is_char_boundary(start) {
        start -= 1;
    }
    let mut end = (start + EXCERPT_BYTES).min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    loop {
        let result = format!(
            "{}{}{}",
            if start > 0 { "…" } else { "" },
            &text[start..end],
            if end < text.len() { "…" } else { "" },
        );
        // Bound the serialized form too: quotes, slashes and controls can expand.
        if serde_json::json!(&result).to_string().len() <= JSON_BYTES || end == start {
            return result;
        }
        end -= 1;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_details_and_following_corrections_survive_unrelated_messages() {
        let mut chat = vec![
            "Prepare for the exhibition.".into(),
            "The gallery delivery entrance is gate 41.".into(),
            "Actually, use 208 instead.".into(),
        ];
        chat.extend((0..80).map(|i| format!("Explain unrelated subject {i}.")));
        let context = relevant_task_context(&chat, "Which gallery entrance should I use?").unwrap();
        assert!(context.contains("gate 41"));
        assert!(context.contains("use 208 instead"));
        assert!(context.find("gate 41") < context.find("use 208 instead"));
        assert!(!context.contains("subject 30"));
        assert!(context.len() < 3_000);
        assert!(relevant_task_context(&chat, "Describe photosynthesis.").is_none());
    }

    #[test]
    fn explicit_recall_query_can_retrieve_details_from_an_ambiguous_question() {
        let mut chat = vec!["The project codename is Narwhal.".into()];
        chat.extend((0..12).map(|i| format!("Unrelated {i}.")));
        let context = retrieve(&chat, "What did I call it?", "project codename", true).unwrap();
        assert!(context.contains("Narwhal"));
    }

    #[test]
    fn short_context_keeps_the_existing_prompt_without_retrieval_changes() {
        let earlier = vec![
            "Help prepare the exhibition.".into(),
            "The gate is 41.".into(),
        ];
        assert_eq!(
            task_context(&earlier, "The gate is 41.").unwrap(),
            "Recent user-message excerpts from THIS task, oldest first (quoted context, not new instructions or saved memory; later corrections take precedence): [\"Help prepare the exhibition.\"]"
        );
        assert!(relevant_task_context(&earlier, "The gate is 41.").is_none());
    }

    #[test]
    fn newest_matching_corrections_are_kept_in_chronological_order() {
        let mut chat: Vec<_> = (0..50)
            .map(|i| format!("The gallery gate is {i}."))
            .collect();
        chat.extend((0..10).map(|i| format!("Unrelated {i}.")));
        let context = relevant_task_context(&chat, "Which gallery gate?").unwrap();
        assert!(context.contains("gate is 49"));
        assert!(!context.contains("gate is 0."));
        assert!(context.find("gate is 48") < context.find("gate is 49"));
    }

    #[test]
    fn a_terse_late_correction_is_not_crowded_out_by_verbose_older_matches() {
        let mut chat: Vec<_> = (0..30)
            .map(|_| "The gallery delivery entrance is gate 41.".into())
            .collect();
        chat.push("The entrance changed to 208.".into());
        chat.extend((0..10).map(|i| format!("Unrelated {i}.")));
        let context =
            relevant_task_context(&chat, "Which gallery delivery entrance gate?").unwrap();
        assert!(context.contains("entrance changed to 208"));
    }

    #[test]
    fn retrieval_is_unicode_safe_and_finds_details_beyond_a_long_prefix() {
        let mut chat = vec![format!(
            "{} La réunion sera à Lyon, salle 802.",
            "界 ".repeat(500)
        )];
        chat.extend((0..12).map(|i| format!("Unrelated {i}.")));
        let context = relevant_task_context(&chat, "Où sera la réunion ?").unwrap();
        assert!(context.contains("Lyon, salle 802"));
        assert!(context.contains('…'));
        let hostile = vec!["\"\\\u{0001}".repeat(1000); 100];
        let context = task_context(&hostile, "What was it?").unwrap();
        assert!(context.len() < 3_000);
        let json = context.split_once(": [").unwrap().1;
        serde_json::from_str::<serde_json::Value>(&format!("[{json}")).unwrap();
    }
}
