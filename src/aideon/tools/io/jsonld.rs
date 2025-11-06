use std::collections::{BTreeMap, BTreeSet};

use futures::executor::block_on;
use iref::Iri;
use json_ld::{JsonLdProcessor, NoLoader, Options, RemoteContextReference, RemoteDocument};
use json_ld_syntax::TryFromJson;
use json_ld_syntax::context::Context as JsonLdContext;
use json_syntax::Value as JsonSyntaxValue;
use serde_json::{Map, Value};
use uuid::Uuid;

use crate::aideon::tools::error::{Result, ToolError};
use crate::aideon::tools::model::{ArrayValue, Node, PropertyValue, ScalarValue};

type NodeKey = (Option<String>, String);

#[derive(Clone, Default)]
struct ActiveContext {
    vocab: Option<String>,
    term_map: BTreeMap<String, String>,
    id_properties: BTreeSet<String>,
}

/// Parses a JSON-LD document into a vector of [`Node`]s.
pub fn parse_jsonld_document(document: &Value) -> Result<Vec<Node>> {
    let mut nodes: BTreeMap<NodeKey, Node> = BTreeMap::new();
    match document {
        Value::Array(items) => {
            for value in items {
                parse_entry(value, None, None, &mut nodes)?;
            }
        }
        Value::Object(map) => {
            let base_context = if let Some(context) = map.get("@context") {
                Some(parse_context_value(context, None)?)
            } else {
                None
            };
            parse_entry(document, None, base_context.as_ref(), &mut nodes)?;
        }
        other => {
            return Err(ToolError::JsonLd(format!(
                "expected JSON-LD array or object, found {other}"
            )));
        }
    }

    Ok(nodes.into_values().collect())
}

fn parse_graph(
    value: &Value,
    active_graph: Option<&str>,
    context: Option<&ActiveContext>,
    nodes: &mut BTreeMap<NodeKey, Node>,
) -> Result<()> {
    match value {
        Value::Array(items) => {
            for entry in items {
                parse_entry(entry, active_graph, context, nodes)?;
            }
        }
        Value::Object(_) => {
            parse_entry(value, active_graph, context, nodes)?;
        }
        Value::Null => {}
        other => {
            return Err(ToolError::JsonLd(format!(
                "invalid @graph entry: expected array or object, found {other}"
            )));
        }
    }
    Ok(())
}

fn parse_entry(
    value: &Value,
    active_graph: Option<&str>,
    context: Option<&ActiveContext>,
    nodes: &mut BTreeMap<NodeKey, Node>,
) -> Result<()> {
    match value {
        Value::Object(object) => {
            let local_context_storage;
            let context_to_use = if let Some(context_value) = object.get("@context") {
                local_context_storage = parse_context_value(context_value, context)?;
                Some(&local_context_storage)
            } else {
                context
            };

            if let Some(graph_value) = object.get("@graph") {
                let next_graph = object.get("@id").and_then(Value::as_str);
                parse_graph(graph_value, next_graph, context_to_use, nodes)?;
                if has_node_properties(object) {
                    parse_node_object(object, active_graph, context_to_use, nodes)?;
                }
            } else {
                parse_node_object(object, active_graph, context_to_use, nodes)?;
            }
            Ok(())
        }
        Value::Array(values) => {
            for item in values {
                parse_entry(item, active_graph, context, nodes)?;
            }
            Ok(())
        }
        other => Err(ToolError::JsonLd(format!(
            "expected JSON object for node, found {other}"
        ))),
    }
}

fn has_node_properties(object: &Map<String, Value>) -> bool {
    object
        .keys()
        .any(|key| key != "@id" && key != "@graph" && key != "@context")
}

fn parse_node_object(
    object: &Map<String, Value>,
    active_graph: Option<&str>,
    context: Option<&ActiveContext>,
    nodes: &mut BTreeMap<NodeKey, Node>,
) -> Result<()> {
    let mut id = object
        .get("@id")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| generate_surrogate_id(object));

    if id.is_empty() {
        id = generate_surrogate_id(object);
    }

    let graph = active_graph.map(str::to_string);
    let key = (graph.clone(), id.clone());
    let node = nodes
        .entry(key)
        .or_insert_with(|| Node::with_graph(id.clone(), graph.clone()));
    node.set_graph(graph);

    if let Some(types) = object.get("@type") {
        match types {
            Value::Array(entries) => {
                for entry in entries {
                    if let Some(value) = entry.as_str() {
                        node.types.insert(expand_term(context, value));
                    }
                }
            }
            Value::String(value) => {
                node.types.insert(expand_term(context, value));
            }
            other => {
                return Err(ToolError::JsonLd(format!(
                    "invalid @type entry: expected string or array, found {other}"
                )));
            }
        }
    }

    for (key, value) in object {
        if matches!(key.as_str(), "@id" | "@type" | "@context" | "@graph") {
            continue;
        }

        let expanded_key = expand_term(context, key);
        let treat_as_id = context
            .map(|ctx| ctx.id_properties.contains(&expanded_key))
            .unwrap_or(false);

        let property_value = parse_property_value(value, context, treat_as_id).map_err(|err| {
            ToolError::JsonLd(format!("failed to parse property '{expanded_key}': {err}"))
        })?;
        node.insert_property(expanded_key, property_value);
    }

    Ok(())
}

fn parse_property_value(
    value: &Value,
    context: Option<&ActiveContext>,
    treat_as_id: bool,
) -> Result<PropertyValue> {
    match value {
        Value::Null => Ok(PropertyValue::Scalar(ScalarValue::Null)),
        Value::Bool(value) => Ok(PropertyValue::Scalar(ScalarValue::Boolean(*value))),
        Value::Number(number) => Ok(PropertyValue::Scalar(ScalarValue::Number(
            number
                .as_f64()
                .ok_or_else(|| ToolError::JsonLd("invalid number literal".into()))?,
        ))),
        Value::String(value) => {
            if treat_as_id {
                Ok(PropertyValue::ObjectRef(expand_term(context, value)))
            } else if looks_like_iri(value) {
                Ok(PropertyValue::ObjectRef(value.clone()))
            } else {
                Ok(PropertyValue::Scalar(ScalarValue::String(value.clone())))
            }
        }
        Value::Array(values) => parse_array(values, context, treat_as_id),
        Value::Object(map) => {
            if let Some(set) = map.get("@set") {
                return parse_property_value(set, context, treat_as_id);
            }

            if let Some(list) = map.get("@list") {
                return parse_property_value(list, context, treat_as_id);
            }

            if let Some(id) = map.get("@id").and_then(Value::as_str) {
                let reference = if treat_as_id {
                    expand_term(context, id)
                } else {
                    id.to_string()
                };
                return Ok(PropertyValue::ObjectRef(reference));
            }

            if let Some(literal) = map.get("@value") {
                return parse_property_value(literal, context, treat_as_id);
            }

            Ok(PropertyValue::Scalar(ScalarValue::String(
                serde_json::to_string(map).map_err(|err| ToolError::JsonLd(err.to_string()))?,
            )))
        }
    }
}

fn parse_array(
    values: &[Value],
    context: Option<&ActiveContext>,
    treat_as_id: bool,
) -> Result<PropertyValue> {
    let mut scalars = Vec::new();
    let mut refs = Vec::new();

    for entry in values {
        match entry {
            Value::Array(items) => {
                let nested = parse_array(items, context, treat_as_id)?;
                collect_array_entry(nested, &mut scalars, &mut refs)?;
            }
            Value::Object(map) if map.contains_key("@set") => {
                let nested = parse_property_value(map.get("@set").unwrap(), context, treat_as_id)?;
                collect_array_entry(nested, &mut scalars, &mut refs)?;
            }
            Value::Object(map) if map.contains_key("@list") => {
                let nested = parse_property_value(map.get("@list").unwrap(), context, treat_as_id)?;
                collect_array_entry(nested, &mut scalars, &mut refs)?;
            }
            Value::Object(map) if map.contains_key("@id") => {
                if let Some(id) = map.get("@id").and_then(Value::as_str) {
                    let reference = if treat_as_id {
                        expand_term(context, id)
                    } else {
                        id.to_string()
                    };
                    refs.push(reference);
                } else {
                    return Err(ToolError::JsonLd("object reference missing @id".into()));
                }
            }
            Value::Object(map) if map.contains_key("@value") => {
                scalars.push(extract_scalar(map.get("@value").unwrap())?);
            }
            Value::Object(map) => {
                scalars.push(ScalarValue::String(
                    serde_json::to_string(map).map_err(|err| ToolError::JsonLd(err.to_string()))?,
                ));
            }
            Value::String(value) if treat_as_id || looks_like_iri(value) => {
                let reference = if treat_as_id {
                    expand_term(context, value)
                } else {
                    value.clone()
                };
                refs.push(reference);
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

fn collect_array_entry(
    value: PropertyValue,
    scalars: &mut Vec<ScalarValue>,
    refs: &mut Vec<String>,
) -> Result<()> {
    match value {
        PropertyValue::Scalar(scalar) => scalars.push(scalar),
        PropertyValue::ObjectRef(reference) => refs.push(reference),
        PropertyValue::Array(ArrayValue::Scalars(mut nested)) => scalars.append(&mut nested),
        PropertyValue::Array(ArrayValue::ObjectRefs(mut nested)) => refs.append(&mut nested),
    }
    Ok(())
}

fn parse_context_value(value: &Value, parent: Option<&ActiveContext>) -> Result<ActiveContext> {
    match value {
        Value::Null => Ok(ActiveContext::default()),
        Value::Array(values) => {
            let mut current = parent.cloned().unwrap_or_default();
            for entry in values {
                current = parse_context_value(entry, Some(&current))?;
            }
            Ok(current)
        }
        Value::Object(object) => parse_context_object(object, parent),
        Value::String(reference) => Err(ToolError::JsonLd(format!(
            "remote context references are not supported: {reference}"
        ))),
        other => Err(ToolError::JsonLd(format!(
            "invalid @context entry: expected object, array, null, or string, found {other}"
        ))),
    }
}

fn parse_context_object(
    object: &Map<String, Value>,
    parent: Option<&ActiveContext>,
) -> Result<ActiveContext> {
    let mut context = parent.cloned().unwrap_or_default();

    if let Some(vocab) = object.get("@vocab") {
        match vocab {
            Value::Null => context.vocab = None,
            Value::String(value) => context.vocab = Some(value.clone()),
            other => {
                return Err(ToolError::JsonLd(format!(
                    "invalid @vocab definition: expected string or null, found {other}"
                )));
            }
        }
    }

    for (term, definition) in object {
        if term.starts_with('@') {
            continue;
        }
        parse_context_term(term, definition, &mut context)?;
    }

    Ok(context)
}

fn parse_context_term(term: &str, definition: &Value, context: &mut ActiveContext) -> Result<()> {
    match definition {
        Value::Null => {
            remove_term_definition(context, term);
        }
        Value::String(target) => {
            let expanded = expand_context_reference(context, target);
            update_term_definition(context, term, expanded, false);
        }
        Value::Object(object) => {
            let mut expanded = if let Some(id_value) = object.get("@id") {
                match id_value {
                    Value::Null => {
                        remove_term_definition(context, term);
                        None
                    }
                    Value::String(reference) => {
                        let expanded = expand_context_reference(context, reference);
                        update_term_definition(context, term, expanded.clone(), false);
                        Some(expanded)
                    }
                    other => {
                        return Err(ToolError::JsonLd(format!(
                            "invalid @id definition for term '{term}': expected string or null, found {other}"
                        )));
                    }
                }
            } else {
                context
                    .term_map
                    .get(term)
                    .cloned()
                    .or_else(|| default_vocab_expansion(context, term))
            };

            if let Some(Value::String(ty)) = object.get("@type") {
                let is_id_type = ty == "@id";
                if let Some(expanded_iri) = expanded.take() {
                    update_term_definition(context, term, expanded_iri, is_id_type);
                } else if is_id_type {
                    let inferred =
                        default_vocab_expansion(context, term).unwrap_or_else(|| term.to_string());
                    update_term_definition(context, term, inferred, true);
                }
            }
        }
        other => {
            return Err(ToolError::JsonLd(format!(
                "invalid term definition for '{term}': expected string, object, or null, found {other}"
            )));
        }
    }

    Ok(())
}

fn remove_term_definition(context: &mut ActiveContext, term: &str) {
    if let Some(previous) = context.term_map.remove(term) {
        context.id_properties.remove(&previous);
    }
}

fn update_term_definition(
    context: &mut ActiveContext,
    term: &str,
    expanded: String,
    is_id_type: bool,
) {
    context.term_map.insert(term.to_string(), expanded.clone());
    if is_id_type {
        context.id_properties.insert(expanded);
    } else {
        context.id_properties.remove(&expanded);
    }
}

fn default_vocab_expansion(context: &ActiveContext, term: &str) -> Option<String> {
    context.vocab.as_ref().map(|vocab| format!("{vocab}{term}"))
}

fn expand_context_reference(context: &ActiveContext, value: &str) -> String {
    if looks_like_iri(value) {
        return value.to_string();
    }

    if let Some(expanded) = expand_compact_iri(context, value) {
        return expanded;
    }

    default_vocab_expansion(context, value).unwrap_or_else(|| value.to_string())
}

fn expand_term(context: Option<&ActiveContext>, term: &str) -> String {
    if term.starts_with('@') || looks_like_iri(term) {
        return term.to_string();
    }

    if let Some(ctx) = context {
        if let Some(mapped) = ctx.term_map.get(term) {
            return mapped.clone();
        }

        if let Some(expanded) = expand_compact_iri(ctx, term) {
            return expanded;
        }

        if let Some(expanded) = default_vocab_expansion(ctx, term) {
            return expanded;
        }
    }

    term.to_string()
}

fn expand_compact_iri(context: &ActiveContext, value: &str) -> Option<String> {
    let (prefix, suffix) = value.split_once(':')?;
    context
        .term_map
        .get(prefix)
        .map(|base| format!("{base}{suffix}"))
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

fn looks_like_iri(value: &str) -> bool {
    Iri::new(value).is_ok()
}

fn generate_surrogate_id(object: &Map<String, Value>) -> String {
    let canonical = canonicalise_object(object);
    let uuid = Uuid::new_v5(&Uuid::NAMESPACE_OID, canonical.as_bytes());
    format!("urn:uuid:{uuid}")
}

fn canonicalise_object(object: &Map<String, Value>) -> String {
    let mut ordered = BTreeMap::new();
    for (key, value) in object {
        if matches!(key.as_str(), "@context" | "@graph") {
            continue;
        }
        ordered.insert(key, value);
    }
    serde_json::to_string(&ordered).unwrap_or_default()
}

/// Serialises a collection of nodes back into a JSON-LD document.
pub fn nodes_to_jsonld(nodes: &[Node], context: Option<Value>) -> Result<Value> {
    let mut default_graph: Vec<Value> = Vec::new();
    let mut named_graphs: BTreeMap<String, Vec<Value>> = BTreeMap::new();

    for node in nodes {
        let entry = node_to_json(node);
        if let Some(graph) = &node.graph {
            named_graphs.entry(graph.clone()).or_default().push(entry);
        } else {
            default_graph.push(entry);
        }
    }

    let mut graph_entries = default_graph;
    for (graph, nodes) in named_graphs {
        let mut container = Map::new();
        container.insert("@id".to_string(), Value::String(graph));
        container.insert("@graph".to_string(), Value::Array(nodes));
        graph_entries.push(Value::Object(container));
    }

    let mut document = Map::new();
    document.insert("@graph".to_string(), Value::Array(graph_entries));
    let expanded = Value::Object(document);

    if let Some(context) = context {
        compact_with_context(expanded, context)
    } else {
        Ok(expanded)
    }
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

fn compact_with_context(expanded: Value, context: Value) -> Result<Value> {
    let document = JsonSyntaxValue::from_serde_json(expanded);
    let remote_document = RemoteDocument::new(None, None, document);

    let context_json = JsonSyntaxValue::from_serde_json(context);
    let context = JsonLdContext::try_from_json(context_json)
        .map_err(|err| ToolError::JsonLd(err.to_string()))?;
    let remote_context = json_ld::RemoteContext::new(None, None, context);
    let context_reference = RemoteContextReference::Loaded(remote_context);

    let loader = NoLoader;
    let options = Options::default();

    let compacted = block_on(remote_document.compact_using(context_reference, &loader, options))
        .map_err(|err| ToolError::JsonLd(err.to_string()))?;

    Ok(JsonSyntaxValue::into_serde_json(compacted))
}
