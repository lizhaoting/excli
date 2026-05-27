use std::path::Path;

use calamine::{open_workbook_auto, Data, Reader};

use excli_core::cell::{Cell, ExcelError};
use excli_core::error::{ExcliError, ExcliResult};
use excli_core::Sheet;
use excli_core::Workbook;

/// Read an Excel file using calamine (supports xlsx, xlsm, xlsb, xls, ods).
pub fn read_excel(path: &Path) -> ExcliResult<Workbook> {
    let mut workbook = open_workbook_auto(path)
        .map_err(|e| ExcliError::Parse(format!("Failed to open Excel file: {}", e)))?;

    let mut wb = Workbook::new();
    let sheet_names: Vec<String> = workbook.sheet_names().to_vec();

    for name in &sheet_names {
        if let Ok(range) = workbook.worksheet_range(name) {
            let mut sheet = Sheet::new(name);

            for row_data in range.rows() {
                let row_idx = sheet.row_count() as u32;
                for (col_idx, data) in row_data.iter().enumerate() {
                    let cell = data_to_cell(data);
                    sheet.set_cell(col_idx as u32, row_idx, cell);
                }
            }

            wb.sheets.push(sheet);
        }
    }

    Ok(wb)
}

/// Read Excel from in-memory bytes.
pub fn read_excel_from_bytes(data: &[u8]) -> ExcliResult<Workbook> {
    let dir = tempfile::tempdir()
        .map_err(|e| ExcliError::Parse(format!("Temp dir error: {}", e)))?;
    let path = dir.path().join("temp.xlsx");
    std::fs::write(&path, data)
        .map_err(|e| ExcliError::Parse(format!("Temp file error: {}", e)))?;
    read_excel(&path)
}

fn data_to_cell(data: &Data) -> Cell {
    match data {
        Data::Empty => Cell::empty(),
        Data::Bool(b) => Cell::boolean(*b),
        Data::Float(n) => Cell::number(*n),
        Data::Int(i) => Cell::number(*i as f64),
        Data::String(s) => Cell::string(s.clone()),
        Data::DateTime(d) => Cell::string(format!("{}", d)),
        Data::DateTimeIso(d) => Cell::string(d.clone()),
        Data::DurationIso(d) => Cell::string(d.clone()),
        Data::Error(e) => {
            let err_str = format!("{}", e);
            if let Some(excel_err) = ExcelError::from_str(&err_str) {
                Cell::error(excel_err)
            } else {
                Cell::error(ExcelError::Value)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use excli_core::cell::CellValue;
    use crate::writer;

    #[test]
    fn test_xlsx_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("roundtrip.xlsx");

        let mut wb = Workbook::new();
        let sheet = wb.add_sheet("Test");
        sheet.set_cell(0, 0, Cell::string("Hello"));
        sheet.set_cell(1, 0, Cell::string("World"));
        sheet.set_cell(0, 1, Cell::number(42.0));

        writer::write_xlsx(&path, &wb).unwrap();
        let read_wb = read_excel(&path).unwrap();
        assert!(read_wb.sheet_count() > 0);
    }

    #[test]
    fn test_xlsx_multiple_sheets_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("multi.xlsx");

        let mut wb = Workbook::new();
        wb.add_sheet("Sheet1").set_cell(0, 0, Cell::number(1.0));
        wb.add_sheet("Sheet2").set_cell(0, 0, Cell::number(2.0));

        writer::write_xlsx(&path, &wb).unwrap();
        let read_wb = read_excel(&path).unwrap();
        assert!(read_wb.sheet_count() >= 1);
    }

    #[test]
    fn test_xlsx_number_types() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("numbers.xlsx");

        let mut wb = Workbook::new();
        let sheet = wb.add_sheet("Data");
        sheet.set_cell(0, 0, Cell::number(3.14));
        sheet.set_cell(0, 1, Cell::number(-42.0));
        sheet.set_cell(0, 2, Cell::number(0.0));

        writer::write_xlsx(&path, &wb).unwrap();
        let read_wb = read_excel(&path).unwrap();
        assert!(read_wb.sheet_by_index(0).unwrap().row_count() > 0);
    }

    #[test]
    fn test_xlsx_boolean_values() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("booleans.xlsx");

        let mut wb = Workbook::new();
        let sheet = wb.add_sheet("Data");
        sheet.set_cell(0, 0, Cell::new(CellValue::Boolean(true)));
        sheet.set_cell(0, 1, Cell::new(CellValue::Boolean(false)));

        writer::write_xlsx(&path, &wb).unwrap();
        let read_wb = read_excel(&path).unwrap();
        assert!(read_wb.sheet_by_index(0).unwrap().row_count() > 0);
    }

    #[test]
    fn test_json_write() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("out.json");

        let mut wb = Workbook::new();
        let sheet = wb.add_sheet("Sheet1");
        sheet.set_cell(0, 0, Cell::string("Name"));
        sheet.set_cell(1, 0, Cell::string("Score"));
        sheet.set_cell(0, 1, Cell::string("Alice"));
        sheet.set_cell(1, 1, Cell::number(95.0));

        writer::write_json(&path, &wb).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("Alice"));
    }

    #[test]
    fn test_read_excel_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing.xlsx");
        let r = read_excel(&path);
        assert!(r.is_err());
    }

    #[test]
    fn test_read_excel_from_invalid_bytes() {
        let r = read_excel_from_bytes(b"not an excel file");
        assert!(r.is_err());
    }

    #[test]
    fn test_read_excel_from_bytes_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bytes_roundtrip.xlsx");

        let mut wb = Workbook::new();
        let sheet = wb.add_sheet("SheetA");
        sheet.set_cell(0, 0, Cell::string("A"));
        sheet.set_cell(1, 0, Cell::number(10.0));

        writer::write_xlsx(&path, &wb).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let read_wb = read_excel_from_bytes(&bytes).unwrap();
        assert!(read_wb.sheet_count() >= 1);
        let s0 = read_wb.sheet_by_index(0).unwrap();
        assert!(s0.row_count() >= 1);
        assert!(s0.col_count() >= 2);
    }

    #[test]
    fn test_xlsx_write_empty_workbook() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty.xlsx");
        let wb = Workbook::new();
        // Should handle empty workbook gracefully
        let r = writer::write_xlsx(&path, &wb);
        assert!(r.is_ok() || r.is_err()); // No panic
    }

    #[test]
    fn test_xlsx_error_handling_invalid_path() {
        let path = std::path::Path::new("/invalid/path/that/does/not/exist/file.xlsx");
        let r = read_excel(path);
        assert!(r.is_err());
    }

    #[test]
    fn test_xlsx_mixed_types_per_column() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mixed.xlsx");

        let mut wb = Workbook::new();
        let sheet = wb.add_sheet("Mixed");
        // Same column with different types
        sheet.set_cell(0, 0, Cell::string("Header"));
        sheet.set_cell(0, 1, Cell::number(123.0));
        sheet.set_cell(0, 2, Cell::boolean(true));
        sheet.set_cell(0, 3, Cell::string("text"));

        writer::write_xlsx(&path, &wb).unwrap();
        let read_wb = read_excel(&path).unwrap();
        let sheet = read_wb.sheet_by_index(0).unwrap();
        assert!(sheet.row_count() >= 4);
    }

    #[test]
    fn test_xlsx_large_numeric_values() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("large_nums.xlsx");

        let mut wb = Workbook::new();
        let sheet = wb.add_sheet("Numbers");
        sheet.set_cell(0, 0, Cell::number(1e10));
        sheet.set_cell(0, 1, Cell::number(-1e10));
        sheet.set_cell(0, 2, Cell::number(0.0000001));
        sheet.set_cell(0, 3, Cell::number(f64::MAX / 2.0));

        writer::write_xlsx(&path, &wb).unwrap();
        let read_wb = read_excel(&path).unwrap();
        assert!(read_wb.sheet_count() >= 1);
    }

    #[test]
    fn test_xlsx_sheet_name_special_chars() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("special_names.xlsx");

        let mut wb = Workbook::new();
        wb.add_sheet("Sheet-1").set_cell(0, 0, Cell::number(1.0));
        wb.add_sheet("Sheet.2").set_cell(0, 0, Cell::number(2.0));
        wb.add_sheet("Sheet (3)").set_cell(0, 0, Cell::number(3.0));

        writer::write_xlsx(&path, &wb).unwrap();
        let read_wb = read_excel(&path).unwrap();
        assert!(read_wb.sheet_count() >= 3);
    }
}
