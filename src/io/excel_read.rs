use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use calamine::{DataType, Reader, Xlsx, open_workbook};
use serde_json::Value;

use crate::error::{Result, ToolError};
use crate::flatten::{ENTITIES_SHEET, METADATA_SHEET, UNTYPED_MARKER};
use crate::model::{ArrayValue, Node, PropertyValue, ScalarValue};

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
    nodes.sort_by(|lhs, rhs| lhs.id.cmp(&rhs.id));
    Ok(nodes)
}

fn read_required_sheet<R: std::io::Read + std::io::Seek>(
    workbook: &mut Xlsx<R>,
    name: &str,
) -> Result<calamine::Range<DataType>> {
    let range_result = workbook
        .worksheet_range(name)
        .ok_or_else(|| ToolError::InvalidWorkbook(format!("missing sheet '{name}'")))?;
    let range = range_result.map_err(ToolError::from)?;
    Ok(range)
}

fn parse_metadata(
    range: &calamine::Range<DataType>,
) -> Result<(HashMap<String, String>, HashMap<String, (String, String)>)> {
    let mut type_sheets = HashMap::new();
    let mut child_sheets = HashMap::new();

    for row in range.rows().skip(1) {
        let kind = cell_to_string(row.get(0));
        if kind.is_empty() {
            continue;
        }
        let sheet = cell_to_string(row.get(1));
        let type_name = cell_to_string(row.get(2));
        let predicate = cell_to_string(row.get(3));

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

fn initialize_nodes(range: &calamine::Range<DataType>) -> Result<BTreeMap<String, Node>> {
    let mut nodes = BTreeMap::new();

    for row in range.rows().skip(1) {
        let id = cell_to_string(row.get(0));
        if id.is_empty() {
            continue;
        }
        let type_name = cell_to_string(row.get(1));
        let entry = nodes
            .entry(id.clone())
            .or_insert_with(|| Node::new(id.clone()));
        if !type_name.is_empty() && type_name != UNTYPED_MARKER {
            entry.types.insert(type_name);
        }
    }

    Ok(nodes)
}

fn ingest_type_sheet(
    range: &calamine::Range<DataType>,
    type_name: &str,
    nodes: &mut BTreeMap<String, Node>,
) -> Result<()> {
    let headers: Vec<String> = match range.rows().next() {
        Some(first_row) => first_row
            .iter()
            .map(|cell| cell_to_string(Some(cell)))
            .collect(),
        None => Vec::new(),
    };

    if headers.is_empty() {
        return Ok(());
    }

    for row in range.rows().skip(1) {
        let id = cell_to_string(row.get(0));
        if id.is_empty() {
            continue;
        }

        let node = nodes
            .entry(id.clone())
            .or_insert_with(|| Node::new(id.clone()));
        if !type_name.is_empty() && type_name != UNTYPED_MARKER {
            node.types.insert(type_name.to_string());
        }

        for (col_idx, cell) in row.iter().enumerate().skip(1) {
            let header = headers.get(col_idx).cloned().unwrap_or_default();
            if header.is_empty() {
                continue;
            }

            let value = cell_to_string(Some(cell));
            if value.trim().is_empty() {
                continue;
            }

            if header.ends_with("Id") {
                let predicate = header.strip_suffix("Id").unwrap().to_string();
                node.properties
                    .insert(predicate, PropertyValue::ObjectRef(value.clone()));
            } else {
                let parsed = serde_json::from_str::<Value>(&value)?;
                match parsed {
                    Value::Array(items) => {
                        let scalars = items
                            .into_iter()
                            .map(value_to_scalar)
                            .collect::<Result<Vec<_>>>()?;
                        node.properties.insert(
                            header.clone(),
                            PropertyValue::Array(ArrayValue::Scalars(scalars)),
                        );
                    }
                    other => {
                        let scalar = value_to_scalar(other)?;
                        node.properties
                            .insert(header.clone(), PropertyValue::Scalar(scalar));
                    }
                }
            }
        }
    }

    Ok(())
}

fn ingest_child_sheet(
    range: &calamine::Range<DataType>,
    predicate: &str,
    nodes: &mut BTreeMap<String, Node>,
) -> Result<()> {
    for row in range.rows().skip(1) {
        let parent = cell_to_string(row.get(0));
        let target = cell_to_string(row.get(1));
        if parent.is_empty() || target.is_empty() {
            continue;
        }

        let node = nodes
            .entry(parent.clone())
            .or_insert_with(|| Node::new(parent.clone()));
        match node.properties.get_mut(predicate) {
            Some(PropertyValue::Array(ArrayValue::ObjectRefs(ids))) => {
                ids.push(target.clone());
            }
            Some(_) => {
                return Err(ToolError::InvalidWorkbook(format!(
                    "predicate '{predicate}' is not an object reference array"
                )));
            }
            None => {
                node.properties.insert(
                    predicate.to_string(),
                    PropertyValue::Array(ArrayValue::ObjectRefs(vec![target.clone()])),
                );
            }
        }
    }

    Ok(())
}

fn cell_to_string(cell: Option<&DataType>) -> String {
    match cell {
        Some(DataType::String(value)) => value.clone(),
        Some(DataType::Float(value)) => value.to_string(),
        Some(DataType::Int(value)) => value.to_string(),
        Some(DataType::Bool(value)) => value.to_string(),
        Some(DataType::Empty) | None => String::new(),
        Some(other) => other.to_string(),
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
