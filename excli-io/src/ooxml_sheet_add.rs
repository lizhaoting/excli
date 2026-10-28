//! Add a worksheet by changing only the package manifest and workbook metadata.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use excli_core::error::{ErrorCode, ExcliError, ExcliResult};
use excli_core::types::FileFormat;
use quick_xml::events::Event;
use quick_xml::Reader;
use zip::ZipArchive;

use crate::format_detect::detect_format;
use crate::ooxml_edit::{ensure_editable, write_package_parts, xml_escape};
use crate::ooxml_package::{index as package_index, read_part, rels_path, resolve_part};
use crate::ooxml_pivot_read::pivot_part_ids;
use crate::ooxml_sheet_rename::{has_old_reference, rewrite_xml_references};
use crate::ooxml_table_read::table_part_ids;

fn corrupted(message: impl Into<String>) -> ExcliError {
    ExcliError::new(ErrorCode::FileCorrupted, message)
}

fn invalid_name(message: impl Into<String>) -> ExcliError {
    ExcliError::new(ErrorCode::ValidationError, message)
}

fn xml_error(error: impl std::fmt::Display) -> ExcliError {
    corrupted(format!("Invalid workbook package XML: {error}"))
}

pub(crate) fn validate_name(name: &str) -> ExcliResult<()> {
    if name.is_empty()
        || name.encode_utf16().count() > 31
        || name.starts_with('\'')
        || name.ends_with('\'')
        || name
            .chars()
            .any(|c| c.is_control() || "[]:*?/\\".contains(c))
    {
        return Err(invalid_name("Invalid Excel worksheet name"));
    }
    Ok(())
}

/// Insert a child before the closing root element, preserving the root's prefix.
fn append_child(
    xml: &[u8],
    root: &[u8],
    child: impl FnOnce(&str) -> String,
) -> ExcliResult<Vec<u8>> {
    let mut reader = Reader::from_reader(xml);
    let mut depth = 0usize;
    let mut insertion = None;
    let mut qualified = None;
    let mut empty = false;
    loop {
        let from = reader.buffer_position() as usize;
        let event = reader.read_event().map_err(xml_error)?;
        let to = reader.buffer_position() as usize;
        match event {
            Event::Start(ref tag) => {
                if depth == 0 {
                    if tag.local_name().as_ref() != root || qualified.is_some() {
                        return Err(corrupted("Unexpected package XML root"));
                    }
                    qualified = Some(String::from_utf8_lossy(tag.name().as_ref()).into_owned());
                }
                depth += 1;
            }
            Event::Empty(ref tag) if depth == 0 => {
                if tag.local_name().as_ref() != root || qualified.is_some() {
                    return Err(corrupted("Unexpected package XML root"));
                }
                qualified = Some(String::from_utf8_lossy(tag.name().as_ref()).into_owned());
                insertion = Some((from, to));
                empty = true;
            }
            Event::End(_) => {
                if depth == 0 {
                    return Err(corrupted("Unexpected closing package XML tag"));
                }
                depth -= 1;
                if depth == 0 {
                    insertion = Some((from, to));
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if depth != 0 {
        return Err(corrupted("Unclosed package XML root"));
    }
    let (from, to) = insertion.ok_or_else(|| corrupted("Missing package XML root"))?;
    let qualified = qualified.ok_or_else(|| corrupted("Missing package XML root"))?;
    let prefix = qualified.rsplit_once(':').map_or("", |(prefix, _)| prefix);
    let child = child(prefix);
    let mut result = Vec::with_capacity(xml.len() + child.len() + 32);
    if empty {
        let opening = &xml[from..to];
        let slash = opening
            .iter()
            .rposition(|&byte| byte == b'/')
            .ok_or_else(|| corrupted("Invalid empty package XML root"))?;
        result.extend_from_slice(&xml[..from]);
        result.extend_from_slice(&opening[..slash]);
        result.push(b'>');
        result.extend_from_slice(child.as_bytes());
        result.extend_from_slice(format!("</{qualified}>").as_bytes());
        result.extend_from_slice(&xml[to..]);
    } else {
        result.extend_from_slice(&xml[..from]);
        result.extend_from_slice(child.as_bytes());
        result.extend_from_slice(&xml[from..]);
    }
    Ok(result)
}

fn workbook_with_sheet(
    xml: &[u8],
    name: &str,
    sheet_id: u32,
    rel_id: &str,
    state: Option<&str>,
) -> ExcliResult<Vec<u8>> {
    let mut reader = Reader::from_reader(xml);
    let mut max_id = 0u32;
    let mut names = Vec::new();
    let mut sheets_span = None;
    let mut sheets_tag = None;
    let mut sheets_empty = false;
    loop {
        let from = reader.buffer_position() as usize;
        let event = reader.read_event().map_err(xml_error)?;
        let to = reader.buffer_position() as usize;
        match event {
            Event::Start(ref tag) | Event::Empty(ref tag)
                if tag.local_name().as_ref() == b"sheet" =>
            {
                let mut found_name = None;
                let mut found_id = None;
                for attr in tag.attributes() {
                    let attr = attr.map_err(xml_error)?;
                    let value = attr.unescape_value().map_err(xml_error)?.into_owned();
                    match attr.key.as_ref() {
                        b"name" => found_name = Some(value),
                        b"sheetId" => found_id = value.parse::<u32>().ok(),
                        _ => {}
                    }
                }
                names.push(found_name.ok_or_else(|| corrupted("Sheet has no name"))?);
                max_id = max_id.max(found_id.ok_or_else(|| corrupted("Sheet has no valid ID"))?);
            }
            Event::Start(ref tag) if tag.local_name().as_ref() == b"sheets" => {
                sheets_tag = Some(String::from_utf8_lossy(tag.name().as_ref()).into_owned());
            }
            Event::Empty(ref tag) if tag.local_name().as_ref() == b"sheets" => {
                sheets_tag = Some(String::from_utf8_lossy(tag.name().as_ref()).into_owned());
                sheets_span = Some((from, to));
                sheets_empty = true;
            }
            Event::End(ref tag) if tag.local_name().as_ref() == b"sheets" => {
                sheets_span = Some((from, to));
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if names
        .iter()
        .any(|existing| existing.to_lowercase() == name.to_lowercase())
    {
        return Err(invalid_name("Worksheet name already exists"));
    }
    if sheet_id
        != max_id
            .checked_add(1)
            .ok_or_else(|| corrupted("Sheet ID overflow"))?
    {
        return Err(corrupted("Inconsistent sheet ID"));
    }
    let (from, to) = sheets_span.ok_or_else(|| corrupted("Workbook has no sheets element"))?;
    let tag = sheets_tag.ok_or_else(|| corrupted("Workbook has no sheets element"))?;
    let prefix = tag.rsplit_once(':').map_or("", |(prefix, _)| prefix);
    let child_tag = if prefix.is_empty() {
        "sheet".to_owned()
    } else {
        format!("{prefix}:sheet")
    };
    let child = format!(
        "<{child_tag} name=\"{}\" sheetId=\"{sheet_id}\"{} xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" r:id=\"{rel_id}\"/>",
        xml_escape(name),
        state.map_or(String::new(), |value| format!(" state=\"{}\"", xml_escape(value)))
    );
    let mut result = Vec::with_capacity(xml.len() + child.len() + 32);
    if sheets_empty {
        let opening = &xml[from..to];
        let slash = opening
            .iter()
            .rposition(|&b| b == b'/')
            .ok_or_else(|| corrupted("Invalid sheets element"))?;
        result.extend_from_slice(&xml[..from]);
        result.extend_from_slice(&opening[..slash]);
        result.push(b'>');
        result.extend_from_slice(child.as_bytes());
        result.extend_from_slice(format!("</{tag}>").as_bytes());
        result.extend_from_slice(&xml[to..]);
    } else {
        result.extend_from_slice(&xml[..from]);
        result.extend_from_slice(child.as_bytes());
        result.extend_from_slice(&xml[from..]);
    }
    Ok(result)
}

fn next_sheet_id(xml: &[u8]) -> ExcliResult<u32> {
    let mut reader = Reader::from_reader(xml);
    let mut max = 0u32;
    loop {
        match reader.read_event().map_err(xml_error)? {
            Event::Start(tag) | Event::Empty(tag) if tag.local_name().as_ref() == b"sheet" => {
                let id = tag
                    .attributes()
                    .find_map(|attr| {
                        let attr = attr.ok()?;
                        (attr.key.as_ref() == b"sheetId")
                            .then(|| attr.unescape_value().ok()?.parse::<u32>().ok())
                            .flatten()
                    })
                    .ok_or_else(|| corrupted("Sheet has no valid ID"))?;
                max = max.max(id);
            }
            Event::Eof => break,
            _ => {}
        }
    }
    max.checked_add(1)
        .ok_or_else(|| corrupted("Sheet ID overflow"))
}

fn unique_relationship_id(xml: &[u8]) -> ExcliResult<String> {
    let mut reader = Reader::from_reader(xml);
    let mut ids = std::collections::HashSet::new();
    loop {
        match reader.read_event().map_err(xml_error)? {
            Event::Start(tag) | Event::Empty(tag)
                if tag.local_name().as_ref() == b"Relationship" =>
            {
                for attr in tag.attributes() {
                    let attr = attr.map_err(xml_error)?;
                    if attr.key.as_ref() == b"Id" {
                        ids.insert(attr.unescape_value().map_err(xml_error)?.into_owned());
                    }
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    for number in 1u32.. {
        let candidate = format!("rId{number}");
        if !ids.contains(&candidate) {
            return Ok(candidate);
        }
    }
    Err(corrupted("No relationship ID available"))
}

/// Update the optional Excel extended-properties sheet list. Other groups in
/// TitlesOfParts (for example chart sheets) keep their original positions.
fn update_app_properties(xml: &[u8], name: &str) -> ExcliResult<Vec<u8>> {
    let mut reader = Reader::from_reader(xml);
    let mut section = Vec::<Vec<u8>>::new();
    let mut heading_label = String::new();
    let mut preceding = 0usize;
    let mut worksheet_group = None;
    let mut count_span = None;
    let mut titles_vector = None;
    let mut titles_end = None;
    let mut title_starts = Vec::new();
    loop {
        let from = reader.buffer_position() as usize;
        let event = reader.read_event().map_err(xml_error)?;
        let to = reader.buffer_position() as usize;
        match event {
            Event::Start(ref tag) => {
                let local = tag.local_name().as_ref().to_vec();
                if section.last().is_some_and(|part| part == b"TitlesOfParts") && local == b"vector"
                {
                    titles_vector = Some((
                        from,
                        to,
                        String::from_utf8_lossy(tag.name().as_ref()).into_owned(),
                    ));
                }
                if section.iter().any(|part| part == b"TitlesOfParts") && local == b"lpstr" {
                    title_starts.push(from);
                }
                section.push(local);
            }
            Event::Empty(ref tag) => {
                if section.iter().any(|part| part == b"TitlesOfParts")
                    && tag.local_name().as_ref() == b"lpstr"
                {
                    title_starts.push(from);
                }
            }
            Event::Text(ref text) => {
                let value = text.unescape().map_err(xml_error)?;
                if section.iter().any(|part| part == b"HeadingPairs") {
                    match section.last().map(Vec::as_slice) {
                        Some(b"lpstr") => heading_label = value.into_owned(),
                        Some(b"i4") => {
                            let count = value.parse::<usize>().map_err(|_| {
                                corrupted("Invalid extended-properties sheet count")
                            })?;
                            if heading_label == "Worksheets" {
                                if worksheet_group.is_some() {
                                    return Err(corrupted("Duplicate Worksheets metadata group"));
                                }
                                worksheet_group = Some((preceding, count));
                                count_span = Some((from, to));
                            }
                            preceding = preceding.checked_add(count).ok_or_else(|| {
                                corrupted("Extended-properties title count overflow")
                            })?;
                            heading_label.clear();
                        }
                        _ => {}
                    }
                }
            }
            Event::End(ref tag) => {
                if section.iter().any(|part| part == b"TitlesOfParts")
                    && tag.local_name().as_ref() == b"vector"
                {
                    titles_end = Some(from);
                }
                if section.pop().as_deref() != Some(tag.local_name().as_ref()) {
                    return Err(corrupted("Invalid extended-properties XML nesting"));
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    let Some((offset, count)) = worksheet_group else {
        return Ok(xml.to_vec());
    };
    let (vector_from, vector_to, vector_tag) =
        titles_vector.ok_or_else(|| corrupted("Missing TitlesOfParts vector"))?;
    let titles_end = titles_end.ok_or_else(|| corrupted("Unclosed TitlesOfParts vector"))?;
    if title_starts.len() != preceding || offset + count > title_starts.len() {
        return Err(corrupted(
            "Extended-properties title list does not match group counts",
        ));
    }
    let count_span = count_span.ok_or_else(|| corrupted("Missing Worksheets count"))?;
    let prefix = vector_tag.rsplit_once(':').map_or("", |(prefix, _)| prefix);
    let title_tag = if prefix.is_empty() {
        "lpstr".to_owned()
    } else {
        format!("{prefix}:lpstr")
    };
    let title_xml = format!("<{title_tag}>{}</{title_tag}>", xml_escape(name));
    let insert_at = title_starts
        .get(offset + count)
        .copied()
        .unwrap_or(titles_end);
    let vector_open = std::str::from_utf8(&xml[vector_from..vector_to]).map_err(xml_error)?;
    let size_start = vector_open
        .find("size=\"")
        .ok_or_else(|| corrupted("TitlesOfParts vector has no size"))?
        + 6;
    let size_end = size_start
        + vector_open[size_start..]
            .find('"')
            .ok_or_else(|| corrupted("Invalid TitlesOfParts size"))?;
    let current_size = vector_open[size_start..size_end]
        .parse::<usize>()
        .map_err(|_| corrupted("Invalid TitlesOfParts size"))?;
    if current_size != title_starts.len() {
        return Err(corrupted("TitlesOfParts size disagrees with entries"));
    }
    let mut edits = vec![
        (
            vector_from + size_start,
            vector_from + size_end,
            (current_size + 1).to_string().into_bytes(),
        ),
        (
            count_span.0,
            count_span.1,
            (count + 1).to_string().into_bytes(),
        ),
        (insert_at, insert_at, title_xml.into_bytes()),
    ];
    edits.sort_by_key(|(from, _, _)| *from);
    let mut result = Vec::with_capacity(xml.len() + name.len() + 32);
    let mut cursor = 0;
    for (from, to, replacement) in edits {
        if from < cursor {
            return Err(corrupted("Overlapping extended-properties edits"));
        }
        result.extend_from_slice(&xml[cursor..from]);
        result.extend_from_slice(&replacement);
        cursor = to;
    }
    result.extend_from_slice(&xml[cursor..]);
    Ok(result)
}

/// Add a blank worksheet to an existing XLSX/XLSM package.
pub fn add_sheet(path: &Path, name: &str) -> ExcliResult<()> {
    validate_name(name)?;
    if !matches!(detect_format(path)?, FileFormat::Xlsx | FileFormat::Xlsm) {
        return Err(ExcliError::new(
            ErrorCode::NotImplemented,
            "Package-preserving sheet add supports XLSX and XLSM",
        ));
    }
    let mut archive = ZipArchive::new(std::fs::File::open(path)?)
        .map_err(|error| corrupted(format!("Invalid Excel ZIP: {error}")))?;
    ensure_editable(&mut archive)?;
    let package = package_index(&mut archive)?;
    let workbook_xml = read_part(&mut archive, &package.workbook_path)?;
    let rels_name = rels_path(&package.workbook_path)?;
    let rels_xml = read_part(&mut archive, &rels_name)?;
    let types_xml = read_part(&mut archive, "[Content_Types].xml")?;
    let sheet_id = next_sheet_id(&workbook_xml)?;
    let rel_id = unique_relationship_id(&rels_xml)?;
    let parent = Path::new(&package.workbook_path)
        .parent()
        .and_then(|p| p.to_str())
        .unwrap_or("");
    let (sheet_path, target) = (1u32..)
        .map(|number| {
            let target = format!("worksheets/sheet{number}.xml");
            let path = if parent.is_empty() {
                target.clone()
            } else {
                format!("{parent}/{target}")
            };
            (path, target)
        })
        .find(|(path, _)| archive.file_names().all(|existing| existing != path))
        .ok_or_else(|| corrupted("No worksheet part name available"))?;
    let workbook_xml = workbook_with_sheet(&workbook_xml, name, sheet_id, &rel_id, None)?;
    let rels_xml = append_child(&rels_xml, b"Relationships", |prefix| {
        let tag = if prefix.is_empty() {
            "Relationship".to_owned()
        } else {
            format!("{prefix}:Relationship")
        };
        format!("<{tag} Id=\"{rel_id}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet\" Target=\"{target}\"/>")
    })?;
    let types_xml = append_child(&types_xml, b"Types", |prefix| {
        let tag = if prefix.is_empty() {
            "Override".to_owned()
        } else {
            format!("{prefix}:Override")
        };
        format!("<{tag} PartName=\"/{sheet_path}\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/>")
    })?;
    let mut modified = HashMap::from([
        (package.workbook_path, workbook_xml),
        (rels_name, rels_xml),
        ("[Content_Types].xml".to_owned(), types_xml),
    ]);
    if archive.file_names().any(|part| part == "docProps/app.xml") {
        let app_xml = read_part(&mut archive, "docProps/app.xml")?;
        modified.insert(
            "docProps/app.xml".to_owned(),
            update_app_properties(&app_xml, name)?,
        );
    }
    let added = HashMap::from([(
        sheet_path,
        b"<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><sheetData/></worksheet>".to_vec(),
    )]);
    write_package_parts(path, &mut archive, &modified, &added)
}

fn source_sheet_details(xml: &[u8], source: &str) -> ExcliResult<(usize, usize, Option<String>)> {
    let mut reader = Reader::from_reader(xml);
    let mut in_sheets = false;
    let mut index = 0;
    let mut found = None;
    loop {
        match reader.read_event().map_err(xml_error)? {
            Event::Start(ref tag) if tag.local_name().as_ref() == b"sheets" => in_sheets = true,
            Event::End(ref tag) if tag.local_name().as_ref() == b"sheets" => in_sheets = false,
            Event::Empty(ref tag) if in_sheets && tag.local_name().as_ref() == b"sheet" => {
                let mut name = None;
                let mut state = None;
                for attribute in tag.attributes() {
                    let attribute = attribute.map_err(xml_error)?;
                    let value = attribute.unescape_value().map_err(xml_error)?.into_owned();
                    match attribute.key.as_ref() {
                        b"name" => name = Some(value),
                        b"state" => state = Some(value),
                        _ => {}
                    }
                }
                if name.as_deref() == Some(source) {
                    if !matches!(
                        state.as_deref(),
                        None | Some("visible" | "hidden" | "veryHidden")
                    ) {
                        return Err(corrupted("Invalid worksheet visibility state"));
                    }
                    found = Some((index, state));
                }
                index += 1;
            }
            Event::Eof => break,
            _ => {}
        }
    }
    let (source_index, state) = found.ok_or_else(|| ExcliError::sheet_not_found(source))?;
    Ok((source_index, index, state))
}

fn copy_scoped_names(
    xml: &[u8],
    source_index: usize,
    new_index: usize,
    source: &str,
    dest: &str,
) -> ExcliResult<Vec<u8>> {
    let mut reader = Reader::from_reader(xml);
    let mut source_start = None;
    let mut insertion = None;
    let mut copies = Vec::new();
    loop {
        let from = reader.buffer_position() as usize;
        let event = reader.read_event().map_err(xml_error)?;
        let to = reader.buffer_position() as usize;
        match event {
            Event::Start(ref tag) if tag.local_name().as_ref() == b"definedName" => {
                if attr_value(tag, b"localSheetId")?
                    .map(|value| value.parse::<usize>().map_err(xml_error))
                    .transpose()?
                    == Some(source_index)
                {
                    source_start = Some(from);
                }
            }
            Event::Empty(ref tag) if tag.local_name().as_ref() == b"definedName" => {
                if attr_value(tag, b"localSheetId")?
                    .map(|value| value.parse::<usize>().map_err(xml_error))
                    .transpose()?
                    == Some(source_index)
                {
                    copies.push(xml[from..to].to_vec());
                }
            }
            Event::End(ref tag) if tag.local_name().as_ref() == b"definedName" => {
                if let Some(start) = source_start.take() {
                    copies.push(xml[start..to].to_vec());
                }
            }
            Event::End(ref tag) if tag.local_name().as_ref() == b"definedNames" => {
                insertion = Some(from);
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if copies.is_empty() {
        return Ok(xml.to_vec());
    }
    let insertion = insertion.ok_or_else(|| corrupted("Missing definedNames end"))?;
    let mut cloned = Vec::new();
    for fragment in copies {
        let mut reader = Reader::from_reader(fragment.as_slice());
        let event = reader.read_event().map_err(xml_error)?;
        let to = reader.buffer_position() as usize;
        let tag = match event {
            Event::Start(tag) | Event::Empty(tag) => tag,
            _ => return Err(corrupted("Invalid definedName element")),
        };
        let updated = apply_copy_edits(
            &fragment,
            vec![(
                0,
                to,
                rewrite_tag(
                    &tag,
                    &fragment[..to],
                    &[(b"localSheetId", new_index.to_string())],
                )?,
            )],
        )?;
        let updated = rewrite_xml_references(&updated, source, dest)?.unwrap_or(updated);
        if has_old_reference(&updated, source)? {
            return Err(ExcliError::new(
                ErrorCode::NotImplemented,
                "Sheet-scoped name has a reference that needs further rewriting",
            ));
        }
        cloned.extend_from_slice(&updated);
    }
    apply_copy_edits(xml, vec![(insertion, insertion, cloned)])
}

#[derive(Default)]
struct CopyRelations {
    tables: Vec<(String, String)>,
    notes: Vec<(String, String)>,
    vml: Vec<(String, String)>,
    threads: Vec<(String, String)>,
    drawings: Vec<(String, String)>,
    pivots: Vec<(String, String)>,
}

fn copy_relationships(xml: &[u8]) -> ExcliResult<CopyRelations> {
    let mut reader = Reader::from_reader(xml);
    let mut result = CopyRelations::default();
    loop {
        match reader.read_event().map_err(xml_error)? {
            Event::Start(ref tag) | Event::Empty(ref tag)
                if tag.local_name().as_ref() == b"Relationship" =>
            {
                let mut kind = None;
                let mut id = None;
                let mut target = None;
                let mut external = false;
                for attribute in tag.attributes() {
                    let attribute = attribute.map_err(xml_error)?;
                    let value = attribute.unescape_value().map_err(xml_error)?.into_owned();
                    match attribute.key.as_ref() {
                        b"Type" => kind = Some(value),
                        b"Id" => id = Some(value),
                        b"Target" => target = Some(value),
                        b"TargetMode" if value == "External" => external = true,
                        _ => {}
                    }
                }
                match kind.as_deref() {
                    Some(value) if value.ends_with("/hyperlink") => {}
                    Some(value) if value.ends_with("/table") && !external => result.tables.push((
                        id.ok_or_else(|| corrupted("Table relationship has no ID"))?,
                        target.ok_or_else(|| corrupted("Table relationship has no target"))?,
                    )),
                    Some(value) if value.ends_with("/comments") && !external => {
                        result.notes.push((
                            id.ok_or_else(|| corrupted("Comments relationship has no ID"))?,
                            target
                                .ok_or_else(|| corrupted("Comments relationship has no target"))?,
                        ))
                    }
                    Some(value) if value.ends_with("/vmlDrawing") && !external => {
                        result.vml.push((
                            id.ok_or_else(|| corrupted("VML relationship has no ID"))?,
                            target.ok_or_else(|| corrupted("VML relationship has no target"))?,
                        ))
                    }
                    Some(value) if value.ends_with("/threadedComment") && !external => {
                        result.threads.push((
                            id.ok_or_else(|| corrupted("Thread relationship has no ID"))?,
                            target.ok_or_else(|| corrupted("Thread relationship has no target"))?,
                        ))
                    }
                    Some(value) if value.ends_with("/drawing") && !external => {
                        result.drawings.push((
                            id.ok_or_else(|| corrupted("Drawing relationship has no ID"))?,
                            target
                                .ok_or_else(|| corrupted("Drawing relationship has no target"))?,
                        ))
                    }
                    Some(value) if value.ends_with("/pivotTable") && !external => {
                        result.pivots.push((
                            id.ok_or_else(|| corrupted("PivotTable relationship has no ID"))?,
                            target.ok_or_else(|| {
                                corrupted("PivotTable relationship has no target")
                            })?,
                        ))
                    }
                    _ => {
                        return Err(ExcliError::new(
                            ErrorCode::NotImplemented,
                            "Source worksheet has relationships that need object cloning",
                        ))
                    }
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(result)
}

fn attr_value(tag: &quick_xml::events::BytesStart<'_>, key: &[u8]) -> ExcliResult<Option<String>> {
    for attribute in tag.attributes() {
        let attribute = attribute.map_err(xml_error)?;
        if attribute.key.as_ref() == key {
            return Ok(Some(
                attribute.unescape_value().map_err(xml_error)?.into_owned(),
            ));
        }
    }
    Ok(None)
}

fn rewrite_tag(
    tag: &quick_xml::events::BytesStart<'_>,
    original: &[u8],
    values: &[(&[u8], String)],
) -> ExcliResult<Vec<u8>> {
    let name = std::str::from_utf8(tag.name().as_ref())
        .map_err(xml_error)?
        .to_owned();
    let mut result = format!("<{name}");
    for attribute in tag.attributes() {
        let attribute = attribute.map_err(xml_error)?;
        let key = std::str::from_utf8(attribute.key.as_ref()).map_err(xml_error)?;
        let value = values
            .iter()
            .find(|(candidate, _)| *candidate == attribute.key.as_ref())
            .map(|(_, value)| value.clone())
            .unwrap_or(attribute.unescape_value().map_err(xml_error)?.into_owned());
        result.push_str(&format!(" {key}=\"{}\"", xml_escape(&value)));
    }
    result.push_str(if original.ends_with(b"/>") { "/>" } else { ">" });
    Ok(result.into_bytes())
}

fn apply_copy_edits(xml: &[u8], mut edits: Vec<(usize, usize, Vec<u8>)>) -> ExcliResult<Vec<u8>> {
    edits.sort_by_key(|(from, _, _)| *from);
    let mut result = Vec::with_capacity(xml.len());
    let mut cursor = 0;
    for (from, to, replacement) in edits {
        if from < cursor || to > xml.len() {
            return Err(corrupted("Overlapping package XML edits"));
        }
        result.extend_from_slice(&xml[cursor..from]);
        result.extend_from_slice(&replacement);
        cursor = to;
    }
    result.extend_from_slice(&xml[cursor..]);
    Ok(result)
}

fn table_identity(xml: &[u8]) -> ExcliResult<(u32, String)> {
    let mut reader = Reader::from_reader(xml);
    loop {
        match reader.read_event().map_err(xml_error)? {
            Event::Start(ref tag) | Event::Empty(ref tag)
                if tag.local_name().as_ref() == b"table" =>
            {
                let id = attr_value(tag, b"id")?
                    .ok_or_else(|| corrupted("Table has no ID"))?
                    .parse::<u32>()
                    .map_err(xml_error)?;
                let name = attr_value(tag, b"displayName")?
                    .or(attr_value(tag, b"name")?)
                    .ok_or_else(|| corrupted("Table has no name"))?;
                return Ok((id, name));
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Err(corrupted("Missing table root"))
}

fn rewrite_table_identity(xml: &[u8], id: u32, name: &str) -> ExcliResult<Vec<u8>> {
    let mut reader = Reader::from_reader(xml);
    loop {
        let from = reader.buffer_position() as usize;
        let event = reader.read_event().map_err(xml_error)?;
        let to = reader.buffer_position() as usize;
        match event {
            Event::Start(ref tag) | Event::Empty(ref tag)
                if tag.local_name().as_ref() == b"table" =>
            {
                let mut changes = vec![(b"id".as_slice(), id.to_string())];
                if attr_value(tag, b"name")?.is_some() {
                    changes.push((b"name".as_slice(), name.to_owned()));
                }
                if attr_value(tag, b"displayName")?.is_some() {
                    changes.push((b"displayName".as_slice(), name.to_owned()));
                }
                return apply_copy_edits(
                    xml,
                    vec![(from, to, rewrite_tag(tag, &xml[from..to], &changes)?)],
                );
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Err(corrupted("Missing table root"))
}

fn pivot_name(xml: &[u8]) -> ExcliResult<String> {
    let mut reader = Reader::from_reader(xml);
    loop {
        match reader.read_event().map_err(xml_error)? {
            Event::Start(ref tag) | Event::Empty(ref tag)
                if tag.local_name().as_ref() == b"pivotTableDefinition" =>
            {
                return attr_value(tag, b"name")?
                    .ok_or_else(|| corrupted("PivotTable has no name"));
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Err(corrupted("Missing PivotTable root"))
}

fn rewrite_pivot_name(xml: &[u8], name: &str) -> ExcliResult<Vec<u8>> {
    let mut reader = Reader::from_reader(xml);
    loop {
        let from = reader.buffer_position() as usize;
        let event = reader.read_event().map_err(xml_error)?;
        let to = reader.buffer_position() as usize;
        match event {
            Event::Start(ref tag) | Event::Empty(ref tag)
                if tag.local_name().as_ref() == b"pivotTableDefinition" =>
            {
                if attr_value(tag, b"name")?.is_none() {
                    return Err(corrupted("PivotTable has no name"));
                }
                return apply_copy_edits(
                    xml,
                    vec![(
                        from,
                        to,
                        rewrite_tag(tag, &xml[from..to], &[(b"name", name.to_owned())])?,
                    )],
                );
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Err(corrupted("Missing PivotTable root"))
}

fn pivot_cache_relationship(xml: &[u8]) -> ExcliResult<String> {
    let mut reader = Reader::from_reader(xml);
    let mut target = None;
    loop {
        match reader.read_event().map_err(xml_error)? {
            Event::Start(ref tag) | Event::Empty(ref tag)
                if tag.local_name().as_ref() == b"Relationship" =>
            {
                let kind = attr_value(tag, b"Type")?
                    .ok_or_else(|| corrupted("PivotTable relationship has no type"))?;
                if !kind.ends_with("/pivotCacheDefinition")
                    || attr_value(tag, b"TargetMode")?.as_deref() == Some("External")
                    || target.is_some()
                {
                    return Err(ExcliError::new(
                        ErrorCode::NotImplemented,
                        "PivotTable has unsupported dependent relationships",
                    ));
                }
                target = Some(
                    attr_value(tag, b"Target")?
                        .ok_or_else(|| corrupted("Pivot cache relationship has no target"))?,
                );
            }
            Event::Eof => break,
            _ => {}
        }
    }
    target.ok_or_else(|| corrupted("PivotTable has no cache relationship"))
}

fn rewrite_table_relationships(
    xml: &[u8],
    targets: &HashMap<String, String>,
) -> ExcliResult<Vec<u8>> {
    let mut reader = Reader::from_reader(xml);
    let mut edits = Vec::new();
    loop {
        let from = reader.buffer_position() as usize;
        let event = reader.read_event().map_err(xml_error)?;
        let to = reader.buffer_position() as usize;
        match event {
            Event::Start(ref tag) | Event::Empty(ref tag)
                if tag.local_name().as_ref() == b"Relationship" =>
            {
                if let Some(id) = attr_value(tag, b"Id")? {
                    if let Some(target) = targets.get(&id) {
                        edits.push((
                            from,
                            to,
                            rewrite_tag(tag, &xml[from..to], &[(b"Target", target.clone())])?,
                        ));
                    }
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    apply_copy_edits(xml, edits)
}

fn part_paths_for_content_type(xml: &[u8], suffix: &str) -> ExcliResult<Vec<String>> {
    let mut reader = Reader::from_reader(xml);
    let mut paths = Vec::new();
    loop {
        match reader.read_event().map_err(xml_error)? {
            Event::Start(ref tag) | Event::Empty(ref tag)
                if tag.local_name().as_ref() == b"Override" =>
            {
                if attr_value(tag, b"ContentType")?
                    .as_deref()
                    .is_some_and(|value| value.ends_with(suffix))
                {
                    paths.push(
                        attr_value(tag, b"PartName")?
                            .ok_or_else(|| corrupted("Table override has no part name"))?
                            .trim_start_matches('/')
                            .to_owned(),
                    );
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(paths)
}

fn table_part_paths(xml: &[u8]) -> ExcliResult<Vec<String>> {
    part_paths_for_content_type(xml, ".table+xml")
}

fn threaded_ids(xml: &[u8]) -> ExcliResult<Vec<String>> {
    let mut reader = Reader::from_reader(xml);
    let mut ids = Vec::new();
    loop {
        match reader.read_event().map_err(xml_error)? {
            Event::Start(ref tag) | Event::Empty(ref tag)
                if tag.local_name().as_ref() == b"threadedComment" =>
            {
                ids.push(
                    attr_value(tag, b"id")?
                        .ok_or_else(|| corrupted("Threaded comment has no ID"))?,
                );
            }
            Event::Start(ref tag) | Event::Empty(ref tag)
                if tag.local_name().as_ref() == b"mention" =>
            {
                ids.push(
                    attr_value(tag, b"mentionId")?.ok_or_else(|| corrupted("Mention has no ID"))?,
                );
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(ids)
}

fn new_comment_guid(used: &mut HashSet<String>, counter: &mut u64) -> ExcliResult<String> {
    while *counter <= 0xFFFF_FFFF_FFFF {
        let candidate = format!("{{00000000-0000-4000-8000-{:012X}}}", *counter);
        *counter += 1;
        if used.insert(candidate.to_lowercase()) {
            return Ok(candidate);
        }
    }
    Err(corrupted("No threaded comment GUID available"))
}

fn clone_threaded_ids(
    xml: &[u8],
    used: &mut HashSet<String>,
    counter: &mut u64,
) -> ExcliResult<Vec<u8>> {
    let mut reader = Reader::from_reader(xml);
    let mut ids = HashMap::new();
    for id in threaded_ids(xml)? {
        if ids.contains_key(&id.to_lowercase()) {
            return Err(corrupted("Duplicate threaded comment or mention ID"));
        }
        ids.insert(id.to_lowercase(), new_comment_guid(used, counter)?);
    }
    let mut edits = Vec::new();
    loop {
        let from = reader.buffer_position() as usize;
        let event = reader.read_event().map_err(xml_error)?;
        let to = reader.buffer_position() as usize;
        match event {
            Event::Start(ref tag) | Event::Empty(ref tag)
                if tag.local_name().as_ref() == b"threadedComment" =>
            {
                let old_id = attr_value(tag, b"id")?
                    .ok_or_else(|| corrupted("Threaded comment has no ID"))?;
                let new_id = ids
                    .get(&old_id.to_lowercase())
                    .ok_or_else(|| corrupted("Missing cloned comment ID"))?;
                let mut changes = vec![(b"id".as_slice(), new_id.clone())];
                if let Some(parent_id) = attr_value(tag, b"parentId")? {
                    let new_parent = ids.get(&parent_id.to_lowercase()).ok_or_else(|| {
                        corrupted("Threaded comment parent is outside copied part")
                    })?;
                    changes.push((b"parentId".as_slice(), new_parent.clone()));
                }
                edits.push((from, to, rewrite_tag(tag, &xml[from..to], &changes)?));
            }
            Event::Start(ref tag) | Event::Empty(ref tag)
                if tag.local_name().as_ref() == b"mention" =>
            {
                let old_id =
                    attr_value(tag, b"mentionId")?.ok_or_else(|| corrupted("Mention has no ID"))?;
                let new_id = ids
                    .get(&old_id.to_lowercase())
                    .ok_or_else(|| corrupted("Missing cloned mention ID"))?;
                edits.push((
                    from,
                    to,
                    rewrite_tag(tag, &xml[from..to], &[(b"mentionId", new_id.clone())])?,
                ));
            }
            Event::Eof => break,
            _ => {}
        }
    }
    apply_copy_edits(xml, edits)
}

fn clone_related_part(
    archive: &mut ZipArchive<std::fs::File>,
    source_sheet: &str,
    target: &str,
    prefix: &str,
    extension: &str,
    added: &HashMap<String, Vec<u8>>,
    allow_rels: bool,
) -> ExcliResult<(String, Vec<u8>)> {
    let old_path = resolve_part(source_sheet, target)?;
    let dependent_rels = rels_path(&old_path)?;
    if !allow_rels && archive.file_names().any(|part| part == dependent_rels) {
        return Err(ExcliError::new(
            ErrorCode::NotImplemented,
            "Package part has dependent relationships",
        ));
    }
    let old_xml = read_part(archive, &old_path)?;
    let parent = Path::new(&old_path)
        .parent()
        .and_then(|parent| parent.to_str())
        .unwrap_or("");
    let new_path = (1u32..)
        .map(|number| {
            let name = format!("{prefix}{number}.{extension}");
            if parent.is_empty() {
                name
            } else {
                format!("{parent}/{name}")
            }
        })
        .find(|candidate| {
            !archive.file_names().any(|part| part == candidate) && !added.contains_key(candidate)
        })
        .ok_or_else(|| corrupted("No related part name available"))?;
    Ok((new_path, old_xml))
}

fn drawing_chart_relationships(xml: &[u8]) -> ExcliResult<Vec<(String, String)>> {
    let mut reader = Reader::from_reader(xml);
    let mut charts = Vec::new();
    loop {
        match reader.read_event().map_err(xml_error)? {
            Event::Start(ref tag) | Event::Empty(ref tag)
                if tag.local_name().as_ref() == b"Relationship" =>
            {
                let kind = attr_value(tag, b"Type")?
                    .ok_or_else(|| corrupted("Drawing relationship has no type"))?;
                let external = attr_value(tag, b"TargetMode")?.as_deref() == Some("External");
                if kind.ends_with("/chart") && !external {
                    charts.push((
                        attr_value(tag, b"Id")?
                            .ok_or_else(|| corrupted("Chart relationship has no ID"))?,
                        attr_value(tag, b"Target")?
                            .ok_or_else(|| corrupted("Chart relationship has no target"))?,
                    ));
                } else if !(kind.ends_with("/image") && !external
                    || kind.ends_with("/hyperlink") && external)
                {
                    return Err(ExcliError::new(
                        ErrorCode::NotImplemented,
                        "Drawing has relationships that need object cloning",
                    ));
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(charts)
}

fn clone_content_type_override(
    xml: &[u8],
    old_path: &str,
    new_path: &str,
    required: bool,
) -> ExcliResult<Vec<u8>> {
    let mut reader = Reader::from_reader(xml);
    let mut content_type = None;
    loop {
        match reader.read_event().map_err(xml_error)? {
            Event::Start(ref tag) | Event::Empty(ref tag)
                if tag.local_name().as_ref() == b"Override"
                    && attr_value(tag, b"PartName")?.as_deref()
                        == Some(&format!("/{old_path}")) =>
            {
                content_type = attr_value(tag, b"ContentType")?;
            }
            Event::Eof => break,
            _ => {}
        }
    }
    let Some(content_type) = content_type else {
        if required {
            return Err(corrupted("Comment part has no content type override"));
        }
        return Ok(xml.to_vec());
    };
    append_child(xml, b"Types", |prefix| {
        let tag = if prefix.is_empty() {
            "Override".to_owned()
        } else {
            format!("{prefix}:Override")
        };
        format!(
            "<{tag} PartName=\"/{new_path}\" ContentType=\"{}\"/>",
            xml_escape(&content_type)
        )
    })
}

/// Copy a worksheet inside an XLSX/XLSM package without rebuilding its cells.
/// Tables, comments, PivotTables, and common drawing parts are cloned; unsupported objects are rejected.
pub fn copy_sheet(path: &Path, source: &str, dest: &str) -> ExcliResult<()> {
    validate_name(dest)?;
    if !matches!(detect_format(path)?, FileFormat::Xlsx | FileFormat::Xlsm) {
        return Err(ExcliError::new(
            ErrorCode::NotImplemented,
            "Package-preserving sheet copy supports XLSX and XLSM",
        ));
    }
    let mut archive = ZipArchive::new(std::fs::File::open(path)?)
        .map_err(|error| corrupted(format!("Invalid Excel ZIP: {error}")))?;
    ensure_editable(&mut archive)?;
    if archive
        .file_names()
        .any(|part| part.ends_with("vbaProject.bin") || part.ends_with("calcChain.xml"))
    {
        return Err(ExcliError::new(
            ErrorCode::NotImplemented,
            "Copying sheets with VBA projects or calculation chains needs additional updates",
        ));
    }
    let package = package_index(&mut archive)?;
    let source_path = package
        .sheets
        .iter()
        .find(|sheet| sheet.name == source)
        .ok_or_else(|| ExcliError::sheet_not_found(source))?
        .path
        .clone();
    let source_xml = read_part(&mut archive, &source_path)?;
    let source_xml = rewrite_xml_references(&source_xml, source, dest)?.unwrap_or(source_xml);
    if has_old_reference(&source_xml, source)? {
        return Err(ExcliError::new(
            ErrorCode::NotImplemented,
            "Source worksheet contains references that need further rewriting",
        ));
    }
    let workbook_xml = read_part(&mut archive, &package.workbook_path)?;
    let (source_index, new_index, state) = source_sheet_details(&workbook_xml, source)?;
    let source_rels_path = rels_path(&source_path)?;
    let source_rels = if archive.file_names().any(|part| part == source_rels_path) {
        let xml = read_part(&mut archive, &source_rels_path)?;
        let relations = copy_relationships(&xml)?;
        let part_ids = table_part_ids(&source_xml)?;
        let relationship_ids: HashSet<&str> =
            relations.tables.iter().map(|(id, _)| id.as_str()).collect();
        if part_ids.len() != relationship_ids.len()
            || part_ids
                .iter()
                .any(|id| !relationship_ids.contains(id.as_str()))
        {
            return Err(corrupted("Worksheet tableParts do not match relationships"));
        }
        let pivot_ids = pivot_part_ids(&source_xml)?;
        let pivot_relationship_ids: HashSet<&str> =
            relations.pivots.iter().map(|(id, _)| id.as_str()).collect();
        if pivot_ids.len() != pivot_relationship_ids.len()
            || pivot_ids
                .iter()
                .any(|id| !pivot_relationship_ids.contains(id.as_str()))
        {
            return Err(corrupted(
                "Worksheet pivotTableParts do not match relationships",
            ));
        }
        Some((xml, relations))
    } else {
        if !table_part_ids(&source_xml)?.is_empty() {
            return Err(corrupted("Worksheet tableParts have no relationships"));
        }
        if !pivot_part_ids(&source_xml)?.is_empty() {
            return Err(corrupted("Worksheet pivotTableParts have no relationships"));
        }
        None
    };
    let rels_name = rels_path(&package.workbook_path)?;
    let rels_xml = read_part(&mut archive, &rels_name)?;
    let sheet_id = next_sheet_id(&workbook_xml)?;
    let rel_id = unique_relationship_id(&rels_xml)?;
    let source_parent = Path::new(&source_path)
        .parent()
        .and_then(|parent| parent.to_str())
        .unwrap_or("");
    let sheet_path = (1u32..)
        .map(|number| {
            if source_parent.is_empty() {
                format!("sheet{number}.xml")
            } else {
                format!("{source_parent}/sheet{number}.xml")
            }
        })
        .find(|candidate| archive.file_names().all(|part| part != candidate))
        .ok_or_else(|| corrupted("No worksheet part name available"))?;
    let target = format!("/{sheet_path}");
    let workbook_xml =
        workbook_with_sheet(&workbook_xml, dest, sheet_id, &rel_id, state.as_deref())?;
    let workbook_xml = copy_scoped_names(&workbook_xml, source_index, new_index, source, dest)?;
    let rels_xml = append_child(&rels_xml, b"Relationships", |prefix| {
        let tag = if prefix.is_empty() {
            "Relationship".to_owned()
        } else {
            format!("{prefix}:Relationship")
        };
        format!("<{tag} Id=\"{rel_id}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet\" Target=\"{target}\"/>")
    })?;
    let mut types_xml = append_child(
        &read_part(&mut archive, "[Content_Types].xml")?,
        b"Types",
        |prefix| {
            let tag = if prefix.is_empty() {
                "Override".to_owned()
            } else {
                format!("{prefix}:Override")
            };
            format!("<{tag} PartName=\"/{sheet_path}\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/>")
        },
    )?;
    let mut added = HashMap::from([(sheet_path.clone(), source_xml.clone())]);
    if let Some((rels, relations)) = source_rels {
        let mut replacements = HashMap::new();
        if !relations.tables.is_empty() {
            let existing_tables = table_part_paths(&types_xml)?;
            let mut names = HashSet::new();
            let mut max_id = 0u32;
            for table_path in &existing_tables {
                let (id, name) = table_identity(&read_part(&mut archive, table_path)?)?;
                max_id = max_id.max(id);
                names.insert(name.to_lowercase());
            }
            for (relationship_id, target) in relations.tables {
                let old_path = resolve_part(&source_path, &target)?;
                if !existing_tables.contains(&old_path) {
                    return Err(corrupted("Table relationship has no content type override"));
                }
                let old_rels_path = rels_path(&old_path)?;
                if archive.file_names().any(|part| part == old_rels_path) {
                    return Err(ExcliError::new(
                        ErrorCode::NotImplemented,
                        "Table has dependent package relationships",
                    ));
                }
                let old_xml = read_part(&mut archive, &old_path)?;
                let (_, old_name) = table_identity(&old_xml)?;
                let needle = format!("{}[", old_name.to_lowercase());
                if String::from_utf8_lossy(&source_xml)
                    .to_lowercase()
                    .contains(&needle)
                    || String::from_utf8_lossy(&old_xml)
                        .to_lowercase()
                        .contains(&needle)
                {
                    return Err(ExcliError::new(
                        ErrorCode::NotImplemented,
                        "Table name is used in a formula that needs rewriting",
                    ));
                }
                max_id = max_id
                    .checked_add(1)
                    .ok_or_else(|| corrupted("Table ID overflow"))?;
                let new_name = (1u32..)
                    .map(|number| format!("Table{number}"))
                    .find(|candidate| !names.contains(&candidate.to_lowercase()))
                    .ok_or_else(|| corrupted("No table name available"))?;
                names.insert(new_name.to_lowercase());
                let table_parent = Path::new(&old_path)
                    .parent()
                    .and_then(|parent| parent.to_str())
                    .unwrap_or("");
                let new_path = (1u32..)
                    .map(|number| {
                        if table_parent.is_empty() {
                            format!("table{number}.xml")
                        } else {
                            format!("{table_parent}/table{number}.xml")
                        }
                    })
                    .find(|candidate| {
                        !archive.file_names().any(|part| part == candidate)
                            && !added.contains_key(candidate)
                    })
                    .ok_or_else(|| corrupted("No table part name available"))?;
                added.insert(
                    new_path.clone(),
                    rewrite_table_identity(&old_xml, max_id, &new_name)?,
                );
                replacements.insert(relationship_id, format!("/{new_path}"));
                types_xml = append_child(&types_xml, b"Types", |prefix| {
                    let tag = if prefix.is_empty() {
                        "Override".to_owned()
                    } else {
                        format!("{prefix}:Override")
                    };
                    format!("<{tag} PartName=\"/{new_path}\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.table+xml\"/>")
                })?;
            }
        }
        for (relationship_id, target) in relations.notes {
            let old_path = resolve_part(&source_path, &target)?;
            let (new_path, contents) = clone_related_part(
                &mut archive,
                &source_path,
                &target,
                "comment",
                "xml",
                &added,
                false,
            )?;
            types_xml = clone_content_type_override(&types_xml, &old_path, &new_path, true)?;
            added.insert(new_path.clone(), contents);
            replacements.insert(relationship_id, format!("/{new_path}"));
        }
        for (relationship_id, target) in relations.vml {
            let old_path = resolve_part(&source_path, &target)?;
            let (new_path, contents) = clone_related_part(
                &mut archive,
                &source_path,
                &target,
                "vmlDrawing",
                "vml",
                &added,
                false,
            )?;
            types_xml = clone_content_type_override(&types_xml, &old_path, &new_path, false)?;
            added.insert(new_path.clone(), contents);
            replacements.insert(relationship_id, format!("/{new_path}"));
        }
        if !relations.threads.is_empty() {
            let existing_threads = part_paths_for_content_type(&types_xml, "threadedcomments+xml")?;
            let mut used: HashSet<String> =
                crate::ooxml_threaded_comment_read::people(&mut archive, &package.workbook_path)?
                    .into_keys()
                    .collect();
            for thread_path in &existing_threads {
                used.extend(
                    threaded_ids(&read_part(&mut archive, thread_path)?)?
                        .into_iter()
                        .map(|id| id.to_lowercase()),
                );
            }
            let mut next_guid = 1u64;
            for (relationship_id, target) in relations.threads {
                let old_path = resolve_part(&source_path, &target)?;
                if !existing_threads.contains(&old_path) {
                    return Err(corrupted("Threaded comment has no content type override"));
                }
                let (new_path, contents) = clone_related_part(
                    &mut archive,
                    &source_path,
                    &target,
                    "threadedComment",
                    "xml",
                    &added,
                    false,
                )?;
                let cloned = clone_threaded_ids(&contents, &mut used, &mut next_guid)?;
                types_xml = clone_content_type_override(&types_xml, &old_path, &new_path, true)?;
                added.insert(new_path.clone(), cloned);
                replacements.insert(relationship_id, format!("/{new_path}"));
            }
        }
        for (relationship_id, target) in relations.drawings {
            let old_path = resolve_part(&source_path, &target)?;
            let (new_path, contents) = clone_related_part(
                &mut archive,
                &source_path,
                &target,
                "drawing",
                "xml",
                &added,
                true,
            )?;
            types_xml = clone_content_type_override(&types_xml, &old_path, &new_path, true)?;
            let old_rels_path = rels_path(&old_path)?;
            if archive.file_names().any(|part| part == old_rels_path) {
                let drawing_rels = read_part(&mut archive, &old_rels_path)?;
                let mut chart_replacements = HashMap::new();
                for (chart_id, chart_target) in drawing_chart_relationships(&drawing_rels)? {
                    let old_chart_path = resolve_part(&old_path, &chart_target)?;
                    let (new_chart_path, chart_xml) = clone_related_part(
                        &mut archive,
                        &old_path,
                        &chart_target,
                        "chart",
                        "xml",
                        &added,
                        false,
                    )?;
                    let chart_xml =
                        rewrite_xml_references(&chart_xml, source, dest)?.unwrap_or(chart_xml);
                    if has_old_reference(&chart_xml, source)? {
                        return Err(ExcliError::new(
                            ErrorCode::NotImplemented,
                            "Chart still contains a reference to the source sheet",
                        ));
                    }
                    types_xml = clone_content_type_override(
                        &types_xml,
                        &old_chart_path,
                        &new_chart_path,
                        true,
                    )?;
                    added.insert(new_chart_path.clone(), chart_xml);
                    chart_replacements.insert(chart_id, format!("/{new_chart_path}"));
                }
                added.insert(
                    rels_path(&new_path)?,
                    rewrite_table_relationships(&drawing_rels, &chart_replacements)?,
                );
            }
            added.insert(new_path.clone(), contents);
            replacements.insert(relationship_id, format!("/{new_path}"));
        }
        if !relations.pivots.is_empty() {
            let existing_pivots = part_paths_for_content_type(&types_xml, "pivotTable+xml")?;
            let mut names = HashSet::new();
            for pivot_path in &existing_pivots {
                names.insert(pivot_name(&read_part(&mut archive, pivot_path)?)?.to_lowercase());
            }
            for (relationship_id, target) in relations.pivots {
                let old_path = resolve_part(&source_path, &target)?;
                if !existing_pivots.contains(&old_path) {
                    return Err(corrupted("PivotTable has no content type override"));
                }
                let old_rels_path = rels_path(&old_path)?;
                if !archive.file_names().any(|part| part == old_rels_path) {
                    return Err(corrupted("PivotTable has no cache relationship part"));
                }
                let pivot_rels = read_part(&mut archive, &old_rels_path)?;
                let cache_target = pivot_cache_relationship(&pivot_rels)?;
                let cache_path = resolve_part(&old_path, &cache_target)?;
                read_part(&mut archive, &cache_path)?;
                let (new_path, contents) = clone_related_part(
                    &mut archive,
                    &source_path,
                    &target,
                    "pivotTable",
                    "xml",
                    &added,
                    true,
                )?;
                let new_name = (1u32..)
                    .map(|number| format!("PivotTable{number}"))
                    .find(|candidate| !names.contains(&candidate.to_lowercase()))
                    .ok_or_else(|| corrupted("No PivotTable name available"))?;
                names.insert(new_name.to_lowercase());
                types_xml = clone_content_type_override(&types_xml, &old_path, &new_path, true)?;
                added.insert(new_path.clone(), rewrite_pivot_name(&contents, &new_name)?);
                added.insert(rels_path(&new_path)?, pivot_rels);
                replacements.insert(relationship_id, format!("/{new_path}"));
            }
        }
        added.insert(
            rels_path(&sheet_path)?,
            rewrite_table_relationships(&rels, &replacements)?,
        );
    }
    let mut modified = HashMap::from([
        (package.workbook_path, workbook_xml),
        (rels_name, rels_xml),
        ("[Content_Types].xml".to_owned(), types_xml),
    ]);
    if archive.file_names().any(|part| part == "docProps/app.xml") {
        modified.insert(
            "docProps/app.xml".to_owned(),
            update_app_properties(&read_part(&mut archive, "docProps/app.xml")?, dest)?,
        );
    }
    write_package_parts(path, &mut archive, &modified, &added)
}

#[cfg(test)]
mod tests {
    use super::*;
    use excli_core::cell::Cell;
    use excli_core::Workbook;

    #[test]
    fn adds_sheet_and_preserves_existing_sheet_parts() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("book.xlsx");
        let mut workbook = Workbook::new();
        workbook
            .add_sheet("Original")
            .set_cell(0, 0, Cell::string("untouched"));
        crate::writer::write_xlsx(&path, &workbook).unwrap();
        let old_part = read_part(
            &mut ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap(),
            "xl/worksheets/sheet1.xml",
        )
        .unwrap();

        add_sheet(&path, "新 & <Data>").unwrap();
        let mut archive = ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
        assert_eq!(
            read_part(&mut archive, "xl/worksheets/sheet1.xml").unwrap(),
            old_part
        );
        assert!(
            String::from_utf8(read_part(&mut archive, "xl/worksheets/sheet2.xml").unwrap())
                .unwrap()
                .contains("<sheetData/>")
        );
        let loaded = crate::reader::read_workbook(&path).unwrap();
        assert_eq!(loaded.sheets.len(), 2);
        assert_eq!(loaded.sheets[0].name, "Original");
        assert_eq!(loaded.sheets[1].name, "新 & <Data>");
        assert_eq!(
            loaded.sheets[0].get_cell(0, 0).unwrap().value.to_string(),
            "untouched"
        );
        drop(archive);
        crate::ooxml_value_edit::set_values(
            &path,
            Some("新 & <Data>"),
            &[((0, 0), excli_core::cell::CellValue::Number(7.0))],
        )
        .unwrap();
        let edited = crate::reader::read_workbook(&path).unwrap();
        assert_eq!(
            edited.sheets[1].get_cell(0, 0).unwrap().value.to_string(),
            "7"
        );
    }

    #[test]
    fn invalid_or_duplicate_name_does_not_modify_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("book.xlsx");
        let mut workbook = Workbook::new();
        workbook.add_sheet("Data");
        crate::writer::write_xlsx(&path, &workbook).unwrap();
        let before = std::fs::read(&path).unwrap();
        for name in [
            "data",
            "bad/name",
            "",
            "'bad",
            "abcdefghijklmnopqrstuvwxyzabcdef",
        ] {
            assert!(add_sheet(&path, name).is_err());
            assert_eq!(std::fs::read(&path).unwrap(), before);
        }
    }

    #[test]
    fn handles_prefixed_and_empty_package_roots() {
        let rels = br#"<p:Relationships xmlns:p="urn:rels"/>"#;
        let changed = append_child(rels, b"Relationships", |prefix| {
            format!("<{prefix}:Relationship/>")
        })
        .unwrap();
        assert_eq!(
            String::from_utf8(changed).unwrap(),
            "<p:Relationships xmlns:p=\"urn:rels\"><p:Relationship/></p:Relationships>"
        );
        let workbook = br#"<x:workbook xmlns:x="urn:book"><x:sheets/></x:workbook>"#;
        let changed = workbook_with_sheet(workbook, "New", 1, "rId1", None).unwrap();
        assert!(String::from_utf8(changed)
            .unwrap()
            .contains("<x:sheets><x:sheet "));
    }

    #[test]
    fn updates_excel_extended_properties_with_other_title_groups() {
        let app = br#"<Properties xmlns:vt="urn:vt"><HeadingPairs><vt:vector size="4" baseType="variant"><vt:variant><vt:lpstr>Worksheets</vt:lpstr></vt:variant><vt:variant><vt:i4>2</vt:i4></vt:variant><vt:variant><vt:lpstr>Charts</vt:lpstr></vt:variant><vt:variant><vt:i4>1</vt:i4></vt:variant></vt:vector></HeadingPairs><TitlesOfParts><vt:vector size="3" baseType="lpstr"><vt:lpstr>A</vt:lpstr><vt:lpstr>B</vt:lpstr><vt:lpstr>Chart1</vt:lpstr></vt:vector></TitlesOfParts></Properties>"#;
        let updated = String::from_utf8(update_app_properties(app, "C & D").unwrap()).unwrap();
        assert!(updated.contains("<vt:i4>3</vt:i4>"));
        assert!(updated.contains("<vt:vector size=\"4\" baseType=\"lpstr\">"));
        assert!(updated.contains(
            "<vt:lpstr>B</vt:lpstr><vt:lpstr>C &amp; D</vt:lpstr><vt:lpstr>Chart1</vt:lpstr>"
        ));
    }

    #[test]
    fn copies_sheet_xml_without_rebuilding_existing_sheets() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("book.xlsx");
        let mut workbook = Workbook::new();
        workbook.add_sheet("Source").set_cell(
            0,
            0,
            Cell::number(7.0).with_formula(excli_core::formula::Formula::new("=3+4")),
        );
        workbook
            .add_sheet("Other")
            .set_cell(0, 0, Cell::string("untouched"));
        crate::writer::write_xlsx(&path, &workbook).unwrap();
        let mut archive = ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
        let source = read_part(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        let other = read_part(&mut archive, "xl/worksheets/sheet2.xml").unwrap();
        drop(archive);
        copy_sheet(&path, "Source", "Copied").unwrap();
        let mut archive = ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
        assert_eq!(
            read_part(&mut archive, "xl/worksheets/sheet3.xml").unwrap(),
            source
        );
        assert_eq!(
            read_part(&mut archive, "xl/worksheets/sheet2.xml").unwrap(),
            other
        );
        let loaded = crate::reader::read_workbook(&path).unwrap();
        assert_eq!(loaded.sheets.len(), 3);
        assert_eq!(loaded.sheets[2].name, "Copied");
        assert_eq!(
            loaded.sheets[2]
                .get_cell(0, 0)
                .unwrap()
                .formula
                .as_ref()
                .unwrap()
                .raw,
            "=3+4"
        );
    }

    #[test]
    fn copy_clones_pivot_definition_and_shares_cache() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pivot.xlsx");
        let mut workbook = Workbook::new();
        workbook.add_sheet("Data");
        workbook.add_sheet("Pivot");
        crate::writer::write_xlsx(&path, &workbook).unwrap();
        let mut archive = ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
        let source =
            String::from_utf8(read_part(&mut archive, "xl/worksheets/sheet2.xml").unwrap())
                .unwrap();
        let sheet = source.replace("</worksheet>", "<pivotTableParts count=\"1\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><pivotTablePart r:id=\"rId1\"/></pivotTableParts></worksheet>");
        let book = String::from_utf8(read_part(&mut archive, "xl/workbook.xml").unwrap()).unwrap().replace("</workbook>", "<pivotCaches xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><pivotCache cacheId=\"1\" r:id=\"rId20\"/></pivotCaches></workbook>");
        let rels = String::from_utf8(read_part(&mut archive, "xl/_rels/workbook.xml.rels").unwrap()).unwrap().replace("</Relationships>", "<Relationship Id=\"rId20\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/pivotCacheDefinition\" Target=\"pivotCache/pivotCacheDefinition1.xml\"/></Relationships>");
        let types = String::from_utf8(read_part(&mut archive, "[Content_Types].xml").unwrap()).unwrap().replace("</Types>", "<Override PartName=\"/xl/pivotTables/pivotTable1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.pivotTable+xml\"/><Override PartName=\"/xl/pivotCache/pivotCacheDefinition1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.pivotCacheDefinition+xml\"/></Types>");
        write_package_parts(&path, &mut archive, &HashMap::from([
            ("xl/worksheets/sheet2.xml".to_owned(), sheet.into_bytes()),
            ("xl/workbook.xml".to_owned(), book.into_bytes()),
            ("xl/_rels/workbook.xml.rels".to_owned(), rels.into_bytes()),
            ("[Content_Types].xml".to_owned(), types.into_bytes()),
        ]), &HashMap::from([
            ("xl/worksheets/_rels/sheet2.xml.rels".to_owned(), b"<Relationships><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/pivotTable\" Target=\"../pivotTables/pivotTable1.xml\"/></Relationships>".to_vec()),
            ("xl/pivotTables/pivotTable1.xml".to_owned(), b"<pivotTableDefinition name=\"SalesPivot\" cacheId=\"1\"><location ref=\"B5:E20\"/></pivotTableDefinition>".to_vec()),
            ("xl/pivotTables/_rels/pivotTable1.xml.rels".to_owned(), b"<Relationships><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/pivotCacheDefinition\" Target=\"../pivotCache/pivotCacheDefinition1.xml\"/></Relationships>".to_vec()),
            ("xl/pivotCache/pivotCacheDefinition1.xml".to_owned(), b"<pivotCacheDefinition><cacheSource type=\"worksheet\"><worksheetSource sheet=\"Data\" ref=\"A1:D10\"/></cacheSource><cacheFields count=\"1\"><cacheField name=\"Region\"/></cacheFields></pivotCacheDefinition>".to_vec()),
        ])).unwrap();

        copy_sheet(&path, "Pivot", "PivotCopy").unwrap();
        let mut archive = ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
        let copied_rels = String::from_utf8(
            read_part(&mut archive, "xl/worksheets/_rels/sheet3.xml.rels").unwrap(),
        )
        .unwrap();
        assert!(copied_rels.contains("/xl/pivotTables/pivotTable2.xml"));
        let cloned =
            String::from_utf8(read_part(&mut archive, "xl/pivotTables/pivotTable2.xml").unwrap())
                .unwrap();
        assert!(cloned.contains("name=\"PivotTable1\""));
        assert_eq!(
            read_part(&mut archive, "xl/pivotTables/_rels/pivotTable2.xml.rels").unwrap(),
            read_part(&mut archive, "xl/pivotTables/_rels/pivotTable1.xml.rels").unwrap()
        );
        let loaded = crate::reader::read_workbook(&path).unwrap();
        assert_eq!(loaded.sheets[1].pivot_tables[0].name, "SalesPivot");
        assert_eq!(loaded.sheets[2].pivot_tables[0].name, "PivotTable1");
        assert_eq!(
            loaded.sheets[2].pivot_tables[0].source_range,
            "'Data'!A1:D10"
        );
        drop(archive);

        let mut archive = ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
        write_package_parts(&path, &mut archive, &HashMap::from([
            ("xl/pivotTables/_rels/pivotTable1.xml.rels".to_owned(), b"<Relationships><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/pivotCacheDefinition\" Target=\"../pivotCache/pivotCacheDefinition1.xml\"/><Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/image\" Target=\"../media/image1.png\"/></Relationships>".to_vec()),
        ]), &HashMap::new()).unwrap();
        let before = std::fs::read(&path).unwrap();
        assert!(copy_sheet(&path, "Pivot", "Another").is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }

    #[test]
    fn copy_rejects_owned_object_relationship_without_changing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("book.xlsx");
        let mut workbook = Workbook::new();
        workbook.add_sheet("Source");
        crate::writer::write_xlsx(&path, &workbook).unwrap();
        let mut archive = ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
        write_package_parts(
            &path,
            &mut archive,
            &HashMap::new(),
            &HashMap::from([(
                "xl/worksheets/_rels/sheet1.xml.rels".to_owned(),
                b"<Relationships><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/drawing\" Target=\"../drawings/drawing1.xml\"/></Relationships>".to_vec(),
            )]),
        )
        .unwrap();
        let before = std::fs::read(&path).unwrap();
        assert!(copy_sheet(&path, "Source", "Copied").is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }

    #[test]
    fn copy_clones_picture_and_chart_drawing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("book.xlsx");
        let mut workbook = Workbook::new();
        workbook.add_sheet("Source");
        crate::writer::write_xlsx(&path, &workbook).unwrap();
        let mut archive = ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
        let sheet = String::from_utf8(read_part(&mut archive, "xl/worksheets/sheet1.xml").unwrap())
            .unwrap();
        let sheet = sheet.replace("</worksheet>", "<drawing xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" r:id=\"rId1\"/></worksheet>");
        let types =
            String::from_utf8(read_part(&mut archive, "[Content_Types].xml").unwrap()).unwrap();
        let types = types.replace("</Types>", "<Default Extension=\"png\" ContentType=\"image/png\"/><Override PartName=\"/xl/drawings/drawing1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.drawing+xml\"/><Override PartName=\"/xl/charts/chart1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.drawingml.chart+xml\"/></Types>");
        let media = vec![137, 80, 78, 71, 13, 10, 26, 10];
        let drawing = b"<xdr:wsDr xmlns:xdr=\"http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><xdr:oneCellAnchor><xdr:from><xdr:col>1</xdr:col><xdr:row>2</xdr:row></xdr:from><xdr:ext cx=\"95250\" cy=\"190500\"/><xdr:pic><xdr:nvPicPr><xdr:cNvPr id=\"7\" name=\"Logo\"/></xdr:nvPicPr><xdr:blipFill><a:blip r:embed=\"rId2\"/></xdr:blipFill></xdr:pic><xdr:clientData/></xdr:oneCellAnchor><xdr:oneCellAnchor><xdr:from><xdr:col>2</xdr:col><xdr:row>3</xdr:row></xdr:from><xdr:ext cx=\"1905000\" cy=\"952500\"/><xdr:graphicFrame><c:chart r:id=\"rId3\"/></xdr:graphicFrame><xdr:clientData/></xdr:oneCellAnchor></xdr:wsDr>".to_vec();
        let chart = b"<c:chartSpace xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\"><c:chart><c:plotArea><c:barChart><c:barDir val=\"col\"/><c:ser><c:val><c:numRef><c:f>Source!$B$2:$B$4</c:f></c:numRef></c:val></c:ser></c:barChart></c:plotArea></c:chart></c:chartSpace>".to_vec();
        write_package_parts(&path, &mut archive, &HashMap::from([
            ("xl/worksheets/sheet1.xml".to_owned(), sheet.into_bytes()),
            ("[Content_Types].xml".to_owned(), types.into_bytes()),
        ]), &HashMap::from([
            ("xl/worksheets/_rels/sheet1.xml.rels".to_owned(), b"<Relationships><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/drawing\" Target=\"../drawings/drawing1.xml\"/></Relationships>".to_vec()),
            ("xl/drawings/drawing1.xml".to_owned(), drawing.clone()),
            ("xl/drawings/_rels/drawing1.xml.rels".to_owned(), b"<Relationships><Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/image\" Target=\"../media/image1.png\"/><Relationship Id=\"rId3\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/chart\" Target=\"../charts/chart1.xml\"/></Relationships>".to_vec()),
            ("xl/charts/chart1.xml".to_owned(), chart.clone()),
            ("xl/media/image1.png".to_owned(), media.clone()),
        ])).unwrap();

        copy_sheet(&path, "Source", "Copied").unwrap();
        let mut archive = ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
        assert_eq!(
            read_part(&mut archive, "xl/drawings/drawing1.xml").unwrap(),
            drawing
        );
        assert_eq!(
            read_part(&mut archive, "xl/charts/chart1.xml").unwrap(),
            chart
        );
        assert_eq!(
            read_part(&mut archive, "xl/drawings/drawing2.xml").unwrap(),
            drawing
        );
        let copied_rels = String::from_utf8(
            read_part(&mut archive, "xl/drawings/_rels/drawing2.xml.rels").unwrap(),
        )
        .unwrap();
        assert!(copied_rels.contains("../media/image1.png"));
        assert!(copied_rels.contains("/xl/charts/chart2.xml"));
        let copied_chart =
            String::from_utf8(read_part(&mut archive, "xl/charts/chart2.xml").unwrap()).unwrap();
        assert!(copied_chart.contains("&apos;Copied&apos;!$B$2:$B$4"));
        let loaded = crate::reader::read_workbook(&path).unwrap();
        assert_eq!(
            loaded.sheets[1].images[0].data.as_deref(),
            Some(media.as_slice())
        );
        assert_eq!(
            loaded.sheets[1].charts[0].source_range,
            "'Copied'!$B$2:$B$4"
        );

        drop(archive);
        let mut archive = ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
        write_package_parts(
            &path,
            &mut archive,
            &HashMap::new(),
            &HashMap::from([(
                "xl/charts/_rels/chart1.xml.rels".to_owned(),
                b"<Relationships/>".to_vec(),
            )]),
        )
        .unwrap();
        let before = std::fs::read(&path).unwrap();
        assert!(copy_sheet(&path, "Source", "Another").is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }

    #[test]
    fn copy_retains_external_hyperlink_relationship() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("book.xlsx");
        let mut workbook = Workbook::new();
        workbook.add_sheet("Source");
        crate::writer::write_xlsx(&path, &workbook).unwrap();
        let rels = b"<Relationships><Relationship Id=\"rId7\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink\" Target=\"https://example.com\" TargetMode=\"External\"/></Relationships>".to_vec();
        let mut archive = ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
        write_package_parts(
            &path,
            &mut archive,
            &HashMap::new(),
            &HashMap::from([(
                "xl/worksheets/_rels/sheet1.xml.rels".to_owned(),
                rels.clone(),
            )]),
        )
        .unwrap();
        copy_sheet(&path, "Source", "Copied").unwrap();
        let mut archive = ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
        assert_eq!(
            read_part(&mut archive, "xl/worksheets/_rels/sheet2.xml.rels").unwrap(),
            rels
        );
    }

    #[test]
    fn copy_clones_table_with_unique_identity() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("book.xlsx");
        let mut workbook = Workbook::new();
        workbook.add_sheet("Source");
        crate::writer::write_xlsx(&path, &workbook).unwrap();
        let mut archive = ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
        let original =
            String::from_utf8(read_part(&mut archive, "xl/worksheets/sheet1.xml").unwrap())
                .unwrap();
        let worksheet = original.replace("</worksheet>", "<tableParts count=\"1\"><tablePart xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" r:id=\"rId7\"/></tableParts></worksheet>");
        let types =
            String::from_utf8(read_part(&mut archive, "[Content_Types].xml").unwrap()).unwrap();
        let types = types.replace("</Types>", "<Override PartName=\"/xl/tables/table1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.table+xml\"/></Types>");
        write_package_parts(&path, &mut archive, &HashMap::from([
            ("xl/worksheets/sheet1.xml".to_owned(), worksheet.into_bytes()),
            ("[Content_Types].xml".to_owned(), types.into_bytes()),
        ]), &HashMap::from([
            ("xl/worksheets/_rels/sheet1.xml.rels".to_owned(), b"<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId7\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/table\" Target=\"../tables/table1.xml\"/></Relationships>".to_vec()),
            ("xl/tables/table1.xml".to_owned(), b"<table xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" id=\"1\" name=\"SalesTable\" displayName=\"SalesTable\" ref=\"A1:B2\"><tableColumns count=\"2\"><tableColumn id=\"1\" name=\"Item\"/><tableColumn id=\"2\" name=\"Price\"/></tableColumns></table>".to_vec()),
        ])).unwrap();
        copy_sheet(&path, "Source", "Copied").unwrap();
        let mut archive = ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
        let copied_rels = String::from_utf8(
            read_part(&mut archive, "xl/worksheets/_rels/sheet2.xml.rels").unwrap(),
        )
        .unwrap();
        assert!(copied_rels.contains("Target=\"/xl/tables/table2.xml\""));
        let cloned = read_part(&mut archive, "xl/tables/table2.xml").unwrap();
        assert_eq!(table_identity(&cloned).unwrap(), (2, "Table1".to_owned()));
        assert_eq!(
            table_identity(&read_part(&mut archive, "xl/tables/table1.xml").unwrap()).unwrap(),
            (1, "SalesTable".to_owned())
        );
        let loaded = crate::reader::read_workbook(&path).unwrap();
        assert_eq!(loaded.sheets[1].tables[0].name, "Table1");
        assert_eq!(loaded.sheets[1].tables[0].range, "A1:B2");
        drop(archive);
        copy_sheet(&path, "Source", "CopiedAgain").unwrap();
        let mut archive = ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
        assert_eq!(
            table_identity(&read_part(&mut archive, "xl/tables/table3.xml").unwrap()).unwrap(),
            (3, "Table2".to_owned())
        );
    }

    #[test]
    fn copy_rewrites_self_references_and_clones_local_names() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("book.xlsx");
        let mut workbook = Workbook::new();
        workbook.add_sheet("Source").set_cell(
            0,
            0,
            Cell::number(1.0).with_formula(excli_core::formula::Formula::new("=Source!B1")),
        );
        workbook.add_sheet("Other");
        crate::writer::write_xlsx(&path, &workbook).unwrap();
        let mut archive = ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
        let original_sheet = read_part(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        let original_book =
            String::from_utf8(read_part(&mut archive, "xl/workbook.xml").unwrap()).unwrap();
        let named_book = original_book.replace("</workbook>", "<definedNames><definedName name=\"LocalRange\" localSheetId=\"0\">Source!$A$1:$B$2</definedName></definedNames></workbook>");
        write_package_parts(
            &path,
            &mut archive,
            &HashMap::from([("xl/workbook.xml".to_owned(), named_book.into_bytes())]),
            &HashMap::new(),
        )
        .unwrap();
        copy_sheet(&path, "Source", "Copy Me").unwrap();
        let mut archive = ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
        assert_eq!(
            read_part(&mut archive, "xl/worksheets/sheet1.xml").unwrap(),
            original_sheet
        );
        let copied_sheet =
            String::from_utf8(read_part(&mut archive, "xl/worksheets/sheet3.xml").unwrap())
                .unwrap();
        assert!(copied_sheet.contains("&apos;Copy Me&apos;!B1"));
        let book = String::from_utf8(read_part(&mut archive, "xl/workbook.xml").unwrap()).unwrap();
        assert!(book.contains("localSheetId=\"0\">Source!$A$1:$B$2"));
        assert!(book.contains("localSheetId=\"2\">&apos;Copy Me&apos;!$A$1:$B$2"));
    }

    #[test]
    fn copy_clones_legacy_comments_and_vml_drawing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("notes.xlsx");
        let mut workbook = Workbook::new();
        workbook
            .add_sheet("Source")
            .set_cell(0, 0, Cell::string("Value"));
        crate::writer::write_xlsx(&path, &workbook).unwrap();
        let mut archive = ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
        let worksheet =
            String::from_utf8(read_part(&mut archive, "xl/worksheets/sheet1.xml").unwrap())
                .unwrap();
        let worksheet = worksheet.replace("</worksheet>", "<legacyDrawing xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" r:id=\"rId8\"/></worksheet>");
        let types =
            String::from_utf8(read_part(&mut archive, "[Content_Types].xml").unwrap()).unwrap();
        let types = types.replace("</Types>", "<Default Extension=\"vml\" ContentType=\"application/vnd.openxmlformats-officedocument.vmlDrawing\"/><Override PartName=\"/xl/comments/comment1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.comments+xml\"/></Types>");
        write_package_parts(&path, &mut archive, &HashMap::from([
            ("xl/worksheets/sheet1.xml".to_owned(), worksheet.into_bytes()),
            ("[Content_Types].xml".to_owned(), types.into_bytes()),
        ]), &HashMap::from([
            ("xl/worksheets/_rels/sheet1.xml.rels".to_owned(), b"<Relationships><Relationship Id=\"rId7\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments\" Target=\"../comments/comment1.xml\"/><Relationship Id=\"rId8\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/vmlDrawing\" Target=\"../drawings/vmlDrawing1.vml\"/></Relationships>".to_vec()),
            ("xl/comments/comment1.xml".to_owned(), b"<comments><authors><author>Alice</author></authors><commentList><comment ref=\"A1\" authorId=\"0\"><text><t>Note</t></text></comment></commentList></comments>".to_vec()),
            ("xl/drawings/vmlDrawing1.vml".to_owned(), b"<xml xmlns:v=\"urn:schemas-microsoft-com:vml\"><v:shape id=\"_x0000_s1025\"/></xml>".to_vec()),
        ])).unwrap();
        copy_sheet(&path, "Source", "Copied").unwrap();
        let mut archive = ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
        let copied_rels = String::from_utf8(
            read_part(&mut archive, "xl/worksheets/_rels/sheet2.xml.rels").unwrap(),
        )
        .unwrap();
        assert!(copied_rels.contains("/xl/comments/comment2.xml"));
        assert!(copied_rels.contains("/xl/drawings/vmlDrawing2.vml"));
        assert_eq!(
            read_part(&mut archive, "xl/comments/comment2.xml").unwrap(),
            read_part(&mut archive, "xl/comments/comment1.xml").unwrap()
        );
        assert_eq!(
            read_part(&mut archive, "xl/drawings/vmlDrawing2.vml").unwrap(),
            read_part(&mut archive, "xl/drawings/vmlDrawing1.vml").unwrap()
        );
        let loaded = crate::reader::read_workbook(&path).unwrap();
        assert_eq!(loaded.sheets[1].comments[0].text, "Note");
    }

    #[test]
    fn copy_clones_threaded_comments_with_new_ids() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("threads.xlsx");
        let mut workbook = Workbook::new();
        workbook
            .add_sheet("Source")
            .set_cell(0, 0, Cell::string("Value"));
        crate::writer::write_xlsx(&path, &workbook).unwrap();
        let mut archive = ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
        let rels =
            String::from_utf8(read_part(&mut archive, "xl/_rels/workbook.xml.rels").unwrap())
                .unwrap();
        let rels = rels.replace("</Relationships>", "<Relationship Id=\"rId99\" Type=\"http://schemas.microsoft.com/office/2017/10/relationships/person\" Target=\"persons/person.xml\"/></Relationships>");
        let types =
            String::from_utf8(read_part(&mut archive, "[Content_Types].xml").unwrap()).unwrap();
        let types = types.replace("</Types>", "<Override PartName=\"/xl/persons/person.xml\" ContentType=\"application/vnd.ms-excel.person+xml\"/><Override PartName=\"/xl/threadedComments/threadedComment1.xml\" ContentType=\"application/vnd.ms-excel.threadedcomments+xml\"/></Types>");
        let original_thread = b"<ThreadedComments><threadedComment ref=\"A1\" personId=\"{alice}\" id=\"{root}\"><text>Hello</text><mentions><mention mentionpersonId=\"{bob}\" mentionId=\"{mention}\" startIndex=\"0\" length=\"5\"/></mentions></threadedComment><threadedComment personId=\"{bob}\" id=\"{reply}\" parentId=\"{root}\"><text>Reply</text></threadedComment></ThreadedComments>".to_vec();
        write_package_parts(&path, &mut archive, &HashMap::from([
            ("xl/_rels/workbook.xml.rels".to_owned(), rels.into_bytes()),
            ("[Content_Types].xml".to_owned(), types.into_bytes()),
        ]), &HashMap::from([
            ("xl/persons/person.xml".to_owned(), b"<personList><person id=\"{alice}\" displayName=\"Alice\"/><person id=\"{bob}\" displayName=\"Bob\"/></personList>".to_vec()),
            ("xl/worksheets/_rels/sheet1.xml.rels".to_owned(), b"<Relationships><Relationship Id=\"rId9\" Type=\"http://schemas.microsoft.com/office/2017/10/relationships/threadedComment\" Target=\"../threadedComments/threadedComment1.xml\"/></Relationships>".to_vec()),
            ("xl/threadedComments/threadedComment1.xml".to_owned(), original_thread.clone()),
        ])).unwrap();
        copy_sheet(&path, "Source", "Copied").unwrap();
        let mut archive = ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
        assert_eq!(
            read_part(&mut archive, "xl/threadedComments/threadedComment1.xml").unwrap(),
            original_thread
        );
        let copied = String::from_utf8(
            read_part(&mut archive, "xl/threadedComments/threadedComment2.xml").unwrap(),
        )
        .unwrap();
        assert!(!copied.contains("id=\"{root}\""));
        assert!(!copied.contains("parentId=\"{root}\""));
        assert!(!copied.contains("mentionId=\"{mention}\""));
        let loaded = crate::reader::read_workbook(&path).unwrap();
        assert_eq!(loaded.sheets[1].threaded_comments.len(), 2);
        assert_eq!(
            loaded.sheets[1].threaded_comments[0].author.as_deref(),
            Some("Alice")
        );
        assert_eq!(
            loaded.sheets[1].threaded_comments[1].author.as_deref(),
            Some("Bob")
        );
        assert_eq!(
            loaded.sheets[1].threaded_comments[1].parent_id.as_deref(),
            Some(loaded.sheets[1].threaded_comments[0].id.as_str())
        );
        assert_ne!(
            loaded.sheets[0].threaded_comments[0].id,
            loaded.sheets[1].threaded_comments[0].id
        );
        assert_ne!(
            loaded.sheets[0].threaded_comments[0].mentions[0].id,
            loaded.sheets[1].threaded_comments[0].mentions[0].id
        );
        drop(archive);
        copy_sheet(&path, "Source", "CopiedAgain").unwrap();
        let repeated = crate::reader::read_workbook(&path).unwrap();
        assert_ne!(
            repeated.sheets[1].threaded_comments[0].id,
            repeated.sheets[2].threaded_comments[0].id
        );
    }
}
