use serde_json::{Value, json};

pub(super) fn object(properties: Value, required: &[&str]) -> Value {
    if required.is_empty() {
        json!({ "type": "object", "properties": properties, "additionalProperties": false })
    } else {
        json!({ "type": "object", "properties": properties, "required": required, "additionalProperties": false })
    }
}

pub(super) fn empty() -> Value {
    object(json!({}), &[])
}

pub(super) fn string(description: &str) -> Value {
    json!({ "type": "string", "description": description })
}

pub(super) fn boolean(description: &str) -> Value {
    json!({ "type": "boolean", "description": description })
}

pub(super) fn integer(description: &str, min: i64, max: i64) -> Value {
    json!({ "type": "integer", "minimum": min, "maximum": max, "description": description })
}

pub(super) fn number(description: &str, min: f64, max: f64) -> Value {
    json!({ "type": "number", "minimum": min, "maximum": max, "description": description })
}

pub(super) fn one_of(description: &str, values: &[&str]) -> Value {
    json!({ "type": "string", "enum": values, "description": description })
}

pub(super) fn vec3(description: &str) -> Value {
    json!({ "type": "array", "items": { "type": "number" }, "minItems": 3, "maxItems": 3, "description": description })
}

pub(super) fn strings(description: &str) -> Value {
    json!({ "type": "array", "items": { "type": "string" }, "description": description })
}
