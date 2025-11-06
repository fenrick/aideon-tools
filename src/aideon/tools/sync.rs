use std::fs;
use std::path::Path;

use serde_json::Value;

use crate::aideon::tools::error::Result;
use crate::aideon::tools::flatten::build_workbook;
use crate::aideon::tools::io::excel_read;
use crate::aideon::tools::io::excel_write;
use crate::aideon::tools::io::jsonld;
use crate::aideon::tools::io::rdf::{self, RdfFormat};
use crate::aideon::tools::model::Node;
use tracing::{debug, info, instrument};

/// Synchronises a JSON-LD document into an Excel workbook.
#[instrument(
    level = "info",
    skip_all,
    fields(input = %input.display(), output = %output.display())
)]
pub fn jsonld_to_excel(input: &Path, output: &Path) -> Result<()> {
    let source = fs::read_to_string(input)?;
    let json: Value = serde_json::from_str(&source)?;
    let nodes = jsonld::parse_jsonld_document(&json)?;
    info!(node_count = nodes.len(), "parsed nodes from JSON-LD source");
    let workbook = build_workbook(&nodes)?;
    debug!(sheet_count = workbook.tables.len(), "workbook constructed");
    excel_write::write_workbook(output, &workbook)
}

/// Synchronises an Excel workbook back into JSON-LD.
#[instrument(
    level = "info",
    skip_all,
    fields(input = %input.display(), output = %output.display())
)]
pub fn excel_to_jsonld(input: &Path, output: &Path, context: Option<Value>) -> Result<()> {
    let nodes = excel_read::read_nodes(input)?;
    info!(node_count = nodes.len(), "read nodes from workbook");
    let json = jsonld::nodes_to_jsonld(&nodes, context)?;
    let json_string = serde_json::to_string_pretty(&json)?;
    fs::write(output, json_string)?;
    Ok(())
}

/// Loads an RDF graph and materialises it as an Excel workbook.
#[instrument(
    level = "info",
    skip_all,
    fields(input = %input.display(), output = %output.display())
)]
pub fn rdf_to_excel(input: &Path, output: &Path) -> Result<()> {
    let nodes = rdf::read_rdf(input, None)?;
    info!(node_count = nodes.len(), "parsed nodes from RDF source");
    let workbook = build_workbook(&nodes)?;
    debug!(sheet_count = workbook.tables.len(), "workbook constructed");
    excel_write::write_workbook(output, &workbook)
}

/// Persists the current node set into an RDF graph.
#[instrument(
    level = "info",
    skip_all,
    fields(input = %input.display(), output = %output.display(), ?format)
)]
pub fn excel_to_rdf(input: &Path, output: &Path, format: RdfFormat) -> Result<()> {
    let nodes = excel_read::read_nodes(input)?;
    info!(node_count = nodes.len(), "read nodes from workbook");
    rdf::write_rdf(output, &nodes, format)
}

/// Converts a JSON-LD document directly into RDF.
#[instrument(
    level = "info",
    skip_all,
    fields(input = %input.display(), output = %output.display(), ?format)
)]
pub fn jsonld_to_rdf(input: &Path, output: &Path, format: RdfFormat) -> Result<()> {
    let source = fs::read_to_string(input)?;
    let json: Value = serde_json::from_str(&source)?;
    let nodes = jsonld::parse_jsonld_document(&json)?;
    info!(node_count = nodes.len(), "parsed nodes from JSON-LD source");
    rdf::write_rdf(output, &nodes, format)
}

/// Converts an RDF graph into JSON-LD.
#[instrument(
    level = "info",
    skip_all,
    fields(input = %input.display(), output = %output.display())
)]
pub fn rdf_to_jsonld(input: &Path, output: &Path, context: Option<Value>) -> Result<()> {
    let nodes = rdf::read_rdf(input, None)?;
    info!(node_count = nodes.len(), "parsed nodes from RDF source");
    excel_to_jsonld_internal(&nodes, output, context)
}

#[instrument(level = "debug", skip(nodes, context), fields(output = %output.display()))]
fn excel_to_jsonld_internal(nodes: &[Node], output: &Path, context: Option<Value>) -> Result<()> {
    let json = jsonld::nodes_to_jsonld(nodes, context)?;
    let json_string = serde_json::to_string_pretty(&json)?;
    fs::write(output, json_string)?;
    Ok(())
}
