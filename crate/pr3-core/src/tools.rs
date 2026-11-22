//! Tool registry: which tools exist, and strict validation of the model's proposed calls.
//! Everything here treats model output as untrusted input (docs Sec. 3, Sec. 7.1).

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use serde_json::{Map, Value};

use crate::error::CoreError;
use crate::text::{next_escape, push_i64, push_json_str, push_u64};
use crate::types::{ProposedToolCall, ToolCall, ToolSpec};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    String,
    Number,
    Integer,
    Boolean,
    Object,
    Array,
}

impl Kind {
    fn parse(name: &str) -> Option<Self> {
        match name {
            "string" => Some(Self::String),
            "number" => Some(Self::Number),
            "integer" => Some(Self::Integer),
            "boolean" => Some(Self::Boolean),
            "object" => Some(Self::Object),
            "array" => Some(Self::Array),
            _ => None,
        }
    }

    fn accepts(self, value: &Value) -> bool {
        match self {
            Self::String => value.is_string(),
            Self::Number => value.is_number(),
            Self::Integer => value.is_i64() || value.is_u64(),
            Self::Boolean => value.is_boolean(),
            Self::Object => value.is_object(),
            Self::Array => value.is_array(),
        }
    }

    fn accepts_arg(self, value: Arg) -> bool {
        match self {
            Self::String => matches!(value, Arg::Str(_)),
            Self::Number | Self::Integer => matches!(value, Arg::U64(_) | Arg::I64(_)),
            Self::Boolean => matches!(value, Arg::Bool(_)),
            Self::Object | Self::Array => false,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::String => "a string",
            Self::Number => "a number",
            Self::Integer => "an integer",
            Self::Boolean => "true or false",
            Self::Object => "an object",
            Self::Array => "a list",
        }
    }
}

/// One argument: its type and, optionally, the values it may take.
#[derive(Debug)]
struct Property {
    kind: Kind,
    allowed: Option<Vec<Value>>,
}

impl Property {
    /// Only what the checks below enforce is accepted, so a schema can never promise a
    /// constraint (a pattern, a range, nested fields) that goes unchecked.
    fn parse(schema: &Value) -> Result<Self, String> {
        let Value::Object(fields) = schema else {
            return Err("must be an object schema".to_string());
        };
        if let Some(other) = fields
            .keys()
            .find(|k| !matches!(k.as_str(), "type" | "description" | "enum"))
        {
            return Err(format!("'{other}' is not supported"));
        }
        let kind = fields
            .get("type")
            .and_then(Value::as_str)
            .and_then(Kind::parse)
            .ok_or("needs one type: string, number, integer, boolean, object or array")?;
        let allowed = match fields.get("enum") {
            None => None,
            Some(Value::Array(values)) if !values.is_empty() => {
                if let Some(bad) = values.iter().find(|v| !kind.accepts(v)) {
                    return Err(format!("enum value {bad} is not {}", kind.name()));
                }
                Some(values.clone())
            }
            Some(_) => return Err("enum must be a non-empty list".to_string()),
        };
        Ok(Self { kind, allowed })
    }
}

#[derive(Debug)]
struct Schema {
    /// By name, in the order `serde_json` writes a map's keys in.
    properties: Vec<(String, Property)>,
    /// The name of each, as JSON, with its colon: how the canonical text writes it.
    keys: Vec<String>,
    required: Vec<String>,
    /// `required`, as places in `properties`.
    required_at: Vec<usize>,
    allow_additional: bool,
}

impl Schema {
    fn parse(spec: &ToolSpec) -> Result<Self, CoreError> {
        let invalid = |why: &str| CoreError::InvalidToolSpec(format!("{}: {why}", spec.name));
        let value: Value = serde_json::from_str(&spec.parameters)
            .map_err(|_| invalid("parameters is not JSON"))?;
        if value.get("type").and_then(Value::as_str) != Some("object") {
            return Err(invalid("parameters must be an object schema"));
        }
        if let Some(other) = value.as_object().and_then(|top| {
            top.keys().find(|k| {
                !matches!(
                    k.as_str(),
                    "type" | "properties" | "required" | "additionalProperties" | "description"
                )
            })
        }) {
            return Err(invalid(&format!("'{other}' is not supported")));
        }
        // A `serde_json` map iterates by name, so the list comes out sorted.
        let properties: Vec<(String, Property)> = match value.get("properties") {
            None => Vec::new(),
            Some(Value::Object(props)) => props
                .iter()
                .map(|(k, v)| {
                    Property::parse(v)
                        .map(|p| (k.clone(), p))
                        .map_err(|why| invalid(&format!("property '{k}': {why}")))
                })
                .collect::<Result<_, _>>()?,
            Some(_) => return Err(invalid("properties must be an object")),
        };
        let required: Vec<String> = match value.get("required") {
            None => Vec::new(),
            Some(Value::Array(items)) => items
                .iter()
                .map(|v| v.as_str().map(ToString::to_string))
                .collect::<Option<_>>()
                .ok_or_else(|| invalid("required must list property names"))?,
            Some(_) => return Err(invalid("required must be a list")),
        };
        let find = |name: &str| properties.binary_search_by(|(k, _)| k.as_str().cmp(name));
        let required_at = required
            .iter()
            .map(|r| find(r))
            .collect::<Result<Vec<usize>, usize>>();
        let Ok(required_at) = required_at else {
            let missing = required.iter().find(|r| find(r).is_err());
            let missing = missing.map_or("", String::as_str);
            return Err(invalid(&format!("required '{missing}' is not a property")));
        };
        let keys = properties
            .iter()
            .map(|(name, _)| {
                let mut key = String::new();
                push_json_str(&mut key, name);
                key.push(':');
                key
            })
            .collect();
        let allow_additional = value
            .get("additionalProperties")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        Ok(Self {
            properties,
            keys,
            required,
            required_at,
            allow_additional,
        })
    }

    fn property(&self, name: &str) -> Option<usize> {
        self.properties
            .binary_search_by(|(k, _)| k.as_str().cmp(name))
            .ok()
    }

    fn check(&self, args: &Map<String, Value>) -> Result<(), String> {
        if let Some(missing) = self.required.iter().find(|r| !args.contains_key(*r)) {
            return Err(format!("missing required argument '{missing}'"));
        }
        for (key, value) in args {
            let property = self.property(key).and_then(|i| self.properties.get(i));
            match property.map(|(_, p)| p) {
                Some(p) if !p.kind.accepts(value) => {
                    return Err(format!("argument '{key}' must be {}", p.kind.name()));
                }
                Some(Property {
                    allowed: Some(allowed),
                    ..
                }) if !allowed.contains(value) => {
                    let options: Vec<String> = allowed.iter().map(Value::to_string).collect();
                    return Err(format!(
                        "argument '{key}' must be one of {}",
                        options.join(", ")
                    ));
                }
                None if !self.allow_additional => {
                    return Err(format!("unexpected argument '{key}'"));
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// [`Self::check`] over arguments parsed in place: the same verdict, in the same
    /// words. That check fails on the first argument, by name, that is wrong; here an
    /// argument for a property sits in its slot, which is in name order already.
    fn check_parsed(&self, args: &ArgBuffer, arguments: &str) -> Result<(), String> {
        let given = |i: usize| args.values.get(i).copied().flatten();
        if let Some(missing) = self
            .required
            .iter()
            .zip(&self.required_at)
            .find_map(|(name, &i)| given(i).is_none().then_some(name))
        {
            return Err(format!("missing required argument '{missing}'"));
        }
        let wrong = self
            .properties
            .iter()
            .enumerate()
            .find_map(|(i, (name, p))| {
                let value = given(i)?;
                if !p.kind.accepts_arg(value) {
                    return Some((name, format!("argument '{name}' must be {}", p.kind.name())));
                }
                let allowed = p.allowed.as_ref()?;
                if allowed.iter().any(|v| args.equals(value, v, arguments)) {
                    return None;
                }
                let options: Vec<String> = allowed.iter().map(Value::to_string).collect();
                let why = format!("argument '{name}' must be one of {}", options.join(", "));
                Some((name, why))
            });
        let unexpected = args.unexpected.map(|name| args.text(name, arguments));
        match (wrong, unexpected) {
            (Some((name, _)), Some(u)) if u < name.as_str() => {
                Err(format!("unexpected argument '{u}'"))
            }
            (Some((_, why)), _) => Err(why),
            (None, Some(u)) => Err(format!("unexpected argument '{u}'")),
            (None, None) => Ok(()),
        }
    }
}

/// Where a parsed string is: as it stands in the arguments, between its quotes, when it
/// has no escapes; otherwise unescaped into [`ArgBuffer::text`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Text {
    Raw(usize, usize),
    Copied(usize, usize),
}

/// One argument value as parsed in place. Anything else (a float, an object, a list)
/// takes the `serde_json::Value` path, so its canonical form is exactly `serde_json`'s.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Arg {
    Str(Text),
    U64(u64),
    I64(i64),
    Bool(bool),
    Null,
}

/// A call's arguments parsed in place, in one pass: each value lands in the slot of its
/// property, so they are in name order with no sort, and a string without escapes stays
/// where it is in the arguments. Kept from call to call, so checking a call allocates
/// nothing.
#[derive(Debug, Default)]
pub(crate) struct ArgBuffer {
    /// Names and strings that had escapes, unescaped.
    text: String,
    /// Per property of the schema, the last value given for it, as in a JSON map.
    values: Vec<Option<Arg>>,
    /// Of the names that are not properties, the first by name.
    unexpected: Option<Text>,
}

impl ArgBuffer {
    fn text<'a>(&'a self, text: Text, arguments: &'a str) -> &'a str {
        let (from, start, end) = match text {
            Text::Raw(start, end) => (arguments, start, end),
            Text::Copied(start, end) => (self.text.as_str(), start, end),
        };
        from.get(start..end).unwrap_or("")
    }

    /// Whether a parsed value equals a schema's `enum` value, as `Value` equality says.
    fn equals(&self, value: Arg, allowed: &Value, arguments: &str) -> bool {
        match (value, allowed) {
            (Arg::Str(text), Value::String(s)) => self.text(text, arguments) == s,
            (Arg::U64(n), Value::Number(m)) => m.as_u64() == Some(n),
            (Arg::I64(n), Value::Number(m)) => match u64::try_from(n) {
                Ok(n) => m.as_u64() == Some(n),
                Err(_) => m.as_i64() == Some(n),
            },
            (Arg::Bool(a), Value::Bool(b)) => a == *b,
            (Arg::Null, Value::Null) => true,
            _ => false,
        }
    }

    /// Parses `arguments` against `schema`. `None` when they are anything but an object
    /// of plain values this parser reads exactly as `serde_json` does, named as the
    /// schema allows; the `serde_json::Value` path then decides, so every verdict, valid
    /// or not, is that path's.
    fn parse(&mut self, arguments: &str, schema: &Schema) -> Option<()> {
        self.text.clear();
        self.values.clear();
        self.values.resize(schema.properties.len(), None);
        self.unexpected = None;
        let mut p = Parser {
            text: arguments,
            bytes: arguments.as_bytes(),
            at: 0,
        };
        p.space();
        p.eat(b'{')?;
        p.space();
        if p.peek() == Some(b'}') {
            p.at += 1;
        } else {
            loop {
                let name = p.string(&mut self.text)?;
                p.space();
                p.eat(b':')?;
                p.space();
                let value = p.value(&mut self.text)?;
                let key = self.text(name, arguments);
                match schema.property(key) {
                    Some(i) => *self.values.get_mut(i)? = Some(value),
                    None if schema.allow_additional => return None,
                    None => {
                        let first = match self.unexpected {
                            Some(u) => key < self.text(u, arguments),
                            None => true,
                        };
                        if first {
                            self.unexpected = Some(name);
                        }
                    }
                }
                p.space();
                match p.next()? {
                    b',' => p.space(),
                    b'}' => break,
                    _ => return None,
                }
            }
        }
        p.space();
        (p.at == p.bytes.len()).then_some(())
    }

    /// The canonical object: keys sorted, no spaces, strings escaped as `serde_json` does.
    fn write_canonical(&self, schema: &Schema, arguments: &str, out: &mut String) {
        out.push('{');
        let mut first = true;
        for (key, value) in schema.keys.iter().zip(&self.values) {
            let Some(value) = *value else { continue };
            if !first {
                out.push(',');
            }
            first = false;
            out.push_str(key);
            match value {
                // No escapes and no control characters: already as `serde_json` writes it.
                Arg::Str(Text::Raw(start, end)) => {
                    out.push('"');
                    out.push_str(arguments.get(start..end).unwrap_or(""));
                    out.push('"');
                }
                Arg::Str(text) => push_json_str(out, self.text(text, arguments)),
                Arg::U64(n) => push_u64(out, n),
                Arg::I64(n) => push_i64(out, n),
                Arg::Bool(b) => out.push_str(if b { "true" } else { "false" }),
                Arg::Null => out.push_str("null"),
            }
        }
        out.push('}');
    }
}

/// A cursor over the arguments. Each method reads one JSON piece exactly as `serde_json`
/// reads it, or gives `None`: for malformed text and for what it leaves to `serde_json`.
struct Parser<'a> {
    text: &'a str,
    bytes: &'a [u8],
    at: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
    }

    fn next(&mut self) -> Option<u8> {
        let b = self.peek()?;
        self.at += 1;
        Some(b)
    }

    fn eat(&mut self, b: u8) -> Option<()> {
        (self.next()? == b).then_some(())
    }

    fn space(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.at += 1;
        }
    }

    fn word(&mut self, word: &[u8], value: Arg) -> Option<Arg> {
        let end = self.at + word.len();
        (self.bytes.get(self.at..end)? == word).then(|| {
            self.at = end;
            value
        })
    }

    fn value(&mut self, copied: &mut String) -> Option<Arg> {
        match self.peek()? {
            b'"' => self.string(copied).map(Arg::Str),
            b'-' | b'0'..=b'9' => self.number(),
            b't' => self.word(b"true", Arg::Bool(true)),
            b'f' => self.word(b"false", Arg::Bool(false)),
            b'n' => self.word(b"null", Arg::Null),
            _ => None,
        }
    }

    /// An integer, as `serde_json` keeps it: unsigned unless negative, and a float (left
    /// to `serde_json`) when it has a fraction or exponent, overflows, or is `-0`.
    fn number(&mut self) -> Option<Arg> {
        let negative = self.peek() == Some(b'-');
        if negative {
            self.at += 1;
        }
        let mut n: u64 = 0;
        match self.next()? {
            b'0' => {}
            d @ b'1'..=b'9' => {
                n = u64::from(d - b'0');
                while let Some(d) = self.peek().filter(u8::is_ascii_digit) {
                    n = n.checked_mul(10)?.checked_add(u64::from(d - b'0'))?;
                    self.at += 1;
                }
            }
            _ => return None,
        }
        // A fraction, an exponent, or a digit after a leading zero.
        if matches!(self.peek(), Some(b'.' | b'e' | b'E' | b'0'..=b'9')) {
            return None;
        }
        if !negative {
            return Some(Arg::U64(n));
        }
        if n == 0 {
            return None;
        }
        0i64.checked_sub_unsigned(n).map(Arg::I64)
    }

    /// A string. Without escapes it stays where it is; with them it is unescaped into
    /// `copied`. A control character, a bad escape or a lone surrogate gives `None`.
    fn string(&mut self, copied: &mut String) -> Option<Text> {
        self.eat(b'"')?;
        let start = self.at;
        let mut at = next_escape(self.bytes, start)?;
        match self.bytes.get(at)? {
            b'"' => {
                self.at = at + 1;
                return Some(Text::Raw(start, at));
            }
            b'\\' => {}
            _ => return None,
        }
        let from = copied.len();
        let mut run = start;
        loop {
            // `at` is on a backslash; every place here is after an ASCII byte, so each
            // run is whole characters.
            copied.push_str(self.text.get(run..at)?);
            let escape = *self.bytes.get(at + 1)?;
            at += 2;
            copied.push(match escape {
                b'"' => '"',
                b'\\' => '\\',
                b'/' => '/',
                b'b' => '\u{8}',
                b'f' => '\u{c}',
                b'n' => '\n',
                b'r' => '\r',
                b't' => '\t',
                b'u' => {
                    let unit = self.hex4(at)?;
                    at += 4;
                    match unit {
                        0xD800..=0xDBFF => {
                            if self.bytes.get(at..at + 2)? != b"\\u" {
                                return None;
                            }
                            let low = self.hex4(at + 2)?;
                            if !(0xDC00..=0xDFFF).contains(&low) {
                                return None;
                            }
                            at += 6;
                            char::from_u32(0x10000 + ((unit - 0xD800) << 10) + (low - 0xDC00))?
                        }
                        0xDC00..=0xDFFF => return None,
                        _ => char::from_u32(unit)?,
                    }
                }
                _ => return None,
            });
            run = at;
            at = next_escape(self.bytes, at)?;
            match self.bytes.get(at)? {
                b'"' => {
                    copied.push_str(self.text.get(run..at)?);
                    self.at = at + 1;
                    return Some(Text::Copied(from, copied.len()));
                }
                b'\\' => {}
                _ => return None,
            }
        }
    }

    fn hex4(&self, at: usize) -> Option<u32> {
        let digits = self.bytes.get(at..at + 4)?;
        digits.iter().try_fold(0u32, |n, &d| {
            let d = char::from(d).to_digit(16)?;
            Some(n << 4 | d)
        })
    }
}

#[derive(Debug)]
struct Entry {
    schema: Schema,
    requires_confirmation: bool,
    untrusted_output: bool,
}

/// How the core treats a tool's calls, as the registry says once per call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ToolFlags {
    pub(crate) requires_confirmation: bool,
    pub(crate) untrusted_output: bool,
}

#[derive(Debug, Default)]
pub(crate) struct ToolRegistry {
    tools: BTreeMap<String, Entry>,
    /// The specs sorted by name, so a request can lend them as one slice.
    specs: Vec<ToolSpec>,
    /// Whether any tool's results carry untrusted text; if none does, no name is looked
    /// up to find out.
    any_untrusted: bool,
}

fn valid_name(name: &str) -> bool {
    (1..=64).contains(&name.len())
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

impl ToolRegistry {
    pub(crate) fn register(&mut self, spec: ToolSpec) -> Result<(), CoreError> {
        if !valid_name(&spec.name) {
            return Err(CoreError::InvalidToolSpec(format!(
                "{}: names use lowercase letters, digits and '_'",
                spec.name
            )));
        }
        if self.tools.contains_key(&spec.name) {
            return Err(CoreError::InvalidToolSpec(format!(
                "{}: registered twice",
                spec.name
            )));
        }
        let schema = Schema::parse(&spec)?;
        self.any_untrusted |= spec.untrusted_output;
        let at = self.specs.partition_point(|s| s.name < spec.name);
        self.tools.insert(
            spec.name.clone(),
            Entry {
                schema,
                requires_confirmation: spec.requires_confirmation,
                untrusted_output: spec.untrusted_output,
            },
        );
        self.specs.insert(at, spec);
        Ok(())
    }

    pub(crate) fn specs(&self) -> &[ToolSpec] {
        &self.specs
    }

    /// Whether results of `name` carry text from outside the user's control.
    pub(crate) fn untrusted_output(&self, name: &str) -> bool {
        self.any_untrusted && self.tools.get(name).is_some_and(|e| e.untrusted_output)
    }

    /// Turns a proposed call into a validated one in `call` (whose `id` the caller has
    /// set) and says how its tool is treated, or explains, in words the model can act
    /// on, why it was refused. `args` is scratch the caller keeps from call to call.
    pub(crate) fn validate_into(
        &self,
        proposed: &ProposedToolCall<'_>,
        call: &mut ToolCall,
        max_args_bytes: usize,
        args: &mut ArgBuffer,
    ) -> Result<ToolFlags, String> {
        let entry = self
            .tools
            .get(proposed.name)
            .ok_or_else(|| format!("unknown tool '{}'", proposed.name))?;
        if proposed.arguments.len() > max_args_bytes {
            return Err(format!("arguments are larger than {max_args_bytes} bytes"));
        }
        call.name.clear();
        call.name.push_str(proposed.name);
        call.arguments.clear();
        let arguments = proposed.arguments;
        if args.parse(arguments, &entry.schema).is_some() {
            entry.schema.check_parsed(args, arguments)?;
            args.write_canonical(&entry.schema, arguments, &mut call.arguments);
        } else {
            check_values(entry, arguments, &mut call.arguments)?;
        }
        Ok(ToolFlags {
            requires_confirmation: entry.requires_confirmation,
            untrusted_output: entry.untrusted_output,
        })
    }

    #[cfg(test)]
    pub(crate) fn validate(
        &self,
        proposed: &ProposedToolCall<'_>,
        id: String,
        max_args_bytes: usize,
    ) -> Result<ToolCall, String> {
        let mut call = ToolCall {
            id,
            ..ToolCall::default()
        };
        self.validate_into(
            proposed,
            &mut call,
            max_args_bytes,
            &mut ArgBuffer::default(),
        )
        .map(|_| call)
    }
}

/// The `serde_json::Value` path: for arguments with floats, objects or arrays, and the
/// reference the in-place path is tested against.
fn check_values(entry: &Entry, arguments: &str, out: &mut String) -> Result<(), String> {
    let args: Map<String, Value> = match serde_json::from_str(arguments) {
        Ok(Value::Object(map)) => map,
        Ok(_) | Err(_) => return Err("arguments must be a JSON object".to_string()),
    };
    entry.schema.check(&args)?;
    // Sorted keys (no "preserve_order"): equal arguments serialize identically.
    let text = serde_json::to_string(&Value::Object(args))
        .map_err(|_| "arguments could not be serialized".to_string())?;
    out.push_str(&text);
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    fn calculator() -> ToolSpec {
        ToolSpec {
            name: "calculate".to_string(),
            description: "Evaluate arithmetic".to_string(),
            parameters: r#"{"type":"object","properties":{"expression":{"type":"string"}},"required":["expression"]}"#.to_string(),
            requires_confirmation: false,
            untrusted_output: false,
            required_capabilities: Vec::new(),
        }
    }

    fn registry() -> ToolRegistry {
        let mut r = ToolRegistry::default();
        r.register(calculator()).unwrap();
        r
    }

    fn call<'a>(name: &'a str, arguments: &'a str) -> ProposedToolCall<'a> {
        ProposedToolCall { name, arguments }
    }

    #[test]
    fn valid_call_is_canonicalized() {
        let ok = registry()
            .validate(
                &call("calculate", r#"{ "expression" : "2+2" }"#),
                "c1".into(),
                1024,
            )
            .unwrap();
        assert_eq!(ok.arguments, r#"{"expression":"2+2"}"#);
    }

    #[test]
    fn untrusted_calls_are_refused_with_a_reason() {
        let r = registry();
        let refuse =
            |name: &str, args: &str| r.validate(&call(name, args), "c".into(), 64).unwrap_err();
        assert_eq!(refuse("rm_rf", "{}"), "unknown tool 'rm_rf'");
        assert_eq!(
            refuse("calculate", "[1]"),
            "arguments must be a JSON object"
        );
        assert_eq!(
            refuse("calculate", "{}"),
            "missing required argument 'expression'"
        );
        assert_eq!(
            refuse("calculate", r#"{"expression":2}"#),
            "argument 'expression' must be a string"
        );
        assert_eq!(
            refuse("calculate", r#"{"expression":"1","shell":"x"}"#),
            "unexpected argument 'shell'"
        );
        assert!(refuse("calculate", &"x".repeat(100)).contains("larger than"));
    }

    #[test]
    fn confirmation_tools_are_validated_like_any_other_and_marked() {
        let mut r = registry();
        r.register(ToolSpec {
            name: "send_mail".to_string(),
            requires_confirmation: true,
            untrusted_output: false,
            ..calculator()
        })
        .unwrap();
        let flags = |name: &str| {
            let proposed = call(name, r#"{"expression":"x"}"#);
            let mut out = ToolCall::default();
            r.validate_into(&proposed, &mut out, 1024, &mut ArgBuffer::default())
                .unwrap()
        };
        assert!(flags("send_mail").requires_confirmation);
        assert!(!flags("calculate").requires_confirmation);
        assert!(
            r.validate(
                &call("send_mail", r#"{"expression":"x"}"#),
                "c".into(),
                1024
            )
            .is_ok()
        );
        assert!(
            r.validate(&call("send_mail", "{}"), "c".into(), 1024)
                .is_err()
        );
    }

    #[test]
    fn bad_specs_are_rejected() {
        let mut r = registry();
        assert!(r.register(calculator()).is_err(), "duplicate");
        let bad = |parameters: &str| ToolSpec {
            name: "t".to_string(),
            parameters: parameters.to_string(),
            ..calculator()
        };
        assert!(ToolRegistry::default().register(bad("not json")).is_err());
        assert!(
            ToolRegistry::default()
                .register(bad(r#"{"type":"string"}"#))
                .is_err()
        );
        assert!(
            ToolRegistry::default()
                .register(bad(r#"{"type":"object","required":["x"]}"#))
                .is_err()
        );
        let mut upper = calculator();
        upper.name = "Calc".to_string();
        assert!(ToolRegistry::default().register(upper).is_err());
    }

    #[test]
    fn unchecked_constraints_are_refused_and_enums_are_checked() {
        let spec = |parameters: &str| ToolSpec {
            name: "t".to_string(),
            parameters: parameters.to_string(),
            ..calculator()
        };
        for unsupported in [
            r#"{"type":"object","properties":{"q":{"type":"string","pattern":"^a"}}}"#,
            r#"{"type":"object","properties":{"n":{"type":["integer","null"]}}}"#,
            r#"{"type":"object","properties":{"n":{}}}"#,
            r#"{"type":"object","properties":{"o":{"type":"object","properties":{}}}}"#,
            r#"{"type":"object","oneOf":[]}"#,
            r#"{"type":"object","properties":{"u":{"type":"string","enum":[1]}}}"#,
        ] {
            let refused = ToolRegistry::default().register(spec(unsupported));
            assert!(refused.is_err(), "{unsupported}");
        }

        let mut r = ToolRegistry::default();
        r.register(spec(
            r#"{"type":"object","properties":{"unit":{"type":"string","enum":["c","f"]}}}"#,
        ))
        .unwrap();
        let check = |args: &str| {
            r.validate(
                &ProposedToolCall {
                    name: "t",
                    arguments: args,
                },
                "c".to_string(),
                1024,
            )
        };
        assert!(check(r#"{"unit":"c"}"#).is_ok());
        let refused = check(r#"{"unit":"rm -rf /"}"#).unwrap_err();
        assert!(refused.contains(r#"one of "c", "f""#), "{refused}");
    }

    /// The in-place path gives the same verdict and the same canonical text as the
    /// `serde_json::Value` path, on arguments built from awkward pieces.
    #[test]
    fn parsing_in_place_matches_the_value_path() {
        use proptest::prelude::*;
        let mut r = ToolRegistry::default();
        // The same properties, strict and with other names allowed.
        for (name, parameters) in [
            (
                "calculate",
                r#"{"type":"object","properties":{"s":{"type":"string"},"n":{"type":"number"},"i":{"type":"integer"},"b":{"type":"boolean"},"e":{"type":"string","enum":["c","f"]},"k":{"type":"integer","enum":[1,-2,3]},"o":{"type":"object"},"a":{"type":"array"}},"required":["s"]}"#,
            ),
            (
                "loose",
                r#"{"type":"object","properties":{"s":{"type":"string"},"n":{"type":"number"},"i":{"type":"integer"},"b":{"type":"boolean"},"e":{"type":"string","enum":["c","f"]},"k":{"type":"integer","enum":[1,-2,3]},"o":{"type":"object"},"a":{"type":"array"}},"required":["s"],"additionalProperties":true}"#,
            ),
        ] {
            r.register(ToolSpec {
                name: name.to_string(),
                parameters: parameters.to_string(),
                ..calculator()
            })
            .unwrap();
        }
        // Per key, values of its own type (and a few that are not); `x` and the escaped
        // or odd names are not properties.
        let strings = [
            r#""hi""#,
            r#""a\nb\"c\\""#,
            r#""é\u0001\u001f\u007f""#,
            r#""é😀/""#,
            r#""😀""#,
            r#""""#,
            r#""\ud83d\ude00 and \uD83D\uDE00""#,
            r#""\ud800""#,
            r#""\ud800\u0041""#,
            r#""\udc00x""#,
            r#""\x""#,
            r#""\u12""#,
            r#""\/\b\f\r\t""#,
            "\"raw\u{1}control\"",
            "\"tab\tinside\"",
            r#""unterminated"#,
            r#""long enough to take more than one eight-byte word, then \"q\"""#,
        ];
        let numbers = [
            "0",
            "1",
            "-1",
            "-0",
            "1.5",
            "1e3",
            "-2",
            "3",
            "18446744073709551615",
            "18446744073709551616",
            "-9223372036854775808",
            "-9223372036854775809",
            "2",
            "01",
            "-01",
            "-",
            "1.",
            "1e",
            "1E5",
            "-1e-2",
            "9223372036854775808",
            "-18446744073709551615",
            "-18446744073709551616",
            "184467440737095516150",
        ];
        let others = [
            "true",
            "false",
            "null",
            "tru",
            "nul",
            "true1",
            "nullx",
            r#"{"k":[1,{"z":2}]}"#,
            "{}",
            "[1,2,[3]]",
            "[]",
        ];
        let keys: [(&str, &[&str]); 12] = [
            ("s", &strings),
            ("n", &numbers),
            ("i", &numbers),
            ("b", &others),
            ("e", &[r#""c""#, r#""f""#, r#""C""#, "1"]),
            ("k", &["1", "-2", "3", "2", "-2.0", r#""1""#]),
            ("o", &others),
            ("a", &others),
            ("x", &strings),
            ("\\u0073", &strings),
            ("s\\n", &numbers),
            ("é", &others),
        ];
        let any: Vec<&str> = strings
            .iter()
            .chain(&numbers)
            .chain(&others)
            .copied()
            .collect();
        let entries = ["calculate", "loose"].map(|name| (name, r.tools.get(name).unwrap()));
        let strategy = (
            proptest::collection::vec((0..keys.len(), 0..64usize, 0..8usize, 0..3usize), 0..6),
            0..6usize,
            0..10usize,
        );
        let passed = core::cell::Cell::new(0u32);
        let in_place = core::cell::Cell::new(0u32);
        proptest!(ProptestConfig::with_cases(4000), |((pairs, s, shape) in strategy)| {
            let space = ["", " ", "\n\t "][shape % 3];
            let mut body = Vec::new();
            if shape < 8 {
                body.push(format!("\"s\":{space}{}", strings[s]));
            }
            for &(k, v, typed, w) in &pairs {
                // Mostly a value of the key's own type; now and then any value at all.
                let (name, own) = keys[k];
                let value = if typed > 0 { own[v % own.len()] } else { any[v % any.len()] };
                let w = ["", " ", "\n"][w];
                body.push(format!("{w}\"{name}\"{w}:{space}{value}{w}"));
            }
            let mut text = format!("{{{space}{}{space}}}", body.join(","));
            match shape {
                7 => text.push_str(" x"),
                8 => text = format!("[{text}]"),
                9 => text.truncate(text.len().saturating_sub(1)),
                _ => {}
            }
            for (name, entry) in entries {
                if ArgBuffer::default().parse(&text, &entry.schema).is_some() {
                    in_place.set(in_place.get() + 1);
                }
                let fast = r.validate(&call(name, &text), "c".into(), 4096).map(|c| c.arguments);
                let mut slow = String::new();
                let slow = check_values(entry, &text, &mut slow).map(|()| slow);
                if fast.is_ok() {
                    passed.set(passed.get() + 1);
                }
                prop_assert_eq!(fast, slow, "{} {}", name, text);
            }
        });
        // Enough of them pass the schema that the canonical text is compared too.
        assert!(passed.get() > 600, "only {} passed", passed.get());
        // And enough are read in place that the one-pass parser is what is compared.
        assert!(in_place.get() > 600, "only {} in place", in_place.get());
    }
}
