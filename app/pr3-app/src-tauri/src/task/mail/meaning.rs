//! Small, auditable concept vocabulary, not a general language classifier. It
//! expands a user's exclusion to related fields across languages without inference.
//! Values still come only from original user messages, never invented replacements.

use super::{Selector, quoted, select};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Category {
    Financial,
    Health,
    Contact,
    Access,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct Target {
    fields: u16,
    /// A user explicitly allowed part of an already established exclusion. The
    /// remaining fields stay excluded if supplied later, even when absent now.
    pub(super) allow_empty: bool,
}

impl Target {
    pub(super) fn without(self, other: Self) -> Option<Self> {
        let fields = self.fields & !other.fields;
        (fields != 0).then_some(Self {
            fields,
            allow_empty: self.allow_empty || fields != self.fields,
        })
    }
}

struct Field {
    category: Category,
    labels: &'static [&'static str],
}
use Category::*;

const FIELDS: &[Field] = &[
    Field {
        category: Financial,
        labels: &[
            "salary",
            "annual salary",
            "income",
            "annual income",
            "salaire",
            "revenu",
            "salario",
            "ingresos",
            "حقوق",
            "درآمد",
            "راتبي",
            "راتب",
            "دخلي",
        ],
    },
    Field {
        category: Financial,
        labels: &[
            "bank account",
            "account number",
            "iban",
            "compte bancaire",
            "numéro de compte",
            "cuenta bancaria",
            "número de cuenta",
            "شماره حساب",
            "حساب بانکی",
            "رقم حسابي",
            "حسابي البنكي",
        ],
    },
    Field {
        category: Financial,
        labels: &[
            "card number",
            "credit card number",
            "numéro de carte",
            "número de tarjeta",
            "شماره کارت",
            "رقم بطاقتي",
        ],
    },
    Field {
        category: Financial,
        labels: &[
            "tax id",
            "tax number",
            "numéro fiscal",
            "identificación fiscal",
            "شناسه مالیاتی",
            "رقمي الضريبي",
        ],
    },
    Field {
        category: Health,
        labels: &[
            "diagnosis",
            "medical condition",
            "diagnostic",
            "diagnóstico",
            "تشخیص",
            "بیماری",
            "تشخيصي",
            "حالتي الصحية",
        ],
    },
    Field {
        category: Health,
        labels: &[
            "medication",
            "medicine",
            "médicament",
            "traitement",
            "medicamento",
            "medicación",
            "دارو",
            "دوائي",
        ],
    },
    Field {
        category: Health,
        labels: &[
            "allergy",
            "allergies",
            "allergie",
            "alergia",
            "حساسیت",
            "حساسيتي",
        ],
    },
    Field {
        category: Contact,
        labels: &[
            "home address",
            "postal address",
            "adresse personnelle",
            "adresse postale",
            "dirección de casa",
            "dirección postal",
            "آدرس منزل",
            "نشانی منزل",
            "عنوان منزلي",
        ],
    },
    Field {
        category: Contact,
        labels: &[
            "phone number",
            "mobile number",
            "numéro de téléphone",
            "número de teléfono",
            "شماره تلفن",
            "شماره موبایل",
            "رقم هاتفي",
        ],
    },
    Field {
        category: Contact,
        labels: &[
            "personal email",
            "personal email address",
            "adresse e-mail personnelle",
            "correo personal",
            "ایمیل شخصی",
            "بريدي الشخصي",
        ],
    },
    Field {
        category: Access,
        labels: &[
            "password",
            "passcode",
            "mot de passe",
            "contraseña",
            "رمز عبور",
            "گذرواژه",
            "كلمة مروري",
        ],
    },
    Field {
        category: Access,
        labels: &[
            "access code",
            "door code",
            "gate code",
            "code d'accès",
            "código de acceso",
            "کد دسترسی",
            "رمز الدخول",
        ],
    },
];

// A larger vocabulary must widen the mask; excess fields fail at compile time.
const _: usize = u16::BITS as usize - FIELDS.len();

const CATEGORIES: &[(Category, &[&str])] = &[
    (
        Financial,
        &[
            "finances",
            "financial details",
            "financial information",
            "financial data",
            "banking details",
            "informations financières",
            "données financières",
            "datos financieros",
            "información financiera",
            "اطلاعات مالی",
            "جزئیات مالی",
            "معلوماتي المالية",
            "المعلومات المالية",
        ],
    ),
    (
        Health,
        &[
            "health details",
            "health information",
            "medical details",
            "medical information",
            "health data",
            "informations médicales",
            "informations de santé",
            "datos médicos",
            "información médica",
            "اطلاعات پزشکی",
            "اطلاعات سلامتی",
            "معلوماتي الصحية",
            "معلوماتي الطبية",
        ],
    ),
    (
        Contact,
        &[
            "contact details",
            "contact information",
            "coordonnées",
            "datos de contacto",
            "información de contacto",
            "اطلاعات تماس",
            "معلومات الاتصال الخاصة بي",
        ],
    ),
    (
        Access,
        &[
            "login details",
            "access details",
            "credentials",
            "identifiants",
            "credenciales",
            "اطلاعات ورود",
            "بيانات الدخول",
        ],
    ),
];

pub(super) fn target(text: &str) -> Option<Target> {
    let text = normalize(text);
    let mut target = text.as_str();
    for _ in 0..4 {
        if let Some(rest) = [
            "anything about ",
            "anything regarding ",
            "the ",
            "my ",
            "mes ",
            "mon ",
            "ma ",
            "mis ",
            "mi ",
            "les ",
            "la ",
            "le ",
        ]
        .iter()
        .find_map(|p| target.strip_prefix(p))
        {
            target = rest;
        }
    }
    target = target.strip_suffix(" من").unwrap_or(target);
    for (category, aliases) in CATEGORIES {
        if aliases.iter().any(|a| normalize(a) == target) {
            return Some(Target {
                fields: FIELDS
                    .iter()
                    .enumerate()
                    .filter(|(_, f)| f.category == *category)
                    .fold(0, |mask, (i, _)| mask | (1 << i)),
                allow_empty: false,
            });
        }
    }
    FIELDS
        .iter()
        .position(|f| f.labels.iter().any(|a| normalize(a) == target))
        .map(|i| Target {
            fields: 1 << i,
            allow_empty: false,
        })
}

pub(super) fn instruction(text: &str) -> Option<(bool, Selector)> {
    let normalized = normalize(text);
    let mut text = normalized.trim_end_matches(['.', '!', '۔']).trim();
    for _ in 0..2 {
        if let Some(rest) = [
            "please ",
            "could you ",
            "can you ",
            "s'il te plaît, ",
            "por favor, ",
            "لطفا ",
            "لطفاً ",
            "من فضلك ",
        ]
        .iter()
        .find_map(|p| {
            text.strip_prefix(format!("{} ", normalize(p)).as_str())
                .map(str::trim_start)
        }) {
            text = rest;
        }
    }
    for prefix in [
        "by that i mean ",
        "je veux dire ",
        "me refiero a ",
        "منظورم ",
        "أعني ",
    ] {
        if let Some(value) = text
            .strip_prefix(format!("{} ", normalize(prefix)).as_str())
            .and_then(|s| quoted(s.trim()))
        {
            return Some((
                true,
                Selector::Clarify {
                    target: None,
                    value: value.into(),
                },
            ));
        }
    }
    if let Some((target, value)) = text
        .strip_prefix("by ")
        .and_then(|s| s.split_once(" i mean "))
        && let Some(value) = quoted(value)
    {
        select(true, target)?;
        return Some((
            true,
            Selector::Clarify {
                target: Some(target.into()),
                value: value.into(),
            },
        ));
    }
    // Full-message anchors are intentional: quotations, questions and embedded
    // instructions do not become commands, and another destination cannot match.
    for (remove, prefix, suffix) in [
        (true, "keep ", " out of the email"),
        (true, "leave out ", " from the email"),
        (true, "don't mention ", " in the email"),
        (true, "do not mention ", " in the email"),
        (true, "ne mentionne pas ", " dans le courriel"),
        (true, "n'inclus pas ", " dans le courriel"),
        (true, "retire ", " du courriel"),
        (false, "inclus ", " dans le courriel"),
        (true, "no incluyas ", " en el correo"),
        (true, "no menciones ", " en el correo"),
        (true, "omite ", " del correo"),
        (false, "incluye ", " en el correo"),
        (true, "", " را از ایمیل حذف کن"),
        (true, "", " را در ایمیل نیاور"),
        (false, "", " را در ایمیل بگنجان"),
        (true, "لا تذكر ", " في البريد الإلكتروني"),
        (true, "لا تدرج ", " في البريد الإلكتروني"),
        (true, "احذف ", " من البريد الإلكتروني"),
        (false, "أدرج ", " في البريد الإلكتروني"),
    ] {
        // normalize trims whitespace; restore the grammar's separators explicitly.
        let prefix = if prefix.is_empty() {
            String::new()
        } else {
            format!("{} ", normalize(prefix))
        };
        let suffix = format!(" {}", normalize(suffix));
        if let Some(target) = text
            .strip_prefix(&prefix)
            .and_then(|s| s.strip_suffix(&suffix))
        {
            return select(remove, target.trim());
        }
    }
    None
}

/// Resolve fields stated directly about the user (or without a named subject).
/// A field about a named third party does not become the user's private detail.
pub(super) fn latest(messages: &[String], target: Target) -> Option<Vec<(String, String)>> {
    let mut found = Vec::new();
    for (index, field) in FIELDS.iter().enumerate() {
        let bit = 1 << index;
        if target.fields & bit == 0 {
            continue;
        }
        for text in messages
            .iter()
            .rev()
            .filter(|text| super::instruction(text).is_none())
        {
            let current = values(
                text,
                Target {
                    fields: bit,
                    allow_empty: false,
                },
            )?;
            if !current.is_empty() {
                found.extend(
                    current
                        .into_iter()
                        .map(|value| (field.labels[0].to_string(), value)),
                );
                break;
            }
        }
    }
    Some(found)
}

/// Extract all matching values for enforcement, including superseded details.
pub(super) fn values(text: &str, target: Target) -> Option<Vec<String>> {
    let text = text.lines().map(normalize).collect::<Vec<_>>().join("\n");
    let mut values = Vec::new();
    for (index, field) in FIELDS.iter().enumerate() {
        if target.fields & (1 << index) == 0 {
            continue;
        }
        for label in field.labels {
            let label = normalize(label);
            for (at, _) in text.match_indices(&label) {
                let before = &text[..at];
                let head = before
                    .rsplit([';', '\n', ':'])
                    .next()
                    .unwrap_or(before)
                    .rsplit(". ")
                    .next()
                    .unwrap_or(before)
                    .rsplit(", ")
                    .next()
                    .unwrap_or(before)
                    .trim();
                if !["", "my", "the", "and my", "mon", "ma", "mes", "mi", "mis"].contains(&head) {
                    continue;
                }
                let tail = &text[at + label.len()..];
                if tail.starts_with(char::is_alphanumeric) {
                    continue;
                }
                let tail = tail.trim_start();
                let own = tail.strip_prefix("من ");
                let tail = own.unwrap_or(tail);
                let value = tail
                    .strip_prefix([':', '='])
                    .map(str::trim_start)
                    .or_else(|| {
                        ["is ", "was ", "est ", "était ", "es ", "era ", "هو ", "هي "]
                            .iter()
                            .find_map(|p| tail.strip_prefix(p))
                    })
                    .or_else(|| {
                        (tail.starts_with(char::is_numeric)
                            || tail.starts_with(['"', '\'', '“', '‘', '`', '$', '€', '£']))
                        .then_some(tail)
                    })
                    .or_else(|| {
                        own.and_then(|tail| tail.split([';', '\n']).next())
                            .and_then(|tail| tail.split(". ").next())
                            .and_then(|tail| tail.trim_end_matches('.').strip_suffix(" است"))
                    });
                let Some(value) = value else {
                    continue;
                };
                let literal = [('"', '"'), ('\'', '\''), ('“', '”'), ('‘', '’'), ('`', '`')]
                    .iter()
                    .find_map(|(open, close)| {
                        let rest = value.strip_prefix(*open)?;
                        rest.find(*close).map(|end| &rest[..end])
                    });
                if value.starts_with(['"', '\'', '“', '‘', '`']) && literal.is_none() {
                    return None;
                }
                let value = literal
                    .unwrap_or_else(|| {
                        value
                            .split([';', '\n', ','])
                            .next()
                            .unwrap_or("")
                            .split(". ")
                            .next()
                            .unwrap_or("")
                            .trim_end_matches('.')
                    })
                    .trim();
                let value = value.strip_suffix(" است").unwrap_or(value).trim();
                if value.is_empty() {
                    return None;
                }
                // Exact absence statements supply no private value. Never accept
                // a prefix such as "omitted SECRET" as an absence of information.
                if [
                    "omitted",
                    "excluded",
                    "redacted",
                    "not included",
                    "unknown",
                    "non inclus",
                    "omis",
                    "omitido",
                    "excluido",
                    "حذف شده",
                    "نامشخص",
                    "محذوف",
                    "غير معروف",
                ]
                .iter()
                .any(|absent| normalize(absent) == value)
                {
                    continue;
                }
                if !values.iter().any(|v| v == value) {
                    values.push(value.to_string());
                }
                // Salary/income can be rendered without its unit. Do not split
                // account identifiers into smaller numbers or match digit substrings.
                if index == 0 {
                    for amount in value.split(|c: char| !c.is_ascii_digit() && c != '.') {
                        if !amount.is_empty()
                            && amount.chars().any(|c| c.is_ascii_digit())
                            && super::contains(value, amount)
                            && !values.iter().any(|v| v == amount)
                        {
                            values.push(amount.into());
                        }
                    }
                }
                if values.len() > super::MAX_VALUES {
                    return None;
                }
            }
        }
    }
    Some(values)
}

pub(super) fn normalize(text: &str) -> String {
    let folded: String = text
        .chars()
        .map(|c| match c {
            '۰'..='۹' => char::from_u32('0' as u32 + c as u32 - '۰' as u32).unwrap_or(c),
            '٠'..='٩' => char::from_u32('0' as u32 + c as u32 - '٠' as u32).unwrap_or(c),
            'ي' => 'ی',
            'ك' => 'ک',
            '\u{200c}' => ' ',
            _ => c,
        })
        .collect();
    let folded = folded
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let chars: Vec<char> = folded.chars().collect();
    chars
        .iter()
        .enumerate()
        .filter_map(|(i, &c)| {
            // Only unambiguous three-digit groups: keep decimal commas and word gaps.
            let group = matches!(c, ',' | '٬' | ' ')
                && i > 0
                && chars[i - 1].is_ascii_digit()
                && chars
                    .get(i + 1..i + 4)
                    .is_some_and(|s| s.iter().all(char::is_ascii_digit))
                && chars.get(i + 4).is_none_or(|c| !c.is_ascii_digit());
            (!group).then_some(c)
        })
        .collect()
}
