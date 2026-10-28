//! Read common PivotTable metadata from worksheet and pivot cache parts.

use std::io::{Read, Seek};

use excli_core::error::{ErrorCode, ExcliError, ExcliResult};
use excli_core::sheet::{PivotAggregate, PivotTable, PivotValue, Sheet};
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use zip::ZipArchive;

use crate::ooxml_package::{read_part, related_parts};

fn malformed(message: impl Into<String>) -> ExcliError {
    ExcliError::new(ErrorCode::FileCorrupted, message)
}

fn attribute(tag: &BytesStart<'_>, key: &[u8]) -> ExcliResult<Option<String>> {
    for item in tag.attributes() {
        let item = item.map_err(|error| malformed(format!("Invalid pivot attribute: {error}")))?;
        if item.key.as_ref() == key {
            return Ok(Some(
                item.unescape_value()
                    .map_err(|error| malformed(format!("Invalid pivot value: {error}")))?
                    .into_owned(),
            ));
        }
    }
    Ok(None)
}

fn required(tag: &BytesStart<'_>, key: &[u8]) -> ExcliResult<String> {
    attribute(tag, key)?.ok_or_else(|| {
        malformed(format!(
            "Pivot part is missing {}",
            String::from_utf8_lossy(key)
        ))
    })
}

fn relationship_id(tag: &BytesStart<'_>) -> ExcliResult<Option<String>> {
    for item in tag.attributes() {
        let item =
            item.map_err(|error| malformed(format!("Invalid pivot relationship: {error}")))?;
        if item.key.as_ref().ends_with(b":id") {
            return Ok(Some(
                item.unescape_value()
                    .map_err(|error| malformed(format!("Invalid pivot relationship ID: {error}")))?
                    .into_owned(),
            ));
        }
    }
    Ok(None)
}

pub(crate) fn pivot_part_ids(xml: &[u8]) -> ExcliResult<Vec<String>> {
    let mut reader = Reader::from_reader(xml);
    let mut in_parts = false;
    let mut ids = Vec::new();
    loop {
        match reader
            .read_event()
            .map_err(|error| malformed(format!("Invalid worksheet XML: {error}")))?
        {
            Event::Start(ref tag) if tag.local_name().as_ref() == b"pivotTableParts" => {
                in_parts = true
            }
            Event::End(ref tag) if tag.local_name().as_ref() == b"pivotTableParts" => {
                in_parts = false
            }
            Event::Start(ref tag) | Event::Empty(ref tag)
                if in_parts && tag.local_name().as_ref() == b"pivotTablePart" =>
            {
                ids.push(
                    relationship_id(tag)?
                        .ok_or_else(|| malformed("PivotTable part has no relationship ID"))?,
                );
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(ids)
}

struct PivotDefinition {
    name: String,
    location: String,
    rows: Vec<usize>,
    cols: Vec<usize>,
    filters: Vec<usize>,
    values: Vec<(usize, PivotAggregate, Option<String>)>,
}

fn field_index(tag: &BytesStart<'_>, key: &[u8]) -> ExcliResult<Option<usize>> {
    let value = required(tag, key)?;
    let index = value
        .parse::<i32>()
        .map_err(|_| malformed("Invalid pivot field index"))?;
    Ok((index >= 0).then_some(index as usize))
}

fn aggregate(value: Option<&str>) -> Option<PivotAggregate> {
    match value.unwrap_or("sum") {
        "sum" => Some(PivotAggregate::Sum),
        "count" => Some(PivotAggregate::Count),
        "average" => Some(PivotAggregate::Average),
        "min" => Some(PivotAggregate::Min),
        "max" => Some(PivotAggregate::Max),
        "product" => Some(PivotAggregate::Product),
        "stdDev" => Some(PivotAggregate::StdDev),
        "var" => Some(PivotAggregate::Var),
        _ => None,
    }
}

fn parse_pivot(xml: &[u8]) -> ExcliResult<PivotDefinition> {
    let mut reader = Reader::from_reader(xml);
    let mut pivot = None;
    let mut section = None::<Vec<u8>>;
    loop {
        match reader
            .read_event()
            .map_err(|error| malformed(format!("Invalid PivotTable XML: {error}")))?
        {
            Event::Start(ref tag) | Event::Empty(ref tag)
                if tag.local_name().as_ref() == b"pivotTableDefinition" && pivot.is_none() =>
            {
                pivot = Some(PivotDefinition {
                    name: required(tag, b"name")?,
                    location: String::new(),
                    rows: Vec::new(),
                    cols: Vec::new(),
                    filters: Vec::new(),
                    values: Vec::new(),
                });
            }
            Event::Start(ref tag)
                if matches!(
                    tag.local_name().as_ref(),
                    b"rowFields" | b"colFields" | b"pageFields" | b"dataFields"
                ) =>
            {
                section = Some(tag.local_name().as_ref().to_vec());
            }
            Event::End(ref tag) if section.as_deref() == Some(tag.local_name().as_ref()) => {
                section = None;
            }
            Event::Start(ref tag) | Event::Empty(ref tag) => {
                let Some(ref mut pivot) = pivot else { continue };
                match tag.local_name().as_ref() {
                    b"location" => pivot.location = required(tag, b"ref")?,
                    b"field" if section.as_deref() == Some(b"rowFields") => {
                        if let Some(index) = field_index(tag, b"x")? {
                            pivot.rows.push(index);
                        }
                    }
                    b"field" if section.as_deref() == Some(b"colFields") => {
                        if let Some(index) = field_index(tag, b"x")? {
                            pivot.cols.push(index);
                        }
                    }
                    b"pageField" if section.as_deref() == Some(b"pageFields") => {
                        if let Some(index) = field_index(tag, b"fld")? {
                            pivot.filters.push(index);
                        }
                    }
                    b"dataField" if section.as_deref() == Some(b"dataFields") => {
                        if let (Some(index), Some(aggregate)) = (
                            field_index(tag, b"fld")?,
                            aggregate(attribute(tag, b"subtotal")?.as_deref()),
                        ) {
                            pivot
                                .values
                                .push((index, aggregate, attribute(tag, b"name")?));
                        }
                    }
                    _ => {}
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    pivot.ok_or_else(|| malformed("PivotTable definition is missing"))
}

fn parse_cache(xml: &[u8]) -> ExcliResult<(String, Vec<String>)> {
    let mut reader = Reader::from_reader(xml);
    let mut in_cache_fields = false;
    let mut source = String::new();
    let mut fields = Vec::new();
    loop {
        match reader
            .read_event()
            .map_err(|error| malformed(format!("Invalid pivot cache XML: {error}")))?
        {
            Event::Start(ref tag) if tag.local_name().as_ref() == b"cacheFields" => {
                in_cache_fields = true
            }
            Event::End(ref tag) if tag.local_name().as_ref() == b"cacheFields" => {
                in_cache_fields = false
            }
            Event::Start(ref tag) | Event::Empty(ref tag)
                if in_cache_fields && tag.local_name().as_ref() == b"cacheField" =>
            {
                fields.push(required(tag, b"name")?);
            }
            Event::Start(ref tag) | Event::Empty(ref tag)
                if tag.local_name().as_ref() == b"worksheetSource" =>
            {
                if let Some(range) = attribute(tag, b"ref")? {
                    source = if let Some(sheet) = attribute(tag, b"sheet")? {
                        format!("'{}'!{range}", sheet.replace('\'', "''"))
                    } else {
                        range
                    };
                } else if let Some(name) = attribute(tag, b"name")? {
                    source = name;
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok((source, fields))
}

fn field_names(indices: Vec<usize>, fields: &[String]) -> ExcliResult<Vec<String>> {
    indices
        .into_iter()
        .map(|index| {
            fields
                .get(index)
                .cloned()
                .ok_or_else(|| malformed("Pivot field index is out of range"))
        })
        .collect()
}

pub(crate) fn enrich_sheet<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    sheet_path: &str,
    sheet_xml: &[u8],
    sheet: &mut Sheet,
) -> ExcliResult<()> {
    let ids = pivot_part_ids(sheet_xml)?;
    if ids.is_empty() {
        return Ok(());
    }
    let sheet_relations = related_parts(archive, sheet_path)?;
    for id in ids {
        let (kind, pivot_path) = sheet_relations
            .get(&id)
            .ok_or_else(|| malformed("Missing PivotTable relationship"))?;
        if !kind.ends_with("/pivotTable") {
            return Err(malformed("PivotTable relationship has an unexpected type"));
        }
        let definition = parse_pivot(&read_part(archive, pivot_path)?)?;
        let pivot_relations = related_parts(archive, pivot_path)?;
        let mut cache_targets = pivot_relations
            .values()
            .filter(|(kind, _)| kind.ends_with("/pivotCacheDefinition"));
        let (_, cache_path) = cache_targets
            .next()
            .ok_or_else(|| malformed("PivotTable has no cache definition relationship"))?;
        if cache_targets.next().is_some() {
            return Err(malformed("PivotTable has multiple cache definitions"));
        }
        let (source_range, fields) = parse_cache(&read_part(archive, cache_path)?)?;
        let values = definition
            .values
            .into_iter()
            .map(|(index, aggregate, display_name)| {
                Ok(PivotValue {
                    field: fields
                        .get(index)
                        .cloned()
                        .ok_or_else(|| malformed("Pivot value field index is out of range"))?,
                    aggregate,
                    display_name,
                })
            })
            .collect::<ExcliResult<Vec<_>>>()?;
        sheet.pivot_tables.push(PivotTable {
            name: definition.name,
            source_range,
            location: definition.location,
            rows: field_names(definition.rows, &fields)?,
            cols: field_names(definition.cols, &fields)?,
            filters: field_names(definition.filters, &fields)?,
            values,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use excli_core::Workbook;
    use std::collections::HashMap;

    use crate::ooxml_edit::write_package_parts;

    #[test]
    fn workbook_reader_resolves_pivot_fields_through_shared_cache() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pivot.xlsx");
        let mut workbook = Workbook::new();
        workbook.add_sheet("Data");
        workbook.add_sheet("Pivot");
        crate::writer::write_xlsx(&path, &workbook).unwrap();
        let mut archive = ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
        let original_sheet =
            String::from_utf8(read_part(&mut archive, "xl/worksheets/sheet2.xml").unwrap())
                .unwrap();
        let sheet = original_sheet.replace("</worksheet>", "<pivotTableParts count=\"1\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><pivotTablePart r:id=\"rId1\"/></pivotTableParts></worksheet>");
        let original_book =
            String::from_utf8(read_part(&mut archive, "xl/workbook.xml").unwrap()).unwrap();
        let book = original_book.replace("</workbook>", "<pivotCaches xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><pivotCache cacheId=\"1\" r:id=\"rId20\"/></pivotCaches></workbook>");
        let original_rels =
            String::from_utf8(read_part(&mut archive, "xl/_rels/workbook.xml.rels").unwrap())
                .unwrap();
        let rels = original_rels.replace("</Relationships>", "<Relationship Id=\"rId20\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/pivotCacheDefinition\" Target=\"pivotCache/pivotCacheDefinition1.xml\"/></Relationships>");
        let original_types =
            String::from_utf8(read_part(&mut archive, "[Content_Types].xml").unwrap()).unwrap();
        let types = original_types.replace("</Types>", "<Override PartName=\"/xl/pivotTables/pivotTable1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.pivotTable+xml\"/><Override PartName=\"/xl/pivotCache/pivotCacheDefinition1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.pivotCacheDefinition+xml\"/></Types>");
        write_package_parts(&path, &mut archive, &HashMap::from([
            ("xl/worksheets/sheet2.xml".to_owned(), sheet.into_bytes()),
            ("xl/workbook.xml".to_owned(), book.into_bytes()),
            ("xl/_rels/workbook.xml.rels".to_owned(), rels.into_bytes()),
            ("[Content_Types].xml".to_owned(), types.into_bytes()),
        ]), &HashMap::from([
            ("xl/worksheets/_rels/sheet2.xml.rels".to_owned(), b"<Relationships><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/pivotTable\" Target=\"../pivotTables/pivotTable1.xml\"/></Relationships>".to_vec()),
            ("xl/pivotTables/pivotTable1.xml".to_owned(), b"<pivotTableDefinition name=\"SalesPivot\" cacheId=\"1\" dataCaption=\"Values\"><location ref=\"B5:E20\"/><rowFields count=\"1\"><field x=\"0\"/></rowFields><colFields count=\"1\"><field x=\"1\"/></colFields><pageFields count=\"1\"><pageField fld=\"3\"/></pageFields><dataFields count=\"1\"><dataField fld=\"2\" subtotal=\"sum\" name=\"Total Amount\"/></dataFields></pivotTableDefinition>".to_vec()),
            ("xl/pivotTables/_rels/pivotTable1.xml.rels".to_owned(), b"<Relationships><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/pivotCacheDefinition\" Target=\"../pivotCache/pivotCacheDefinition1.xml\"/></Relationships>".to_vec()),
            ("xl/pivotCache/pivotCacheDefinition1.xml".to_owned(), b"<pivotCacheDefinition><cacheSource type=\"worksheet\"><worksheetSource sheet=\"Data\" ref=\"A1:D10\"/></cacheSource><cacheFields count=\"4\"><cacheField name=\"Region\"/><cacheField name=\"Month\"/><cacheField name=\"Amount\"/><cacheField name=\"Segment\"/></cacheFields></pivotCacheDefinition>".to_vec()),
        ])).unwrap();
        let loaded = crate::reader::read_workbook(&path).unwrap();
        let pivot = &loaded.sheets[1].pivot_tables[0];
        assert_eq!(pivot.name, "SalesPivot");
        assert_eq!(pivot.source_range, "'Data'!A1:D10");
        assert_eq!(pivot.location, "B5:E20");
        assert_eq!(pivot.rows, ["Region"]);
        assert_eq!(pivot.cols, ["Month"]);
        assert_eq!(pivot.filters, ["Segment"]);
        assert_eq!(pivot.values[0].field, "Amount");
        assert!(matches!(pivot.values[0].aggregate, PivotAggregate::Sum));
        assert_eq!(
            pivot.values[0].display_name.as_deref(),
            Some("Total Amount")
        );
    }
}
