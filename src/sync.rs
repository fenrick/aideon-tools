use std::fs;
use std::path::Path;

use serde_json::Value;

use crate::error::Result;
use crate::flatten::build_workbook;
use crate::io::excel_read;
use crate::io::excel_write;
use crate::io::jsonld;
use crate::io::rdf::{self, RdfFormat};

/// Synchronises a JSON-LD document into an Excel workbook.
pub fn jsonld_to_excel(input: &Path, output: &Path) -> Result<()> {
    let source = fs::read_to_string(input)?;
    let json: Value = serde_json::from_str(&source)?;
    let nodes = jsonld::parse_jsonld_document(&json)?;
    let workbook = build_workbook(&nodes)?;
    excel_write::write_workbook(output, &workbook)
}

/// Synchronises an Excel workbook back into JSON-LD.
pub fn excel_to_jsonld(input: &Path, output: &Path, context: Option<Value>) -> Result<()> {
    let nodes = excel_read::read_nodes(input)?;
    let json = jsonld::nodes_to_jsonld(&nodes, context);
    let json_string = serde_json::to_string_pretty(&json)?;
    fs::write(output, json_string)?;
    Ok(())
}

/// Loads an RDF graph and materialises it as an Excel workbook.
pub fn rdf_to_excel(input: &Path, output: &Path) -> Result<()> {
    let nodes = rdf::read_rdf(input, None)?;
    let workbook = build_workbook(&nodes)?;
    excel_write::write_workbook(output, &workbook)
}

/// Persists the current node set into an RDF graph.
pub fn excel_to_rdf(input: &Path, output: &Path, format: RdfFormat) -> Result<()> {
    let nodes = excel_read::read_nodes(input)?;
    rdf::write_rdf(output, &nodes, format)
}

/// Converts a JSON-LD document directly into RDF.
pub fn jsonld_to_rdf(input: &Path, output: &Path, format: RdfFormat) -> Result<()> {
    let source = fs::read_to_string(input)?;
    let json: Value = serde_json::from_str(&source)?;
    let nodes = jsonld::parse_jsonld_document(&json)?;
    rdf::write_rdf(output, &nodes, format)
}

/// Converts an RDF graph into JSON-LD.
pub fn rdf_to_jsonld(input: &Path, output: &Path, context: Option<Value>) -> Result<()> {
    let nodes = rdf::read_rdf(input, None)?;
    excel_to_jsonld_internal(&nodes, output, context)
}

fn excel_to_jsonld_internal(
    nodes: &[crate::model::Node],
    output: &Path,
    context: Option<Value>,
) -> Result<()> {
    let json = jsonld::nodes_to_jsonld(nodes, context);
    let json_string = serde_json::to_string_pretty(&json)?;
    fs::write(output, json_string)?;
    Ok(())
}
