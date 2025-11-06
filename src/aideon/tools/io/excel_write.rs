use std::path::Path;

use rust_xlsxwriter::Workbook;

use crate::aideon::tools::error::Result;
use crate::aideon::tools::flatten::WorkbookData;

/// Writes the provided workbook data to the given path.
pub fn write_workbook(path: &Path, workbook: &WorkbookData) -> Result<()> {
    let mut workbook_writer = Workbook::new();

    for table in &workbook.tables {
        let worksheet = workbook_writer.add_worksheet();
        worksheet.set_name(&table.sheet_name)?;

        for (col_idx, header) in table.columns.iter().enumerate() {
            worksheet.write_string(0, col_idx as u16, header)?;
        }

        for (row_idx, row) in table.rows.iter().enumerate() {
            for (col_idx, cell) in row.iter().enumerate() {
                worksheet.write_string((row_idx + 1) as u32, col_idx as u16, cell)?;
            }
        }

        let excel_table = rust_xlsxwriter::Table::new().set_autofilter(true);

        let col_end = (table.columns.len() as u16).saturating_sub(1);
        let row_end = if table.rows.is_empty() {
            0
        } else {
            table.rows.len() as u32
        };
        worksheet.add_table(0, 0, row_end, col_end, &excel_table)?;
    }

    workbook_writer.save(path)?;
    Ok(())
}
