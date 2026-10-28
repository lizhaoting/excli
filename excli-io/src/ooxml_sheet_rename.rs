//! Package-preserving worksheet rename with a conservative reference guard.

use std::collections::HashMap;
use std::io::Read;
use std::path::Path;

use excli_core::error::{ErrorCode, ExcliError, ExcliResult};
use excli_core::types::FileFormat;
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use zip::ZipArchive;

use crate::format_detect::detect_format;
use crate::ooxml_edit::{ensure_editable, write_modified_parts, xml_escape};
use crate::ooxml_package::{index as package_index, read_part};
use crate::ooxml_sheet_add::validate_name;

fn corrupted(message: impl Into<String>) -> ExcliError {
    ExcliError::new(ErrorCode::FileCorrupted, message)
}

fn unsupported(message: impl Into<String>) -> ExcliError {
    ExcliError::new(ErrorCode::NotImplemented, message)
}

fn attribute(tag: &BytesStart<'_>, key: &[u8]) -> ExcliResult<Option<String>> {
    for item in tag.attributes() {
        let item =
            item.map_err(|error| corrupted(format!("Invalid worksheet attribute: {error}")))?;
        if item.key.as_ref() == key {
            return Ok(Some(
                item.unescape_value()
                    .map_err(|error| corrupted(format!("Invalid worksheet value: {error}")))?
                    .into_owned(),
            ));
        }
    }
    Ok(None)
}

fn workbook_rename(xml: &[u8], old: &str, new: &str) -> ExcliResult<Option<Vec<u8>>> {
    let mut reader = Reader::from_reader(xml);
    let mut target = None;
    let mut names = Vec::new();
    loop {
        let from = reader.buffer_position() as usize;
        match reader
            .read_event()
            .map_err(|error| corrupted(format!("Invalid workbook XML: {error}")))?
        {
            Event::Start(ref tag) | Event::Empty(ref tag)
                if tag.local_name().as_ref() == b"sheet" =>
            {
                let to = reader.buffer_position() as usize;
                let name =
                    attribute(tag, b"name")?.ok_or_else(|| corrupted("Sheet has no name"))?;
                if name == old {
                    if target.is_some() {
                        return Err(corrupted("Duplicate sheet name"));
                    }
                    target = Some((from, to, tag.to_owned(), xml[from..to].ends_with(b"/>")));
                }
                names.push(name);
            }
            Event::Eof => break,
            _ => {}
        }
    }
    let (from, to, tag, empty) = target.ok_or_else(|| ExcliError::sheet_not_found(old))?;
    if names
        .iter()
        .any(|name| name != old && name.to_lowercase() == new.to_lowercase())
    {
        return Err(ExcliError::new(
            ErrorCode::ValidationError,
            "Worksheet name already exists",
        ));
    }
    if old == new {
        return Ok(None);
    }
    let qualified = std::str::from_utf8(tag.name().as_ref())
        .map_err(|error| corrupted(format!("Invalid sheet tag: {error}")))?
        .to_owned();
    let mut replacement = format!("<{qualified}");
    for item in tag.attributes() {
        let item = item.map_err(|error| corrupted(format!("Invalid sheet attribute: {error}")))?;
        let key = std::str::from_utf8(item.key.as_ref())
            .map_err(|error| corrupted(format!("Invalid sheet attribute: {error}")))?;
        let value = if key == "name" {
            new.to_owned()
        } else {
            item.unescape_value()
                .map_err(|error| corrupted(format!("Invalid sheet attribute: {error}")))?
                .into_owned()
        };
        replacement.push_str(&format!(" {key}=\"{}\"", xml_escape(&value)));
    }
    replacement.push_str(if empty { "/>" } else { ">" });
    let mut result = Vec::with_capacity(xml.len() + replacement.len());
    result.extend_from_slice(&xml[..from]);
    result.extend_from_slice(replacement.as_bytes());
    result.extend_from_slice(&xml[to..]);
    Ok(Some(result))
}

fn rename_app_title(xml: &[u8], old: &str, new: &str) -> ExcliResult<Vec<u8>> {
    let mut reader = Reader::from_reader(xml);
    let mut in_titles = false;
    let mut in_title = false;
    let mut titles_present = false;
    let mut edits = Vec::new();
    loop {
        let from = reader.buffer_position() as usize;
        match reader
            .read_event()
            .map_err(|error| corrupted(format!("Invalid extended properties XML: {error}")))?
        {
            Event::Start(ref tag) if tag.local_name().as_ref() == b"TitlesOfParts" => {
                in_titles = true;
                titles_present = true;
            }
            Event::End(ref tag) if tag.local_name().as_ref() == b"TitlesOfParts" => {
                in_titles = false
            }
            Event::Start(ref tag) if in_titles && tag.local_name().as_ref() == b"lpstr" => {
                in_title = true
            }
            Event::End(ref tag) if tag.local_name().as_ref() == b"lpstr" => in_title = false,
            Event::Text(ref text) if in_title => {
                let to = reader.buffer_position() as usize;
                if text
                    .unescape()
                    .map_err(|error| corrupted(format!("Invalid title: {error}")))?
                    .as_ref()
                    == old
                {
                    edits.push((from, to));
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if !titles_present {
        return Ok(xml.to_vec());
    }
    if edits.len() != 1 {
        return Err(unsupported(
            "Extended properties do not identify exactly one worksheet title",
        ));
    }
    let (from, to) = edits[0];
    let mut result = Vec::with_capacity(xml.len() + new.len());
    result.extend_from_slice(&xml[..from]);
    result.extend_from_slice(xml_escape(new).as_bytes());
    result.extend_from_slice(&xml[to..]);
    Ok(result)
}

fn has_ref_attributes(tag: &BytesStart<'_>, old: &str, quoted: &str) -> ExcliResult<bool> {
    for item in tag.attributes() {
        let item =
            item.map_err(|error| corrupted(format!("Invalid package attribute: {error}")))?;
        if item.key.as_ref().starts_with(b"xmlns")
            || matches!(item.key.as_ref(), b"Type" | b"Target" | b"ContentType")
        {
            continue;
        }
        let value = item
            .unescape_value()
            .map_err(|error| corrupted(format!("Invalid package attribute: {error}")))?
            .to_lowercase();
        if value == old || has_reference_syntax(&value, old, quoted) {
            return Ok(true);
        }
    }
    Ok(false)
}

fn has_reference_syntax(value: &str, old: &str, quoted: &str) -> bool {
    value.contains(&format!("{old}!"))
        || value.contains(&format!("'{quoted}'!"))
        || value.contains(&format!("{old}:"))
        || value.contains(&format!(":{quoted}'!"))
}

fn without_string_literals(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    let mut chars = value.chars().peekable();
    let mut in_string = false;
    while let Some(ch) = chars.next() {
        if ch == '"' {
            if in_string && chars.peek() == Some(&'"') {
                chars.next();
            } else {
                in_string = !in_string;
            }
        } else if !in_string {
            result.push(ch);
        }
    }
    result
}

fn formula_tag(local: &[u8]) -> bool {
    matches!(
        local,
        b"f" | b"definedName"
            | b"formula"
            | b"formula1"
            | b"formula2"
            | b"calculatedColumnFormula"
            | b"totalsRowFormula"
    )
}

fn word_char(ch: char) -> bool {
    ch.is_alphanumeric() || matches!(ch, '_' | '.')
}

/// Rewrite qualified sheet references while leaving Excel string literals intact.
fn rewrite_formula(value: &str, old: &str, new: &str) -> Option<String> {
    let replacement = format!("'{}'!", new.replace('\'', "''"));
    let mut result = String::with_capacity(value.len() + replacement.len());
    let mut index = 0usize;
    let mut changed = false;
    while index < value.len() {
        let ch = value[index..].chars().next()?;
        if ch == '"' {
            let start = index;
            index += 1;
            while index < value.len() {
                let next = value[index..].chars().next()?;
                index += next.len_utf8();
                if next == '"' {
                    if value[index..].starts_with('"') {
                        index += 1;
                    } else {
                        break;
                    }
                }
            }
            result.push_str(&value[start..index]);
        } else if ch == '\'' {
            let start = index;
            index += 1;
            let mut name = String::new();
            let mut closed = false;
            while index < value.len() {
                let next = value[index..].chars().next()?;
                index += next.len_utf8();
                if next == '\'' {
                    if value[index..].starts_with('\'') {
                        name.push('\'');
                        index += 1;
                    } else {
                        closed = true;
                        break;
                    }
                } else {
                    name.push(next);
                }
            }
            if closed
                && value[index..].starts_with('!')
                && name.to_lowercase() == old.to_lowercase()
                && !name.contains('[')
                && !matches!(value[..start].chars().next_back(), Some(':'))
            {
                result.push_str(&replacement);
                index += 1;
                changed = true;
            } else {
                result.push_str(&value[start..index]);
            }
        } else if word_char(ch) {
            let start = index;
            index += ch.len_utf8();
            while index < value.len() {
                let next = value[index..].chars().next()?;
                if !word_char(next) {
                    break;
                }
                index += next.len_utf8();
            }
            let token = &value[start..index];
            let preceding = value[..start].chars().next_back();
            if value[index..].starts_with('!')
                && token.to_lowercase() == old.to_lowercase()
                && !matches!(preceding, Some(']' | ':'))
            {
                result.push_str(&replacement);
                index += 1;
                changed = true;
            } else {
                result.push_str(token);
            }
        } else {
            result.push(ch);
            index += ch.len_utf8();
        }
    }
    changed.then_some(result)
}

fn rewrite_tag(
    tag: &BytesStart<'_>,
    empty: bool,
    old: &str,
    new: &str,
) -> ExcliResult<Option<Vec<u8>>> {
    let qualified = std::str::from_utf8(tag.name().as_ref())
        .map_err(|error| corrupted(format!("Invalid XML tag: {error}")))?
        .to_owned();
    let mut attributes = Vec::new();
    let mut changed = false;
    for item in tag.attributes() {
        let item = item.map_err(|error| corrupted(format!("Invalid XML attribute: {error}")))?;
        let key = std::str::from_utf8(item.key.as_ref())
            .map_err(|error| corrupted(format!("Invalid XML attribute name: {error}")))?
            .to_owned();
        let value = item
            .unescape_value()
            .map_err(|error| corrupted(format!("Invalid XML attribute value: {error}")))?
            .into_owned();
        let replacement = if matches!(item.key.local_name().as_ref(), b"sheet" | b"sheetName")
            && value.to_lowercase() == old.to_lowercase()
        {
            Some(new.to_owned())
        } else if item.key.local_name().as_ref() == b"location" {
            rewrite_formula(&value, old, new)
        } else {
            None
        };
        if let Some(replacement) = replacement {
            changed = true;
            attributes.push((key, replacement));
        } else {
            attributes.push((key, value));
        }
    }
    if !changed {
        return Ok(None);
    }
    let mut output = format!("<{qualified}");
    for (key, value) in attributes {
        output.push_str(&format!(" {key}=\"{}\"", xml_escape(&value)));
    }
    output.push_str(if empty { "/>" } else { ">" });
    Ok(Some(output.into_bytes()))
}

pub(crate) fn rewrite_xml_references(
    xml: &[u8],
    old: &str,
    new: &str,
) -> ExcliResult<Option<Vec<u8>>> {
    let mut reader = Reader::from_reader(xml);
    let mut formula_depth = 0usize;
    let mut edits = Vec::new();
    loop {
        let from = reader.buffer_position() as usize;
        match reader
            .read_event()
            .map_err(|error| corrupted(format!("Invalid package XML: {error}")))?
        {
            Event::Start(ref tag) => {
                let to = reader.buffer_position() as usize;
                if let Some(replacement) = rewrite_tag(tag, false, old, new)? {
                    edits.push((from, to, replacement));
                }
                if formula_tag(tag.local_name().as_ref()) {
                    formula_depth += 1;
                }
            }
            Event::Empty(ref tag) => {
                let to = reader.buffer_position() as usize;
                if let Some(replacement) = rewrite_tag(tag, true, old, new)? {
                    edits.push((from, to, replacement));
                }
            }
            Event::Text(ref text) if formula_depth > 0 => {
                let value = text
                    .unescape()
                    .map_err(|error| corrupted(format!("Invalid formula XML: {error}")))?;
                if let Some(rewritten) = rewrite_formula(&value, old, new) {
                    edits.push((
                        from,
                        reader.buffer_position() as usize,
                        xml_escape(&rewritten).into_bytes(),
                    ));
                }
            }
            Event::CData(ref text) if formula_depth > 0 => {
                let value = std::str::from_utf8(text.as_ref())
                    .map_err(|error| corrupted(format!("Invalid formula XML: {error}")))?;
                if let Some(rewritten) = rewrite_formula(value, old, new) {
                    edits.push((
                        from,
                        reader.buffer_position() as usize,
                        format!("<![CDATA[{rewritten}]]>").into_bytes(),
                    ));
                }
            }
            Event::End(ref tag) if formula_tag(tag.local_name().as_ref()) => {
                formula_depth = formula_depth.saturating_sub(1)
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if edits.is_empty() {
        return Ok(None);
    }
    let mut output = Vec::with_capacity(xml.len() + edits.len() * new.len());
    let mut cursor = 0;
    for (from, to, replacement) in edits {
        if from < cursor {
            return Err(corrupted("Overlapping formula edits"));
        }
        output.extend_from_slice(&xml[cursor..from]);
        output.extend_from_slice(&replacement);
        cursor = to;
    }
    output.extend_from_slice(&xml[cursor..]);
    Ok(Some(output))
}

pub(crate) fn has_old_reference(xml: &[u8], old: &str) -> ExcliResult<bool> {
    let mut reader = Reader::from_reader(xml);
    let old_folded = old.to_lowercase();
    let escaped_apostrophes = old.replace('\'', "''").to_lowercase();
    let mut formula_depth = 0usize;
    let mut value_depth = 0usize;
    loop {
        match reader
            .read_event()
            .map_err(|error| corrupted(format!("Invalid package XML: {error}")))?
        {
            Event::Start(ref tag) => {
                if has_ref_attributes(tag, &old_folded, &escaped_apostrophes)? {
                    return Ok(true);
                }
                if formula_tag(tag.local_name().as_ref()) {
                    formula_depth += 1;
                }
                if matches!(tag.local_name().as_ref(), b"t" | b"v") {
                    value_depth += 1;
                }
            }
            Event::Empty(ref tag)
                if has_ref_attributes(tag, &old_folded, &escaped_apostrophes)? =>
            {
                return Ok(true)
            }
            Event::Text(ref text) => {
                if value_depth > 0 && formula_depth == 0 {
                    continue;
                }
                let value = text
                    .unescape()
                    .map_err(|error| corrupted(format!("Invalid package text: {error}")))?
                    .to_lowercase();
                let candidate = if formula_depth > 0 {
                    without_string_literals(&value)
                } else {
                    value
                };
                if has_reference_syntax(&candidate, &old_folded, &escaped_apostrophes) {
                    return Ok(true);
                }
            }
            Event::CData(ref text) => {
                if value_depth > 0 && formula_depth == 0 {
                    continue;
                }
                let value = String::from_utf8_lossy(text.as_ref()).to_lowercase();
                let candidate = if formula_depth > 0 {
                    without_string_literals(&value)
                } else {
                    value
                };
                if has_reference_syntax(&candidate, &old_folded, &escaped_apostrophes) {
                    return Ok(true);
                }
            }
            Event::End(ref tag) if formula_tag(tag.local_name().as_ref()) => {
                formula_depth = formula_depth.saturating_sub(1)
            }
            Event::End(ref tag) if matches!(tag.local_name().as_ref(), b"t" | b"v") => {
                value_depth = value_depth.saturating_sub(1)
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(false)
}

/// Rename a sheet while preserving unrelated OOXML parts. Common formula and
/// local sheet references are updated; unresolved references are refused.
pub fn rename_sheet(path: &Path, old: &str, new: &str) -> ExcliResult<bool> {
    validate_name(new)?;
    if !matches!(detect_format(path)?, FileFormat::Xlsx | FileFormat::Xlsm) {
        return Err(unsupported(
            "Package-preserving sheet rename supports XLSX and XLSM",
        ));
    }
    let mut archive = ZipArchive::new(std::fs::File::open(path)?)
        .map_err(|error| corrupted(format!("Invalid Excel ZIP: {error}")))?;
    ensure_editable(&mut archive)?;
    let package = package_index(&mut archive)?;
    let original = read_part(&mut archive, &package.workbook_path)?;
    let Some(workbook_xml) = workbook_rename(&original, old, new)? else {
        return Ok(false);
    };
    let mut modified = HashMap::from([(package.workbook_path.clone(), workbook_xml)]);
    if archive.file_names().any(|name| name == "docProps/app.xml") {
        let app = read_part(&mut archive, "docProps/app.xml")?;
        modified.insert(
            "docProps/app.xml".to_owned(),
            rename_app_title(&app, old, new)?,
        );
    }
    if old.to_lowercase() != new.to_lowercase() {
        for index in 0..archive.len() {
            let mut entry = archive
                .by_index(index)
                .map_err(|error| corrupted(format!("Invalid ZIP entry: {error}")))?;
            let name = entry.name().to_owned();
            if !name.ends_with(".xml") || name == "docProps/app.xml" {
                continue;
            }
            let mut xml = Vec::new();
            entry.read_to_end(&mut xml)?;
            drop(entry);
            let candidate = modified.get(&name).map_or(xml.as_slice(), Vec::as_slice);
            if !name.contains("/externalLinks/") && !name.starts_with("externalLinks/") {
                if let Some(rewritten) = rewrite_xml_references(candidate, old, new)? {
                    modified.insert(name.clone(), rewritten);
                }
            }
            let candidate = modified.get(&name).map_or(xml.as_slice(), Vec::as_slice);
            if has_old_reference(candidate, old)? {
                return Err(unsupported(format!(
                    "Sheet rename requires updating references in {name}"
                )));
            }
        }
    }
    write_modified_parts(path, &mut archive, &modified)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use excli_core::cell::Cell;
    use excli_core::Workbook;

    #[test]
    fn simple_rename_preserves_sheet_parts() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("book.xlsx");
        let mut workbook = Workbook::new();
        workbook
            .add_sheet("Data")
            .set_cell(0, 0, Cell::string("keep"));
        workbook.add_sheet("Other");
        crate::writer::write_xlsx(&path, &workbook).unwrap();
        let mut archive = ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
        let before = read_part(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        drop(archive);
        assert!(rename_sheet(&path, "Data", "Sales").unwrap());
        let mut archive = ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
        assert_eq!(
            read_part(&mut archive, "xl/worksheets/sheet1.xml").unwrap(),
            before
        );
        let loaded = crate::reader::read_workbook(&path).unwrap();
        assert_eq!(loaded.sheets[0].name, "Sales");
        assert_eq!(
            loaded.sheets[0].get_cell(0, 0).unwrap().value.to_string(),
            "keep"
        );
    }

    #[test]
    fn referenced_sheet_formula_is_updated() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("book.xlsx");
        let mut workbook = Workbook::new();
        workbook.add_sheet("Data").set_cell(0, 0, Cell::number(4.0));
        workbook.add_sheet("Other").set_cell(
            0,
            0,
            Cell::number(4.0).with_formula(excli_core::formula::Formula::new("=Data!A1")),
        );
        crate::writer::write_xlsx(&path, &workbook).unwrap();
        assert!(rename_sheet(&path, "Data", "Sales").unwrap());
        let loaded = crate::reader::read_workbook(&path).unwrap();
        assert_eq!(loaded.sheets[0].name, "Sales");
        assert_eq!(
            loaded.sheets[1]
                .get_cell(0, 0)
                .unwrap()
                .formula
                .as_ref()
                .unwrap()
                .raw,
            "='Sales'!A1"
        );
    }

    #[test]
    fn unrelated_cell_formula_does_not_block_short_sheet_name() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("book.xlsx");
        let mut workbook = Workbook::new();
        workbook.add_sheet("A");
        workbook.add_sheet("Other").set_cell(
            0,
            0,
            Cell::number(1.0).with_formula(excli_core::formula::Formula::new("=B1+1")),
        );
        crate::writer::write_xlsx(&path, &workbook).unwrap();
        assert!(rename_sheet(&path, "A", "X").unwrap());
        let loaded = crate::reader::read_workbook(&path).unwrap();
        assert_eq!(loaded.sheets[0].name, "X");
        assert_eq!(
            loaded.sheets[1]
                .get_cell(0, 0)
                .unwrap()
                .formula
                .as_ref()
                .unwrap()
                .raw,
            "=B1+1"
        );
    }

    #[test]
    fn unsupported_three_dimensional_reference_is_atomic() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("book.xlsx");
        let mut workbook = Workbook::new();
        workbook.add_sheet("Data");
        workbook.add_sheet("Other").set_cell(
            0,
            0,
            Cell::number(1.0)
                .with_formula(excli_core::formula::Formula::new("=SUM(Data:Other!A1)")),
        );
        crate::writer::write_xlsx(&path, &workbook).unwrap();
        let before = std::fs::read(&path).unwrap();
        assert!(rename_sheet(&path, "Data", "Sales").is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }

    #[test]
    fn updates_app_title() {
        let xml = br#"<Properties><TitlesOfParts><vt:vector><vt:lpstr>Data</vt:lpstr><vt:lpstr>Other</vt:lpstr></vt:vector></TitlesOfParts></Properties>"#;
        let updated =
            String::from_utf8(rename_app_title(xml, "Data", "Sales & More").unwrap()).unwrap();
        assert!(updated.contains("<vt:lpstr>Sales &amp; More</vt:lpstr>"));
        assert!(updated.contains("<vt:lpstr>Other</vt:lpstr>"));
    }

    #[test]
    fn formula_rewrite_handles_quoting_and_string_literals() {
        assert_eq!(
            rewrite_formula("Data!A1+'Data'!A2", "Data", "New Name"),
            Some("'New Name'!A1+'New Name'!A2".to_owned())
        );
        assert_eq!(
            rewrite_formula("'O''Brien'!A1", "O'Brien", "Sales'24"),
            Some("'Sales''24'!A1".to_owned())
        );
        assert_eq!(
            rewrite_formula("\"Data!A1\"&Data!A1", "Data", "Sales"),
            Some("\"Data!A1\"&'Sales'!A1".to_owned())
        );
        assert_eq!(
            rewrite_formula("[Other.xlsx]Data!A1", "Data", "Sales"),
            None
        );
        assert_eq!(rewrite_formula("Data:Other!A1", "Data", "Sales"), None);
    }

    #[test]
    fn formula_string_literal_survives_full_rename() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("book.xlsx");
        let mut workbook = Workbook::new();
        workbook.add_sheet("Data").set_cell(0, 0, Cell::number(2.0));
        workbook.add_sheet("Other").set_cell(
            0,
            0,
            Cell::string("Data!A1 2")
                .with_formula(excli_core::formula::Formula::new("=\"Data!A1\"&Data!A1")),
        );
        crate::writer::write_xlsx(&path, &workbook).unwrap();
        assert!(rename_sheet(&path, "Data", "Sales").unwrap());
        let loaded = crate::reader::read_workbook(&path).unwrap();
        assert_eq!(
            loaded.sheets[1]
                .get_cell(0, 0)
                .unwrap()
                .formula
                .as_ref()
                .unwrap()
                .raw,
            "=\"Data!A1\"&'Sales'!A1"
        );
    }

    #[test]
    fn rewrites_defined_names_chart_formulas_and_local_sheet_attributes() {
        let xml = br#"<root><definedName>'Data'!$A$1</definedName><c:f>SUM(Data!A1)</c:f><worksheetSource sheet="Data" ref="A1:B2"/><hyperlink location="'Data'!A1"/></root>"#;
        let updated = String::from_utf8(
            rewrite_xml_references(xml, "Data", "Sales & More")
                .unwrap()
                .unwrap(),
        )
        .unwrap();
        assert!(updated.contains("<definedName>&apos;Sales &amp; More&apos;!$A$1</definedName>"));
        assert!(updated.contains("<c:f>SUM(&apos;Sales &amp; More&apos;!A1)</c:f>"));
        assert!(updated.contains("sheet=\"Sales &amp; More\""));
        assert!(updated.contains("location=\"&apos;Sales &amp; More&apos;!A1\""));
        assert!(!has_old_reference(updated.as_bytes(), "Data").unwrap());
    }

    #[test]
    fn invalid_or_duplicate_name_does_not_modify_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("book.xlsx");
        let mut workbook = Workbook::new();
        workbook.add_sheet("Data");
        workbook.add_sheet("Other");
        crate::writer::write_xlsx(&path, &workbook).unwrap();
        let before = std::fs::read(&path).unwrap();
        for new in ["other", "bad/name", ""] {
            assert!(rename_sheet(&path, "Data", new).is_err());
            assert_eq!(std::fs::read(&path).unwrap(), before);
        }
    }
}
