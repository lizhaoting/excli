//! Targeted edits to existing OOXML workbooks. Untouched ZIP members are copied
//! without rebuilding the workbook from the reduced in-memory data model.

use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::path::Path;

use excli_core::error::{ErrorCode, ExcliError, ExcliResult};
use excli_core::sheet::SheetState;
use excli_core::types::FileFormat;
use quick_xml::events::Event;
use quick_xml::Reader;
use zip::write::SimpleFileOptions;
use zip::{ZipArchive, ZipWriter};

use crate::format_detect::detect_format;
use crate::ooxml_package::{index as package_index, read_part};

#[derive(Default)]
pub(crate) struct SharedString {
    pub(crate) text: String,
    pub(crate) rich: bool,
    pub(crate) raw_xml: Option<Vec<u8>>,
}

fn invalid(message: impl Into<String>) -> ExcliError {
    ExcliError::new(ErrorCode::FileCorrupted, message)
}

fn unsupported(message: impl Into<String>) -> ExcliError {
    ExcliError::new(ErrorCode::NotImplemented, message)
}

fn xml_error(error: impl std::fmt::Display) -> ExcliError {
    invalid(format!("Invalid worksheet XML: {error}"))
}

pub(crate) fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

pub(crate) fn ensure_editable(archive: &mut ZipArchive<std::fs::File>) -> ExcliResult<()> {
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|e| invalid(format!("Invalid Excel ZIP entry: {e}")))?;
        let name = entry.name();
        if name.starts_with("_xmlsignatures/") || name.starts_with("xl/vbaProjectSignature") {
            return Err(unsupported(
                "Digitally signed workbooks cannot be edited without invalidating the signature",
            ));
        }
    }
    Ok(())
}

pub(crate) fn write_modified_parts(
    path: &Path,
    archive: &mut ZipArchive<std::fs::File>,
    modified: &HashMap<String, Vec<u8>>,
) -> ExcliResult<()> {
    write_package_parts(path, archive, modified, &HashMap::new())
}

pub(crate) fn write_package_parts(
    path: &Path,
    archive: &mut ZipArchive<std::fs::File>,
    modified: &HashMap<String, Vec<u8>>,
    added: &HashMap<String, Vec<u8>>,
) -> ExcliResult<()> {
    write_package_changes(path, archive, modified, added, &HashSet::new())
}

pub(crate) fn write_package_changes(
    path: &Path,
    archive: &mut ZipArchive<std::fs::File>,
    modified: &HashMap<String, Vec<u8>>,
    added: &HashMap<String, Vec<u8>>,
    removed: &HashSet<String>,
) -> ExcliResult<()> {
    if modified.keys().any(|name| removed.contains(name))
        || added.keys().any(|name| removed.contains(name))
    {
        return Err(invalid(
            "A package part cannot be added or modified and removed",
        ));
    }
    let mut found = std::collections::HashSet::new();
    let mut existing = std::collections::HashSet::new();
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|e| invalid(format!("Invalid Excel ZIP entry: {e}")))?;
        if !existing.insert(entry.name().to_owned()) {
            return Err(invalid(format!("Duplicate package part: {}", entry.name())));
        }
        if added.contains_key(entry.name()) {
            return Err(invalid(format!(
                "Package part already exists: {}",
                entry.name()
            )));
        }
        if modified.contains_key(entry.name()) {
            found.insert(entry.name().to_owned());
        }
    }
    if found.len() != modified.len() {
        return Err(invalid("A modified package part does not exist"));
    }
    if !removed.is_subset(&existing) {
        return Err(invalid("A removed package part does not exist"));
    }
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    let permissions = std::fs::metadata(path)?.permissions();
    {
        let mut writer = ZipWriter::new(temp.as_file_mut());
        writer.set_raw_comment(archive.comment().to_vec().into_boxed_slice());
        for index in 0..archive.len() {
            let entry = archive
                .by_index(index)
                .map_err(|e| invalid(format!("Invalid Excel ZIP entry: {e}")))?;
            if removed.contains(entry.name()) {
                continue;
            }
            if let Some(xml) = modified.get(entry.name()) {
                let options = SimpleFileOptions::default()
                    .compression_method(entry.compression())
                    .unix_permissions(entry.unix_mode().unwrap_or(0o644));
                writer
                    .start_file(entry.name(), options)
                    .map_err(|e| ExcliError::new(ErrorCode::WriteError, e.to_string()))?;
                writer.write_all(xml)?;
            } else {
                writer
                    .raw_copy_file(entry)
                    .map_err(|e| ExcliError::new(ErrorCode::WriteError, e.to_string()))?;
            }
        }
        let mut added_names: Vec<_> = added.keys().collect();
        added_names.sort();
        for name in added_names {
            writer
                .start_file(
                    name,
                    SimpleFileOptions::default()
                        .compression_method(zip::CompressionMethod::Deflated),
                )
                .map_err(|e| ExcliError::new(ErrorCode::WriteError, e.to_string()))?;
            writer.write_all(&added[name])?;
        }
        writer
            .finish()
            .map_err(|e| ExcliError::new(ErrorCode::WriteError, e.to_string()))?;
    }
    temp.as_file_mut().sync_all()?;
    temp.as_file_mut().set_permissions(permissions)?;
    temp.persist(path)
        .map_err(|e| ExcliError::new(ErrorCode::WriteError, e.error.to_string()))?;
    Ok(())
}

pub(crate) fn read_shared_strings(xml: &[u8]) -> ExcliResult<Vec<SharedString>> {
    let mut reader = Reader::from_reader(xml);
    let mut strings = Vec::new();
    let mut current: Option<SharedString> = None;
    let mut in_text = false;
    let mut item_start = None;

    loop {
        let from = reader.buffer_position() as usize;
        let event = reader.read_event().map_err(xml_error)?;
        let to = reader.buffer_position() as usize;
        match event {
            Event::Start(event) if event.local_name().as_ref() == b"si" => {
                current = Some(SharedString::default());
                item_start = Some(from);
            }
            Event::Start(event) if matches!(event.local_name().as_ref(), b"r" | b"rPh") => {
                if let Some(value) = &mut current {
                    value.rich = true;
                }
            }
            Event::Start(event) if event.local_name().as_ref() == b"t" => in_text = true,
            Event::Text(event) if in_text => {
                if let Some(value) = &mut current {
                    value.text.push_str(&event.unescape().map_err(xml_error)?);
                }
            }
            Event::CData(event) if in_text => {
                if let Some(value) = &mut current {
                    value
                        .text
                        .push_str(&String::from_utf8_lossy(event.as_ref()));
                }
            }
            Event::End(event) if event.local_name().as_ref() == b"t" => in_text = false,
            Event::End(event) if event.local_name().as_ref() == b"si" => {
                if let Some(mut value) = current.take() {
                    if value.rich {
                        let start =
                            item_start.ok_or_else(|| invalid("Missing shared-string start"))?;
                        value.raw_xml = Some(xml[start..to].to_vec());
                    }
                    strings.push(value);
                }
                item_start = None;
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(strings)
}

fn rich_text_tag(tag: &quick_xml::events::BytesStart<'_>, text: &str) -> ExcliResult<Vec<u8>> {
    let name = std::str::from_utf8(tag.name().as_ref())
        .map_err(xml_error)?
        .to_owned();
    let mut output = format!("<{name}");
    let mut has_space = false;
    for item in tag.attributes() {
        let item = item.map_err(xml_error)?;
        let key = std::str::from_utf8(item.key.as_ref()).map_err(xml_error)?;
        if key == "xml:space" {
            output.push_str(" xml:space=\"preserve\"");
            has_space = true;
        } else {
            let value = item.unescape_value().map_err(xml_error)?;
            output.push_str(&format!(" {key}=\"{}\"", xml_escape(&value)));
        }
    }
    if !has_space {
        output.push_str(" xml:space=\"preserve\"");
    }
    output.push('>');
    output.push_str(&xml_escape(text));
    output.push_str(&format!("</{name}>"));
    Ok(output.into_bytes())
}

fn replace_inline_rich_cell(xml: &[u8], find: &str, replace: &str) -> ExcliResult<Option<Vec<u8>>> {
    struct TextSpan {
        from: usize,
        to: usize,
        tag: quick_xml::events::BytesStart<'static>,
        text: String,
    }

    let mut reader = Reader::from_reader(xml);
    let mut in_inline = false;
    let mut in_run = false;
    let mut active: Option<TextSpan> = None;
    let mut spans = Vec::new();
    loop {
        let from = reader.buffer_position() as usize;
        let event = reader.read_event().map_err(xml_error)?;
        let to = reader.buffer_position() as usize;
        match event {
            Event::Start(ref tag) if matches!(tag.local_name().as_ref(), b"is" | b"si") => {
                in_inline = true;
            }
            Event::Start(ref tag) if in_inline && tag.local_name().as_ref() == b"r" => {
                in_run = true;
            }
            Event::Start(ref tag) if in_run && tag.local_name().as_ref() == b"t" => {
                active = Some(TextSpan {
                    from,
                    to,
                    tag: tag.to_owned(),
                    text: String::new(),
                });
            }
            Event::Text(ref event) if active.is_some() => {
                if let Some(ref mut span) = active {
                    span.text.push_str(&event.unescape().map_err(xml_error)?);
                }
            }
            Event::CData(ref event) if active.is_some() => {
                if let Some(ref mut span) = active {
                    span.text
                        .push_str(std::str::from_utf8(event.as_ref()).map_err(xml_error)?);
                }
            }
            Event::End(ref tag) if tag.local_name().as_ref() == b"t" && active.is_some() => {
                if let Some(mut span) = active.take() {
                    span.to = to;
                    spans.push(span);
                }
            }
            Event::End(ref tag) if tag.local_name().as_ref() == b"r" => in_run = false,
            Event::End(ref tag) if matches!(tag.local_name().as_ref(), b"is" | b"si") => {
                in_inline = false;
            }
            Event::Start(ref tag) | Event::Empty(ref tag)
                if matches!(
                    tag.local_name().as_ref(),
                    b"rPh" | b"phoneticPr" | b"extLst"
                ) =>
            {
                return Err(unsupported(
                    "Rich-text phonetics or extensions need dedicated editing",
                ));
            }
            Event::Eof => break,
            _ => {}
        }
    }
    let full: String = spans.iter().map(|span| span.text.as_str()).collect();
    if !full.contains(find) {
        return Ok(None);
    }
    let replacements: Vec<String> = spans
        .iter()
        .map(|span| span.text.replace(find, replace))
        .collect();
    let joined: String = replacements.iter().map(String::as_str).collect();
    if joined != full.replace(find, replace) {
        return Err(unsupported(
            "Replacement crosses rich-text formatting boundaries",
        ));
    }
    let mut output = Vec::with_capacity(xml.len());
    let mut cursor = 0;
    for (span, replacement) in spans.iter().zip(&replacements) {
        if replacement != &span.text {
            output.extend_from_slice(&xml[cursor..span.from]);
            output.extend_from_slice(&rich_text_tag(&span.tag, replacement)?);
            cursor = span.to;
        }
    }
    output.extend_from_slice(&xml[cursor..]);
    Ok(Some(output))
}

fn shared_rich_as_inline(xml: &[u8]) -> ExcliResult<Vec<u8>> {
    const MAIN_NS: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
    let mut reader = Reader::from_reader(xml);
    let mut opening_end = None;
    let mut closing_start = None;
    loop {
        let from = reader.buffer_position() as usize;
        let event = reader.read_event().map_err(xml_error)?;
        let to = reader.buffer_position() as usize;
        match event {
            Event::Start(ref tag) => {
                if tag.name().as_ref().contains(&b':') {
                    return Err(unsupported(
                        "Prefixed shared rich text needs namespace-aware cloning",
                    ));
                }
                if opening_end.is_none() {
                    if tag.name().as_ref() != b"si" {
                        return Err(invalid("Shared rich text has no si root"));
                    }
                    for item in tag.attributes() {
                        let item = item.map_err(xml_error)?;
                        if item.key.as_ref() != b"xmlns"
                            || item.unescape_value().map_err(xml_error)?.as_ref() != MAIN_NS
                        {
                            return Err(unsupported("Shared rich text has dependent namespaces"));
                        }
                    }
                    opening_end = Some(to);
                } else {
                    for item in tag.attributes() {
                        let item = item.map_err(xml_error)?;
                        if item.key.as_ref().starts_with(b"xmlns")
                            || (item.key.as_ref().contains(&b':')
                                && !item.key.as_ref().starts_with(b"xml:"))
                        {
                            return Err(unsupported("Shared rich text has dependent namespaces"));
                        }
                    }
                }
            }
            Event::Empty(ref tag) => {
                if tag.name().as_ref().contains(&b':') {
                    return Err(unsupported(
                        "Prefixed shared rich text needs namespace-aware cloning",
                    ));
                }
                for item in tag.attributes() {
                    let item = item.map_err(xml_error)?;
                    if item.key.as_ref().starts_with(b"xmlns")
                        || (item.key.as_ref().contains(&b':')
                            && !item.key.as_ref().starts_with(b"xml:"))
                    {
                        return Err(unsupported("Shared rich text has dependent namespaces"));
                    }
                }
            }
            Event::End(ref tag) if tag.name().as_ref() == b"si" => {
                closing_start = Some(from);
            }
            Event::Eof => break,
            _ => {}
        }
    }
    let start = opening_end.ok_or_else(|| invalid("Shared rich text has no si root"))?;
    let end = closing_start.ok_or_else(|| invalid("Shared rich text has no si end"))?;
    let mut output = format!("<is xmlns=\"{MAIN_NS}\">").into_bytes();
    output.extend_from_slice(&xml[start..end]);
    output.extend_from_slice(b"</is>");
    Ok(output)
}

fn replacement_cell(
    xml: &[u8],
    shared_strings: &[SharedString],
    find: &str,
    replace: &str,
) -> ExcliResult<Option<Vec<u8>>> {
    let mut reader = Reader::from_reader(xml);
    let mut cell_type = String::new();
    let mut cell_tag = None;
    let mut attributes = Vec::new();
    let mut value = String::new();
    let mut inline = String::new();
    let mut in_value = false;
    let mut in_text = false;
    let mut has_formula = false;
    let mut rich = false;
    let mut unsupported_feature = false;
    let mut unsupported_cell_attributes = false;

    loop {
        match reader.read_event().map_err(xml_error)? {
            Event::Start(event) if event.local_name().as_ref() == b"c" => {
                cell_tag = Some(
                    std::str::from_utf8(event.name().as_ref())
                        .map_err(xml_error)?
                        .to_owned(),
                );
                for attr in event.attributes() {
                    let attr = attr.map_err(xml_error)?;
                    let key = std::str::from_utf8(attr.key.as_ref()).map_err(xml_error)?;
                    let val = attr.unescape_value().map_err(xml_error)?.into_owned();
                    if key == "t" {
                        cell_type = val;
                    } else {
                        if matches!(key, "cm" | "vm") {
                            unsupported_feature = true;
                            unsupported_cell_attributes = true;
                        }
                        attributes.push((key.to_owned(), val));
                    }
                }
            }
            Event::Start(event) if event.local_name().as_ref() == b"v" => in_value = true,
            Event::Start(event) if event.local_name().as_ref() == b"t" => in_text = true,
            Event::Start(event) if event.local_name().as_ref() == b"f" => has_formula = true,
            Event::Empty(event) if event.local_name().as_ref() == b"f" => has_formula = true,
            Event::Start(event) if matches!(event.local_name().as_ref(), b"r" | b"rPh") => {
                rich = true
            }
            Event::Start(event) | Event::Empty(event)
                if !matches!(event.local_name().as_ref(), b"c" | b"v" | b"is" | b"t") =>
            {
                unsupported_feature = true;
            }
            Event::Text(event) => {
                if in_value {
                    value.push_str(&event.unescape().map_err(xml_error)?);
                } else if in_text {
                    inline.push_str(&event.unescape().map_err(xml_error)?);
                }
            }
            Event::CData(event) => {
                if in_text {
                    inline.push_str(&String::from_utf8_lossy(event.as_ref()));
                }
            }
            Event::End(event) if event.local_name().as_ref() == b"v" => in_value = false,
            Event::End(event) if event.local_name().as_ref() == b"t" => in_text = false,
            Event::Eof => break,
            _ => {}
        }
    }

    if has_formula {
        return Ok(None);
    }
    let (original, is_rich, raw_rich) = match cell_type.as_str() {
        "s" => {
            let index: usize = value
                .parse()
                .map_err(|_| invalid("Invalid shared string index"))?;
            let item = shared_strings
                .get(index)
                .ok_or_else(|| invalid("Shared string index is out of range"))?;
            (&item.text, item.rich, item.raw_xml.as_deref())
        }
        "inlineStr" => (&inline, rich, None),
        _ => return Ok(None),
    };
    if !original.contains(find) {
        return Ok(None);
    }
    if unsupported_cell_attributes {
        return Err(unsupported(
            "Replacement cannot discard unsupported cell metadata",
        ));
    }
    if is_rich {
        if cell_type == "inlineStr" {
            return replace_inline_rich_cell(xml, find, replace);
        }
        if unsupported_feature {
            return Err(unsupported(
                "Replacement cannot discard unsupported cell metadata",
            ));
        }
        let raw = raw_rich.ok_or_else(|| invalid("Missing rich shared-string XML"))?;
        let replaced = replace_inline_rich_cell(raw, find, replace)?
            .ok_or_else(|| invalid("Rich shared-string text does not match"))?;
        let inline = shared_rich_as_inline(&replaced)?;
        let cell_tag = cell_tag.ok_or_else(|| invalid("Replacement cell has no root"))?;
        let mut output = format!("<{cell_tag}");
        for (key, value) in attributes {
            output.push_str(&format!(" {key}=\"{}\"", xml_escape(&value)));
        }
        output.push_str(" t=\"inlineStr\">");
        output.push_str(std::str::from_utf8(&inline).map_err(xml_error)?);
        output.push_str(&format!("</{cell_tag}>"));
        return Ok(Some(output.into_bytes()));
    }
    if unsupported_feature {
        return Err(unsupported(
            "Replacement cannot discard unsupported cell metadata",
        ));
    }

    let new_text = original.replace(find, replace);
    let cell_tag = cell_tag.ok_or_else(|| invalid("Replacement cell has no root"))?;
    let prefix = cell_tag
        .rsplit_once(':')
        .map_or(String::new(), |(prefix, _)| format!("{prefix}:"));
    let mut output = format!("<{cell_tag}");
    for (key, value) in attributes {
        output.push_str(&format!(" {key}=\"{}\"", xml_escape(&value)));
    }
    output.push_str(&format!(
        " t=\"inlineStr\"><{prefix}is><{prefix}t xml:space=\"preserve\">"
    ));
    output.push_str(&xml_escape(&new_text));
    output.push_str(&format!("</{prefix}t></{prefix}is></{cell_tag}>"));
    Ok(Some(output.into_bytes()))
}

fn replace_sheet(
    xml: &[u8],
    shared_strings: &[SharedString],
    find: &str,
    replace: &str,
) -> ExcliResult<(Vec<u8>, usize)> {
    let mut reader = Reader::from_reader(xml);
    let mut cell_start = None;
    let mut changes = Vec::new();

    loop {
        let start = reader.buffer_position() as usize;
        match reader.read_event().map_err(xml_error)? {
            Event::Start(event) if event.local_name().as_ref() == b"c" => {
                cell_start = Some(start);
            }
            Event::End(event) if event.local_name().as_ref() == b"c" => {
                if let Some(from) = cell_start.take() {
                    let to = reader.buffer_position() as usize;
                    if let Some(new_cell) =
                        replacement_cell(&xml[from..to], shared_strings, find, replace)?
                    {
                        changes.push((from, to, new_cell));
                    }
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }

    let count = changes.len();
    if count == 0 {
        return Ok((xml.to_vec(), 0));
    }
    let mut output = Vec::with_capacity(xml.len());
    let mut cursor = 0;
    for (from, to, new_cell) in changes {
        output.extend_from_slice(&xml[cursor..from]);
        output.extend_from_slice(&new_cell);
        cursor = to;
    }
    output.extend_from_slice(&xml[cursor..]);
    Ok((output, count))
}

pub(crate) fn recalculation_xml(xml: &[u8]) -> ExcliResult<Vec<u8>> {
    let mut reader = Reader::from_reader(xml);
    let mut workbook_end = None;
    loop {
        let from = reader.buffer_position() as usize;
        match reader.read_event().map_err(xml_error)? {
            Event::Start(event) | Event::Empty(event)
                if event.local_name().as_ref() == b"calcPr" =>
            {
                let to = reader.buffer_position() as usize;
                let name = std::str::from_utf8(event.name().as_ref())
                    .map_err(xml_error)?
                    .to_owned();
                let mut tag = format!("<{name}");
                for attr in event.attributes() {
                    let attr = attr.map_err(xml_error)?;
                    let key = std::str::from_utf8(attr.key.as_ref()).map_err(xml_error)?;
                    if key != "fullCalcOnLoad" && key != "forceFullCalc" {
                        let value = attr.unescape_value().map_err(xml_error)?;
                        tag.push_str(&format!(" {key}=\"{}\"", xml_escape(&value)));
                    }
                }
                tag.push_str(" fullCalcOnLoad=\"1\" forceFullCalc=\"1\"");
                if xml[from..to].ends_with(b"/>") {
                    tag.push_str("/>");
                } else {
                    tag.push('>');
                }
                let mut result = Vec::with_capacity(xml.len() + tag.len());
                result.extend_from_slice(&xml[..from]);
                result.extend_from_slice(tag.as_bytes());
                result.extend_from_slice(&xml[to..]);
                return Ok(result);
            }
            Event::End(event) if event.local_name().as_ref() == b"workbook" => {
                workbook_end = Some((
                    from,
                    std::str::from_utf8(event.name().as_ref())
                        .map_err(xml_error)?
                        .to_owned(),
                ));
            }
            Event::Eof => break,
            _ => {}
        }
    }
    let (at, name) =
        workbook_end.ok_or_else(|| invalid("Workbook XML has no closing workbook tag"))?;
    let prefix = name.split_once(':').map_or("", |(prefix, _)| prefix);
    let tag = if prefix.is_empty() {
        "<calcPr fullCalcOnLoad=\"1\" forceFullCalc=\"1\"/>".to_owned()
    } else {
        format!("<{prefix}:calcPr fullCalcOnLoad=\"1\" forceFullCalc=\"1\"/>")
    };
    let mut result = Vec::with_capacity(xml.len() + 80);
    result.extend_from_slice(&xml[..at]);
    result.extend_from_slice(tag.as_bytes());
    result.extend_from_slice(&xml[at..]);
    Ok(result)
}

fn sheet_state_xml(
    xml: &[u8],
    sheet_name: &str,
    state: SheetState,
) -> ExcliResult<Option<Vec<u8>>> {
    let mut reader = Reader::from_reader(xml);
    let mut visible_count = 0usize;
    let mut target = None;

    loop {
        let from = reader.buffer_position() as usize;
        match reader.read_event().map_err(xml_error)? {
            Event::Start(event) | Event::Empty(event)
                if event.local_name().as_ref() == b"sheet" =>
            {
                let to = reader.buffer_position() as usize;
                let tag_name = std::str::from_utf8(event.name().as_ref())
                    .map_err(xml_error)?
                    .to_owned();
                let mut attributes = Vec::new();
                let mut name = None;
                let mut existing_state = SheetState::Visible;
                for attribute in event.attributes() {
                    let attribute = attribute.map_err(xml_error)?;
                    let key = std::str::from_utf8(attribute.key.as_ref()).map_err(xml_error)?;
                    let value = attribute.unescape_value().map_err(xml_error)?.into_owned();
                    if key == "name" {
                        name = Some(value.clone());
                    } else if key == "state" {
                        existing_state = match value.as_str() {
                            "hidden" => SheetState::Hidden,
                            "veryHidden" => SheetState::VeryHidden,
                            "visible" => SheetState::Visible,
                            _ => return Err(invalid(format!("Unknown sheet state: {value}"))),
                        };
                    }
                    if key != "state" {
                        attributes.push((key.to_owned(), value));
                    }
                }
                if existing_state == SheetState::Visible {
                    visible_count += 1;
                }
                if name.as_deref() == Some(sheet_name) {
                    if target.is_some() {
                        return Err(invalid("Duplicate sheet name in workbook XML"));
                    }
                    target = Some((from, to, tag_name, attributes, existing_state));
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }

    let (from, to, tag_name, attributes, old_state) =
        target.ok_or_else(|| ExcliError::sheet_not_found(sheet_name))?;
    if old_state == state {
        return Ok(None);
    }
    if old_state == SheetState::Visible && visible_count == 1 && state != SheetState::Visible {
        return Err(ExcliError::new(
            ErrorCode::GeneralError,
            "A workbook must contain at least one visible sheet",
        ));
    }

    let mut tag = format!("<{tag_name}");
    for (key, value) in attributes {
        tag.push_str(&format!(" {key}=\"{}\"", xml_escape(&value)));
    }
    match state {
        SheetState::Visible => {}
        SheetState::Hidden => tag.push_str(" state=\"hidden\""),
        SheetState::VeryHidden => tag.push_str(" state=\"veryHidden\""),
    }
    if xml[from..to].ends_with(b"/>") {
        tag.push_str("/>");
    } else {
        tag.push('>');
    }
    let mut output = Vec::with_capacity(xml.len() + tag.len());
    output.extend_from_slice(&xml[..from]);
    output.extend_from_slice(tag.as_bytes());
    output.extend_from_slice(&xml[to..]);
    Ok(Some(output))
}

fn reindex_tag(
    xml: &[u8],
    from: usize,
    to: usize,
    event: &quick_xml::events::BytesStart<'_>,
    mapping: &[usize],
    keys: &[&str],
) -> ExcliResult<Option<Vec<u8>>> {
    let name = std::str::from_utf8(event.name().as_ref())
        .map_err(xml_error)?
        .to_owned();
    let mut attributes = Vec::new();
    let mut changed = false;
    for attribute in event.attributes() {
        let attribute = attribute.map_err(xml_error)?;
        let key = std::str::from_utf8(attribute.key.as_ref()).map_err(xml_error)?;
        let value = attribute.unescape_value().map_err(xml_error)?.into_owned();
        let value = if keys.contains(&key) {
            let old = value
                .parse::<usize>()
                .map_err(|_| invalid(format!("Invalid {key} index")))?;
            let new = *mapping
                .get(old)
                .ok_or_else(|| invalid(format!("{key} index is outside sheet list")))?;
            changed |= old != new;
            new.to_string()
        } else {
            value
        };
        attributes.push((key.to_owned(), value));
    }
    if !changed {
        return Ok(None);
    }
    let mut tag = format!("<{name}");
    for (key, value) in attributes {
        tag.push_str(&format!(" {key}=\"{}\"", xml_escape(&value)));
    }
    if xml[from..to].ends_with(b"/>") {
        tag.push_str("/>");
    } else {
        tag.push('>');
    }
    Ok(Some(tag.into_bytes()))
}

fn move_sheet_xml(xml: &[u8], sheet_name: &str, index: usize) -> ExcliResult<Option<Vec<u8>>> {
    let mut reader = Reader::from_reader(xml);
    let mut in_sheets = false;
    let mut sheets = Vec::new();
    loop {
        let from = reader.buffer_position() as usize;
        let event = reader.read_event().map_err(xml_error)?;
        let to = reader.buffer_position() as usize;
        match event {
            Event::Start(event) if event.local_name().as_ref() == b"sheets" => in_sheets = true,
            Event::End(event) if event.local_name().as_ref() == b"sheets" => in_sheets = false,
            Event::Empty(event) if in_sheets && event.local_name().as_ref() == b"sheet" => {
                let mut name = None;
                for attribute in event.attributes() {
                    let attribute = attribute.map_err(xml_error)?;
                    if attribute.key.as_ref() == b"name" {
                        name = Some(attribute.unescape_value().map_err(xml_error)?.into_owned());
                    }
                }
                let name = name.ok_or_else(|| invalid("Sheet has no valid name"))?;
                if sheets
                    .iter()
                    .any(|(_, _, existing): &(usize, usize, String)| existing == &name)
                {
                    return Err(invalid("Duplicate sheet name in workbook XML"));
                }
                sheets.push((from, to, name));
            }
            Event::Start(event) if in_sheets && event.local_name().as_ref() == b"sheet" => {
                return Err(unsupported("Nonempty sheet elements are not yet movable"));
            }
            Event::Eof => break,
            _ => {}
        }
    }
    let old = sheets
        .iter()
        .position(|(_, _, name)| name == sheet_name)
        .ok_or_else(|| ExcliError::sheet_not_found(sheet_name))?;
    let destination = index.min(sheets.len() - 1);
    if old == destination {
        return Ok(None);
    }
    let mut order: Vec<usize> = (0..sheets.len()).collect();
    let moved = order.remove(old);
    order.insert(destination, moved);
    let mut mapping = vec![0usize; sheets.len()];
    for (new, &original) in order.iter().enumerate() {
        mapping[original] = new;
    }
    let mut edits: Vec<(usize, usize, Vec<u8>)> = sheets
        .iter()
        .enumerate()
        .map(|(position, &(from, to, _))| {
            let (source_from, source_to, _) = &sheets[order[position]];
            (from, to, xml[*source_from..*source_to].to_vec())
        })
        .collect();

    let mut reader = Reader::from_reader(xml);
    loop {
        let from = reader.buffer_position() as usize;
        let event = reader.read_event().map_err(xml_error)?;
        let to = reader.buffer_position() as usize;
        match event {
            Event::Start(event) | Event::Empty(event)
                if event.local_name().as_ref() == b"workbookView" =>
            {
                if let Some(tag) = reindex_tag(
                    xml,
                    from,
                    to,
                    &event,
                    &mapping,
                    &["activeTab", "firstSheet"],
                )? {
                    edits.push((from, to, tag));
                }
            }
            Event::Start(event) | Event::Empty(event)
                if event.local_name().as_ref() == b"definedName" =>
            {
                if let Some(tag) = reindex_tag(xml, from, to, &event, &mapping, &["localSheetId"])?
                {
                    edits.push((from, to, tag));
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    edits.sort_by_key(|(from, _, _)| *from);
    let mut result = Vec::with_capacity(xml.len());
    let mut cursor = 0;
    for (from, to, replacement) in edits {
        if from < cursor {
            return Err(invalid("Overlapping workbook edits"));
        }
        result.extend_from_slice(&xml[cursor..from]);
        result.extend_from_slice(&replacement);
        cursor = to;
    }
    result.extend_from_slice(&xml[cursor..]);
    Ok(Some(result))
}

/// Reorder a worksheet without rebuilding its cells or related package parts.
/// The destination index is zero-based and clamped to the last sheet.
pub fn move_sheet(path: &Path, sheet_name: &str, index: usize) -> ExcliResult<bool> {
    if !matches!(detect_format(path)?, FileFormat::Xlsx | FileFormat::Xlsm) {
        return Err(unsupported(
            "Package-preserving sheet move supports XLSX and XLSM",
        ));
    }
    let mut archive = ZipArchive::new(std::fs::File::open(path)?)
        .map_err(|error| invalid(format!("Invalid Excel ZIP: {error}")))?;
    ensure_editable(&mut archive)?;
    let package = package_index(&mut archive)?;
    let workbook_xml = read_part(&mut archive, &package.workbook_path)?;
    let Some(updated) = move_sheet_xml(&workbook_xml, sheet_name, index)? else {
        return Ok(false);
    };
    write_modified_parts(
        path,
        &mut archive,
        &HashMap::from([(package.workbook_path, updated)]),
    )?;
    Ok(true)
}

/// Change a sheet's visibility without rebuilding its workbook package.
/// Returns whether the file changed.
pub fn set_sheet_state(path: &Path, sheet_name: &str, state: SheetState) -> ExcliResult<bool> {
    if !matches!(detect_format(path)?, FileFormat::Xlsx | FileFormat::Xlsm) {
        return Err(unsupported(
            "Package-preserving sheet edits support XLSX and XLSM",
        ));
    }
    let mut archive = ZipArchive::new(std::fs::File::open(path)?)
        .map_err(|e| invalid(format!("Invalid Excel ZIP: {e}")))?;
    ensure_editable(&mut archive)?;
    let package = package_index(&mut archive)?;
    let workbook_xml = read_part(&mut archive, &package.workbook_path)?;
    let Some(updated) = sheet_state_xml(&workbook_xml, sheet_name, state)? else {
        return Ok(false);
    };
    let modified = HashMap::from([(package.workbook_path, updated)]);
    write_modified_parts(path, &mut archive, &modified)?;
    Ok(true)
}

/// Replace plain string cells in an existing XLSX/XLSM package while retaining
/// untouched ZIP members, styles, formulas, macros, and other workbook parts.
/// Returns the number of changed cells.
pub fn replace_text(path: &Path, find: &str, replace: &str, dry_run: bool) -> ExcliResult<usize> {
    if find.is_empty() {
        return Err(ExcliError::new(
            ErrorCode::GeneralError,
            "Find text cannot be empty",
        ));
    }
    if replace
        .chars()
        .any(|ch| !matches!(ch as u32, 0x9 | 0xA | 0xD | 0x20..=0xD7FF | 0xE000..=0xFFFD | 0x10000..=0x10FFFF))
    {
        return Err(ExcliError::new(
            ErrorCode::GeneralError,
            "Replacement contains a character that XML cannot represent",
        ));
    }
    if !matches!(detect_format(path)?, FileFormat::Xlsx | FileFormat::Xlsm) {
        return Err(unsupported(
            "Package-preserving replacement supports XLSX and XLSM",
        ));
    }

    let file = std::fs::File::open(path)?;
    let mut archive =
        ZipArchive::new(file).map_err(|e| invalid(format!("Invalid Excel ZIP: {e}")))?;
    ensure_editable(&mut archive)?;
    let package = package_index(&mut archive)?;
    let shared_strings = match &package.shared_strings_path {
        Some(path) => read_shared_strings(&read_part(&mut archive, path)?)?,
        None => Vec::new(),
    };

    let mut modified = HashMap::new();
    let mut total = 0;
    for sheet in &package.sheets {
        let xml = read_part(&mut archive, &sheet.path)?;
        let (patched, count) = replace_sheet(&xml, &shared_strings, find, replace)?;
        if count > 0 {
            modified.insert(sheet.path.clone(), patched);
            total += count;
        }
    }
    if dry_run || total == 0 {
        return Ok(total);
    }

    let workbook_xml = read_part(&mut archive, &package.workbook_path)?;
    modified.insert(package.workbook_path, recalculation_xml(&workbook_xml)?);

    write_modified_parts(path, &mut archive, &modified)?;
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;
    use excli_core::cell::Cell;
    use excli_core::Workbook;
    use std::io::Read;

    #[test]
    fn moving_a_sheet_updates_workbook_index_references() {
        let xml = br#"<workbook><bookViews><workbookView activeTab="1" firstSheet="0"/></bookViews><sheets><sheet name="A" sheetId="7" r:id="r1"/><sheet name="B" sheetId="9" r:id="r2"/><sheet name="C" sheetId="12" r:id="r3"/></sheets><definedNames><definedName name="Scoped" localSheetId="2">C!$A$1</definedName></definedNames></workbook>"#;
        let moved = move_sheet_xml(xml, "C", 0).unwrap().unwrap();
        let text = String::from_utf8(moved.clone()).unwrap();
        assert!(text.find("name=\"C\"").unwrap() < text.find("name=\"A\"").unwrap());
        assert!(text.find("name=\"A\"").unwrap() < text.find("name=\"B\"").unwrap());
        assert!(text.contains("activeTab=\"2\" firstSheet=\"1\""));
        assert!(text.contains("localSheetId=\"0\""));
        assert!(move_sheet_xml(&moved, "C", 0).unwrap().is_none());
    }

    #[test]
    fn moving_a_sheet_keeps_worksheet_parts_byte_identical() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("move.xlsx");
        let mut workbook = Workbook::new();
        for (name, value) in [("A", "alpha"), ("B", "beta"), ("C", "gamma")] {
            workbook.add_sheet(name).set_cell(0, 0, Cell::string(value));
        }
        crate::writer::write_xlsx(&path, &workbook).unwrap();
        let untouched = std::fs::read(&path).unwrap();
        assert!(!move_sheet(&path, "A", 0).unwrap());
        assert_eq!(std::fs::read(&path).unwrap(), untouched);
        let before: Vec<_> = (1..=3)
            .map(|index| {
                read_part(
                    &mut ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap(),
                    &format!("xl/worksheets/sheet{index}.xml"),
                )
                .unwrap()
            })
            .collect();
        assert!(move_sheet(&path, "C", 0).unwrap());
        let loaded = crate::reader::read_workbook(&path).unwrap();
        assert_eq!(
            loaded
                .sheets
                .iter()
                .map(|sheet| sheet.name.as_str())
                .collect::<Vec<_>>(),
            vec!["C", "A", "B"]
        );
        assert_eq!(
            loaded
                .sheet_by_name("C")
                .unwrap()
                .get_cell(0, 0)
                .unwrap()
                .value
                .to_string(),
            "gamma"
        );
        for (index, expected) in (1..=3).zip(before) {
            let after = read_part(
                &mut ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap(),
                &format!("xl/worksheets/sheet{index}.xml"),
            )
            .unwrap();
            assert_eq!(after, expected);
        }
    }

    #[test]
    fn replacement_refuses_cell_extensions_and_preserves_prefixes() {
        let cell = b"<c r=\"A1\" t=\"inlineStr\"><is><t>Hello</t></is><extLst/></c>";
        assert!(replacement_cell(cell, &[], "Hello", "World").is_err());
        let sheet = b"<x:worksheet xmlns:x=\"urn:sheet\"><x:sheetData><x:row r=\"1\"><x:c r=\"A1\" t=\"inlineStr\"><x:is><x:t>Hello</x:t></x:is></x:c></x:row></x:sheetData></x:worksheet>";
        let (updated, count) = replace_sheet(sheet, &[], "Hello", "World").unwrap();
        assert_eq!(count, 1);
        let updated = String::from_utf8(updated).unwrap();
        assert!(updated.contains("<x:c r=\"A1\" t=\"inlineStr\"><x:is><x:t xml:space=\"preserve\">World</x:t></x:is></x:c>"));
    }

    #[test]
    fn replacement_edits_prefixed_worksheet_package() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("prefixed.xlsx");
        fixture(&path);
        let mut archive = ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
        let prefixed = br#"<x:worksheet xmlns:x="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><x:sheetData><x:row r="1"><x:c r="A1" t="inlineStr"><x:is><x:t>hello</x:t></x:is></x:c></x:row></x:sheetData></x:worksheet>"#;
        write_modified_parts(
            &path,
            &mut archive,
            &HashMap::from([("xl/worksheets/sheet1.xml".to_owned(), prefixed.to_vec())]),
        )
        .unwrap();
        assert_eq!(replace_text(&path, "hello", "world", false).unwrap(), 1);
        let sheet = String::from_utf8(member(&path, "xl/worksheets/sheet1.xml")).unwrap();
        assert!(sheet.contains("<x:c r=\"A1\" t=\"inlineStr\"><x:is><x:t xml:space=\"preserve\">world</x:t></x:is></x:c>"));
        assert_eq!(
            crate::reader::read_workbook(&path).unwrap().sheets[0]
                .get_cell(0, 0)
                .unwrap()
                .value
                .to_string(),
            "world"
        );
    }

    fn fixture(path: &Path) {
        let file = std::fs::File::create(path).unwrap();
        let mut zip = ZipWriter::new(file);
        let options = SimpleFileOptions::default();
        let entries = [
            (
                "[Content_Types].xml",
                r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/sharedStrings.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml"/><Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/></Types>"#,
            ),
            (
                "_rels/.rels",
                r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#,
            ),
            (
                "xl/workbook.xml",
                r#"<?xml version="1.0"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Data" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings" Target="sharedStrings.xml"/><Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"#,
            ),
            (
                "xl/sharedStrings.xml",
                r#"<?xml version="1.0"?><sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" count="3" uniqueCount="2"><si><t>hello &amp; world</t></si><si><r><rPr><b/></rPr><t>Rich</t></r><r><rPr><i/></rPr><t> text</t></r></si></sst>"#,
            ),
            (
                "xl/styles.xml",
                r#"<?xml version="1.0"?><styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><fonts count="1"><font><b/></font></fonts><fills count="2"><fill><patternFill patternType="none"/></fill><fill><patternFill patternType="gray125"/></fill></fills><borders count="1"><border/></borders><cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs><cellXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/></cellXfs></styleSheet>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<?xml version="1.0"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="1" ht="24"><c r="A1" s="0" t="s"><v>0</v></c><c r="B1" t="s"><v>0</v></c><c r="C1"><f>1+2</f><v>3</v></c><c r="D1" t="inlineStr"><is><t>hello</t></is></c><c r="E1" t="s"><v>1</v></c></row></sheetData><mergeCells count="1"><mergeCell ref="A2:B2"/></mergeCells></worksheet>"#,
            ),
            ("customXml/item1.xml", "<custom>must stay intact</custom>"),
            ("xl/vbaProject.bin", "macro bytes"),
        ];
        for (name, content) in entries {
            zip.start_file(name, options).unwrap();
            zip.write_all(content.as_bytes()).unwrap();
        }
        zip.set_comment("original archive comment");
        zip.finish().unwrap();
    }

    fn member(path: &Path, name: &str) -> Vec<u8> {
        let mut archive = ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
        let mut bytes = Vec::new();
        archive
            .by_name(name)
            .unwrap()
            .read_to_end(&mut bytes)
            .unwrap();
        bytes
    }

    #[test]
    fn replacement_preserves_unrelated_workbook_parts() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("styled.xlsm");
        fixture(&path);
        let before = std::fs::read(&path).unwrap();
        assert_eq!(replace_text(&path, "hello", "goodbye", true).unwrap(), 3);
        assert_eq!(std::fs::read(&path).unwrap(), before);

        assert_eq!(replace_text(&path, "hello", "goodbye", false).unwrap(), 3);
        let sheet = String::from_utf8(member(&path, "xl/worksheets/sheet1.xml")).unwrap();
        assert!(sheet.contains("r=\"A1\" s=\"0\" t=\"inlineStr\""));
        assert!(sheet.contains("goodbye &amp; world"));
        assert!(sheet.contains("r=\"D1\" t=\"inlineStr\""));
        assert!(sheet.contains("<f>1+2</f><v>3</v>"));
        assert!(sheet.contains("<mergeCell ref=\"A2:B2\"/>"));
        let workbook = String::from_utf8(member(&path, "xl/workbook.xml")).unwrap();
        assert!(workbook.contains("fullCalcOnLoad=\"1\" forceFullCalc=\"1\""));
        assert_eq!(member(&path, "xl/vbaProject.bin"), b"macro bytes");
        assert_eq!(
            ZipArchive::new(std::fs::File::open(&path).unwrap())
                .unwrap()
                .comment(),
            b"original archive comment"
        );
        assert_eq!(
            member(&path, "customXml/item1.xml"),
            b"<custom>must stay intact</custom>"
        );
        assert!(String::from_utf8(member(&path, "xl/sharedStrings.xml"))
            .unwrap()
            .contains("hello &amp; world"));
    }

    #[test]
    fn shared_rich_replacement_preserves_runs_and_rejects_cross_run_match() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("styled.xlsx");
        fixture(&path);
        let mut archive = ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
        let sheet = String::from_utf8(read_part(&mut archive, "xl/worksheets/sheet1.xml").unwrap())
            .unwrap();
        let sheet = sheet.replace("</row>", "<c r=\"F1\" t=\"s\"><v>1</v></c></row>");
        write_modified_parts(
            &path,
            &mut archive,
            &HashMap::from([("xl/worksheets/sheet1.xml".to_owned(), sheet.into_bytes())]),
        )
        .unwrap();
        let before = std::fs::read(&path).unwrap();
        assert!(replace_text(&path, "Rich text", "Plain", false).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
        let shared_before = member(&path, "xl/sharedStrings.xml");
        assert_eq!(replace_text(&path, "Rich", "Plain", false).unwrap(), 2);
        let sheet = String::from_utf8(member(&path, "xl/worksheets/sheet1.xml")).unwrap();
        assert_eq!(sheet.matches("t=\"inlineStr\"><is xmlns=").count(), 2);
        assert!(sheet.contains("r=\"E1\" t=\"inlineStr\"><is xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><r><rPr><b/></rPr><t xml:space=\"preserve\">Plain</t></r>"));
        assert!(sheet.contains("<r><rPr><i/></rPr><t> text</t></r>"));
        assert_eq!(member(&path, "xl/sharedStrings.xml"), shared_before);
        let loaded = crate::reader::read_workbook(&path).unwrap();
        let excli_core::cell::CellValue::RichText(runs) =
            &loaded.sheets[0].get_cell(4, 0).unwrap().value
        else {
            panic!("shared rich text should remain structured");
        };
        assert_eq!(runs[0].text, "Plain");
        assert_eq!(runs[0].bold, Some(true));
        assert_eq!(runs[1].text, " text");
        assert_eq!(runs[1].italic, Some(true));

        let shared = read_shared_strings(&shared_before).unwrap();
        let prefixed = b"<x:c r=\"A1\" t=\"s\"><x:v>1</x:v></x:c>";
        let cloned = replacement_cell(prefixed, &shared, "Rich", "Fresh")
            .unwrap()
            .unwrap();
        let cloned = String::from_utf8(cloned).unwrap();
        assert!(cloned.starts_with("<x:c r=\"A1\" t=\"inlineStr\"><is xmlns="));
        assert!(cloned.contains("<t xml:space=\"preserve\">Fresh</t>"));
    }

    #[test]
    fn inline_rich_replacement_preserves_run_formatting_and_rejects_cross_run_match() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rich.xlsx");
        fixture(&path);
        let mut archive = ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
        let original =
            String::from_utf8(read_part(&mut archive, "xl/worksheets/sheet1.xml").unwrap())
                .unwrap();
        let old = "<c r=\"D1\" t=\"inlineStr\"><is><t>hello</t></is></c>";
        let rich = "<c r=\"D1\" t=\"inlineStr\"><is><r><rPr><b/><color rgb=\"FFFF0000\"/></rPr><t>Quick</t></r><r><rPr><i/></rPr><t> Brown</t></r></is></c>";
        assert!(original.contains(old));
        write_modified_parts(
            &path,
            &mut archive,
            &HashMap::from([(
                "xl/worksheets/sheet1.xml".to_owned(),
                original.replace(old, rich).into_bytes(),
            )]),
        )
        .unwrap();
        let before = std::fs::read(&path).unwrap();
        assert!(replace_text(&path, "Quick Brown", "Fox", false).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert_eq!(replace_text(&path, "Quick", "Fast", false).unwrap(), 1);
        let updated = String::from_utf8(member(&path, "xl/worksheets/sheet1.xml")).unwrap();
        assert!(updated.contains(
            "<rPr><b/><color rgb=\"FFFF0000\"/></rPr><t xml:space=\"preserve\">Fast</t>"
        ));
        assert!(updated.contains("<r><rPr><i/></rPr><t> Brown</t></r>"));
        let loaded = crate::reader::read_workbook(&path).unwrap();
        let excli_core::cell::CellValue::RichText(runs) =
            &loaded.sheets[0].get_cell(3, 0).unwrap().value
        else {
            panic!("rich text should remain structured");
        };
        assert_eq!(runs[0].text, "Fast");
        assert_eq!(runs[0].bold, Some(true));
        assert_eq!(runs[1].text, " Brown");
        assert_eq!(runs[1].italic, Some(true));
    }

    #[test]
    fn replaced_package_is_readable_and_no_match_is_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("styled.xlsx");
        fixture(&path);
        assert_eq!(replace_text(&path, "hello", "new", false).unwrap(), 3);
        let workbook = crate::reader::read_workbook(&path).unwrap();
        let sheet = workbook.sheet_by_name("Data").unwrap();
        assert_eq!(
            sheet.get_cell(0, 0).unwrap().value.to_string(),
            "new & world"
        );
        assert_eq!(
            sheet.get_cell(1, 0).unwrap().value.to_string(),
            "new & world"
        );
        assert_eq!(sheet.get_cell(3, 0).unwrap().value.to_string(), "new");

        let before = std::fs::read(&path).unwrap();
        assert_eq!(replace_text(&path, "missing", "text", false).unwrap(), 0);
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }

    #[test]
    fn existing_calculation_settings_are_retained() {
        let xml = br#"<workbook><calcPr calcId="42" calcMode="manual"/></workbook>"#;
        let changed = String::from_utf8(recalculation_xml(xml).unwrap()).unwrap();
        assert!(changed.contains("calcId=\"42\" calcMode=\"manual\""));
        assert!(changed.contains("fullCalcOnLoad=\"1\" forceFullCalc=\"1\""));

        let prefixed = br#"<x:workbook xmlns:x="urn:test"></x:workbook>"#;
        let changed = String::from_utf8(recalculation_xml(prefixed).unwrap()).unwrap();
        assert!(changed.contains("<x:calcPr fullCalcOnLoad=\"1\""));
    }

    #[test]
    fn signed_package_is_not_modified() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().join("base.xlsx");
        let signed = dir.path().join("signed.xlsx");
        fixture(&base);
        let mut archive = ZipArchive::new(std::fs::File::open(&base).unwrap()).unwrap();
        let mut writer = ZipWriter::new(std::fs::File::create(&signed).unwrap());
        for index in 0..archive.len() {
            writer
                .raw_copy_file(archive.by_index(index).unwrap())
                .unwrap();
        }
        writer
            .start_file("_xmlsignatures/sig1.xml", SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"signature").unwrap();
        writer.finish().unwrap();

        let before = std::fs::read(&signed).unwrap();
        assert!(replace_text(&signed, "hello", "new", false).is_err());
        assert_eq!(std::fs::read(&signed).unwrap(), before);
    }

    #[test]
    fn invalid_xml_text_does_not_modify_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("styled.xlsx");
        fixture(&path);
        let before = std::fs::read(&path).unwrap();
        assert!(replace_text(&path, "hello", "bad\0text", false).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }

    #[test]
    fn hiding_a_sheet_preserves_worksheets_and_is_readable() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("two_sheets.xlsx");
        let mut workbook = Workbook::new();
        workbook.add_sheet("Visible");
        workbook.add_sheet("Archive");
        crate::writer::write_xlsx(&path, &workbook).unwrap();
        let sheet_xml = member(&path, "xl/worksheets/sheet2.xml");

        assert!(set_sheet_state(&path, "Archive", SheetState::Hidden).unwrap());
        let read_back = crate::reader::read_workbook(&path).unwrap();
        assert_eq!(
            read_back.sheet_by_name("Archive").unwrap().state,
            SheetState::Hidden
        );
        assert_eq!(member(&path, "xl/worksheets/sheet2.xml"), sheet_xml);
        let hidden_bytes = std::fs::read(&path).unwrap();
        assert!(!set_sheet_state(&path, "Archive", SheetState::Hidden).unwrap());
        assert_eq!(std::fs::read(&path).unwrap(), hidden_bytes);

        assert!(set_sheet_state(&path, "Archive", SheetState::Visible).unwrap());
        assert_eq!(
            crate::reader::read_workbook(&path)
                .unwrap()
                .sheet_by_name("Archive")
                .unwrap()
                .state,
            SheetState::Visible
        );
    }

    #[test]
    fn hiding_the_last_visible_sheet_fails_without_modifying_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("one_sheet.xlsx");
        let mut workbook = Workbook::new();
        workbook.add_sheet("Only");
        crate::writer::write_xlsx(&path, &workbook).unwrap();
        let before = std::fs::read(&path).unwrap();
        assert!(set_sheet_state(&path, "Only", SheetState::Hidden).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }
}
