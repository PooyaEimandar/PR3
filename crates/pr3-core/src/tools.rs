//! Tool registry: which tools exist, and strict validation of the model's proposed calls.
//! Everything here treats model output as untrusted input (Roadmap §3, §7.1).

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use serde_json::{Map, Value};

use crate::error::CoreError;
use crate::types::{ProposedToolCall, ToolCall, ToolSpec};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    String,
    Number,
    Integer,
    Boolean,
    Object,
    Array,
    Any,
}

impl Kind {
    fn parse(schema: &Value) -> Self {
        match schema.get("type").and_then(Value::as_str) {
            Some("string") => Self::String,
            Some("number") => Self::Number,
            Some("integer") => Self::Integer,
            Some("boolean") => Self::Boolean,
            Some("object") => Self::Object,
            Some("array") => Self::Array,
            _ => Self::Any,
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
            Self::Any => true,
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
            Self::Any => "any value",
        }
    }
}

#[derive(Debug)]
struct Schema {
    properties: BTreeMap<String, Kind>,
    required: Vec<String>,
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
        let properties = match value.get("properties") {
            None => BTreeMap::new(),
            Some(Value::Object(props)) => props
                .iter()
                .map(|(k, v)| (k.clone(), Kind::parse(v)))
                .collect(),
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
        if let Some(missing) = required.iter().find(|r| !properties.contains_key(*r)) {
            return Err(invalid(&format!("required '{missing}' is not a property")));
        }
        let allow_additional = value
            .get("additionalProperties")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        Ok(Self {
            properties,
            required,
            allow_additional,
        })
    }

    fn check(&self, args: &Map<String, Value>) -> Result<(), String> {
        if let Some(missing) = self.required.iter().find(|r| !args.contains_key(*r)) {
            return Err(format!("missing required argument '{missing}'"));
        }
        for (key, value) in args {
            match self.properties.get(key) {
                Some(kind) if !kind.accepts(value) => {
                    return Err(format!("argument '{key}' must be {}", kind.name()));
                }
                None if !self.allow_additional => {
                    return Err(format!("unexpected argument '{key}'"));
                }
                _ => {}
            }
        }
        Ok(())
    }
}

#[derive(Debug)]
struct Entry {
    spec: ToolSpec,
    schema: Schema,
}

#[derive(Debug, Default)]
pub(crate) struct ToolRegistry {
    tools: BTreeMap<String, Entry>,
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
        self.tools.insert(spec.name.clone(), Entry { spec, schema });
        Ok(())
    }

    pub(crate) fn specs(&self) -> impl Iterator<Item = &ToolSpec> {
        self.tools.values().map(|e| &e.spec)
    }

    /// Turns a proposed call into a validated one, or explains — in words the model can
    /// act on — why it was refused.
    pub(crate) fn validate(
        &self,
        proposed: &ProposedToolCall,
        id: String,
        max_args_bytes: usize,
    ) -> Result<ToolCall, String> {
        let entry = self
            .tools
            .get(&proposed.name)
            .ok_or_else(|| format!("unknown tool '{}'", proposed.name))?;
        if entry.spec.requires_confirmation {
            return Err(format!(
                "tool '{}' needs the user's confirmation, which is not available yet",
                proposed.name
            ));
        }
        if proposed.arguments.len() > max_args_bytes {
            return Err(format!("arguments are larger than {max_args_bytes} bytes"));
        }
        let args: Map<String, Value> = match serde_json::from_str(&proposed.arguments) {
            Ok(Value::Object(map)) => map,
            Ok(_) | Err(_) => return Err("arguments must be a JSON object".to_string()),
        };
        entry.schema.check(&args)?;
        // Sorted keys (no "preserve_order"): equal arguments serialize identically.
        let arguments = serde_json::to_string(&Value::Object(args))
            .map_err(|_| "arguments could not be serialized".to_string())?;
        Ok(ToolCall {
            id,
            name: proposed.name.clone(),
            arguments,
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn calculator() -> ToolSpec {
        ToolSpec {
            name: "calculate".to_string(),
            description: "Evaluate arithmetic".to_string(),
            parameters: r#"{"type":"object","properties":{"expression":{"type":"string"}},"required":["expression"]}"#.to_string(),
            requires_confirmation: false,
            required_capabilities: Vec::new(),
        }
    }

    fn registry() -> ToolRegistry {
        let mut r = ToolRegistry::default();
        r.register(calculator()).unwrap();
        r
    }

    fn call(name: &str, arguments: &str) -> ProposedToolCall {
        ProposedToolCall {
            name: name.to_string(),
            arguments: arguments.to_string(),
        }
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
    fn confirmation_tools_are_refused_until_the_trusted_side_exists() {
        let mut r = ToolRegistry::default();
        r.register(ToolSpec {
            name: "send_mail".to_string(),
            requires_confirmation: true,
            ..calculator()
        })
        .unwrap();
        let err = r
            .validate(
                &call("send_mail", r#"{"expression":"x"}"#),
                "c".into(),
                1024,
            )
            .unwrap_err();
        assert!(err.contains("confirmation"));
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
}
