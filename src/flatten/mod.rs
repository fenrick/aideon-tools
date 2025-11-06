use std::collections::{BTreeMap, BTreeSet, HashSet};

use serde_json::Value;

use crate::error::Result;
use crate::model::{ArrayValue, Node, PropertyValue, ScalarValue};

/// Name used for nodes that do not declare a type.
pub const UNTYPED_MARKER: &str = "__untyped__";
/// Sheet name storing the entity → type index.
pub const ENTITIES_SHEET: &str = "Entities";
/// Sheet name storing metadata such as sheet → type mappings.
pub const METADATA_SHEET: &str = "Metadata";

/// A table that will be materialised as an Excel sheet.
#[derive(Debug, Clone, PartialEq)]
pub struct SheetTable {
    pub sheet_name: String,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

/// Represents all tables required to materialise the Excel workbook.
#[derive(Debug, Clone, PartialEq)]
pub struct WorkbookData {
    pub tables: Vec<SheetTable>,
}

/// Flattens the provided nodes into a set of tables following the spreadsheet
/// conventions described in the project documentation.
pub fn build_workbook(nodes: &[Node]) -> Result<WorkbookData> {
    let mut sheet_names = SheetNameRegistry::default();

    let mut type_builders: BTreeMap<String, TypeTableBuilder> = BTreeMap::new();
    let mut child_builders: BTreeMap<(String, String), ChildTableBuilder> = BTreeMap::new();
    let mut entities: Vec<(String, String)> = Vec::new();

    for node in nodes {
        let node_types: Vec<String> = if node.types.is_empty() {
            vec![UNTYPED_MARKER.to_string()]
        } else {
            node.types.iter().cloned().collect()
        };

        for (type_index, type_name) in node_types.iter().enumerate() {
            entities.push((node.id.clone(), type_name.clone()));

            let builder = type_builders
                .entry(type_name.clone())
                .or_insert_with(TypeTableBuilder::new);

            let mut row_values: BTreeMap<String, String> = BTreeMap::new();

            for (predicate, value) in &node.properties {
                match value {
                    PropertyValue::Scalar(scalar) => {
                        builder.columns.insert(predicate.clone());
                        row_values.insert(predicate.clone(), scalar_to_cell_value(scalar)?);
                    }
                    PropertyValue::ObjectRef(target) => {
                        let column_name = format!("{predicate}Id");
                        builder.columns.insert(column_name.clone());
                        row_values.insert(column_name, target.clone());
                    }
                    PropertyValue::Array(ArrayValue::Scalars(items)) => {
                        builder.columns.insert(predicate.clone());
                        let json_items: Vec<Value> =
                            items.iter().map(ScalarValue::to_json).collect();
                        let json_string = serde_json::to_string(&Value::Array(json_items))?;
                        row_values.insert(predicate.clone(), json_string);
                    }
                    PropertyValue::Array(ArrayValue::ObjectRefs(targets)) => {
                        if type_index == 0 {
                            let child_builder = child_builders
                                .entry((type_name.clone(), predicate.clone()))
                                .or_insert_with(|| ChildTableBuilder::new(predicate.clone()));
                            for target in targets {
                                child_builder.rows.push((node.id.clone(), target.clone()));
                            }
                        }
                    }
                }
            }

            builder.rows.push(RowData {
                id: node.id.clone(),
                values: row_values,
            });
        }
    }

    entities.sort_by(|lhs, rhs| lhs.cmp(rhs));

    let mut tables: Vec<SheetTable> = Vec::new();
    let mut metadata_rows: Vec<Vec<String>> = Vec::new();

    // Reserve names for Entities and Metadata to avoid collisions.
    sheet_names.claim(ENTITIES_SHEET.to_string());
    sheet_names.claim(METADATA_SHEET.to_string());

    for (type_name, mut builder) in type_builders {
        builder.rows.sort_by(|lhs, rhs| lhs.id.cmp(&rhs.id));
        let sheet_name = sheet_names.assign(&type_name);

        metadata_rows.push(vec![
            "type".to_string(),
            sheet_name.clone(),
            type_name.clone(),
            String::new(),
        ]);

        tables.push(builder.into_table(sheet_name));
    }

    for ((type_name, predicate), mut builder) in child_builders {
        builder.rows.sort_by(|lhs, rhs| lhs.cmp(rhs));
        let raw_sheet = format!("{type_name}__{predicate}");
        let sheet_name = sheet_names.assign(&raw_sheet);

        metadata_rows.push(vec![
            "child".to_string(),
            sheet_name.clone(),
            type_name,
            predicate,
        ]);

        tables.push(builder.into_table(sheet_name));
    }

    tables.sort_by(|lhs, rhs| lhs.sheet_name.cmp(&rhs.sheet_name));

    let entities_table = build_entities_table(entities);
    let metadata_table = SheetTable {
        sheet_name: METADATA_SHEET.to_string(),
        columns: vec![
            "kind".to_string(),
            "sheet".to_string(),
            "type".to_string(),
            "predicate".to_string(),
        ],
        rows: metadata_rows,
    };

    let mut all_tables = vec![entities_table, metadata_table];
    all_tables.extend(tables);

    Ok(WorkbookData { tables: all_tables })
}

fn build_entities_table(entries: Vec<(String, String)>) -> SheetTable {
    let rows = entries
        .into_iter()
        .map(|(id, type_name)| vec![id, type_name])
        .collect();

    SheetTable {
        sheet_name: ENTITIES_SHEET.to_string(),
        columns: vec!["id".to_string(), "type".to_string()],
        rows,
    }
}

#[derive(Debug, Default)]
struct SheetNameRegistry {
    used: HashSet<String>,
}

impl SheetNameRegistry {
    fn claim(&mut self, name: String) {
        self.used.insert(name);
    }

    fn assign(&mut self, raw: &str) -> String {
        let base = sanitize_sheet_name(raw);
        if !self.used.contains(&base) {
            self.used.insert(base.clone());
            return base;
        }

        let mut counter = 1;
        loop {
            let suffix = format!("_{counter}");
            let max_len = 31 - suffix.len();
            let mut prefix = base.clone();
            if prefix.len() > max_len {
                prefix.truncate(max_len);
            }
            let candidate = format!("{prefix}{suffix}");
            if !self.used.contains(&candidate) {
                self.used.insert(candidate.clone());
                return candidate;
            }
            counter += 1;
        }
    }
}

fn sanitize_sheet_name(raw: &str) -> String {
    let invalid = [':', '\\', '/', '?', '*', '[', ']', '\'', '"'];
    let mut sanitized: String = raw
        .chars()
        .map(|ch| {
            if invalid.contains(&ch) || ch.is_control() {
                '_'
            } else {
                ch
            }
        })
        .collect();

    sanitized = sanitized.trim().to_string();
    if sanitized.is_empty() {
        sanitized = "Sheet".to_string();
    }

    if sanitized.len() > 31 {
        sanitized.truncate(31);
    }

    sanitized
}

struct TypeTableBuilder {
    columns: BTreeSet<String>,
    rows: Vec<RowData>,
}

impl TypeTableBuilder {
    fn new() -> Self {
        Self {
            columns: BTreeSet::new(),
            rows: Vec::new(),
        }
    }

    fn into_table(self, sheet_name: String) -> SheetTable {
        let mut columns = Vec::with_capacity(self.columns.len() + 1);
        columns.push("id".to_string());
        columns.extend(self.columns.into_iter());

        let mut rows = Vec::with_capacity(self.rows.len());
        for row in self.rows {
            let mut cells = Vec::with_capacity(columns.len());
            cells.push(row.id);
            for column in columns.iter().skip(1) {
                cells.push(row.values.get(column).cloned().unwrap_or_default());
            }
            rows.push(cells);
        }

        SheetTable {
            sheet_name,
            columns,
            rows,
        }
    }
}

struct RowData {
    id: String,
    values: BTreeMap<String, String>,
}

struct ChildTableBuilder {
    predicate: String,
    rows: Vec<(String, String)>,
}

impl ChildTableBuilder {
    fn new(predicate: String) -> Self {
        Self {
            predicate,
            rows: Vec::new(),
        }
    }

    fn into_table(self, sheet_name: String) -> SheetTable {
        let column_name = format!("{}Id", self.predicate);
        let rows = self
            .rows
            .into_iter()
            .map(|(parent, target)| vec![parent, target])
            .collect();

        SheetTable {
            sheet_name,
            columns: vec!["ParentId".to_string(), column_name],
            rows,
        }
    }
}

fn scalar_to_cell_value(value: &ScalarValue) -> Result<String> {
    let json_value = value.to_json();
    Ok(serde_json::to_string(&json_value)?)
}
