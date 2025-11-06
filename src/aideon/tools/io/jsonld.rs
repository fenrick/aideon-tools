use std::collections::BTreeMap;

use serde_json::{Map, Value};
use uuid::Uuid;

use crate::aideon::tools::error::{Result, ToolError};
use crate::aideon::tools::model::{ArrayValue, Node, PropertyValue, ScalarValue};

/// Parses a JSON-LD document into a vector of [`Node`]s.
pub fn parse_jsonld_document(document: &Value) -> Result<Vec<Node>> {
    match document {
        Value::Array(items) => items.iter().map(parse_node_value).collect(),
        Value::Object(object) => {
            if let Some(graph) = object.get("@graph") {
                parse_jsonld_document(graph)
            } else {
                Ok(vec![parse_node(object)?])
            }
        }
        _ => Err(ToolError::JsonLd("expected JSON-LD array or object".into())),
    }
}

fn parse_node_value(value: &Value) -> Result<Node> {
    match value {
        Value::Object(map) => parse_node(map),
        _ => Err(ToolError::JsonLd("expected JSON object for node".into())),
    }
}

fn parse_node(object: &Map<String, Value>) -> Result<Node> {
    let mut id = object
        .get("@id")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| generate_surrogate_id(object));

    if id.is_empty() {
        id = generate_surrogate_id(object);
    }

    let mut node = Node::new(id);

    if let Some(types) = object.get("@type") {
        match types {
            Value::Array(entries) => {
                for entry in entries {
                    if let Some(value) = entry.as_str() {
                        node.types.insert(value.to_string());
                    }
                }
            }
            Value::String(value) => {
                node.types.insert(value.to_string());
            }
            other => {
                return Err(ToolError::JsonLd(format!(
                    "invalid @type entry: expected string or array, found {other}"
                )));
            }
        }
    }

    for (key, value) in object {
        if key == "@id" || key == "@type" || key == "@context" {
            continue;
        }

        let property_value = parse_property_value(value)
            .map_err(|err| ToolError::JsonLd(format!("failed to parse property '{key}': {err}")))?;
        node.insert_property(key.clone(), property_value);
    }

    Ok(node)
}

fn parse_property_value(value: &Value) -> Result<PropertyValue> {
    match value {
        Value::Null => Ok(PropertyValue::Scalar(ScalarValue::Null)),
        Value::Bool(value) => Ok(PropertyValue::Scalar(ScalarValue::Boolean(*value))),
        Value::Number(number) => Ok(PropertyValue::Scalar(ScalarValue::Number(
            number
                .as_f64()
                .ok_or_else(|| ToolError::JsonLd("invalid number literal".into()))?,
        ))),
        Value::String(value) => Ok(PropertyValue::Scalar(ScalarValue::String(value.clone()))),
        Value::Array(values) => parse_array(values),
        Value::Object(map) => {
            if let Some(id) = map.get("@id").and_then(Value::as_str) {
                Ok(PropertyValue::ObjectRef(id.to_string()))
            } else if let Some(literal) = map.get("@value") {
                parse_property_value(literal)
            } else {
                // Nested objects are stored as compact JSON strings to avoid
                // accidental data loss while still representing them in Excel.
                Ok(PropertyValue::Scalar(ScalarValue::String(
                    serde_json::to_string(map).map_err(|err| ToolError::JsonLd(err.to_string()))?,
                )))
            }
        }
    }
}

fn parse_array(values: &[Value]) -> Result<PropertyValue> {
    let mut scalars = Vec::new();
    let mut refs = Vec::new();

    for entry in values {
        match entry {
            Value::Object(map) if map.contains_key("@id") => {
                if let Some(id) = map.get("@id").and_then(Value::as_str) {
                    refs.push(id.to_string());
                } else {
                    return Err(ToolError::JsonLd("object reference missing @id".into()));
                }
            }
            Value::Object(map) if map.contains_key("@value") => {
                scalars.push(extract_scalar(map.get("@value").unwrap())?);
            }
            Value::Object(map) => {
                // Nested object arrays are not fully supported; we preserve the
                // raw JSON representation as a string literal.
                scalars.push(ScalarValue::String(
                    serde_json::to_string(map).map_err(|err| ToolError::JsonLd(err.to_string()))?,
                ));
            }
            other => scalars.push(extract_scalar(other)?),
        }
    }

    match (scalars.is_empty(), refs.is_empty()) {
        (false, true) => Ok(PropertyValue::Array(ArrayValue::Scalars(scalars))),
        (true, false) => Ok(PropertyValue::Array(ArrayValue::ObjectRefs(refs))),
        (true, true) => Ok(PropertyValue::Array(ArrayValue::Scalars(vec![]))),
        (false, false) => Err(ToolError::JsonLd(
            "mixed arrays of literals and object references are not supported".into(),
        )),
    }
}

fn extract_scalar(value: &Value) -> Result<ScalarValue> {
    match value {
        Value::Null => Ok(ScalarValue::Null),
        Value::Bool(value) => Ok(ScalarValue::Boolean(*value)),
        Value::Number(number) => {
            Ok(ScalarValue::Number(number.as_f64().ok_or_else(|| {
                ToolError::JsonLd("invalid number literal".into())
            })?))
        }
        Value::String(value) => Ok(ScalarValue::String(value.clone())),
        other => Ok(ScalarValue::String(serde_json::to_string(other)?)),
    }
}

fn generate_surrogate_id(object: &Map<String, Value>) -> String {
    let canonical = canonicalise_object(object);
    let uuid = Uuid::new_v5(&Uuid::NAMESPACE_OID, canonical.as_bytes());
    format!("urn:uuid:{uuid}")
}

fn canonicalise_object(object: &Map<String, Value>) -> String {
    let mut ordered = BTreeMap::new();
    for (key, value) in object {
        if key == "@context" {
            continue;
        }
        ordered.insert(key, value);
    }
    serde_json::to_string(&ordered).unwrap_or_default()
}

/// Serialises a collection of nodes back into a JSON-LD document.
pub fn nodes_to_jsonld(nodes: &[Node], context: Option<Value>) -> Value {
    let graph: Vec<Value> = nodes.iter().map(node_to_json).collect();

    let mut document = Map::new();
    if let Some(context) = context {
        document.insert("@context".to_string(), context);
    }
    document.insert("@graph".to_string(), Value::Array(graph));
    Value::Object(document)
}

fn node_to_json(node: &Node) -> Value {
    let mut map = Map::new();
    map.insert("@id".to_string(), Value::String(node.id.clone()));

    if !node.types.is_empty() {
        if node.types.len() == 1 {
            map.insert(
                "@type".to_string(),
                Value::String(node.types.iter().next().unwrap().clone()),
            );
        } else {
            map.insert(
                "@type".to_string(),
                Value::Array(node.types.iter().cloned().map(Value::String).collect()),
            );
        }
    }

    for (predicate, value) in &node.properties {
        let json_value = match value {
            PropertyValue::Scalar(scalar) => scalar.to_json(),
            PropertyValue::ObjectRef(target) => {
                let mut ref_map = Map::new();
                ref_map.insert("@id".to_string(), Value::String(target.clone()));
                Value::Object(ref_map)
            }
            PropertyValue::Array(ArrayValue::Scalars(values)) => {
                Value::Array(values.iter().map(ScalarValue::to_json).collect())
            }
            PropertyValue::Array(ArrayValue::ObjectRefs(values)) => Value::Array(
                values
                    .iter()
                    .map(|target| {
                        let mut ref_map = Map::new();
                        ref_map.insert("@id".to_string(), Value::String(target.clone()));
                        Value::Object(ref_map)
                    })
                    .collect(),
            ),
        };

        map.insert(predicate.clone(), json_value);
    }

    Value::Object(map)
}
