"""AgentDojo's tool schemas, reduced to the JSON Schema subset PR3's core checks.

Every arm gets the same reduced schemas, so the model sees identical tool definitions.
"""


def reduce(schema: dict) -> dict:
    """Keeps `type`, `description` and `enum` per property, plus `required`. Optional
    values (`anyOf` with `null`) become their type; references are inlined."""
    defs = schema.get("$defs", {})
    properties = {}
    for name, prop in schema.get("properties", {}).items():
        properties[name] = _property(name, prop, defs)
    return {
        "type": "object",
        "properties": properties,
        "required": list(schema.get("required", [])),
    }


def _property(name: str, prop: dict, defs: dict) -> dict:
    if "$ref" in prop:
        prop = {**defs[prop["$ref"].split("/")[-1]], **{k: v for k, v in prop.items() if k != "$ref"}}
    kind = prop.get("type")
    if kind is None and "anyOf" in prop:
        options = [_property(name, option, defs) for option in prop["anyOf"] if option.get("type") != "null"]
        if len(options) == 1:
            kind = options[0]["type"]
            prop = {**options[0], **{k: v for k, v in prop.items() if k != "anyOf"}}
    if kind is None and "enum" in prop:
        kind = "string"
    if not isinstance(kind, str):
        raise ValueError(f"property {name!r} has no single type: {prop}")
    out = {"type": kind}
    if "description" in prop:
        out["description"] = prop["description"]
    if "enum" in prop:
        out["enum"] = prop["enum"]
    return out
