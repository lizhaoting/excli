//! Resolve worksheet table relationships and read common table metadata.

use std::io::{Read, Seek};

use excli_core::error::{ErrorCode, ExcliError, ExcliResult};
use excli_core::sheet::{Table, TableColumn};
use excli_core::Sheet;
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use zip::ZipArchive;

use crate::ooxml_package::{read_part, related_parts};

fn corrupted(message: impl Into<String>) -> ExcliError {
    ExcliError::new(ErrorCode::FileCorrupted, message)
}

fn attribute(tag: &BytesStart<'_>, key: &[u8]) -> ExcliResult<Option<String>> {
    for item in tag.attributes() {
        let item = item.map_err(|error| corrupted(format!("Invalid table attribute: {error}")))?;
        if item.key.as_ref() == key {
            return Ok(Some(
                item.unescape_value()
                    .map_err(|error| corrupted(format!("Invalid table attribute value: {error}")))?
                    .into_owned(),
            ));
        }
    }
    Ok(None)
}

fn required(tag: &BytesStart<'_>, key: &[u8]) -> ExcliResult<String> {
    attribute(tag, key)?
        .ok_or_else(|| corrupted(format!("Table is missing {}", String::from_utf8_lossy(key))))
}

fn flag(tag: &BytesStart<'_>, key: &[u8]) -> ExcliResult<bool> {
    Ok(matches!(
        attribute(tag, key)?.as_deref(),
        Some("1" | "true")
    ))
}

pub(crate) fn table_part_ids(xml: &[u8]) -> ExcliResult<Vec<String>> {
    let mut reader = Reader::from_reader(xml);
    let mut in_parts = false;
    let mut ids = Vec::new();
    loop {
        match reader
            .read_event()
            .map_err(|error| corrupted(format!("Invalid worksheet XML: {error}")))?
        {
            Event::Start(ref tag) if tag.local_name().as_ref() == b"tableParts" => in_parts = true,
            Event::End(ref tag) if tag.local_name().as_ref() == b"tableParts" => in_parts = false,
            Event::Start(ref tag) | Event::Empty(ref tag)
                if in_parts && tag.local_name().as_ref() == b"tablePart" =>
            {
                let mut id = None;
                for item in tag.attributes() {
                    let item = item.map_err(|error| {
                        corrupted(format!("Invalid tablePart attribute: {error}"))
                    })?;
                    if item.key.as_ref().ends_with(b":id") {
                        id = Some(
                            item.unescape_value()
                                .map_err(|error| {
                                    corrupted(format!("Invalid table relationship ID: {error}"))
                                })?
                                .into_owned(),
                        );
                    }
                }
                ids.push(id.ok_or_else(|| corrupted("tablePart has no relationship ID"))?);
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(ids)
}

fn table_column(tag: &BytesStart<'_>) -> ExcliResult<TableColumn> {
    Ok(TableColumn {
        name: required(tag, b"name")?,
        formula: None,
        totals_function: attribute(tag, b"totalsRowFunction")?,
    })
}

fn parse_table(xml: &[u8]) -> ExcliResult<Table> {
    let mut reader = Reader::from_reader(xml);
    let mut table = None;
    let mut column: Option<TableColumn> = None;
    let mut in_formula = false;
    loop {
        match reader
            .read_event()
            .map_err(|error| corrupted(format!("Invalid table XML: {error}")))?
        {
            Event::Start(ref tag) | Event::Empty(ref tag)
                if tag.local_name().as_ref() == b"table" && table.is_none() =>
            {
                let name = required(tag, b"name")?;
                let display_name = attribute(tag, b"displayName")?.unwrap_or_else(|| name.clone());
                let range = required(tag, b"ref")?;
                let show_header = attribute(tag, b"headerRowCount")?.map_or(Ok(true), |count| {
                    count
                        .parse::<u32>()
                        .map(|count| count > 0)
                        .map_err(|_| corrupted("Invalid table header row count"))
                })?;
                let totals_count = attribute(tag, b"totalsRowCount")?
                    .map(|count| {
                        count
                            .parse::<u32>()
                            .map_err(|_| corrupted("Invalid table totals row count"))
                    })
                    .transpose()?
                    .unwrap_or(0);
                table = Some(Table {
                    name,
                    display_name,
                    range,
                    columns: Vec::new(),
                    style_name: None,
                    show_header,
                    show_totals: totals_count > 0 || flag(tag, b"totalsRowShown")?,
                    auto_filter: false,
                });
            }
            Event::Start(ref tag) if tag.local_name().as_ref() == b"tableColumn" => {
                if column.is_some() {
                    return Err(corrupted("Nested table column"));
                }
                column = Some(table_column(tag)?);
            }
            Event::Empty(ref tag) if tag.local_name().as_ref() == b"tableColumn" => {
                table
                    .as_mut()
                    .ok_or_else(|| corrupted("Table column outside table"))?
                    .columns
                    .push(table_column(tag)?);
            }
            Event::Start(ref tag) if tag.local_name().as_ref() == b"calculatedColumnFormula" => {
                in_formula = true;
                if let Some(column) = &mut column {
                    column.formula = Some(String::new());
                }
            }
            Event::Text(ref text) if in_formula => {
                let value = text
                    .unescape()
                    .map_err(|error| corrupted(format!("Invalid table formula: {error}")))?;
                if let Some(formula) = column.as_mut().and_then(|column| column.formula.as_mut()) {
                    formula.push_str(&value);
                }
            }
            Event::CData(ref text) if in_formula => {
                let value = std::str::from_utf8(text.as_ref())
                    .map_err(|error| corrupted(format!("Invalid table formula: {error}")))?;
                if let Some(formula) = column.as_mut().and_then(|column| column.formula.as_mut()) {
                    formula.push_str(value);
                }
            }
            Event::End(ref tag) if tag.local_name().as_ref() == b"calculatedColumnFormula" => {
                in_formula = false
            }
            Event::End(ref tag) if tag.local_name().as_ref() == b"tableColumn" => {
                table
                    .as_mut()
                    .ok_or_else(|| corrupted("Table column outside table"))?
                    .columns
                    .push(
                        column
                            .take()
                            .ok_or_else(|| corrupted("Unexpected table column end"))?,
                    );
            }
            Event::Start(ref tag) | Event::Empty(ref tag)
                if tag.local_name().as_ref() == b"autoFilter" =>
            {
                if let Some(table) = &mut table {
                    table.auto_filter = true;
                }
            }
            Event::Start(ref tag) | Event::Empty(ref tag)
                if tag.local_name().as_ref() == b"tableStyleInfo" =>
            {
                if let Some(table) = &mut table {
                    table.style_name = attribute(tag, b"name")?;
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if column.is_some() {
        return Err(corrupted("Unclosed table column"));
    }
    table.ok_or_else(|| corrupted("Missing table root"))
}

pub(crate) fn enrich_sheet<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    worksheet_path: &str,
    worksheet_xml: &[u8],
    sheet: &mut Sheet,
) -> ExcliResult<()> {
    let ids = table_part_ids(worksheet_xml)?;
    if ids.is_empty() {
        return Ok(());
    }
    let relations = related_parts(archive, worksheet_path)?;
    for id in ids {
        let (kind, path) = relations
            .get(&id)
            .ok_or_else(|| corrupted(format!("Missing table relationship: {id}")))?;
        if !kind.ends_with("/table") {
            return Err(corrupted(format!("Relationship {id} is not a table")));
        }
        sheet.tables.push(parse_table(&read_part(archive, path)?)?);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use excli_core::Workbook;
    use std::collections::HashMap;

    #[test]
    fn parses_table_columns_and_style() {
        let xml = br#"<table name="Sales" displayName="Sales" ref="A1:C9" headerRowCount="1" totalsRowCount="1"><autoFilter ref="A1:C8"/><tableColumns count="3"><tableColumn id="1" name="Item"/><tableColumn id="2" name="Qty" totalsRowFunction="sum"><calculatedColumnFormula>[Price]*[Qty]</calculatedColumnFormula></tableColumn><tableColumn id="3" name="Price"/></tableColumns><tableStyleInfo name="TableStyleMedium2"/></table>"#;
        let table = parse_table(xml).unwrap();
        assert_eq!(table.range, "A1:C9");
        assert!(table.show_header && table.show_totals && table.auto_filter);
        assert_eq!(table.style_name.as_deref(), Some("TableStyleMedium2"));
        assert_eq!(table.columns[1].name, "Qty");
        assert_eq!(table.columns[1].formula.as_deref(), Some("[Price]*[Qty]"));
        assert_eq!(table.columns[1].totals_function.as_deref(), Some("sum"));
    }

    #[test]
    fn reads_table_through_sheet_relationship() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tables.xlsx");
        let mut workbook = Workbook::new();
        workbook.add_sheet("Data");
        crate::writer::write_xlsx(&path, &workbook).unwrap();
        let mut archive = ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
        let worksheet_path = "xl/worksheets/sheet1.xml";
        let xml = String::from_utf8(read_part(&mut archive, worksheet_path).unwrap()).unwrap();
        let xml = xml.replace("</worksheet>", "<tableParts count=\"1\"><tablePart xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" r:id=\"rId7\"/></tableParts></worksheet>");
        let types_path = "[Content_Types].xml";
        let types = String::from_utf8(read_part(&mut archive, types_path).unwrap()).unwrap();
        let types = types.replace("</Types>", "<Override PartName=\"/xl/tables/table1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.table+xml\"/></Types>");
        let modified = HashMap::from([
            (worksheet_path.to_owned(), xml.into_bytes()),
            (types_path.to_owned(), types.into_bytes()),
        ]);
        let added = HashMap::from([
            ("xl/worksheets/_rels/sheet1.xml.rels".to_owned(), b"<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId7\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/table\" Target=\"../tables/table1.xml\"/></Relationships>".to_vec()),
            ("xl/tables/table1.xml".to_owned(), b"<table xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" id=\"1\" name=\"Sales\" displayName=\"Sales\" ref=\"A1:B2\"><tableColumns count=\"2\"><tableColumn id=\"1\" name=\"Item\"/><tableColumn id=\"2\" name=\"Price\"/></tableColumns></table>".to_vec()),
        ]);
        crate::ooxml_edit::write_package_parts(&path, &mut archive, &modified, &added).unwrap();
        let loaded = crate::reader::read_workbook(&path).unwrap();
        assert_eq!(loaded.sheets[0].tables.len(), 1);
        assert_eq!(loaded.sheets[0].tables[0].name, "Sales");
        assert_eq!(loaded.sheets[0].tables[0].columns[1].name, "Price");
    }
}
