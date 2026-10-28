//! Remove a worksheet and package parts owned exclusively by it.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use excli_core::error::{ErrorCode, ExcliError, ExcliResult};
use excli_core::types::FileFormat;
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use zip::ZipArchive;

use crate::format_detect::detect_format;
use crate::ooxml_edit::{ensure_editable, write_package_changes, xml_escape};
use crate::ooxml_package::{
    index, read_part, related_parts, relationships, rels_path, resolve_part,
};
use crate::ooxml_sheet_rename::has_old_reference;

fn corrupted(message: impl Into<String>) -> ExcliError {
    ExcliError::new(ErrorCode::FileCorrupted, message)
}
fn unsupported(message: impl Into<String>) -> ExcliError {
    ExcliError::new(ErrorCode::NotImplemented, message)
}
fn xml_error(error: impl std::fmt::Display) -> ExcliError {
    corrupted(format!("Invalid workbook package XML: {error}"))
}

fn attr(tag: &BytesStart<'_>, key: &[u8]) -> ExcliResult<Option<String>> {
    for item in tag.attributes() {
        let item = item.map_err(xml_error)?;
        if item.key.as_ref() == key {
            return Ok(Some(item.unescape_value().map_err(xml_error)?.into_owned()));
        }
    }
    Ok(None)
}

fn replace_tag(
    tag: &BytesStart<'_>,
    original: &[u8],
    edits: &[(&str, String)],
) -> ExcliResult<Vec<u8>> {
    let qualified = std::str::from_utf8(tag.name().as_ref())
        .map_err(xml_error)?
        .to_owned();
    let mut output = format!("<{qualified}");
    for item in tag.attributes() {
        let item = item.map_err(xml_error)?;
        let key = std::str::from_utf8(item.key.as_ref()).map_err(xml_error)?;
        let value = edits
            .iter()
            .find(|(name, _)| *name == key)
            .map(|(_, value)| value.clone())
            .unwrap_or(item.unescape_value().map_err(xml_error)?.into_owned());
        output.push_str(&format!(" {key}=\"{}\"", xml_escape(&value)));
    }
    output.push_str(if original.ends_with(b"/>") { "/>" } else { ">" });
    Ok(output.into_bytes())
}

fn apply_edits(xml: &[u8], mut edits: Vec<(usize, usize, Vec<u8>)>) -> ExcliResult<Vec<u8>> {
    edits.sort_by_key(|(from, _, _)| *from);
    let mut result = Vec::with_capacity(xml.len());
    let mut cursor = 0;
    for (from, to, replacement) in edits {
        if from < cursor || to > xml.len() {
            return Err(corrupted("Overlapping XML edits"));
        }
        result.extend_from_slice(&xml[cursor..from]);
        result.extend_from_slice(&replacement);
        cursor = to;
    }
    result.extend_from_slice(&xml[cursor..]);
    Ok(result)
}

struct SheetTarget {
    index: usize,
    rel_id: String,
    count: usize,
}

fn sheet_target(xml: &[u8], name: &str) -> ExcliResult<SheetTarget> {
    let mut reader = Reader::from_reader(xml);
    let mut in_sheets = false;
    let mut target = None;
    let mut count = 0usize;
    let mut visible_count = 0usize;
    loop {
        match reader.read_event().map_err(xml_error)? {
            Event::Start(ref tag) if tag.local_name().as_ref() == b"sheets" => in_sheets = true,
            Event::End(ref tag) if tag.local_name().as_ref() == b"sheets" => in_sheets = false,
            Event::Empty(ref tag) if in_sheets && tag.local_name().as_ref() == b"sheet" => {
                let current = attr(tag, b"name")?.ok_or_else(|| corrupted("Sheet has no name"))?;
                let visible = !matches!(
                    attr(tag, b"state")?.as_deref(),
                    Some("hidden" | "veryHidden")
                );
                if visible {
                    visible_count += 1;
                }
                if current == name {
                    let mut rel_id = None;
                    for item in tag.attributes() {
                        let item = item.map_err(xml_error)?;
                        if item.key.as_ref().ends_with(b":id") {
                            rel_id = Some(item.unescape_value().map_err(xml_error)?.into_owned());
                        }
                    }
                    target = Some((
                        count,
                        rel_id.ok_or_else(|| corrupted("Sheet has no relationship ID"))?,
                        visible,
                    ));
                }
                count += 1;
            }
            Event::Start(ref tag) if in_sheets && tag.local_name().as_ref() == b"sheet" => {
                return Err(unsupported("Nonempty sheet elements cannot be deleted"))
            }
            Event::Eof => break,
            _ => {}
        }
    }
    let (index, rel_id, visible) = target.ok_or_else(|| ExcliError::sheet_not_found(name))?;
    if count <= 1 || (visible && visible_count <= 1) {
        return Err(ExcliError::new(
            ErrorCode::ValidationError,
            "Deleting this sheet would leave no visible worksheet",
        ));
    }
    Ok(SheetTarget {
        index,
        rel_id,
        count,
    })
}

fn adjust_index(value: &str, deleted: usize, remaining: usize) -> ExcliResult<String> {
    let old = value
        .parse::<usize>()
        .map_err(|_| corrupted("Invalid sheet index"))?;
    Ok(if old > deleted {
        old - 1
    } else if old == deleted {
        deleted.min(remaining - 1)
    } else {
        old
    }
    .to_string())
}

fn remove_from_workbook(xml: &[u8], name: &str, target: &SheetTarget) -> ExcliResult<Vec<u8>> {
    let mut reader = Reader::from_reader(xml);
    let mut edits = Vec::new();
    let mut scoped_name_start = None;
    loop {
        let from = reader.buffer_position() as usize;
        let event = reader.read_event().map_err(xml_error)?;
        let to = reader.buffer_position() as usize;
        match event {
            Event::Empty(ref tag)
                if tag.local_name().as_ref() == b"sheet"
                    && attr(tag, b"name")?.as_deref() == Some(name) =>
            {
                edits.push((from, to, Vec::new()))
            }
            Event::Start(ref tag) | Event::Empty(ref tag)
                if tag.local_name().as_ref() == b"workbookView" =>
            {
                let mut replacements = Vec::new();
                for key in [b"activeTab".as_slice(), b"firstSheet".as_slice()] {
                    if let Some(value) = attr(tag, key)? {
                        replacements.push((
                            std::str::from_utf8(key).map_err(xml_error)?,
                            adjust_index(&value, target.index, target.count - 1)?,
                        ));
                    }
                }
                if !replacements.is_empty() {
                    edits.push((from, to, replace_tag(tag, &xml[from..to], &replacements)?));
                }
            }
            Event::Start(ref tag) if tag.local_name().as_ref() == b"definedName" => {
                if let Some(local) = attr(tag, b"localSheetId")? {
                    let local = local
                        .parse::<usize>()
                        .map_err(|_| corrupted("Invalid localSheetId"))?;
                    if local == target.index {
                        scoped_name_start = Some(from);
                    } else if local > target.index {
                        edits.push((
                            from,
                            to,
                            replace_tag(
                                tag,
                                &xml[from..to],
                                &[("localSheetId", (local - 1).to_string())],
                            )?,
                        ));
                    }
                }
            }
            Event::Empty(ref tag) if tag.local_name().as_ref() == b"definedName" => {
                if let Some(local) = attr(tag, b"localSheetId")? {
                    let local = local
                        .parse::<usize>()
                        .map_err(|_| corrupted("Invalid localSheetId"))?;
                    if local == target.index {
                        edits.push((from, to, Vec::new()));
                    } else if local > target.index {
                        edits.push((
                            from,
                            to,
                            replace_tag(
                                tag,
                                &xml[from..to],
                                &[("localSheetId", (local - 1).to_string())],
                            )?,
                        ));
                    }
                }
            }
            Event::End(ref tag) if tag.local_name().as_ref() == b"definedName" => {
                if let Some(start) = scoped_name_start.take() {
                    edits.push((start, to, Vec::new()));
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    apply_edits(xml, edits)
}

fn remove_relationship(xml: &[u8], id: &str) -> ExcliResult<Vec<u8>> {
    let mut reader = Reader::from_reader(xml);
    let mut edit = None;
    loop {
        let from = reader.buffer_position() as usize;
        match reader.read_event().map_err(xml_error)? {
            Event::Empty(ref tag)
                if tag.local_name().as_ref() == b"Relationship"
                    && attr(tag, b"Id")?.as_deref() == Some(id) =>
            {
                edit = Some((from, reader.buffer_position() as usize))
            }
            Event::Eof => break,
            _ => {}
        }
    }
    let (from, to) = edit.ok_or_else(|| corrupted("Missing worksheet relationship"))?;
    apply_edits(xml, vec![(from, to, Vec::new())])
}

fn remove_content_types(xml: &[u8], removed: &HashSet<String>) -> ExcliResult<Vec<u8>> {
    let mut reader = Reader::from_reader(xml);
    let mut edits = Vec::new();
    loop {
        let from = reader.buffer_position() as usize;
        match reader.read_event().map_err(xml_error)? {
            Event::Empty(ref tag) if tag.local_name().as_ref() == b"Override" => {
                if let Some(part) = attr(tag, b"PartName")? {
                    if removed.contains(part.trim_start_matches('/')) {
                        edits.push((from, reader.buffer_position() as usize, Vec::new()));
                    }
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    apply_edits(xml, edits)
}

fn remove_app_title(xml: &[u8], name: &str) -> ExcliResult<Vec<u8>> {
    let mut reader = Reader::from_reader(xml);
    let mut in_headings = false;
    let mut in_titles = false;
    let mut in_lpstr = false;
    let mut heading_label = String::new();
    let mut count_span = None;
    let mut count = None;
    let mut vector_open = None;
    let mut title_start = None;
    let mut title_matches = Vec::new();
    loop {
        let from = reader.buffer_position() as usize;
        let event = reader.read_event().map_err(xml_error)?;
        let to = reader.buffer_position() as usize;
        match event {
            Event::Start(ref tag) if tag.local_name().as_ref() == b"HeadingPairs" => {
                in_headings = true
            }
            Event::End(ref tag) if tag.local_name().as_ref() == b"HeadingPairs" => {
                in_headings = false
            }
            Event::Start(ref tag) if tag.local_name().as_ref() == b"TitlesOfParts" => {
                in_titles = true
            }
            Event::End(ref tag) if tag.local_name().as_ref() == b"TitlesOfParts" => {
                in_titles = false
            }
            Event::Start(ref tag) if in_titles && tag.local_name().as_ref() == b"vector" => {
                vector_open = Some((from, to))
            }
            Event::Start(ref tag)
                if (in_headings || in_titles) && tag.local_name().as_ref() == b"lpstr" =>
            {
                in_lpstr = true;
                if in_titles {
                    title_start = Some(from);
                }
            }
            Event::Text(ref text) if in_lpstr => {
                let value = text.unescape().map_err(xml_error)?;
                if in_headings {
                    heading_label = value.into_owned();
                } else if in_titles && value.as_ref() == name {
                    if let Some(start) = title_start {
                        title_matches.push((start, 0));
                    }
                }
            }
            Event::Text(ref text) if in_headings && heading_label == "Worksheets" => {
                let value = text.unescape().map_err(xml_error)?;
                if let Ok(parsed) = value.parse::<usize>() {
                    count = Some(parsed);
                    count_span = Some((from, to));
                    heading_label.clear();
                }
            }
            Event::End(ref tag) if tag.local_name().as_ref() == b"lpstr" => {
                if in_titles {
                    if let Some(start) = title_start.take() {
                        if let Some(last) = title_matches.last_mut() {
                            if last.0 == start && last.1 == 0 {
                                last.1 = to;
                            }
                        }
                    }
                }
                in_lpstr = false;
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if vector_open.is_none() {
        return Ok(xml.to_vec());
    }
    if title_matches.len() != 1 || title_matches[0].1 == 0 {
        return Err(unsupported(
            "Extended properties do not identify one worksheet title",
        ));
    }
    let (vector_from, vector_to) = vector_open.ok_or_else(|| corrupted("Missing titles vector"))?;
    let open = std::str::from_utf8(&xml[vector_from..vector_to]).map_err(xml_error)?;
    let size_start = open
        .find("size=\"")
        .ok_or_else(|| corrupted("Missing titles vector size"))?
        + 6;
    let size_end = size_start
        + open[size_start..]
            .find('"')
            .ok_or_else(|| corrupted("Invalid titles vector size"))?;
    let size = open[size_start..size_end]
        .parse::<usize>()
        .map_err(|_| corrupted("Invalid titles vector size"))?;
    let count = count.ok_or_else(|| corrupted("Missing Worksheets count"))?;
    let count_span = count_span.ok_or_else(|| corrupted("Missing Worksheets count"))?;
    if size == 0 || count == 0 {
        return Err(corrupted("Invalid extended-properties counts"));
    }
    apply_edits(
        xml,
        vec![
            (
                vector_from + size_start,
                vector_from + size_end,
                (size - 1).to_string().into_bytes(),
            ),
            (
                count_span.0,
                count_span.1,
                (count - 1).to_string().into_bytes(),
            ),
            (title_matches[0].0, title_matches[0].1, Vec::new()),
        ],
    )
}

fn removable_parts(
    archive: &mut ZipArchive<std::fs::File>,
    sheet_path: &str,
) -> ExcliResult<HashSet<String>> {
    let names: Vec<String> = archive.file_names().map(str::to_owned).collect();
    let existing: HashSet<String> = names.iter().cloned().collect();
    let mut outgoing: HashMap<String, Vec<String>> = HashMap::new();
    let mut incoming: HashMap<String, HashSet<String>> = HashMap::new();
    for (id, (_, target)) in relationships(&read_part(archive, "_rels/.rels")?)? {
        let part = resolve_part("", &target)?;
        incoming
            .entry(part)
            .or_default()
            .insert(format!("root:{id}"));
    }
    for source in &names {
        if source.ends_with(".rels") || source.ends_with('/') || source == "[Content_Types].xml" {
            continue;
        }
        for (_, (_, target)) in related_parts(archive, source)? {
            incoming
                .entry(target.clone())
                .or_default()
                .insert(source.clone());
            outgoing.entry(source.clone()).or_default().push(target);
        }
    }
    let mut removed = HashSet::from([sheet_path.to_owned()]);
    loop {
        let mut changed = false;
        let candidates: Vec<String> = removed
            .iter()
            .flat_map(|source| outgoing.get(source).into_iter().flatten().cloned())
            .collect();
        for target in candidates {
            if !existing.contains(&target) {
                return Err(corrupted(format!("Missing related package part: {target}")));
            }
            if !removed.contains(&target)
                && incoming
                    .get(&target)
                    .is_some_and(|sources| sources.iter().all(|source| removed.contains(source)))
            {
                removed.insert(target);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    let rels: Vec<String> = removed
        .iter()
        .filter_map(|part| rels_path(part).ok())
        .filter(|path| existing.contains(path))
        .collect();
    removed.extend(rels);
    Ok(removed)
}

/// Delete one worksheet from an XLSX/XLSM package while retaining all
/// unrelated ZIP members. References from retained parts cause an error.
pub fn delete_sheet(path: &Path, name: &str) -> ExcliResult<()> {
    if !matches!(detect_format(path)?, FileFormat::Xlsx | FileFormat::Xlsm) {
        return Err(unsupported(
            "Package-preserving sheet delete supports XLSX and XLSM",
        ));
    }
    let mut archive = ZipArchive::new(std::fs::File::open(path)?)
        .map_err(|error| corrupted(format!("Invalid Excel ZIP: {error}")))?;
    ensure_editable(&mut archive)?;
    if archive
        .file_names()
        .any(|part| part.ends_with("vbaProject.bin") || part.ends_with("calcChain.xml"))
    {
        return Err(unsupported(
            "Deleting sheets with VBA projects or calculation chains needs additional updates",
        ));
    }
    let package = index(&mut archive)?;
    let workbook_xml = read_part(&mut archive, &package.workbook_path)?;
    let target = sheet_target(&workbook_xml, name)?;
    let sheet_path = package
        .sheets
        .iter()
        .find(|sheet| sheet.name == name)
        .ok_or_else(|| ExcliError::sheet_not_found(name))?
        .path
        .clone();
    let removed = removable_parts(&mut archive, &sheet_path)?;
    let mut modified = HashMap::new();
    modified.insert(
        package.workbook_path.clone(),
        remove_from_workbook(&workbook_xml, name, &target)?,
    );
    let rels_name = rels_path(&package.workbook_path)?;
    modified.insert(
        rels_name.clone(),
        remove_relationship(&read_part(&mut archive, &rels_name)?, &target.rel_id)?,
    );
    modified.insert(
        "[Content_Types].xml".to_owned(),
        remove_content_types(&read_part(&mut archive, "[Content_Types].xml")?, &removed)?,
    );
    if archive.file_names().any(|part| part == "docProps/app.xml") {
        modified.insert(
            "docProps/app.xml".to_owned(),
            remove_app_title(&read_part(&mut archive, "docProps/app.xml")?, name)?,
        );
    }
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(xml_error)?;
        let part = entry.name().to_owned();
        if removed.contains(&part) || part == "docProps/app.xml" || !part.ends_with(".xml") {
            continue;
        }
        let mut xml = Vec::new();
        std::io::Read::read_to_end(&mut entry, &mut xml)?;
        drop(entry);
        let candidate = modified.get(&part).map_or(xml.as_slice(), Vec::as_slice);
        if has_old_reference(candidate, name)? {
            return Err(unsupported(format!(
                "Deleted sheet is still referenced in {part}"
            )));
        }
    }
    write_package_changes(path, &mut archive, &modified, &HashMap::new(), &removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use excli_core::cell::Cell;
    use excli_core::Workbook;

    #[test]
    fn deletes_sheet_without_rebuilding_other_sheet() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("book.xlsx");
        let mut workbook = Workbook::new();
        workbook
            .add_sheet("Delete")
            .set_cell(0, 0, Cell::string("discard"));
        workbook
            .add_sheet("Keep")
            .set_cell(0, 0, Cell::string("preserve"));
        crate::writer::write_xlsx(&path, &workbook).unwrap();
        let mut archive = ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
        let kept = read_part(&mut archive, "xl/worksheets/sheet2.xml").unwrap();
        drop(archive);
        delete_sheet(&path, "Delete").unwrap();
        let mut archive = ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
        assert!(archive.by_name("xl/worksheets/sheet1.xml").is_err());
        assert_eq!(
            read_part(&mut archive, "xl/worksheets/sheet2.xml").unwrap(),
            kept
        );
        let loaded = crate::reader::read_workbook(&path).unwrap();
        assert_eq!(loaded.sheets.len(), 1);
        assert_eq!(loaded.sheets[0].name, "Keep");
        assert_eq!(
            loaded.sheets[0].get_cell(0, 0).unwrap().value.to_string(),
            "preserve"
        );
    }

    #[test]
    fn referenced_sheet_is_not_deleted() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("book.xlsx");
        let mut workbook = Workbook::new();
        workbook
            .add_sheet("Delete")
            .set_cell(0, 0, Cell::number(1.0));
        workbook.add_sheet("Keep").set_cell(
            0,
            0,
            Cell::number(1.0).with_formula(excli_core::formula::Formula::new("=Delete!A1")),
        );
        crate::writer::write_xlsx(&path, &workbook).unwrap();
        let before = std::fs::read(&path).unwrap();
        assert!(delete_sheet(&path, "Delete").is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }

    #[test]
    fn app_metadata_drops_title_and_decrements_counts() {
        let app = br#"<Properties><HeadingPairs><vt:vector size="2"><vt:variant><vt:lpstr>Worksheets</vt:lpstr></vt:variant><vt:variant><vt:i4>2</vt:i4></vt:variant></vt:vector></HeadingPairs><TitlesOfParts><vt:vector size="2"><vt:lpstr>Delete</vt:lpstr><vt:lpstr>Keep</vt:lpstr></vt:vector></TitlesOfParts></Properties>"#;
        let updated = String::from_utf8(remove_app_title(app, "Delete").unwrap()).unwrap();
        assert!(updated.contains("<vt:i4>1</vt:i4>"));
        assert!(updated.contains("<vt:vector size=\"1\"><vt:lpstr>Keep</vt:lpstr>"));
        assert!(!updated.contains("<vt:lpstr>Delete</vt:lpstr>"));
    }

    #[test]
    fn adjusts_workbook_view_and_scoped_names() {
        let workbook = br#"<workbook><bookViews><workbookView activeTab="2" firstSheet="1"/></bookViews><sheets><sheet name="Delete" sheetId="1" r:id="rId1"/><sheet name="Keep" sheetId="2" r:id="rId2"/><sheet name="Later" sheetId="3" r:id="rId3"/></sheets><definedNames><definedName name="Old" localSheetId="0">$A$1</definedName><definedName name="KeepLocal" localSheetId="1">$B$1</definedName><definedName name="LaterLocal" localSheetId="2">$C$1</definedName></definedNames></workbook>"#;
        let target = sheet_target(workbook, "Delete").unwrap();
        let updated =
            String::from_utf8(remove_from_workbook(workbook, "Delete", &target).unwrap()).unwrap();
        assert!(updated.contains("activeTab=\"1\" firstSheet=\"0\""));
        assert!(!updated.contains("name=\"Old\""));
        assert!(updated.contains("name=\"KeepLocal\" localSheetId=\"0\""));
        assert!(updated.contains("name=\"LaterLocal\" localSheetId=\"1\""));
    }

    #[test]
    fn refuses_to_delete_last_visible_sheet() {
        let workbook = br#"<workbook><sheets><sheet name="Visible" r:id="rId1"/><sheet name="Hidden" state="hidden" r:id="rId2"/></sheets></workbook>"#;
        assert!(sheet_target(workbook, "Visible").is_err());
    }

    #[test]
    fn deletes_exclusive_table_part_and_its_content_type() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("book.xlsx");
        let mut workbook = Workbook::new();
        workbook.add_sheet("Delete");
        workbook.add_sheet("Keep");
        crate::writer::write_xlsx(&path, &workbook).unwrap();
        let mut archive = ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
        let worksheet_path = "xl/worksheets/sheet1.xml";
        let original = String::from_utf8(read_part(&mut archive, worksheet_path).unwrap()).unwrap();
        let worksheet = original.replace("</worksheet>", "<tableParts count=\"1\"><tablePart xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" r:id=\"rId7\"/></tableParts></worksheet>");
        let types =
            String::from_utf8(read_part(&mut archive, "[Content_Types].xml").unwrap()).unwrap();
        let types = types.replace("</Types>", "<Override PartName=\"/xl/tables/table1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.table+xml\"/></Types>");
        crate::ooxml_edit::write_package_parts(&path, &mut archive, &HashMap::from([
            (worksheet_path.to_owned(), worksheet.into_bytes()),
            ("[Content_Types].xml".to_owned(), types.into_bytes()),
        ]), &HashMap::from([
            ("xl/worksheets/_rels/sheet1.xml.rels".to_owned(), b"<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId7\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/table\" Target=\"../tables/table1.xml\"/></Relationships>".to_vec()),
            ("xl/tables/table1.xml".to_owned(), b"<table xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" id=\"1\" name=\"Table1\" displayName=\"Table1\" ref=\"A1:B2\"><tableColumns count=\"2\"><tableColumn id=\"1\" name=\"A\"/><tableColumn id=\"2\" name=\"B\"/></tableColumns></table>".to_vec()),
        ])).unwrap();
        delete_sheet(&path, "Delete").unwrap();
        let mut archive = ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
        assert!(archive.by_name("xl/tables/table1.xml").is_err());
        assert!(archive
            .by_name("xl/worksheets/_rels/sheet1.xml.rels")
            .is_err());
        let types =
            String::from_utf8(read_part(&mut archive, "[Content_Types].xml").unwrap()).unwrap();
        assert!(!types.contains("table1.xml"));
        assert_eq!(
            crate::reader::read_workbook(&path).unwrap().sheets[0].name,
            "Keep"
        );
    }
}
