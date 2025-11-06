use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

/// Identifier assigned to a node. It mirrors the JSON-LD `@id` semantics and
/// intentionally keeps the plain string representation for ease of
/// interoperability with Excel.
pub type NodeId = String;

/// Represents a scalar literal value in the graph.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum ScalarValue {
    /// Plain string literal.
    String(String),
    /// Floating point number literal.
    Number(f64),
    /// Boolean literal.
    Boolean(bool),
    /// Explicit JSON `null` literal.
    Null,
}

impl ScalarValue {
    /// Converts the scalar into the JSON representation used in JSON-LD
    /// payloads.
    pub fn to_json(&self) -> serde_json::Value {
        match self {
            ScalarValue::String(value) => serde_json::Value::String(value.clone()),
            ScalarValue::Number(value) => serde_json::Number::from_f64(*value)
                .map(serde_json::Value::Number)
                .unwrap_or(serde_json::Value::Null),
            ScalarValue::Boolean(value) => serde_json::Value::Bool(*value),
            ScalarValue::Null => serde_json::Value::Null,
        }
    }
}

/// Represents multi-valued predicates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "items")]
pub enum ArrayValue {
    /// Array of scalar literals.
    Scalars(Vec<ScalarValue>),
    /// Array of object references (node identifiers).
    ObjectRefs(Vec<NodeId>),
}

/// Represents property values associated with a node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "variant", content = "value")]
pub enum PropertyValue {
    /// Scalar literal value.
    Scalar(ScalarValue),
    /// Object reference pointing to another node.
    ObjectRef(NodeId),
    /// Array value consisting either of literals or object references.
    Array(ArrayValue),
}

/// Represents an entity in the graph.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Node {
    /// Node identifier.
    pub id: NodeId,
    /// Optional name of the graph the node belongs to.
    pub graph: Option<String>,
    /// Node types. Multi-typed nodes contain multiple entries.
    pub types: BTreeSet<String>,
    /// Predicate → value mapping.
    pub properties: BTreeMap<String, PropertyValue>,
}

impl Node {
    /// Creates a new node with the provided identifier.
    pub fn new(id: impl Into<NodeId>) -> Self {
        Self {
            id: id.into(),
            graph: None,
            types: BTreeSet::new(),
            properties: BTreeMap::new(),
        }
    }

    /// Creates a new node with the provided identifier assigned to the given graph.
    pub fn with_graph(id: impl Into<NodeId>, graph: Option<String>) -> Self {
        Self {
            id: id.into(),
            graph,
            types: BTreeSet::new(),
            properties: BTreeMap::new(),
        }
    }

    /// Sets the graph the node belongs to.
    pub fn set_graph(&mut self, graph: Option<String>) {
        self.graph = graph;
    }

    /// Inserts or replaces a property value.
    pub fn insert_property(&mut self, predicate: String, value: PropertyValue) {
        self.properties.insert(predicate, value);
    }
}
