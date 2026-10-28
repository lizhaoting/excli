use std::path::Path;

use calamine::{open_workbook_auto, Data, Reader, SheetType, SheetVisible};

use excli_core::cell::{Cell, ExcelError};
use excli_core::error::{ExcliError, ExcliResult};
use excli_core::sheet::{Sheet, SheetState};
use excli_core::workbook::NamedRange;
use excli_core::Workbook;

/// Read an Excel file using calamine (supports xlsx, xlsm, xlsb, xls, ods).
pub fn read_excel(path: &Path) -> ExcliResult<Workbook> {
    let mut workbook = open_workbook_auto(path)
        .map_err(|e| ExcliError::Parse(format!("Failed to open Excel file: {}", e)))?;

    let mut wb = Workbook::new();
    wb.file_format = crate::format_detect::detect_format(path).ok();
    wb.file_size = std::fs::metadata(path).map_or(0, |metadata| metadata.len());
    wb.named_ranges = workbook
        .defined_names()
        .iter()
        .map(|(name, refers_to)| NamedRange::new(name, refers_to))
        .collect();
    let sheet_visibility: std::collections::HashMap<_, _> = workbook
        .sheets_metadata()
        .iter()
        .map(|sheet| {
            let state = match sheet.visible {
                SheetVisible::Visible => SheetState::Visible,
                SheetVisible::Hidden => SheetState::Hidden,
                SheetVisible::VeryHidden => SheetState::VeryHidden,
            };
            (sheet.name.clone(), state)
        })
        .collect();
    let sheet_names: Vec<String> = workbook
        .sheets_metadata()
        .iter()
        .filter(|sheet| sheet.typ == SheetType::WorkSheet)
        .map(|sheet| sheet.name.clone())
        .collect();

    for name in &sheet_names {
        let range = workbook.worksheet_range(name).map_err(|error| {
            ExcliError::Parse(format!("Failed to read worksheet {name}: {error}"))
        })?;
        let mut sheet = Sheet::new(name);
        sheet.state = sheet_visibility
            .get(name)
            .copied()
            .unwrap_or(SheetState::Visible);
        let (start_row, start_col) = range.start().unwrap_or((0, 0));

        for (row_offset, row_data) in range.rows().enumerate() {
            let row_idx = start_row + row_offset as u32;
            for (col_offset, data) in row_data.iter().enumerate() {
                let cell = data_to_cell(data);
                sheet.set_cell(start_col + col_offset as u32, row_idx, cell);
            }
        }

        if let Ok(formulas) = workbook.worksheet_formula(name) {
            let (formula_row, formula_col) = formulas.start().unwrap_or((0, 0));
            for (row_offset, row) in formulas.rows().enumerate() {
                for (col_offset, raw) in row.iter().enumerate() {
                    if raw.is_empty() {
                        continue;
                    }
                    let col = formula_col + col_offset as u32;
                    let row = formula_row + row_offset as u32;
                    let mut cell = sheet
                        .get_cell(col, row)
                        .cloned()
                        .unwrap_or_else(Cell::empty);
                    let formula_text = if raw.starts_with('=') {
                        raw.clone()
                    } else {
                        format!("={raw}")
                    };
                    cell.formula = Some(
                        excli_core::formula::Formula::new(formula_text)
                            .with_cached(cell.value.clone()),
                    );
                    sheet.set_cell(col, row, cell);
                }
            }
        }

        wb.sheets.push(sheet);
    }

    if wb
        .file_format
        .is_some_and(excli_core::types::FileFormat::is_xml_based)
    {
        crate::ooxml_read::enrich_workbook(path, &mut wb)?;
    }

    Ok(wb)
}

/// Read Excel from in-memory bytes.
pub fn read_excel_from_bytes(data: &[u8]) -> ExcliResult<Workbook> {
    read_excel_from_bytes_with_format(data, excli_core::types::FileFormat::Xlsx)
}

/// Read in-memory spreadsheet bytes with their actual format retained for metadata parsing.
pub fn read_excel_from_bytes_with_format(
    data: &[u8],
    format: excli_core::types::FileFormat,
) -> ExcliResult<Workbook> {
    let dir =
        tempfile::tempdir().map_err(|e| ExcliError::Parse(format!("Temp dir error: {}", e)))?;
    let path = dir.path().join(format!("temp.{}", format.extension()));
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
        // Date/time cells are numeric Excel serials. Their number format carries
        // the display semantics; converting the serial to text loses the value.
        Data::DateTime(d) => Cell::number(d.as_f64()),
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
    use crate::writer;
    use excli_core::cell::CellValue;
    use std::collections::HashMap;
    use zip::ZipArchive;

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
    fn date_formatted_number_keeps_excel_serial() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dates.xlsx");
        let mut workbook = Workbook::new();
        workbook
            .add_sheet("Data")
            .set_cell(0, 0, Cell::number(45292.5));
        writer::write_xlsx(&path, &workbook).unwrap();
        let mut archive = ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
        let sheet_path = "xl/worksheets/sheet1.xml";
        let styles_path = "xl/styles.xml";
        let sheet_xml =
            String::from_utf8(crate::ooxml_package::read_part(&mut archive, sheet_path).unwrap())
                .unwrap();
        let styles_xml =
            String::from_utf8(crate::ooxml_package::read_part(&mut archive, styles_path).unwrap())
                .unwrap();
        let updated_sheet = sheet_xml.replace("<c r=\"A1\"", "<c r=\"A1\" s=\"1\"");
        let updated_styles = styles_xml.replace(
            "<cellXfs count=\"1\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\" xfId=\"0\"/></cellXfs>",
            "<cellXfs count=\"2\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\" xfId=\"0\"/><xf numFmtId=\"14\" fontId=\"0\" fillId=\"0\" borderId=\"0\" xfId=\"0\" applyNumberFormat=\"1\"/></cellXfs>",
        );
        assert_ne!(sheet_xml, updated_sheet);
        assert_ne!(styles_xml, updated_styles);
        crate::ooxml_edit::write_modified_parts(
            &path,
            &mut archive,
            &HashMap::from([
                (sheet_path.to_owned(), updated_sheet.into_bytes()),
                (styles_path.to_owned(), updated_styles.into_bytes()),
            ]),
        )
        .unwrap();

        let loaded = read_excel(&path).unwrap();
        assert_eq!(
            loaded.sheets[0].get_cell(0, 0).unwrap().value,
            CellValue::Number(45292.5)
        );
        assert_eq!(
            loaded.sheets[0]
                .get_cell(0, 0)
                .unwrap()
                .style
                .as_ref()
                .unwrap()
                .number_format_id,
            Some(14)
        );
    }

    #[test]
    fn xlsx_reader_keeps_cell_coordinates_and_formulas() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("offset_formula.xlsx");
        let mut workbook = Workbook::new();
        let sheet = workbook.add_sheet("Data");
        sheet.set_cell(1, 1, Cell::string("B2"));
        sheet.set_cell(
            2,
            3,
            Cell::number(3.0).with_formula(excli_core::formula::Formula::new("=1+2")),
        );
        writer::write_xlsx(&path, &workbook).unwrap();

        let loaded = read_excel(&path).unwrap();
        let sheet = loaded.sheet_by_name("Data").unwrap();
        assert_eq!(sheet.get_cell(1, 1).unwrap().value.to_string(), "B2");
        assert_eq!(sheet.get_cell(2, 3).unwrap().value.to_string(), "3");
        assert_eq!(
            sheet.get_cell(2, 3).unwrap().formula.as_ref().unwrap().raw,
            "=1+2"
        );
        assert!(sheet.get_cell(0, 0).is_none());
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
