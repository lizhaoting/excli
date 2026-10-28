//! Read external workbook references from OOXML package relationships.

use std::io::{Read, Seek};

use excli_core::error::{ErrorCode, ExcliError, ExcliResult};
use excli_core::workbook::ExternalLink;
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use zip::ZipArchive;

use crate::ooxml_package::{read_part, relationships, rels_path, resolve_part};

fn malformed(message: impl Into<String>) -> ExcliError {
    ExcliError::new(ErrorCode::FileCorrupted, message)
}

fn attribute(tag: &BytesStart<'_>, key: &[u8]) -> ExcliResult<Option<String>> {
    for item in tag.attributes() {
        let item =
            item.map_err(|error| malformed(format!("Invalid external link attribute: {error}")))?;
        if item.key.as_ref() == key {
            return Ok(Some(
                item.unescape_value()
                    .map_err(|error| malformed(format!("Invalid external link value: {error}")))?
                    .into_owned(),
            ));
        }
    }
    Ok(None)
}

fn relationship_id(tag: &BytesStart<'_>) -> ExcliResult<Option<String>> {
    for item in tag.attributes() {
        let item =
            item.map_err(|error| malformed(format!("Invalid external link attribute: {error}")))?;
        if item.key.as_ref().ends_with(b":id") {
            return Ok(Some(
                item.unescape_value()
                    .map_err(|error| malformed(format!("Invalid external link ID: {error}")))?
                    .into_owned(),
            ));
        }
    }
    Ok(None)
}

fn workbook_reference_ids(xml: &[u8]) -> ExcliResult<Vec<String>> {
    let mut reader = Reader::from_reader(xml);
    let mut in_references = false;
    let mut ids = Vec::new();
    loop {
        match reader
            .read_event()
            .map_err(|error| malformed(format!("Invalid workbook XML: {error}")))?
        {
            Event::Start(ref tag) if tag.local_name().as_ref() == b"externalReferences" => {
                in_references = true;
            }
            Event::End(ref tag) if tag.local_name().as_ref() == b"externalReferences" => {
                in_references = false;
            }
            Event::Start(ref tag) | Event::Empty(ref tag)
                if in_references && tag.local_name().as_ref() == b"externalReference" =>
            {
                ids.push(
                    relationship_id(tag)?
                        .ok_or_else(|| malformed("External reference has no relationship ID"))?,
                );
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(ids)
}

fn external_book(xml: &[u8]) -> ExcliResult<Option<(String, Vec<String>)>> {
    let mut reader = Reader::from_reader(xml);
    let mut book_id = None;
    let mut in_sheet_names = false;
    let mut sheet_names = Vec::new();
    loop {
        match reader
            .read_event()
            .map_err(|error| malformed(format!("Invalid external link XML: {error}")))?
        {
            Event::Start(ref tag) | Event::Empty(ref tag)
                if tag.local_name().as_ref() == b"externalBook" =>
            {
                book_id = Some(
                    relationship_id(tag)?
                        .ok_or_else(|| malformed("External book has no relationship ID"))?,
                );
            }
            Event::Start(ref tag) if tag.local_name().as_ref() == b"sheetNames" => {
                in_sheet_names = true;
            }
            Event::End(ref tag) if tag.local_name().as_ref() == b"sheetNames" => {
                in_sheet_names = false;
            }
            Event::Start(ref tag) | Event::Empty(ref tag)
                if in_sheet_names && tag.local_name().as_ref() == b"sheetName" =>
            {
                sheet_names.push(
                    attribute(tag, b"val")?
                        .ok_or_else(|| malformed("External sheet name has no value"))?,
                );
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(book_id.map(|id| (id, sheet_names)))
}

fn external_target(xml: &[u8], id: &str) -> ExcliResult<String> {
    let mut reader = Reader::from_reader(xml);
    loop {
        match reader
            .read_event()
            .map_err(|error| malformed(format!("Invalid external link relationships: {error}")))?
        {
            Event::Start(ref tag) | Event::Empty(ref tag)
                if tag.local_name().as_ref() == b"Relationship"
                    && attribute(tag, b"Id")?.as_deref() == Some(id) =>
            {
                if !attribute(tag, b"Type")?
                    .as_deref()
                    .is_some_and(|kind| kind.ends_with("/externalLinkPath"))
                    || attribute(tag, b"TargetMode")?.as_deref() != Some("External")
                {
                    return Err(malformed(
                        "External book relationship has an unexpected type",
                    ));
                }
                return attribute(tag, b"Target")?
                    .ok_or_else(|| malformed("External book relationship has no target"));
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Err(malformed("Missing external book path relationship"))
}

fn is_relative_path(path: &str) -> bool {
    if path.starts_with('/') || path.starts_with("\\\\") {
        return false;
    }
    if let Some((scheme, _)) = path.split_once(':') {
        if scheme
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphabetic)
            && scheme
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.'))
        {
            return false;
        }
    }
    true
}

pub(crate) fn read_external_links<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    workbook_path: &str,
    workbook_xml: &[u8],
) -> ExcliResult<Vec<ExternalLink>> {
    let ids = workbook_reference_ids(workbook_xml)?;
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let workbook_relations = relationships(&read_part(archive, &rels_path(workbook_path)?)?)?;
    let mut links = Vec::new();
    for id in ids {
        let (kind, target) = workbook_relations
            .get(&id)
            .ok_or_else(|| malformed("Missing external link relationship"))?;
        if !kind.ends_with("/externalLink") {
            return Err(malformed("External reference targets a non-link part"));
        }
        let path = resolve_part(workbook_path, target)?;
        let Some((book_id, sheet_names)) = external_book(&read_part(archive, &path)?)? else {
            continue;
        };
        let target = external_target(&read_part(archive, &rels_path(&path)?)?, &book_id)?;
        links.push(ExternalLink {
            is_relative: is_relative_path(&target),
            path: target,
            sheet_names,
        });
    }
    Ok(links)
}

#[cfg(test)]
mod tests {
    use super::*;
    use excli_core::Workbook;
    use std::collections::HashMap;

    use crate::ooxml_edit::write_package_parts;

    #[test]
    fn parses_external_book_sheets_and_path_mode() {
        let xml = br#"<externalLink xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><externalBook r:id="rId1"><sheetNames><sheetName val="Data"/><sheetName val="Budget &amp; Plan"/></sheetNames></externalBook></externalLink>"#;
        let (id, names) = external_book(xml).unwrap().unwrap();
        assert_eq!(id, "rId1");
        assert_eq!(names, ["Data", "Budget & Plan"]);
        assert!(is_relative_path("../budget.xlsx"));
        assert!(!is_relative_path("file:///C:/budget.xlsx"));
        assert!(!is_relative_path("file:budget.xlsx"));
    }

    #[test]
    fn workbook_reader_follows_external_reference_order() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("linked.xlsx");
        let mut workbook = Workbook::new();
        workbook.add_sheet("Data");
        crate::writer::write_xlsx(&path, &workbook).unwrap();
        let mut archive = ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
        let workbook_xml =
            String::from_utf8(read_part(&mut archive, "xl/workbook.xml").unwrap()).unwrap();
        let workbook_xml = workbook_xml.replace("</workbook>", "<externalReferences xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><externalReference r:id=\"rId21\"/><externalReference r:id=\"rId20\"/></externalReferences></workbook>");
        let workbook_rels =
            String::from_utf8(read_part(&mut archive, "xl/_rels/workbook.xml.rels").unwrap())
                .unwrap();
        let workbook_rels = workbook_rels.replace("</Relationships>", "<Relationship Id=\"rId20\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/externalLink\" Target=\"externalLinks/externalLink1.xml\"/><Relationship Id=\"rId21\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/externalLink\" Target=\"externalLinks/externalLink2.xml\"/></Relationships>");
        let types =
            String::from_utf8(read_part(&mut archive, "[Content_Types].xml").unwrap()).unwrap();
        let types = types.replace("</Types>", "<Override PartName=\"/xl/externalLinks/externalLink1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.externalLink+xml\"/><Override PartName=\"/xl/externalLinks/externalLink2.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.externalLink+xml\"/></Types>");
        write_package_parts(&path, &mut archive, &HashMap::from([
            ("xl/workbook.xml".to_owned(), workbook_xml.into_bytes()),
            ("xl/_rels/workbook.xml.rels".to_owned(), workbook_rels.into_bytes()),
            ("[Content_Types].xml".to_owned(), types.into_bytes()),
        ]), &HashMap::from([
            ("xl/externalLinks/externalLink1.xml".to_owned(), b"<externalLink xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><externalBook r:id=\"rId1\"><sheetNames><sheetName val=\"Old\"/></sheetNames></externalBook></externalLink>".to_vec()),
            ("xl/externalLinks/_rels/externalLink1.xml.rels".to_owned(), b"<Relationships><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/externalLinkPath\" Target=\"../old.xlsx\" TargetMode=\"External\"/></Relationships>".to_vec()),
            ("xl/externalLinks/externalLink2.xml".to_owned(), b"<externalLink xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><externalBook r:id=\"rId2\"><sheetNames><sheetName val=\"New\"/></sheetNames></externalBook></externalLink>".to_vec()),
            ("xl/externalLinks/_rels/externalLink2.xml.rels".to_owned(), b"<Relationships><Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/externalLinkPath\" Target=\"file:///tmp/new.xlsx\" TargetMode=\"External\"/></Relationships>".to_vec()),
        ])).unwrap();
        let loaded = crate::reader::read_workbook(&path).unwrap();
        assert_eq!(loaded.external_links.len(), 2);
        assert_eq!(loaded.external_links[0].path, "file:///tmp/new.xlsx");
        assert!(!loaded.external_links[0].is_relative);
        assert_eq!(loaded.external_links[0].sheet_names, ["New"]);
        assert_eq!(loaded.external_links[1].path, "../old.xlsx");
        assert!(loaded.external_links[1].is_relative);
        assert_eq!(loaded.external_links[1].sheet_names, ["Old"]);
    }
}
