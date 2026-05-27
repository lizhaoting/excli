#[allow(unused_imports)]
use excli_core::cell::Cell;
use excli_core::{Sheet, Workbook};

/// Axis for concatenation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConcatAxis {
    Rows,
    Cols,
}

/// Transpose a sheet (swap rows and columns).
pub fn transpose(sheet: &Sheet) -> Sheet {
    let mut result = Sheet::new(format!("{}_transposed", sheet.name));

    let row_count = sheet.row_count();
    let col_count = sheet.col_count();

    for row_idx in 0..row_count {
        for col_idx in 0..col_count {
            if let Some(cell) = sheet.get_cell(col_idx as u32, row_idx as u32) {
                result.set_cell(row_idx as u32, col_idx as u32, cell.clone());
            }
        }
    }

    result
}

/// Concatenate multiple workbooks.
pub fn concat_workbooks(workbooks: &[&Workbook], axis: ConcatAxis) -> Workbook {
    match axis {
        ConcatAxis::Rows => concat_rows(workbooks),
        ConcatAxis::Cols => concat_cols(workbooks),
    }
}

fn concat_rows(workbooks: &[&Workbook]) -> Workbook {
    let mut result = Workbook::new();
    let result_sheet = result.add_sheet("Concatenated");

    let mut current_row = 0u32;

    for wb in workbooks {
        if let Some(sheet) = wb.sheet_by_index(0) {
            for row in &sheet.rows {
                for (col_idx, cell) in row.cells.iter().enumerate() {
                    result_sheet.set_cell(col_idx as u32, current_row, cell.clone());
                }
                current_row += 1;
            }
        }
    }

    result
}

fn concat_cols(workbooks: &[&Workbook]) -> Workbook {
    let mut result = Workbook::new();
    let result_sheet = result.add_sheet("Concatenated");

    let mut current_col = 0u32;
    let mut max_rows = 0u32;

    for wb in workbooks {
        if let Some(sheet) = wb.sheet_by_index(0) {
            for (row_idx, row) in sheet.rows.iter().enumerate() {
                for (col_idx, cell) in row.cells.iter().enumerate() {
                    result_sheet.set_cell(current_col + col_idx as u32, row_idx as u32, cell.clone());
                }
            }
            max_rows = max_rows.max(sheet.row_count() as u32);
            current_col += sheet.col_count() as u32;
        }
    }

    result
}

/// Split a sheet by column values.
pub fn split_by_column(sheet: &Sheet, column: &str, _output_dir: &std::path::Path) -> Vec<(String, Sheet)> {
    let col_idx = find_column_index(sheet, column).unwrap_or(0);
    let mut groups: std::collections::HashMap<String, Sheet> = std::collections::HashMap::new();

    // First row is usually header
    let _has_header = sheet.rows.first().is_some();

    for row in &sheet.rows {
        let key = row
            .cells
            .get(col_idx as usize)
            .map(|c| c.value.to_string())
            .unwrap_or_else(|| "unknown".to_string());

        let group_sheet = groups.entry(key.clone()).or_insert_with(|| {
            let mut s = Sheet::new(&key);
            // Copy header if present
            if let Some(header_row) = sheet.rows.first() {
                for (col_idx, cell) in header_row.cells.iter().enumerate() {
                    s.set_cell(col_idx as u32, 0, cell.clone());
                }
            }
            s
        });

        let row_idx = group_sheet.row_count() as u32;
        for (col_idx, cell) in row.cells.iter().enumerate() {
            group_sheet.set_cell(col_idx as u32, row_idx, cell.clone());
        }
    }

    groups.into_iter().collect()
}

/// Split a sheet by row count.
pub fn split_by_rows(sheet: &Sheet, rows_per_chunk: usize) -> Vec<Sheet> {
    let mut chunks = Vec::new();
    let header_row = sheet.rows.first().cloned();

    let data_rows: Vec<_> = sheet.rows.iter().skip(1).collect();
    for chunk in data_rows.chunks(rows_per_chunk) {
        let mut chunk_sheet = Sheet::new(format!("{}_chunk_{}", sheet.name, chunks.len() + 1));

        if let Some(ref header) = header_row {
            for (col_idx, cell) in header.cells.iter().enumerate() {
                chunk_sheet.set_cell(col_idx as u32, 0, cell.clone());
            }
        }

        for (row_offset, row) in chunk.iter().enumerate() {
            let row_idx = (row_offset + 1) as u32;
            for (col_idx, cell) in row.cells.iter().enumerate() {
                chunk_sheet.set_cell(col_idx as u32, row_idx, cell.clone());
            }
        }

        chunks.push(chunk_sheet);
    }

    chunks
}

fn find_column_index(sheet: &Sheet, col_ref: &str) -> Option<u32> {
    if let Some(idx) = excli_core::reference::column_name_to_index(col_ref) {
        return Some(idx);
    }
    if let Some(header_row) = sheet.rows.first() {
        for (idx, cell) in header_row.cells.iter().enumerate() {
            if cell.value.to_string().eq_ignore_ascii_case(col_ref) {
                return Some(idx as u32);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transpose() {
        let mut sheet = Sheet::new("Test");
        sheet.set_cell(0, 0, Cell::string("A"));
        sheet.set_cell(1, 0, Cell::string("B"));
        sheet.set_cell(0, 1, Cell::string("1"));
        sheet.set_cell(1, 1, Cell::string("2"));

        let result = transpose(&sheet);
        assert_eq!(result.row_count(), 2);
        assert_eq!(result.col_count(), 2);
        // (0,0) was "A", now at (0,0) row/col swapped
        // A at original (col=0,row=0) -> transposed (col=0,row=0)
        // B at original (col=1,row=0) -> transposed (col=0,row=1)
        assert_eq!(result.get_cell(0, 1).unwrap().value.as_string().unwrap(), "B");
    }

    #[test]
    fn test_concat_rows() {
        let mut wb1 = Workbook::new();
        wb1.add_sheet("A").set_cell(0, 0, Cell::string("row1"));
        let mut wb2 = Workbook::new();
        wb2.add_sheet("B").set_cell(0, 0, Cell::string("row2"));

        let result = concat_workbooks(&[&wb1, &wb2], ConcatAxis::Rows);
        let sheet = result.sheet_by_index(0).unwrap();
        assert_eq!(sheet.row_count(), 2);
    }

    #[test]
    fn test_split_by_rows() {
        let mut sheet = Sheet::new("Test");
        sheet.set_cell(0, 0, Cell::string("Header"));
        for i in 0..5 {
            sheet.set_cell(0, (i + 1) as u32, Cell::string(format!("Row{}", i)));
        }

        let chunks = split_by_rows(&sheet, 2);
        assert_eq!(chunks.len(), 3); // ceil(5/2) = 3
    }

    #[test]
    fn test_concat_columns() {
        let mut wb1 = Workbook::new();
        wb1.add_sheet("A").set_cell(0, 0, Cell::string("col1"));
        let mut wb2 = Workbook::new();
        wb2.add_sheet("B").set_cell(0, 0, Cell::string("col2"));

        let result = concat_workbooks(&[&wb1, &wb2], ConcatAxis::Cols);
        let sheet = result.sheet_by_index(0).unwrap();
        assert_eq!(sheet.col_count(), 2);
    }

    #[test]
    fn test_transpose_empty() {
        let sheet = Sheet::new("Empty");
        let result = transpose(&sheet);
        assert_eq!(result.row_count(), 0);
    }

    #[test]
    fn test_split_by_rows_exact_multiple() {
        let mut sheet = Sheet::new("Test");
        for i in 0..4 {
            sheet.set_cell(0, i as u32, Cell::string(format!("Row{}", i)));
        }
        let chunks = split_by_rows(&sheet, 2);
        assert_eq!(chunks.len(), 2);
    }

    #[test]
    fn test_split_by_column() {
        use std::path::Path;
        let mut sheet = Sheet::new("Test");
        sheet.set_cell(0, 0, Cell::string("Cat"));
        sheet.set_cell(1, 0, Cell::string("A"));
        sheet.set_cell(0, 1, Cell::string("Cat"));
        sheet.set_cell(1, 1, Cell::string("B"));
        sheet.set_cell(0, 2, Cell::string("Dog"));
        sheet.set_cell(1, 2, Cell::string("C"));

        let result = split_by_column(&sheet, "A", Path::new("/tmp"));
        // Should split into 2 groups: Cat, Dog
        assert_eq!(result.len(), 2);
    }
}
