use std::io::Write;
use std::path::Path;

use excli_core::cell::CellValue;
use excli_core::error::{ErrorCode, ExcliError, ExcliResult};
use excli_core::types::FileFormat;
use excli_core::Workbook;

use crate::format_detect::detect_format;

fn persist_temp(mut temp: tempfile::NamedTempFile, path: &Path) -> ExcliResult<()> {
    if let Ok(metadata) = std::fs::metadata(path) {
        temp.as_file_mut().set_permissions(metadata.permissions())?;
    }
    temp.as_file_mut().sync_all()?;
    temp.persist(path)
        .map_err(|e| ExcliError::new(ErrorCode::WriteError, e.error.to_string()))?;
    Ok(())
}

/// Copy a workbook byte-for-byte when input and output use the same Excel format.
pub fn copy_workbook_unchanged(input: &Path, output: &Path) -> ExcliResult<()> {
    let input_format = detect_format(input)?;
    let output_format = detect_format(output)?;
    if input_format != output_format
        || !matches!(
            input_format,
            FileFormat::Xlsx
                | FileFormat::Xlsm
                | FileFormat::Xlsb
                | FileFormat::Xls
                | FileFormat::Ods
        )
    {
        return Err(ExcliError::new(
            ErrorCode::FormatNotSupported,
            "Lossless copy requires matching Excel or ODS formats",
        ));
    }
    if output.exists() && input.canonicalize()? == output.canonicalize()? {
        return Ok(());
    }
    let parent = output.parent().unwrap_or_else(|| Path::new("."));
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    let mut source = std::fs::File::open(input)?;
    std::io::copy(&mut source, temp.as_file_mut())?;
    temp.as_file_mut()
        .set_permissions(source.metadata()?.permissions())?;
    temp.as_file_mut().sync_all()?;
    temp.persist(output)
        .map_err(|error| ExcliError::new(ErrorCode::WriteError, error.error.to_string()))?;
    Ok(())
}

/// Extract one worksheet from an XLSX package, retaining its package parts.
/// The destination changes only after all other sheets have been removed.
pub fn copy_workbook_sheet(input: &Path, output: &Path, sheet: &str) -> ExcliResult<()> {
    if detect_format(input)? != FileFormat::Xlsx || detect_format(output)? != FileFormat::Xlsx {
        return Err(ExcliError::new(
            ErrorCode::FormatNotSupported,
            "Package-preserving sheet extraction requires XLSX input and output",
        ));
    }
    let parent = output.parent().unwrap_or_else(|| Path::new("."));
    let staging = tempfile::tempdir_in(parent)?;
    let staged_path = staging.path().join("workbook.xlsx");
    copy_workbook_unchanged(input, &staged_path)?;
    let mut archive = zip::ZipArchive::new(std::fs::File::open(&staged_path)?)
        .map_err(|error| ExcliError::new(ErrorCode::FileCorrupted, error.to_string()))?;
    let names: Vec<String> = crate::ooxml_package::index(&mut archive)?
        .sheets
        .into_iter()
        .map(|sheet| sheet.name)
        .collect();
    drop(archive);
    if !names.iter().any(|name| name == sheet) {
        return Err(ExcliError::sheet_not_found(sheet));
    }
    for name in names.iter().filter(|name| name.as_str() != sheet) {
        crate::ooxml_sheet_delete::delete_sheet(&staged_path, name)?;
    }
    std::fs::rename(&staged_path, output)?;
    Ok(())
}

/// Write a workbook to a file.
pub fn write_workbook(path: &Path, wb: &Workbook) -> ExcliResult<()> {
    let format = detect_format(path)?;
    write_workbook_with_format(path, wb, format)
}

/// Write a workbook with explicit format.
pub fn write_workbook_with_format(
    path: &Path,
    wb: &Workbook,
    format: FileFormat,
) -> ExcliResult<()> {
    if matches!(
        wb.file_format,
        Some(
            FileFormat::Xlsx
                | FileFormat::Xlsm
                | FileFormat::Xlsb
                | FileFormat::Xls
                | FileFormat::Ods
        )
    ) && matches!(format, FileFormat::Xlsx | FileFormat::Xlsm)
    {
        return Err(ExcliError::new(
            ErrorCode::NotImplemented,
            "Rebuilding an existing Excel workbook would discard formulas and formatting; use a package-preserving command",
        ));
    }
    match format {
        FileFormat::Csv | FileFormat::Tsv => {
            let delimiter = if format == FileFormat::Tsv {
                b'\t'
            } else {
                b','
            };
            write_csv(path, wb, delimiter)
        }
        FileFormat::Xlsx => write_xlsx(path, wb),
        FileFormat::Json => write_json(path, wb),
        _ => Err(ExcliError::format_not_supported(format!(
            "Writing {} is not supported",
            format
        ))),
    }
}

/// Write workbook as CSV.
pub fn write_csv(path: &Path, wb: &Workbook, delimiter: u8) -> ExcliResult<()> {
    if wb.sheets.len() > 1 {
        return Err(ExcliError::new(
            ErrorCode::FormatNotSupported,
            "CSV/TSV can contain only one sheet; select a sheet before converting",
        ));
    }
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    let mut writer = csv::WriterBuilder::new()
        .delimiter(delimiter)
        .has_headers(false)
        .flexible(true)
        .from_writer(temp.as_file_mut());

    for sheet in &wb.sheets {
        for row in &sheet.rows {
            let cells: Vec<String> = row
                .cells
                .iter()
                .map(|c| match &c.value {
                    CellValue::Empty => String::new(),
                    CellValue::InlineString(s) => s.clone(),
                    CellValue::Number(n) => n.to_string(),
                    CellValue::Boolean(b) => b.to_string(),
                    CellValue::Error(e) => e.to_string(),
                    _ => format!("{}", c.value),
                })
                .collect();
            writer
                .write_record(cells)
                .map_err(|e| ExcliError::new(ErrorCode::WriteError, e.to_string()))?;
        }
    }
    writer.flush()?;
    drop(writer);
    persist_temp(temp, path)
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
    if matches!(
        wb.file_format,
        Some(
            FileFormat::Xlsx
                | FileFormat::Xlsm
                | FileFormat::Xlsb
                | FileFormat::Xls
                | FileFormat::Ods
        )
    ) {
        return Err(ExcliError::new(
            ErrorCode::NotImplemented,
            "Rebuilding an existing Excel workbook would discard formulas and formatting",
        ));
    }
    use std::io::BufWriter;
    use zip::write::SimpleFileOptions;

    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let temp = tempfile::NamedTempFile::new_in(parent)?;
    let writer = BufWriter::new(temp.reopen()?);
    let mut zip = zip::ZipWriter::new(writer);

    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

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
            $zip.write_all($data.as_bytes()).map_err(|e| {
                ExcliError::new(ErrorCode::WriteError, format!("Write error: {}", e))
            })?;
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
            safe_name,
            i + 1,
            i + 1
        ));
    }
    wb_xml.push_str("</sheets><calcPr fullCalcOnLoad=\"1\" forceFullCalc=\"1\"/></workbook>");
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
         <Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">",
    );
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
             <sheetData>",
        );

        for (row_idx, row) in sheet.rows.iter().enumerate() {
            sheet_xml.push_str(&format!("<row r=\"{}\">", row_idx + 1));
            for (col_idx, cell) in row.cells.iter().enumerate() {
                if cell.value.is_empty() && cell.formula.is_none() {
                    continue;
                }
                let col_letter = excli_core::reference::column_index_to_name(col_idx as u32);
                let cell_ref = format!("{}{}", col_letter, row_idx + 1);

                if let Some(formula) = &cell.formula {
                    if matches!(
                        &cell.value,
                        CellValue::SharedString(_)
                            | CellValue::RichText(_)
                            | CellValue::RangeArray(_)
                    ) {
                        return Err(ExcliError::new(
                            ErrorCode::NotImplemented,
                            "Cannot write a formula with an unresolved or rich-text cached value",
                        ));
                    }
                    let expression = formula.raw.strip_prefix('=').unwrap_or(&formula.raw);
                    let cell_type = match &cell.value {
                        CellValue::Boolean(_) => " t=\"b\"",
                        CellValue::InlineString(_) => " t=\"str\"",
                        CellValue::Error(_) => " t=\"e\"",
                        _ => "",
                    };
                    sheet_xml.push_str(&format!(
                        "<c r=\"{}\"{}><f>{}</f>",
                        cell_ref,
                        cell_type,
                        xml_escape(expression)
                    ));
                    let cached = match &cell.value {
                        CellValue::Empty => None,
                        CellValue::Boolean(true) => Some("1".to_owned()),
                        CellValue::Boolean(false) => Some("0".to_owned()),
                        value => Some(value.to_string()),
                    };
                    if let Some(value) = cached {
                        sheet_xml.push_str(&format!("<v>{}</v>", xml_escape(&value)));
                    }
                    sheet_xml.push_str("</c>");
                    continue;
                }

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
                                "<c r=\"{}\" t=\"s\"><v>{}</v></c>",
                                cell_ref, pos
                            ));
                        } else {
                            sheet_xml.push_str(&format!(
                                "<c r=\"{}\" t=\"inlineStr\"><is><t>{}</t></is></c>",
                                cell_ref,
                                xml_escape(s)
                            ));
                        }
                    }
                    CellValue::Error(error) => {
                        sheet_xml.push_str(&format!(
                            "<c r=\"{}\" t=\"e\"><v>{}</v></c>",
                            cell_ref, error
                        ));
                    }
                    CellValue::RichText(_)
                    | CellValue::SharedString(_)
                    | CellValue::RangeArray(_) => {
                        return Err(ExcliError::new(
                            ErrorCode::NotImplemented,
                            "This cell value cannot be written to XLSX without losing information",
                        ));
                    }
                    _ => {
                        sheet_xml
                            .push_str(&format!("<c r=\"{}\"><v>{}</v></c>", cell_ref, cell.value));
                    }
                }
            }
            sheet_xml.push_str("</row>");
        }

        sheet_xml.push_str("</sheetData></worksheet>");
        zip_write!(zip, &sheet_path, sheet_xml, options);
    }

    let mut output = zip
        .finish()
        .map_err(|e| ExcliError::new(ErrorCode::WriteError, format!("Zip finish error: {}", e)))?;
    output.flush()?;
    output.get_ref().sync_all()?;
    drop(output);
    persist_temp(temp, path)
}

/// Write workbook as JSON.
pub fn write_json(path: &Path, wb: &Workbook) -> ExcliResult<()> {
    let json_value = serde_json::to_value(wb)
        .map_err(|e| ExcliError::new(ErrorCode::WriteError, e.to_string()))?;
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let temp = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer_pretty(temp.as_file(), &json_value)
        .map_err(|e| ExcliError::new(ErrorCode::WriteError, e.to_string()))?;
    persist_temp(temp, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use excli_core::cell::Cell;

    use crate::ooxml_package::read_part;

    #[test]
    fn extracts_sheet_without_rebuilding_its_xml_and_keeps_output_on_error() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("input.xlsx");
        let output = dir.path().join("output.xlsx");
        let mut workbook = Workbook::new();
        workbook
            .add_sheet("Keep")
            .set_cell(0, 0, Cell::string("original"));
        workbook
            .add_sheet("Discard")
            .set_cell(0, 0, Cell::number(9.0));
        write_xlsx(&input, &workbook).unwrap();
        let mut source = zip::ZipArchive::new(std::fs::File::open(&input).unwrap()).unwrap();
        let selected_xml =
            String::from_utf8(read_part(&mut source, "xl/worksheets/sheet1.xml").unwrap()).unwrap();
        let selected_xml = selected_xml.replace("</worksheet>", "<drawing xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" r:id=\"rId1\"/></worksheet>").into_bytes();
        let types =
            String::from_utf8(read_part(&mut source, "[Content_Types].xml").unwrap()).unwrap();
        let types = types.replace("</Types>", "<Override PartName=\"/xl/drawings/drawing1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.drawing+xml\"/></Types>").into_bytes();
        crate::ooxml_edit::write_package_parts(&input, &mut source, &std::collections::HashMap::from([
            ("xl/worksheets/sheet1.xml".to_owned(), selected_xml.clone()),
            ("[Content_Types].xml".to_owned(), types),
        ]), &std::collections::HashMap::from([
            ("xl/worksheets/_rels/sheet1.xml.rels".to_owned(), b"<Relationships><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/drawing\" Target=\"../drawings/drawing1.xml\"/></Relationships>".to_vec()),
            ("xl/drawings/drawing1.xml".to_owned(), b"<xdr:wsDr xmlns:xdr=\"http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing\"/>".to_vec()),
        ])).unwrap();
        drop(source);

        copy_workbook_sheet(&input, &output, "Keep").unwrap();
        let mut extracted = zip::ZipArchive::new(std::fs::File::open(&output).unwrap()).unwrap();
        assert_eq!(
            read_part(&mut extracted, "xl/worksheets/sheet1.xml").unwrap(),
            selected_xml
        );
        assert!(extracted.by_name("xl/worksheets/sheet2.xml").is_err());
        assert!(extracted.by_name("xl/drawings/drawing1.xml").is_ok());
        let loaded = crate::reader::read_workbook(&output).unwrap();
        assert_eq!(loaded.sheets.len(), 1);
        assert_eq!(loaded.sheets[0].name, "Keep");
        let before = std::fs::read(&output).unwrap();
        assert!(copy_workbook_sheet(&input, &output, "Missing").is_err());
        assert_eq!(std::fs::read(&output).unwrap(), before);

        let mut referenced = Workbook::new();
        referenced.add_sheet("Keep").set_cell(
            0,
            0,
            Cell::number(9.0).with_formula(excli_core::formula::Formula::new("=Discard!A1")),
        );
        referenced
            .add_sheet("Discard")
            .set_cell(0, 0, Cell::number(9.0));
        write_xlsx(&input, &referenced).unwrap();
        assert!(copy_workbook_sheet(&input, &output, "Keep").is_err());
        assert_eq!(std::fs::read(&output).unwrap(), before);
    }

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
    fn existing_xlsx_is_not_silently_rebuilt() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.xlsx");
        let destination = dir.path().join("destination.xlsx");
        let mut workbook = Workbook::new();
        workbook
            .add_sheet("Data")
            .set_cell(0, 0, Cell::string("Value"));
        write_xlsx(&source, &workbook).unwrap();

        let loaded = crate::reader::read_workbook(&source).unwrap();
        let before = std::fs::read(&source).unwrap();
        assert!(write_workbook(&source, &loaded).is_err());
        assert!(write_workbook(&destination, &loaded).is_err());
        assert_eq!(std::fs::read(&source).unwrap(), before);
        assert!(!destination.exists());
    }

    #[test]
    fn unsupported_output_and_multisheet_csv_fail_without_writing() {
        let dir = tempfile::tempdir().unwrap();
        let mut workbook = Workbook::new();
        workbook.add_sheet("First");
        workbook.add_sheet("Second");
        let csv = dir.path().join("output.csv");
        let ods = dir.path().join("output.ods");
        assert!(write_workbook(&csv, &workbook).is_err());
        assert!(write_workbook(&ods, &workbook).is_err());
        assert!(!csv.exists());
        assert!(!ods.exists());
    }

    #[test]
    fn failed_xlsx_write_does_not_replace_existing_destination() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("existing.xlsx");
        std::fs::write(&path, b"original bytes").unwrap();
        let mut workbook = Workbook::new();
        workbook.add_sheet("Data").set_cell(
            0,
            0,
            Cell::new(CellValue::RichText(vec![excli_core::cell::RichTextRun {
                text: "formatted".to_owned(),
                bold: Some(true),
                italic: None,
                underline: None,
                strikethrough: None,
                font_name: None,
                font_size: None,
                color: None,
            }])),
        );
        assert!(write_xlsx(&path, &workbook).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"original bytes");
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

    #[test]
    fn tsv_roundtrip_preserves_embedded_tabs_and_newlines() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("special.tsv");
        let mut workbook = Workbook::new();
        workbook
            .add_sheet("Data")
            .set_cell(0, 0, Cell::string("one\ttwo\nthree"));
        write_workbook(&path, &workbook).unwrap();
        let loaded = crate::reader::read_workbook(&path).unwrap();
        assert_eq!(
            loaded
                .sheet_by_index(0)
                .unwrap()
                .get_cell(0, 0)
                .unwrap()
                .value
                .to_string(),
            "one\ttwo\nthree"
        );
    }
}
