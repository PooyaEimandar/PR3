//! Whole-message commands only. Translation never grants write permission.

use super::Command;

const SAVE: &[&str] = &[
    "remember that",
    "remember:",
    "save to memory:",
    "به خاطر بسپار:",
    "یادت باشد:",
    "ذخیره در حافظه:",
    "تذكر:",
    "احفظ في الذاكرة:",
    "recuerda que",
    "recuerda:",
    "guarda en memoria:",
    "retiens que",
    "mémorise:",
    "souviens-toi que",
    "merke dir:",
    "speichere in der erinnerung:",
    "记住:",
    "記住:",
    "覚えて:",
];
const UPDATE: &[&str] = &[
    "update memory:",
    "به‌روزرسانی حافظه:",
    "به روز رسانی حافظه:",
    "حدّث الذاكرة:",
    "حدث الذاكرة:",
    "actualiza la memoria:",
    "mets à jour la mémoire:",
    "aktualisiere die erinnerung:",
    "更新记忆:",
    "更新記憶:",
    "記憶を更新:",
];
const CORRECT: &[&str] = &[
    "correct memory #",
    "تصحیح حافظه #",
    "صحح الذاكرة #",
    "corrige la memoria #",
    "corrige la mémoire #",
    "korrigiere erinnerung #",
    "更正记忆 #",
    "更正記憶 #",
    "記憶を訂正 #",
];

fn without_please(text: &str) -> &str {
    [
        "please",
        "لطفاً",
        "لطفا",
        "من فضلك",
        "por favor",
        "s'il te plaît",
        "bitte",
    ]
    .iter()
    .find_map(|p| prefix(text, p))
    .unwrap_or(text)
}

/// Case-insensitive Unicode prefix, with a word boundary unless delimited by ':'
/// or '#'. Fullwidth colons work in every command; the fact stays byte-for-byte intact.
fn prefix<'a>(text: &'a str, command: &str) -> Option<&'a str> {
    let mut chars = text.char_indices();
    let mut end = 0;
    for expected in command.chars() {
        let (offset, actual) = chars.next()?;
        if !(expected == ':' && actual == '：')
            && !actual.to_lowercase().eq(expected.to_lowercase())
        {
            return None;
        }
        end = offset + actual.len_utf8();
    }
    let rest = text.get(end..)?;
    if !command.ends_with([':', '#']) && !rest.is_empty() && !rest.starts_with(char::is_whitespace)
    {
        return None;
    }
    Some(rest.trim_start())
}

pub fn capture(text: &str) -> Option<&str> {
    let text = without_please(text.trim());
    SAVE.iter().find_map(|p| prefix(text, p)).map(str::trim)
}

pub fn command(text: &str) -> Option<Command<'_>> {
    let text = without_please(text.trim());
    if let Some(rest) = CORRECT.iter().find_map(|p| prefix(text, p)) {
        return Some(match rest.split_once([':', '：']) {
            Some((id, fact)) => match number(id.trim()) {
                Some(id) => Command::Correct {
                    id,
                    text: fact.trim(),
                },
                None => Command::Invalid,
            },
            None => Command::Invalid,
        });
    }
    if let Some(text) = prefix(text, "save separate memory:") {
        return Some(Command::Save {
            text: text.trim(),
            separate: true,
        });
    }
    UPDATE
        .iter()
        .find_map(|p| prefix(text, p))
        .or_else(|| capture(text))
        .map(|text| Command::Save {
            text: text.trim(),
            separate: false,
        })
}

fn number(text: &str) -> Option<i64> {
    if text.is_empty() {
        return None;
    }
    let mut value = 0i64;
    for c in text.chars() {
        let digit = match c {
            '0'..='9' => c as u32 - '0' as u32,
            '٠'..='٩' => c as u32 - '٠' as u32,
            '۰'..='۹' => c as u32 - '۰' as u32,
            '０'..='９' => c as u32 - '０' as u32,
            _ => return None,
        };
        value = value.checked_mul(10)?.checked_add(i64::from(digit))?;
    }
    (value > 0).then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multilingual_commands_preserve_facts_and_require_explicit_boundaries() {
        for (message, fact) in [
            ("Please remember that I live in Dubai.", "I live in Dubai."),
            (
                "لطفاً به خاطر بسپار: من در دبی زندگی می‌کنم.",
                "من در دبی زندگی می‌کنم.",
            ),
            ("احفظ في الذاكرة: أسكن في دبي.", "أسكن في دبي."),
            ("Recuerda que vivo en Dubái.", "vivo en Dubái."),
            ("MÉMORISE: J’habite à Dubaï.", "J’habite à Dubaï."),
            ("Merke dir: Ich wohne in Dubai.", "Ich wohne in Dubai."),
            ("记住：我住在迪拜。", "我住在迪拜。"),
            ("覚えて：ドバイに住んでいます。", "ドバイに住んでいます。"),
            ("更新記憶：我住在台北。", "我住在台北。"),
        ] {
            assert!(
                matches!(command(message), Some(Command::Save { text, separate: false }) if text == fact),
                "{message}"
            );
        }
        for message in [
            "تصحیح حافظه #۱۲: دبی",
            "صحح الذاكرة #١٢: دبي",
            "Corrige la mémoire #12: Dubaï",
            "更正记忆 #１２：迪拜",
        ] {
            assert!(
                matches!(command(message), Some(Command::Correct { id: 12, .. })),
                "{message}"
            );
        }
        for message in [
            "تصحیح حافظه #۰: x",
            "صحح الذاكرة #-١: x",
            "Correct memory #9223372036854775808: x",
            "Correct memory #3 x",
        ] {
            assert!(
                matches!(command(message), Some(Command::Invalid)),
                "{message}"
            );
        }
        for message in [
            "don't remember that x",
            "لا تتذكر: دبي",
            "یادت باشدها",
            "remember thatcher",
            "recuerda queso",
            "Translate ‘记住：我住在迪拜’",
            "Do you remember my address?",
        ] {
            assert!(command(message).is_none(), "{message}");
        }
    }
}
