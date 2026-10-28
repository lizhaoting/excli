//! Relationships and part locations inside an OOXML workbook package.

use std::collections::HashMap;
use std::io::{Read, Seek};
use std::path::Path;

use excli_core::error::{ErrorCode, ExcliError, ExcliResult};
use quick_xml::events::Event;
use quick_xml::Reader;
use zip::ZipArchive;

#[derive(Debug, Clone)]
pub struct SheetPart {
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone)]
pub struct PackageIndex {
    pub workbook_path: String,
    pub shared_strings_path: Option<String>,
    pub styles_path: Option<String>,
    pub sheets: Vec<SheetPart>,
}

fn corrupted(message: impl Into<String>) -> ExcliError {
    ExcliError::new(ErrorCode::FileCorrupted, message)
}

pub fn read_part<R: Read + Seek>(archive: &mut ZipArchive<R>, path: &str) -> ExcliResult<Vec<u8>> {
    let mut entry = archive
        .by_name(path)
        .map_err(|e| corrupted(format!("Missing package part {path}: {e}")))?;
    let mut bytes = Vec::new();
    entry.read_to_end(&mut bytes)?;
    Ok(bytes)
}

pub(crate) fn relationships(xml: &[u8]) -> ExcliResult<HashMap<String, (String, String)>> {
    let mut reader = Reader::from_reader(xml);
    let mut result = HashMap::new();
    loop {
        match reader
            .read_event()
            .map_err(|e| corrupted(format!("Invalid relationships XML: {e}")))?
        {
            Event::Start(event) | Event::Empty(event)
                if event.local_name().as_ref() == b"Relationship" =>
            {
                let mut id = None;
                let mut kind = None;
                let mut target = None;
                let mut external = false;
                for attribute in event.attributes() {
                    let attribute = attribute
                        .map_err(|e| corrupted(format!("Invalid relationship attribute: {e}")))?;
                    let value = attribute
                        .unescape_value()
                        .map_err(|e| corrupted(format!("Invalid relationship value: {e}")))?
                        .into_owned();
                    match attribute.key.as_ref() {
                        b"Id" => id = Some(value),
                        b"Type" => kind = Some(value),
                        b"Target" => target = Some(value),
                        b"TargetMode" if value == "External" => external = true,
                        _ => {}
                    }
                }
                if !external {
                    if let (Some(id), Some(kind), Some(target)) = (id, kind, target) {
                        if result.insert(id, (kind, target)).is_some() {
                            return Err(corrupted("Duplicate relationship ID"));
                        }
                    }
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(result)
}

pub(crate) fn resolve_part(source: &str, target: &str) -> ExcliResult<String> {
    let mut parts = Vec::new();
    if !target.starts_with('/') {
        if let Some(parent) = Path::new(source).parent().and_then(|p| p.to_str()) {
            parts.extend(parent.split('/').filter(|segment| !segment.is_empty()));
        }
    }
    for segment in target.trim_start_matches('/').split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                if parts.pop().is_none() {
                    return Err(corrupted("Relationship target escapes the package"));
                }
            }
            _ => parts.push(segment),
        }
    }
    if parts.is_empty() {
        return Err(corrupted("Empty relationship target"));
    }
    Ok(parts.join("/"))
}

pub(crate) fn rels_path(part: &str) -> ExcliResult<String> {
    let path = Path::new(part);
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| corrupted("Invalid workbook part name"))?;
    let parent = path
        .parent()
        .and_then(|parent| parent.to_str())
        .unwrap_or("");
    if parent.is_empty() {
        Ok(format!("_rels/{name}.rels"))
    } else {
        Ok(format!("{parent}/_rels/{name}.rels"))
    }
}

/// Resolve internal relationships of a package part to their target paths.
pub(crate) fn related_parts<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    source: &str,
) -> ExcliResult<HashMap<String, (String, String)>> {
    let path = rels_path(source)?;
    if !archive.file_names().any(|name| name == path) {
        return Ok(HashMap::new());
    }
    relationships(&read_part(archive, &path)?)?
        .into_iter()
        .map(|(id, (kind, target))| Ok((id, (kind, resolve_part(source, &target)?))))
        .collect()
}

fn workbook_sheets(xml: &[u8]) -> ExcliResult<Vec<(String, String)>> {
    let mut reader = Reader::from_reader(xml);
    let mut sheets = Vec::new();
    loop {
        match reader
            .read_event()
            .map_err(|e| corrupted(format!("Invalid workbook XML: {e}")))?
        {
            Event::Start(event) | Event::Empty(event)
                if event.local_name().as_ref() == b"sheet" =>
            {
                let mut name = None;
                let mut relationship_id = None;
                for attribute in event.attributes() {
                    let attribute = attribute
                        .map_err(|e| corrupted(format!("Invalid sheet attribute: {e}")))?;
                    let key = attribute.key.as_ref();
                    let value = attribute
                        .unescape_value()
                        .map_err(|e| corrupted(format!("Invalid sheet name: {e}")))?
                        .into_owned();
                    if key == b"name" {
                        name = Some(value);
                    } else if key.ends_with(b":id") {
                        relationship_id = Some(value);
                    }
                }
                if let (Some(name), Some(id)) = (name, relationship_id) {
                    sheets.push((name, id));
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(sheets)
}

pub fn index<R: Read + Seek>(archive: &mut ZipArchive<R>) -> ExcliResult<PackageIndex> {
    let root_rels = relationships(&read_part(archive, "_rels/.rels")?)?;
    let workbook_target = root_rels
        .values()
        .find(|(kind, _)| kind.ends_with("/officeDocument"))
        .map(|(_, target)| target)
        .ok_or_else(|| corrupted("Package has no workbook relationship"))?;
    let workbook_path = resolve_part("", workbook_target)?;
    let workbook_xml = read_part(archive, &workbook_path)?;
    let workbook_rels = relationships(&read_part(archive, &rels_path(&workbook_path)?)?)?;
    let mut sheets = Vec::new();
    for (name, id) in workbook_sheets(&workbook_xml)? {
        let (kind, target) = workbook_rels
            .get(&id)
            .ok_or_else(|| corrupted(format!("Sheet {name} has no relationship")))?;
        if kind.ends_with("/worksheet") {
            sheets.push(SheetPart {
                name,
                path: resolve_part(&workbook_path, target)?,
            });
        }
    }
    let shared_strings_path = workbook_rels
        .values()
        .find(|(kind, _)| kind.ends_with("/sharedStrings"))
        .map(|(_, target)| resolve_part(&workbook_path, target))
        .transpose()?;
    let styles_path = workbook_rels
        .values()
        .find(|(kind, _)| kind.ends_with("/styles"))
        .map(|(_, target)| resolve_part(&workbook_path, target))
        .transpose()?;
    Ok(PackageIndex {
        workbook_path,
        shared_strings_path,
        styles_path,
        sheets,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    #[test]
    fn relative_and_absolute_targets_resolve() {
        assert_eq!(
            resolve_part("xl/workbook.xml", "worksheets/data.xml").unwrap(),
            "xl/worksheets/data.xml"
        );
        assert_eq!(
            resolve_part("xl/workbook.xml", "/custom/data.xml").unwrap(),
            "custom/data.xml"
        );
        assert!(resolve_part("xl/workbook.xml", "../../escape.xml").is_err());
    }

    #[test]
    fn finds_worksheet_and_shared_strings_from_relationships() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("custom.xlsx");
        let mut writer = ZipWriter::new(std::fs::File::create(&path).unwrap());
        let parts = [
            (
                "_rels/.rels",
                r#"<Relationships><Relationship Id="r1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="custom/book.xml"/></Relationships>"#,
            ),
            (
                "custom/book.xml",
                r#"<workbook xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Data" sheetId="1" r:id="r9"/></sheets></workbook>"#,
            ),
            (
                "custom/_rels/book.xml.rels",
                r#"<Relationships><Relationship Id="r9" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="sheets/data.xml"/><Relationship Id="r10" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings" Target="../strings.xml"/></Relationships>"#,
            ),
            ("custom/sheets/data.xml", "<worksheet/>"),
        ];
        for (name, body) in parts {
            writer
                .start_file(name, SimpleFileOptions::default())
                .unwrap();
            writer.write_all(body.as_bytes()).unwrap();
        }
        writer.finish().unwrap();

        let mut archive = ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
        let found = index(&mut archive).unwrap();
        assert_eq!(found.workbook_path, "custom/book.xml");
        assert_eq!(found.sheets[0].path, "custom/sheets/data.xml");
        assert_eq!(found.shared_strings_path.as_deref(), Some("strings.xml"));
    }
}
