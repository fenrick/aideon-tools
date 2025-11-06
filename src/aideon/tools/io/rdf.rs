use std::collections::BTreeMap;
use std::fs::File;
use std::path::Path;

pub use oxigraph::io::{JsonLdProfileSet, RdfFormat};
use oxigraph::io::{RdfParser, RdfSerializer};
use oxigraph::model::{BlankNode, GraphName, Literal, NamedNode, NamedOrBlankNode, Quad, Term};

use crate::aideon::tools::error::{Result, ToolError};
use crate::aideon::tools::model::{ArrayValue, Node, PropertyValue, ScalarValue};

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const XSD_BOOLEAN: &str = "http://www.w3.org/2001/XMLSchema#boolean";
const XSD_INTEGER: &str = "http://www.w3.org/2001/XMLSchema#integer";
const XSD_DECIMAL: &str = "http://www.w3.org/2001/XMLSchema#decimal";
const XSD_DOUBLE: &str = "http://www.w3.org/2001/XMLSchema#double";

/// Loads an RDF graph from the provided path and converts it into the internal
/// node representation.
pub fn read_rdf(path: &Path, format: Option<RdfFormat>) -> Result<Vec<Node>> {
    let format = format.or_else(|| detect_format(path)).ok_or_else(|| {
        ToolError::Rdf(format!(
            "unable to infer RDF format from extension for file {}",
            path.display()
        ))
    })?;

    let file = File::open(path)?;
    let parser = RdfParser::from_format(format);
    let quad_parser = parser.for_reader(file);

    let mut nodes: BTreeMap<(Option<String>, String), Node> = BTreeMap::new();

    for quad_result in quad_parser {
        let quad = quad_result.map_err(|err| ToolError::Rdf(err.to_string()))?;

        let subject_id = subject_to_id(&quad.subject)?;
        let predicate = quad.predicate.as_str().to_string();
        let graph_name = graph_name_to_string(&quad.graph_name)?;
        let node = nodes
            .entry((graph_name.clone(), subject_id.clone()))
            .or_insert_with(|| Node::with_graph(subject_id.clone(), graph_name.clone()));
        node.set_graph(graph_name);

        if predicate == RDF_TYPE {
            if let Term::NamedNode(object) = &quad.object {
                node.types.insert(object.as_str().to_string());
            }
            continue;
        }

        let property = term_to_property(&quad.object)?;
        merge_property(node, predicate, property);
    }

    Ok(nodes.into_values().collect())
}

/// Serialises the provided nodes into an RDF graph.
pub fn write_rdf(path: &Path, nodes: &[Node], format: RdfFormat) -> Result<()> {
    let file = File::create(path)?;
    let mut serializer = RdfSerializer::from_format(format).for_writer(file);

    let rdf_type = NamedNode::new(RDF_TYPE).map_err(|err| ToolError::Rdf(err.to_string()))?;

    for node in nodes {
        let subject = id_to_subject(&node.id)?;
        let graph_name = graph_to_name(node.graph.as_ref())?;

        for type_name in &node.types {
            let class = NamedNode::new(type_name).map_err(|err| ToolError::Rdf(err.to_string()))?;
            let quad = Quad::new(
                subject.clone(),
                rdf_type.clone(),
                class.clone(),
                graph_name.clone(),
            );
            serializer
                .serialize_quad(&quad)
                .map_err(|err| ToolError::Rdf(err.to_string()))?;
        }

        for (predicate, value) in &node.properties {
            let predicate_node =
                NamedNode::new(predicate).map_err(|err| ToolError::Rdf(err.to_string()))?;
            match value {
                PropertyValue::Scalar(scalar) => {
                    if let Some(term) = scalar_to_term(scalar)? {
                        let quad = Quad::new(
                            subject.clone(),
                            predicate_node.clone(),
                            term,
                            graph_name.clone(),
                        );
                        serializer
                            .serialize_quad(&quad)
                            .map_err(|err| ToolError::Rdf(err.to_string()))?;
                    }
                }
                PropertyValue::ObjectRef(target) => {
                    let term = id_to_term(target)?;
                    let quad = Quad::new(
                        subject.clone(),
                        predicate_node.clone(),
                        term,
                        graph_name.clone(),
                    );
                    serializer
                        .serialize_quad(&quad)
                        .map_err(|err| ToolError::Rdf(err.to_string()))?;
                }
                PropertyValue::Array(ArrayValue::Scalars(items)) => {
                    for scalar in items {
                        if let Some(term) = scalar_to_term(scalar)? {
                            let quad = Quad::new(
                                subject.clone(),
                                predicate_node.clone(),
                                term,
                                graph_name.clone(),
                            );
                            serializer
                                .serialize_quad(&quad)
                                .map_err(|err| ToolError::Rdf(err.to_string()))?;
                        }
                    }
                }
                PropertyValue::Array(ArrayValue::ObjectRefs(targets)) => {
                    for target in targets {
                        let term = id_to_term(target)?;
                        let quad = Quad::new(
                            subject.clone(),
                            predicate_node.clone(),
                            term,
                            graph_name.clone(),
                        );
                        serializer
                            .serialize_quad(&quad)
                            .map_err(|err| ToolError::Rdf(err.to_string()))?;
                    }
                }
            }
        }
    }

    serializer
        .finish()
        .map_err(|err| ToolError::Rdf(err.to_string()))?;
    Ok(())
}

pub fn detect_format(path: &Path) -> Option<RdfFormat> {
    let extension = path.extension()?.to_ascii_lowercase();
    match extension.to_str()? {
        "ttl" | "turtle" => Some(RdfFormat::Turtle),
        "nt" => Some(RdfFormat::NTriples),
        "nq" => Some(RdfFormat::NQuads),
        "trig" => Some(RdfFormat::TriG),
        "jsonld" => Some(RdfFormat::JsonLd {
            profile: JsonLdProfileSet::empty(),
        }),
        _ => None,
    }
}

fn subject_to_id(subject: &NamedOrBlankNode) -> Result<String> {
    match subject {
        NamedOrBlankNode::NamedNode(node) => Ok(node.as_str().to_string()),
        NamedOrBlankNode::BlankNode(node) => Ok(format!("_:{}", node.as_str())),
    }
}

fn term_to_property(term: &Term) -> Result<PropertyValue> {
    Ok(match term {
        Term::NamedNode(node) => PropertyValue::ObjectRef(node.as_str().to_string()),
        Term::BlankNode(node) => PropertyValue::ObjectRef(format!("_:{}", node.as_str())),
        Term::Literal(literal) => PropertyValue::Scalar(literal_to_scalar(literal)?),
    })
}

fn literal_to_scalar(literal: &Literal) -> Result<ScalarValue> {
    if let Some(language) = literal.language() {
        return Ok(ScalarValue::String(format!(
            "{}@{}",
            literal.value(),
            language
        )));
    }

    match literal.datatype().as_str() {
        XSD_BOOLEAN => Ok(ScalarValue::Boolean(matches!(
            literal.value(),
            "true" | "1"
        ))),
        XSD_INTEGER | XSD_DECIMAL | XSD_DOUBLE => literal
            .value()
            .parse::<f64>()
            .map(ScalarValue::Number)
            .map_err(|err| ToolError::Rdf(err.to_string())),
        _ => Ok(ScalarValue::String(literal.value().to_string())),
    }
}

fn scalar_to_term(value: &ScalarValue) -> Result<Option<Term>> {
    Ok(match value {
        ScalarValue::String(text) => {
            let literal = Literal::new_simple_literal(text);
            Some(Term::Literal(literal))
        }
        ScalarValue::Number(number) => {
            let datatype = NamedNode::new(XSD_DOUBLE)?;
            let literal = Literal::new_typed_literal(number.to_string(), datatype);
            Some(Term::Literal(literal))
        }
        ScalarValue::Boolean(flag) => {
            let datatype = NamedNode::new(XSD_BOOLEAN)?;
            let literal = Literal::new_typed_literal(flag.to_string(), datatype);
            Some(Term::Literal(literal))
        }
        ScalarValue::Null => None,
    })
}

fn id_to_subject(id: &str) -> Result<NamedOrBlankNode> {
    if let Some(rest) = id.strip_prefix("_:") {
        let blank = BlankNode::new(rest).map_err(|err| ToolError::Rdf(err.to_string()))?;
        Ok(NamedOrBlankNode::BlankNode(blank))
    } else {
        let named = NamedNode::new(id)?;
        Ok(NamedOrBlankNode::NamedNode(named))
    }
}

fn id_to_term(id: &str) -> Result<Term> {
    if let Some(rest) = id.strip_prefix("_:") {
        let blank = BlankNode::new(rest).map_err(|err| ToolError::Rdf(err.to_string()))?;
        Ok(Term::from(blank))
    } else {
        let named = NamedNode::new(id)?;
        Ok(Term::from(named))
    }
}

fn graph_name_to_string(name: &GraphName) -> Result<Option<String>> {
    Ok(match name {
        GraphName::DefaultGraph => None,
        GraphName::NamedNode(node) => Some(node.as_str().to_string()),
        GraphName::BlankNode(node) => Some(format!("_:{}", node.as_str())),
    })
}

fn graph_to_name(graph: Option<&String>) -> Result<GraphName> {
    match graph {
        None => Ok(GraphName::DefaultGraph),
        Some(value) => {
            if let Some(rest) = value.strip_prefix("_:") {
                let blank = BlankNode::new(rest).map_err(|err| ToolError::Rdf(err.to_string()))?;
                Ok(GraphName::BlankNode(blank))
            } else {
                let named = NamedNode::new(value)?;
                Ok(GraphName::NamedNode(named))
            }
        }
    }
}

fn merge_property(node: &mut Node, predicate: String, value: PropertyValue) {
    use std::collections::btree_map::Entry;

    match node.properties.entry(predicate) {
        Entry::Vacant(entry) => {
            entry.insert(value);
        }
        Entry::Occupied(mut entry) => match (entry.get_mut(), value) {
            (PropertyValue::Scalar(existing), PropertyValue::Scalar(new_value)) => {
                let values = vec![existing.clone(), new_value];
                entry.insert(PropertyValue::Array(ArrayValue::Scalars(values)));
            }
            (PropertyValue::ObjectRef(existing), PropertyValue::ObjectRef(new_value)) => {
                let values = vec![existing.clone(), new_value];
                entry.insert(PropertyValue::Array(ArrayValue::ObjectRefs(values)));
            }
            (
                PropertyValue::Array(ArrayValue::Scalars(existing)),
                PropertyValue::Scalar(new_value),
            ) => {
                existing.push(new_value);
            }
            (
                PropertyValue::Array(ArrayValue::ObjectRefs(existing)),
                PropertyValue::ObjectRef(new_value),
            ) => {
                existing.push(new_value);
            }
            (
                PropertyValue::Array(ArrayValue::Scalars(existing)),
                PropertyValue::Array(ArrayValue::Scalars(mut incoming)),
            ) => {
                existing.append(&mut incoming);
            }
            (
                PropertyValue::Array(ArrayValue::ObjectRefs(existing)),
                PropertyValue::Array(ArrayValue::ObjectRefs(mut incoming)),
            ) => {
                existing.append(&mut incoming);
            }
            (_, other) => {
                entry.insert(other);
            }
        },
    }
}
