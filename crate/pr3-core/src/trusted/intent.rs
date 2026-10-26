//! Structured intents and their validator (docs Sec. 4.6).
//!
//! The model proposes an operation with arguments; the runtime wraps the proposal in an
//! [`Envelope`] carrying identity, a nonce and the time — values the model never
//! chooses. The validator checks freshness and replay, accepts only the fields the
//! operation defines, bounds every model-chosen value, and resolves references
//! against the user's own data: the model names a contact, and only the directory
//! supplies an address.

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::types::Timestamp;

/// The one consequential operation of the spike: sending an email.
pub const MAIL_SEND: &str = "mail.send";

/// Recipients one message may name, whatever the policy allows.
pub const MAX_RECIPIENTS: usize = 10;
pub const MAX_SUBJECT_BYTES: usize = 200;
pub const MAX_BODY_BYTES: usize = 8 * 1024;
/// An envelope older than this is stale.
pub const MAX_AGE_MS: u64 = 30_000;
/// An envelope this far ahead of the clock is refused.
pub const MAX_SKEW_MS: u64 = 2_000;
pub const MAX_NONCE_BYTES: usize = 64;
/// Longest piece of model text quoted back in a refusal.
const MAX_QUOTED: usize = 64;

/// What the model proposed: untrusted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Proposal {
    pub operation: String,
    /// The arguments as JSON text.
    pub arguments: String,
}

/// A proposal as the runtime received it. Every field but `proposal` comes from the
/// session's binding and the host's entropy and clock, never from the model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Envelope {
    pub actor: String,
    pub principal: String,
    /// Unique per envelope, from host entropy; letters and digits.
    pub nonce: String,
    pub received_at: Timestamp,
    pub proposal: Proposal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Contact {
    pub name: String,
    pub address: String,
}

/// The user's own data that references resolve against. The host keeps it; the model
/// can only refer to it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Directory {
    pub contacts: Vec<Contact>,
}

/// An email, resolved: real addresses from the directory, bounded text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mail {
    pub to: Vec<Contact>,
    pub subject: String,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Intent {
    SendMail(Mail),
}

/// An intent that passed validation, still carrying its envelope's identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidIntent {
    pub actor: String,
    pub principal: String,
    pub nonce: String,
    pub received_at: Timestamp,
    pub intent: Intent,
}

/// Why a proposal was refused, in words the model can act on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    Stale,
    FromTheFuture,
    BadNonce,
    Replayed,
    UnknownOperation(String),
    Malformed(&'static str),
    UnknownField(String),
    /// The model typed an address instead of naming a contact.
    RawAddress(String),
    UnknownContact {
        name: String,
        known: Vec<String>,
    },
    AmbiguousContact {
        name: String,
        matches: Vec<String>,
    },
    TooManyRecipients {
        max: usize,
    },
    BadText {
        field: &'static str,
        why: &'static str,
    },
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Stale => f.write_str("the request is too old"),
            Self::FromTheFuture => f.write_str("the request is dated in the future"),
            Self::BadNonce => f.write_str("the request has no valid nonce"),
            Self::Replayed => f.write_str("the request was already submitted"),
            Self::UnknownOperation(op) => write!(f, "unknown operation '{op}'"),
            Self::Malformed(why) => f.write_str(why),
            Self::UnknownField(name) => write!(f, "unexpected argument '{name}'"),
            Self::RawAddress(text) => write!(
                f,
                "'{text}' is an address; name a contact instead, PR3 sends only to the user's contacts"
            ),
            Self::UnknownContact { name, known } if known.is_empty() => {
                write!(f, "no contact named '{name}'; the user has no contacts")
            }
            Self::UnknownContact { name, known } => write!(
                f,
                "no contact named '{name}'; contacts: {}",
                known.join(", ")
            ),
            Self::AmbiguousContact { name, matches } => write!(
                f,
                "'{name}' could be {}; use the full name",
                matches.join(" or ")
            ),
            Self::TooManyRecipients { max } => write!(f, "at most {max} recipients"),
            Self::BadText { field, why } => write!(f, "the {field} {why}"),
        }
    }
}

/// Remembers recent nonces, so each envelope is accepted at most once.
#[derive(Debug, Default)]
pub struct ReplayWindow {
    seen: BTreeMap<String, Timestamp>,
}

impl ReplayWindow {
    fn admit(
        &mut self,
        nonce: &str,
        received_at: Timestamp,
        now: Timestamp,
    ) -> Result<(), Refusal> {
        // An envelope older than MAX_AGE_MS fails the freshness check before it gets
        // here, so its nonce need not be kept.
        self.seen
            .retain(|_, at| now.0.saturating_sub(at.0) <= MAX_AGE_MS);
        if self.seen.contains_key(nonce) {
            return Err(Refusal::Replayed);
        }
        self.seen.insert(nonce.to_string(), received_at);
        Ok(())
    }
}

/// Validates an envelope: fresh, never seen, a known operation with exactly its own
/// fields, bounded values and references that resolve in `directory`.
pub fn validate(
    envelope: &Envelope,
    directory: &Directory,
    replay: &mut ReplayWindow,
    now: Timestamp,
) -> Result<ValidIntent, Refusal> {
    if envelope.received_at.0 > now.0.saturating_add(MAX_SKEW_MS) {
        return Err(Refusal::FromTheFuture);
    }
    if now.0.saturating_sub(envelope.received_at.0) > MAX_AGE_MS {
        return Err(Refusal::Stale);
    }
    let nonce = &envelope.nonce;
    if nonce.is_empty()
        || nonce.len() > MAX_NONCE_BYTES
        || !nonce.bytes().all(|b| b.is_ascii_alphanumeric())
    {
        return Err(Refusal::BadNonce);
    }
    // Admitted before the arguments are read: a refused envelope is used up too.
    replay.admit(nonce, envelope.received_at, now)?;
    let intent = match envelope.proposal.operation.as_str() {
        MAIL_SEND => Intent::SendMail(mail(&envelope.proposal.arguments, directory)?),
        other => return Err(Refusal::UnknownOperation(quote(other))),
    };
    Ok(ValidIntent {
        actor: envelope.actor.clone(),
        principal: envelope.principal.clone(),
        nonce: nonce.clone(),
        received_at: envelope.received_at,
        intent,
    })
}

/// `mail.send` takes exactly `to` (contact names, separated by commas), `subject` and
/// `body`. Identity, sender, nonce or time fields are not accepted from the model.
fn mail(arguments: &str, directory: &Directory) -> Result<Mail, Refusal> {
    let Ok(Value::Object(args)) = serde_json::from_str::<Value>(arguments) else {
        return Err(Refusal::Malformed("arguments must be a JSON object"));
    };
    if let Some(other) = args
        .keys()
        .find(|k| !matches!(k.as_str(), "to" | "subject" | "body"))
    {
        return Err(Refusal::UnknownField(quote(other)));
    }
    let Some(Value::String(to)) = args.get("to") else {
        return Err(Refusal::Malformed("'to' must name one or more contacts"));
    };
    let names: Vec<&str> = to
        .split(',')
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .collect();
    if names.is_empty() {
        return Err(Refusal::Malformed("'to' must name one or more contacts"));
    }
    if names.len() > MAX_RECIPIENTS {
        return Err(Refusal::TooManyRecipients {
            max: MAX_RECIPIENTS,
        });
    }
    let mut recipients: Vec<Contact> = Vec::new();
    for name in names {
        let contact = resolve(name, &directory.contacts)?;
        if !recipients.contains(contact) {
            recipients.push(contact.clone());
        }
    }
    Ok(Mail {
        to: recipients,
        subject: text(args.get("subject"), "subject", MAX_SUBJECT_BYTES, false)?,
        body: text(args.get("body"), "body", MAX_BODY_BYTES, true)?,
    })
}

/// A contact by full name, or by first name when exactly one contact has it; case and
/// spacing do not matter.
fn resolve<'a>(reference: &str, contacts: &'a [Contact]) -> Result<&'a Contact, Refusal> {
    if reference.contains('@') {
        return Err(Refusal::RawAddress(quote(reference)));
    }
    let wanted = normalize(reference);
    let by_name: Vec<&Contact> = contacts
        .iter()
        .filter(|c| normalize(&c.name) == wanted)
        .collect();
    let candidates = if by_name.is_empty() {
        contacts
            .iter()
            .filter(|c| normalize(&c.name).split(' ').next() == Some(wanted.as_str()))
            .collect()
    } else {
        by_name
    };
    match candidates.as_slice() {
        [one] => Ok(one),
        [] => Err(Refusal::UnknownContact {
            name: quote(reference),
            known: contacts.iter().map(|c| c.name.clone()).collect(),
        }),
        many => Err(Refusal::AmbiguousContact {
            name: quote(reference),
            matches: many.iter().map(|c| c.name.clone()).collect(),
        }),
    }
}

/// Bounded text the user will read on the confirmation screen. Control characters
/// are refused (a subject may not even break a line, so it cannot add mail headers),
/// and so are the invisible characters that reorder text, so the screen shows the
/// text exactly as it will be sent.
fn text(
    value: Option<&Value>,
    field: &'static str,
    max: usize,
    multiline: bool,
) -> Result<String, Refusal> {
    let bad = |why| Refusal::BadText { field, why };
    let Some(Value::String(text)) = value else {
        return Err(bad("must be text"));
    };
    let text = text.trim();
    if text.is_empty() {
        return Err(bad("is empty"));
    }
    if text.len() > max {
        return Err(bad("is too long"));
    }
    if text
        .chars()
        .any(|c| c.is_control() && !(multiline && (c == '\n' || c == '\t')))
    {
        return Err(bad("contains control characters"));
    }
    if text.chars().any(reorders_text) {
        return Err(bad("contains invisible direction marks"));
    }
    Ok(text.to_string())
}

/// Unicode's bidirectional controls: they make text display in a different order
/// than it is stored.
fn reorders_text(c: char) -> bool {
    matches!(
        c,
        '\u{061C}' | '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}'
    )
}

fn normalize(name: &str) -> String {
    name.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Model text quoted back to it, cut short and kept on one line.
fn quote(text: &str) -> String {
    let line: String = text
        .chars()
        .filter(|c| !c.is_control())
        .take(MAX_QUOTED)
        .collect();
    if text.chars().count() > MAX_QUOTED {
        format!("{line}…")
    } else {
        line
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
pub(crate) mod tests {
    use super::*;
    use alloc::vec;

    pub(crate) fn directory() -> Directory {
        let contact = |name: &str, address: &str| Contact {
            name: name.to_string(),
            address: address.to_string(),
        };
        Directory {
            contacts: vec![
                contact("Alex Rivera", "alex@example.com"),
                contact("Sam Chen", "sam@example.com"),
                contact("Sam Okafor", "okafor@example.org"),
            ],
        }
    }

    fn envelope(arguments: &str, nonce: &str, at: u64) -> Envelope {
        Envelope {
            actor: "agent:pr3".to_string(),
            principal: "user:local".to_string(),
            nonce: nonce.to_string(),
            received_at: Timestamp(at),
            proposal: Proposal {
                operation: MAIL_SEND.to_string(),
                arguments: arguments.to_string(),
            },
        }
    }

    fn check(arguments: &str) -> Result<ValidIntent, Refusal> {
        validate(
            &envelope(arguments, "n1", 1_000),
            &directory(),
            &mut ReplayWindow::default(),
            Timestamp(1_000),
        )
    }

    #[test]
    fn names_resolve_to_the_directorys_addresses() {
        let valid =
            check(r#"{"to":"alex, Sam Chen ,ALEX rivera","subject":"Hi","body":"Hello"}"#).unwrap();
        let Intent::SendMail(mail) = valid.intent;
        let addresses: Vec<&str> = mail.to.iter().map(|c| c.address.as_str()).collect();
        assert_eq!(addresses, ["alex@example.com", "sam@example.com"]);
        assert_eq!(valid.principal, "user:local");
    }

    #[test]
    fn the_model_cannot_supply_addresses_identity_or_extra_fields() {
        let refused = |args: &str| check(args).unwrap_err();
        assert!(matches!(
            refused(r#"{"to":"eve@evil.example","subject":"s","body":"b"}"#),
            Refusal::RawAddress(_)
        ));
        for field in ["from", "principal", "nonce", "received_at", "cc", "bcc"] {
            let args = format!(r#"{{"to":"Alex","subject":"s","body":"b","{field}":"x"}}"#);
            assert_eq!(refused(&args), Refusal::UnknownField(field.to_string()));
        }
        assert!(matches!(
            refused(r#"{"to":"Bob","subject":"s","body":"b"}"#),
            Refusal::UnknownContact { .. }
        ));
        let ambiguous = refused(r#"{"to":"Sam","subject":"s","body":"b"}"#);
        assert_eq!(
            ambiguous.to_string(),
            "'Sam' could be Sam Chen or Sam Okafor; use the full name"
        );
    }

    #[test]
    fn text_is_bounded_single_line_where_needed_and_never_reordered() {
        let refused = |subject: &str, body: &str| {
            let args = serde_json::json!({"to": "Alex", "subject": subject, "body": body});
            check(&args.to_string()).unwrap_err()
        };
        assert!(
            refused("Hi\nBcc: eve@evil.example", "b")
                .to_string()
                .contains("control")
        );
        assert!(
            refused("Invoice \u{202E}fdp.exe", "b")
                .to_string()
                .contains("direction")
        );
        assert!(refused("", "b").to_string().contains("empty"));
        assert!(
            refused("s", &"x".repeat(MAX_BODY_BYTES + 1))
                .to_string()
                .contains("long")
        );
        let ok = serde_json::json!({"to": "Alex", "subject": " Hi ", "body": "Line 1\n\tLine 2"});
        let Intent::SendMail(mail) = check(&ok.to_string()).unwrap().intent;
        assert_eq!(
            (mail.subject.as_str(), mail.body.as_str()),
            ("Hi", "Line 1\n\tLine 2")
        );
    }

    #[test]
    fn envelopes_are_fresh_and_used_once() {
        let args = r#"{"to":"Alex","subject":"s","body":"b"}"#;
        let dir = directory();
        let mut window = ReplayWindow::default();
        let now = Timestamp(100_000);
        let at = |t: u64, nonce: &str| envelope(args, nonce, t);
        assert!(validate(&at(100_000, "a1"), &dir, &mut window, now).is_ok());
        assert_eq!(
            validate(&at(100_000, "a1"), &dir, &mut window, now),
            Err(Refusal::Replayed)
        );
        assert_eq!(
            validate(&at(100_000 - MAX_AGE_MS - 1, "a2"), &dir, &mut window, now),
            Err(Refusal::Stale)
        );
        assert_eq!(
            validate(&at(100_000 + MAX_SKEW_MS + 1, "a3"), &dir, &mut window, now),
            Err(Refusal::FromTheFuture)
        );
        for nonce in ["", "has space", &"x".repeat(MAX_NONCE_BYTES + 1)] {
            assert_eq!(
                validate(&at(100_000, nonce), &dir, &mut window, now),
                Err(Refusal::BadNonce)
            );
        }
        // A refused envelope is used up too.
        let bad = envelope(r#"{"to":"Bob","subject":"s","body":"b"}"#, "a4", 100_000);
        assert!(validate(&bad, &dir, &mut window, now).is_err());
        assert_eq!(
            validate(&bad, &dir, &mut window, now),
            Err(Refusal::Replayed)
        );
    }

    #[test]
    fn unknown_operations_are_refused() {
        let mut e = envelope("{}", "n1", 1_000);
        e.proposal.operation = "payment.send".to_string();
        assert_eq!(
            validate(
                &e,
                &directory(),
                &mut ReplayWindow::default(),
                Timestamp(1_000)
            ),
            Err(Refusal::UnknownOperation("payment.send".to_string()))
        );
    }
}
