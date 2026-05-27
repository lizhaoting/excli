use std::io::Write;
use std::path::Path;

use excli_core::error::{ExcliError, ExcliResult, ErrorCode};
use excli_core::types::FileFormat;
use excli_core::Workbook;
use excli_core::cell::CellValue;
use excli_core::utils::csv_escape;

use crate::format_detect::detect_format;

/// Write a workbook to a file.
pub fn write_workbook(path: &Path, wb: &Workbook) -> ExcliResult<()> {
    let format = detect_format(path)?;
    write_workbook_with_format(path, wb, format)
}

/// Write a workbook with explicit format.
pub fn write_workbook_with_format(path: &Path, wb: &Workbook, format: FileFormat) -> ExcliResult<()> {
    match format {
        FileFormat::Csv | FileFormat::Tsv => {
            let delimiter = if format == FileFormat::Tsv { b'\t' } else { b',' };
            write_csv(path, wb, delimiter)
        }
        FileFormat::Xlsx | FileFormat::Xlsm => {
            write_xlsx(path, wb)
        }
        _ => {
            if format == FileFormat::Json {
                write_json(path, wb)
            } else {
                write_xlsx(path, wb)
            }
        }
    }
}

/// Write workbook as CSV.
pub fn write_csv(path: &Path, wb: &Workbook, delimiter: u8) -> ExcliResult<()> {
    let mut file = std::fs::File::create(path)?;

    for sheet in &wb.sheets {
        for row in &sheet.rows {
            let cells: Vec<String> = row
                .cells
                .iter()
                .map(|c| match &c.value {
                    CellValue::Empty => String::new(),
                    CellValue::InlineString(s) => csv_escape(s),
                    CellValue::Number(n) => n.to_string(),
                    CellValue::Boolean(b) => b.to_string(),
                    CellValue::Error(e) => e.to_string(),
                    _ => format!("{}", c.value),
                })
                .collect();
            let line = cells.join(&(delimiter as char).to_string());
            writeln!(file, "{}", line)?;
        }
    }
    Ok(())
}

/// Simple XML string escaping.
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Write workbook as xlsx (simplified single-sheet writer).
pub fn write_xlsx(path: &Path, wb: &Workbook) -> ExcliResult<()> {
    use std::io::BufWriter;
    use zip::write::SimpleFileOptions;

    let file = std::fs::File::create(path)?;
    let writer = BufWriter::new(file);
    let mut zip = zip::ZipWriter::new(writer);

    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    // Collect shared strings
    let mut strings: Vec<String> = Vec::new();
    for sheet in &wb.sheets {
        for row in &sheet.rows {
            for cell in &row.cells {
                if let CellValue::InlineString(s) = &cell.value {
                    if !strings.contains(s) {
                        strings.push(s.clone());
                    }
                }
            }
        }
    }

    macro_rules! zip_write {
        ($zip:expr, $name:expr, $data:expr, $opts:expr) => {{
            $zip.start_file($name, $opts)
                .map_err(|e| ExcliError::new(ErrorCode::WriteError, format!("Zip error: {}", e)))?;
            $zip.write_all($data.as_bytes())
                .map_err(|e| ExcliError::new(ErrorCode::WriteError, format!("Write error: {}", e)))?;
        }};
    }

    // [Content_Types].xml
    let mut ct = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
         <Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">\
         <Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>\
         <Default Extension=\"xml\" ContentType=\"application/xml\"/>\
         <Override PartName=\"/xl/workbook.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml\"/>");
    for i in 0..wb.sheets.len() {
        ct.push_str(&format!(
            "<Override PartName=\"/xl/worksheets/sheet{}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/>",
            i + 1
        ));
    }
    ct.push_str("<Override PartName=\"/xl/sharedStrings.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml\"/>");
    ct.push_str("<Override PartName=\"/xl/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml\"/>");
    ct.push_str("</Types>");
    zip_write!(zip, "[Content_Types].xml", ct, options);

    // _rels/.rels
    zip_write!(zip, "_rels/.rels", "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
        <Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
        <Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"xl/workbook.xml\"/>\
        </Relationships>", options);

    // xl/workbook.xml
    let mut wb_xml = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
         <workbook xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\">\
         <sheets>");
    for (i, sheet) in wb.sheets.iter().enumerate() {
        let safe_name = xml_escape(&sheet.name);
        wb_xml.push_str(&format!(
            "<sheet name=\"{}\" sheetId=\"{}\" r:id=\"rId{}\"/>",
            safe_name, i + 1, i + 1
        ));
    }
    wb_xml.push_str("</sheets></workbook>");
    zip_write!(zip, "xl/workbook.xml", wb_xml, options);

    // xl/sharedStrings.xml
    let mut sst = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
         <sst xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" count=\"{}\" uniqueCount=\"{}\">",
        strings.len(), strings.len()
    );
    for s in &strings {
        sst.push_str(&format!("<si><t>{}</t></si>", xml_escape(s)));
    }
    sst.push_str("</sst>");
    zip_write!(zip, "xl/sharedStrings.xml", sst, options);

    // xl/styles.xml
    zip_write!(zip, "xl/styles.xml",
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
         <styleSheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\">\
         <fonts count=\"1\"><font><sz val=\"11\"/><name val=\"Calibri\"/></font></fonts>\
         <fills count=\"2\"><fill><patternFill patternType=\"none\"/></fill><fill><patternFill patternType=\"gray125\"/></fill></fills>\
         <borders count=\"1\"><border><left/><right/><top/><bottom/><diagonal/></border></borders>\
         <cellStyleXfs count=\"1\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\"/></cellStyleXfs>\
         <cellXfs count=\"1\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\" xfId=\"0\"/></cellXfs>\
         </styleSheet>", options);

    // xl/_rels/workbook.xml.rels
    let mut rels = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
         <Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">");
    for i in 0..wb.sheets.len() {
        rels.push_str(&format!(
            "<Relationship Id=\"rId{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet\" Target=\"worksheets/sheet{}.xml\"/>",
            i + 1, i + 1
        ));
    }
    rels.push_str("<Relationship Id=\"rIdSharedStrings\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings\" Target=\"sharedStrings.xml\"/>");
    rels.push_str("<Relationship Id=\"rIdStyles\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/>");
    rels.push_str("</Relationships>");
    zip_write!(zip, "xl/_rels/workbook.xml.rels", rels, options);

    // Sheet data
    for (i, sheet) in wb.sheets.iter().enumerate() {
        let sheet_path = format!("xl/worksheets/sheet{}.xml", i + 1);
        let mut sheet_xml = String::from(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
             <worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\">\
             <sheetData>");

        for (row_idx, row) in sheet.rows.iter().enumerate() {
            sheet_xml.push_str(&format!("<row r=\"{}\">", row_idx + 1));
            for (col_idx, cell) in row.cells.iter().enumerate() {
                if cell.value.is_empty() && cell.formula.is_none() {
                    continue;
                }
                let col_letter = excli_core::reference::column_index_to_name(col_idx as u32);
                let cell_ref = format!("{}{}", col_letter, row_idx + 1);

                match &cell.value {
                    CellValue::Number(n) => {
                        sheet_xml.push_str(&format!("<c r=\"{}\"><v>{}</v></c>", cell_ref, n));
                    }
                    CellValue::Boolean(b) => {
                        sheet_xml.push_str(&format!(
                            "<c r=\"{}\" t=\"b\"><v>{}</v></c>",
                            cell_ref,
                            if *b { 1 } else { 0 }
                        ));
                    }
                    CellValue::InlineString(s) => {
                        if let Some(pos) = strings.iter().position(|st| st == s) {
                            sheet_xml.push_str(&format!(
                                "<c r=\"{}\" t=\"s\"><v>{}</v></c>", cell_ref, pos
                            ));
                        } else {
                            sheet_xml.push_str(&format!(
                                "<c r=\"{}\" t=\"inlineStr\"><is><t>{}</t></is></c>",
                                cell_ref,
                                xml_escape(s)
                            ));
                        }
                    }
                    _ => {
                        sheet_xml.push_str(&format!(
                            "<c r=\"{}\"><v>{}</v></c>", cell_ref, cell.value
                        ));
                    }
                }
            }
            sheet_xml.push_str("</row>");
        }

        sheet_xml.push_str("</sheetData></worksheet>");
        zip_write!(zip, &sheet_path, sheet_xml, options);
    }

    zip.finish()
        .map_err(|e| ExcliError::new(ErrorCode::WriteError, format!("Zip finish error: {}", e)))?;
    Ok(())
}

/// Write workbook as JSON.
pub fn write_json(path: &Path, wb: &Workbook) -> ExcliResult<()> {
    let json_value = serde_json::to_value(wb)
        .map_err(|e| ExcliError::new(ErrorCode::WriteError, e.to_string()))?;
    let file = std::fs::File::create(path)?;
    serde_json::to_writer_pretty(file, &json_value)
        .map_err(|e| ExcliError::new(ErrorCode::WriteError, e.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use excli_core::cell::Cell;

    #[test]
    fn test_write_csv() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("output.csv");

        let mut wb = Workbook::new();
        let sheet = wb.add_sheet("Data");
        sheet.set_cell(0, 0, Cell::string("Name"));
        sheet.set_cell(1, 0, Cell::string("Age"));
        sheet.set_cell(0, 1, Cell::string("Alice"));
        sheet.set_cell(1, 1, Cell::number(30.0));

        write_workbook(&path, &wb).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("Name,Age"));
        assert!(content.contains("Alice,30"));
    }

    #[test]
    fn test_write_xlsx() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("output.xlsx");

        let mut wb = Workbook::new();
        let sheet = wb.add_sheet("Sheet1");
        sheet.set_cell(0, 0, Cell::string("Hello"));
        sheet.set_cell(1, 0, Cell::number(42.0));

        let result = write_workbook(&path, &wb);
        assert!(result.is_ok());
        assert!(path.exists());
    }

    #[test]
    fn test_csv_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("roundtrip.csv");

        let mut wb = Workbook::new();
        let sheet = wb.add_sheet("Data");
        sheet.set_cell(0, 0, Cell::string("Name"));
        sheet.set_cell(1, 0, Cell::string("Score"));
        sheet.set_cell(0, 1, Cell::string("Bob"));
        sheet.set_cell(1, 1, Cell::number(95.5));

        write_workbook(&path, &wb).unwrap();
        let read_wb = crate::reader::read_workbook(&path).unwrap();

        assert_eq!(read_wb.sheet_count(), 1);
    }

    #[test]
    fn test_write_tsv() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("output.tsv");

        let mut wb = Workbook::new();
        let sheet = wb.add_sheet("Data");
        sheet.set_cell(0, 0, Cell::string("A"));
        sheet.set_cell(1, 0, Cell::string("B"));
        sheet.set_cell(0, 1, Cell::number(1.0));
        sheet.set_cell(1, 1, Cell::number(2.0));

        write_workbook(&path, &wb).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains('\t'));
        assert!(content.contains("A\tB"));
    }

    #[test]
    fn test_csv_roundtrip_cell_values() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("values.csv");

        let mut wb = Workbook::new();
        let sheet = wb.add_sheet("Sheet1");
        sheet.set_cell(0, 0, Cell::string("hello"));
        sheet.set_cell(1, 0, Cell::number(3.14));
        sheet.set_cell(2, 0, Cell::boolean(true));

        write_workbook(&path, &wb).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();

        assert!(content.contains("hello"));
        assert!(content.contains("3.14"));
        assert!(content.contains("true"));
    }

    #[test]
    fn test_write_json() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("output.json");

        let mut wb = Workbook::new();
        let sheet = wb.add_sheet("Data");
        sheet.set_cell(0, 0, Cell::string("x"));
        sheet.set_cell(0, 1, Cell::number(1.0));

        write_workbook(&path, &wb).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        // JSON output should be valid JSON
        let _: serde_json::Value = serde_json::from_str(&content).unwrap();
    }

    #[test]
    fn test_write_empty_workbook() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty.csv");

        let mut wb = Workbook::new();
        wb.add_sheet("Empty");

        assert!(write_workbook(&path, &wb).is_ok());
    }

    #[test]
    fn test_csv_special_chars_escaped() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("special.csv");

        let mut wb = Workbook::new();
        let sheet = wb.add_sheet("S");
        sheet.set_cell(0, 0, Cell::string("hello,world")); // comma in value

        write_workbook(&path, &wb).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        // Value with comma should be quoted
        assert!(content.contains('"'));
    }
}
