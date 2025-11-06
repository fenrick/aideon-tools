use std::collections::{BTreeMap, HashMap, btree_map::Entry};
use std::path::Path;

use calamine::{Data, Reader, Xlsx, open_workbook};
use serde_json::Value;

use crate::aideon::tools::error::{Result, ToolError};
use crate::aideon::tools::flatten::{ENTITIES_SHEET, METADATA_SHEET, UNTYPED_MARKER};
use crate::aideon::tools::model::{ArrayValue, Node, PropertyValue, ScalarValue};

type NodeKey = (Option<String>, String);

type TypeSheetMap = HashMap<String, String>;
type ChildSheetMap = HashMap<String, (String, String)>;

/// Reads nodes from an Excel workbook following the conventions produced by the
/// [`excel_write`](crate::io::excel_write) module.
pub fn read_nodes(path: &Path) -> Result<Vec<Node>> {
    let mut workbook: Xlsx<_> = open_workbook(path)?;

    let metadata_range = read_required_sheet(&mut workbook, METADATA_SHEET)?;
    let entities_range = read_required_sheet(&mut workbook, ENTITIES_SHEET)?;

    let (type_sheets, child_sheets) = parse_metadata(&metadata_range)?;
    let mut nodes = initialize_nodes(&entities_range)?;

    for (sheet_name, type_name) in &type_sheets {
        let range = read_required_sheet(&mut workbook, sheet_name)?;
        ingest_type_sheet(&range, type_name, &mut nodes)?;
    }

    for (sheet_name, (_type_name, predicate)) in &child_sheets {
        let range = read_required_sheet(&mut workbook, sheet_name)?;
        ingest_child_sheet(&range, predicate, &mut nodes)?;
    }

    let mut nodes: Vec<Node> = nodes.into_values().collect();
    nodes.sort_by(|lhs, rhs| lhs.graph.cmp(&rhs.graph).then_with(|| lhs.id.cmp(&rhs.id)));
    Ok(nodes)
}

fn read_required_sheet<R: std::io::Read + std::io::Seek>(
    workbook: &mut Xlsx<R>,
    name: &str,
) -> Result<calamine::Range<Data>> {
    match workbook.worksheet_range(name) {
        Ok(range) => Ok(range),
        Err(calamine::XlsxError::WorksheetNotFound(_)) => Err(ToolError::InvalidWorkbook(format!(
            "missing sheet '{name}'"
        ))),
        Err(err) => Err(err.into()),
    }
}

fn parse_metadata(range: &calamine::Range<Data>) -> Result<(TypeSheetMap, ChildSheetMap)> {
    let mut type_sheets: TypeSheetMap = HashMap::new();
    let mut child_sheets: ChildSheetMap = HashMap::new();

    for row in range.rows().skip(1) {
        let kind = string_at(row, 0);
        if kind.is_empty() {
            continue;
        }
        let sheet = string_at(row, 1);
        let type_name = string_at(row, 2);
        let predicate = string_at(row, 3);

        match kind.as_str() {
            "type" => {
                type_sheets.insert(sheet, type_name);
            }
            "child" => {
                child_sheets.insert(sheet, (type_name, predicate));
            }
            other => {
                return Err(ToolError::InvalidWorkbook(format!(
                    "unknown metadata kind '{other}'"
                )));
            }
        }
    }

    Ok((type_sheets, child_sheets))
}

fn initialize_nodes(range: &calamine::Range<Data>) -> Result<BTreeMap<NodeKey, Node>> {
    let mut nodes = BTreeMap::new();

    for row in range.rows().skip(1) {
        let id = string_at(row, 0);
        if id.is_empty() {
            continue;
        }
        let type_name = string_at(row, 1);
        let node = ensure_node(&mut nodes, &id, string_at(row, 2));
        if !type_name.is_empty() && type_name != UNTYPED_MARKER {
            node.types.insert(type_name);
        }
    }

    Ok(nodes)
}

fn ingest_type_sheet(
    range: &calamine::Range<Data>,
    type_name: &str,
    nodes: &mut BTreeMap<NodeKey, Node>,
) -> Result<()> {
    let headers = read_headers(range);
    if headers.is_empty() {
        return Ok(());
    }

    for row in range.rows().skip(1) {
        let id = string_at(row, 0);
        if id.is_empty() {
            continue;
        }

        let node = ensure_node(nodes, &id, string_at(row, 1));
        if !type_name.is_empty() && type_name != UNTYPED_MARKER {
            node.types.insert(type_name.to_owned());
        }

        for (col_idx, cell) in row.iter().enumerate().skip(2) {
            let Some(header) = headers.get(col_idx) else {
                continue;
            };
            if header.is_empty() {
                continue;
            }

            let raw_value = cell_to_string(Some(cell));
            if raw_value.trim().is_empty() {
                continue;
            }

            let (predicate, property) = parse_property_entry(header, &raw_value)?;
            node.insert_property(predicate, property);
        }
    }

    Ok(())
}

fn ingest_child_sheet(
    range: &calamine::Range<Data>,
    predicate: &str,
    nodes: &mut BTreeMap<NodeKey, Node>,
) -> Result<()> {
    let header_width = range.rows().next().map(|row| row.len()).unwrap_or(0);
    let has_graph_column = header_width >= 3;

    for row in range.rows().skip(1) {
        let parent = string_at(row, 0);
        let target_index = if has_graph_column { 2 } else { 1 };
        let target = string_at(row, target_index);
        if parent.is_empty() || target.is_empty() {
            continue;
        }

        let raw_graph = if has_graph_column {
            string_at(row, 1)
        } else {
            String::new()
        };
        let node = ensure_node(nodes, &parent, raw_graph);
        let predicate_key = predicate.to_string();

        match node.properties.entry(predicate_key) {
            Entry::Occupied(mut entry) => match entry.get_mut() {
                PropertyValue::Array(ArrayValue::ObjectRefs(ids)) => {
                    ids.push(target);
                }
                _ => {
                    return Err(ToolError::InvalidWorkbook(format!(
                        "predicate '{predicate}' is not an object reference array"
                    )));
                }
            },
            Entry::Vacant(entry) => {
                entry.insert(PropertyValue::Array(ArrayValue::ObjectRefs(vec![target])));
            }
        }
    }

    Ok(())
}

/// Extracts the header row as owned strings, returning an empty collection when absent.
fn read_headers(range: &calamine::Range<Data>) -> Vec<String> {
    range
        .rows()
        .next()
        .map(|row| row.iter().map(|cell| cell_to_string(Some(cell))).collect())
        .unwrap_or_default()
}

/// Converts the cell at `index` into a `String`, returning an empty string when missing.
fn string_at(row: &[Data], index: usize) -> String {
    cell_to_string(row.get(index))
}

/// Returns the node matching `id` and `raw_graph`, normalising the graph identifier in the process.
fn ensure_node<'a>(
    nodes: &'a mut BTreeMap<NodeKey, Node>,
    id: &str,
    raw_graph: String,
) -> &'a mut Node {
    let graph = normalize_optional(raw_graph);
    let id_key = id.to_owned();
    let key = (graph.clone(), id_key.clone());
    let node = nodes
        .entry(key)
        .or_insert_with(|| Node::with_graph(id_key.clone(), graph.clone()));
    node.set_graph(graph);
    node
}

/// Converts a header/value pair coming from a type sheet row into a property entry.
fn parse_property_entry(header: &str, raw_value: &str) -> Result<(String, PropertyValue)> {
    if let Some(predicate) = header.strip_suffix("Id") {
        return Ok((
            predicate.to_string(),
            PropertyValue::ObjectRef(raw_value.to_string()),
        ));
    }

    let parsed = serde_json::from_str::<Value>(raw_value)?;
    let property = match parsed {
        Value::Array(items) => {
            let scalars = items
                .into_iter()
                .map(value_to_scalar)
                .collect::<Result<Vec<_>>>()?;
            PropertyValue::Array(ArrayValue::Scalars(scalars))
        }
        other => PropertyValue::Scalar(value_to_scalar(other)?),
    };

    Ok((header.to_string(), property))
}

fn cell_to_string(cell: Option<&Data>) -> String {
    match cell {
        Some(Data::String(value)) => value.clone(),
        Some(Data::Float(value)) => value.to_string(),
        Some(Data::Int(value)) => value.to_string(),
        Some(Data::Bool(value)) => value.to_string(),
        Some(Data::DateTime(value)) => value.to_string(),
        Some(Data::DateTimeIso(value)) => value.clone(),
        Some(Data::DurationIso(value)) => value.clone(),
        Some(Data::Error(value)) => value.to_string(),
        Some(Data::Empty) | None => String::new(),
    }
}

fn value_to_scalar(value: Value) -> Result<ScalarValue> {
    Ok(match value {
        Value::Null => ScalarValue::Null,
        Value::Bool(value) => ScalarValue::Boolean(value),
        Value::Number(number) => ScalarValue::Number(
            number
                .as_f64()
                .ok_or_else(|| ToolError::InvalidWorkbook("invalid number literal".into()))?,
        ),
        Value::String(value) => ScalarValue::String(value),
        other => ScalarValue::String(serde_json::to_string(&other)?),
    })
}
fn normalize_optional(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}
