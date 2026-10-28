//! Read worksheet metadata that calamine's value ranges do not expose.

use std::collections::HashMap;
use std::path::Path;

use excli_core::error::{ErrorCode, ExcliError, ExcliResult};
use excli_core::reference::{CellReference, RangeReference};
use excli_core::sheet::{
    AutoFilter, Column, Hyperlink, MergedRegion, PageOrientation, PaneState, Row, Sheet,
    SheetProtection,
};
use excli_core::types::DateSystem;
use excli_core::workbook::{NameScope, NamedRange};
use excli_core::Workbook;
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use zip::ZipArchive;

use crate::ooxml_package::{index, read_part, rels_path};

fn malformed(message: impl Into<String>) -> ExcliError {
    ExcliError::new(ErrorCode::FileCorrupted, message)
}

fn attribute(event: &BytesStart<'_>, key: &[u8]) -> ExcliResult<Option<String>> {
    for item in event.attributes() {
        let item = item.map_err(|e| malformed(format!("Invalid worksheet attribute: {e}")))?;
        if item.key.as_ref() == key {
            return Ok(Some(
                item.unescape_value()
                    .map_err(|e| malformed(format!("Invalid worksheet value: {e}")))?
                    .into_owned(),
            ));
        }
    }
    Ok(None)
}

fn true_attribute(event: &BytesStart<'_>, key: &[u8]) -> ExcliResult<bool> {
    Ok(matches!(
        attribute(event, key)?.as_deref(),
        Some("1" | "true")
    ))
}

fn protection_flag(event: &BytesStart<'_>, key: &[u8], default: bool) -> ExcliResult<bool> {
    match attribute(event, key)?.as_deref() {
        None => Ok(default),
        Some("1" | "true") => Ok(true),
        Some("0" | "false") => Ok(false),
        Some(value) => Err(malformed(format!("Invalid sheet protection flag: {value}"))),
    }
}

fn number_attribute<T: std::str::FromStr>(
    event: &BytesStart<'_>,
    key: &[u8],
) -> ExcliResult<Option<T>> {
    attribute(event, key)?
        .map(|value| {
            value
                .parse()
                .map_err(|_| malformed(format!("Invalid numeric worksheet attribute: {value}")))
        })
        .transpose()
}

fn relationship_id(event: &BytesStart<'_>) -> ExcliResult<Option<String>> {
    for item in event.attributes() {
        let item = item.map_err(|e| malformed(format!("Invalid hyperlink attribute: {e}")))?;
        if item.key.as_ref().ends_with(b":id") {
            return Ok(Some(
                item.unescape_value()
                    .map_err(|e| malformed(format!("Invalid hyperlink relationship: {e}")))?
                    .into_owned(),
            ));
        }
    }
    Ok(None)
}

fn hyperlink_targets(xml: &[u8]) -> ExcliResult<HashMap<String, String>> {
    let mut reader = Reader::from_reader(xml);
    let mut targets = HashMap::new();
    loop {
        match reader
            .read_event()
            .map_err(|e| malformed(format!("Invalid sheet relationships: {e}")))?
        {
            Event::Start(event) | Event::Empty(event)
                if event.local_name().as_ref() == b"Relationship" =>
            {
                let id = attribute(&event, b"Id")?;
                let kind = attribute(&event, b"Type")?;
                let target = attribute(&event, b"Target")?;
                if kind
                    .as_deref()
                    .is_some_and(|kind| kind.ends_with("/hyperlink"))
                {
                    if let (Some(id), Some(target)) = (id, target) {
                        targets.insert(id, target);
                    }
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(targets)
}

fn workbook_date_system(xml: &[u8]) -> ExcliResult<DateSystem> {
    let mut reader = Reader::from_reader(xml);
    loop {
        match reader
            .read_event()
            .map_err(|error| malformed(format!("Invalid workbook XML: {error}")))?
        {
            Event::Start(event) | Event::Empty(event)
                if event.local_name().as_ref() == b"workbookPr" =>
            {
                return match attribute(&event, b"date1904")?.as_deref() {
                    Some("1" | "true") => Ok(DateSystem::Date1904),
                    None | Some("0" | "false") => Ok(DateSystem::Date1900),
                    Some(value) => Err(malformed(format!("Invalid date1904 value: {value}"))),
                };
            }
            Event::Eof => return Ok(DateSystem::Date1900),
            _ => {}
        }
    }
}

fn print_title_parts(formula: &str) -> (Option<String>, Option<String>) {
    let mut rows = None;
    let mut columns = None;
    let mut start = 0;
    let mut quoted = false;
    let mut chars = formula.char_indices().peekable();
    while let Some((index, ch)) = chars.next() {
        if ch == '\'' {
            if quoted && chars.peek().is_some_and(|(_, next)| *next == '\'') {
                chars.next();
            } else {
                quoted = !quoted;
            }
        }
        if ch == ',' && !quoted {
            assign_print_title(&formula[start..index], &mut rows, &mut columns);
            start = index + 1;
        }
    }
    assign_print_title(&formula[start..], &mut rows, &mut columns);
    (rows, columns)
}

fn assign_print_title(part: &str, rows: &mut Option<String>, columns: &mut Option<String>) {
    let Some((_, reference)) = part.trim().rsplit_once('!') else {
        return;
    };
    let reference = reference.trim();
    let Some((first, last)) = reference.split_once(':') else {
        return;
    };
    let first = first.trim_start_matches('$');
    let last = last.trim_start_matches('$');
    if first.is_empty() || last.is_empty() {
        return;
    }
    if first.bytes().all(|byte| byte.is_ascii_digit())
        && last.bytes().all(|byte| byte.is_ascii_digit())
    {
        *rows = Some(reference.to_owned());
    } else if first.bytes().all(|byte| byte.is_ascii_alphabetic())
        && last.bytes().all(|byte| byte.is_ascii_alphabetic())
    {
        *columns = Some(reference.to_owned());
    }
}

fn enrich_defined_names(xml: &[u8], workbook: &mut Workbook) -> ExcliResult<()> {
    struct PendingName {
        name: String,
        local_index: Option<usize>,
        hidden: bool,
        comment: Option<String>,
        refers_to: String,
    }

    let mut reader = Reader::from_reader(xml);
    let mut in_sheets = false;
    let mut sheet_names = Vec::new();
    let mut pending: Option<PendingName> = None;
    let mut definitions = Vec::<PendingName>::new();
    loop {
        match reader
            .read_event()
            .map_err(|error| malformed(format!("Invalid workbook XML: {error}")))?
        {
            Event::Start(ref tag) if tag.local_name().as_ref() == b"sheets" => {
                in_sheets = true;
            }
            Event::End(ref tag) if tag.local_name().as_ref() == b"sheets" => {
                in_sheets = false;
            }
            Event::Start(ref tag) | Event::Empty(ref tag)
                if in_sheets && tag.local_name().as_ref() == b"sheet" =>
            {
                sheet_names.push(
                    attribute(tag, b"name")?
                        .ok_or_else(|| malformed("Workbook sheet has no name"))?,
                );
            }
            Event::Start(ref tag) if tag.local_name().as_ref() == b"definedName" => {
                let local_index = attribute(tag, b"localSheetId")?
                    .map(|value| {
                        value
                            .parse()
                            .map_err(|_| malformed("Invalid defined name sheet index"))
                    })
                    .transpose()?;
                let hidden = match attribute(tag, b"hidden")?.as_deref() {
                    None | Some("0" | "false") => false,
                    Some("1" | "true") => true,
                    Some(value) => {
                        return Err(malformed(format!(
                            "Invalid defined name hidden flag: {value}"
                        )))
                    }
                };
                pending = Some(PendingName {
                    name: attribute(tag, b"name")?
                        .ok_or_else(|| malformed("Defined name has no name"))?,
                    local_index,
                    hidden,
                    comment: attribute(tag, b"comment")?,
                    refers_to: String::new(),
                });
            }
            Event::Text(ref content) if pending.is_some() => {
                if let Some(ref mut active) = pending {
                    active.refers_to.push_str(
                        &content
                            .unescape()
                            .map_err(|error| malformed(format!("Invalid defined name: {error}")))?,
                    );
                }
            }
            Event::CData(ref content) if pending.is_some() => {
                if let Some(ref mut active) = pending {
                    active
                        .refers_to
                        .push_str(std::str::from_utf8(content.as_ref()).map_err(|error| {
                            malformed(format!("Invalid defined name: {error}"))
                        })?);
                }
            }
            Event::End(ref tag) if tag.local_name().as_ref() == b"definedName" => {
                if let Some(name) = pending.take() {
                    definitions.push(name);
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    let mut named_ranges = Vec::with_capacity(definitions.len());
    for definition in definitions {
        let scope = match definition.local_index {
            Some(index) => NameScope::Sheet(
                sheet_names
                    .get(index)
                    .ok_or_else(|| malformed("Defined name sheet index is out of range"))?
                    .clone(),
            ),
            None => NameScope::Workbook,
        };
        if let NameScope::Sheet(ref sheet_name) = scope {
            if let Some(sheet) = workbook.sheet_by_name_mut(sheet_name) {
                if definition.name == "_xlnm.Print_Area" {
                    sheet.page_setup.print_area = Some(definition.refers_to.clone());
                } else if definition.name == "_xlnm.Print_Titles" {
                    let (rows, columns) = print_title_parts(&definition.refers_to);
                    sheet.page_setup.print_title_rows = rows;
                    sheet.page_setup.print_title_cols = columns;
                }
            }
        }
        named_ranges.push(NamedRange {
            name: definition.name,
            refers_to: definition.refers_to,
            scope,
            is_hidden: definition.hidden,
            comment: definition.comment,
        });
    }
    workbook.named_ranges = named_ranges;
    Ok(())
}

fn parse_sheet(xml: &[u8], sheet: &mut Sheet, links: &HashMap<String, String>) -> ExcliResult<()> {
    let mut reader = Reader::from_reader(xml);
    let mut header_or_footer: Option<&[u8]> = None;
    sheet.page_setup.show_gridlines = true;
    sheet.page_setup.show_headings = true;
    loop {
        match reader
            .read_event()
            .map_err(|e| malformed(format!("Invalid worksheet XML: {e}")))?
        {
            Event::Empty(event)
                if matches!(event.local_name().as_ref(), b"oddHeader" | b"oddFooter") => {}
            Event::Start(event) | Event::Empty(event) => match event.local_name().as_ref() {
                b"mergeCell" => {
                    if let Some(reference) = attribute(&event, b"ref")? {
                        let range = RangeReference::from_a1(&reference).ok_or_else(|| {
                            malformed(format!("Invalid merged range: {reference}"))
                        })?;
                        sheet.merged_cells.push(MergedRegion {
                            start_col: range.start.column,
                            start_row: range.start.row,
                            end_col: range.end.column,
                            end_row: range.end.row,
                        });
                    }
                }
                b"autoFilter" => {
                    if let Some(range) = attribute(&event, b"ref")? {
                        sheet.auto_filter = Some(AutoFilter {
                            range,
                            filters: Vec::new(),
                        });
                    }
                }
                b"sheetProtection" => {
                    if protection_flag(&event, b"sheet", false)? {
                        sheet.protection = Some(SheetProtection {
                            password_hash: attribute(&event, b"hashValue")?
                                .or(attribute(&event, b"password")?),
                            allow_sort: !protection_flag(&event, b"sort", true)?,
                            allow_filter: !protection_flag(&event, b"autoFilter", true)?,
                            allow_format_cells: !protection_flag(&event, b"formatCells", true)?,
                            allow_format_columns: !protection_flag(&event, b"formatColumns", true)?,
                            allow_format_rows: !protection_flag(&event, b"formatRows", true)?,
                            allow_insert_rows: !protection_flag(&event, b"insertRows", true)?,
                            allow_insert_columns: !protection_flag(&event, b"insertColumns", true)?,
                            allow_insert_hyperlinks: !protection_flag(
                                &event,
                                b"insertHyperlinks",
                                true,
                            )?,
                            allow_delete_rows: !protection_flag(&event, b"deleteRows", true)?,
                            allow_delete_columns: !protection_flag(&event, b"deleteColumns", true)?,
                        });
                    }
                }
                b"hyperlink" => {
                    let cell = attribute(&event, b"ref")?
                        .ok_or_else(|| malformed("Hyperlink has no reference"))?;
                    let relationship_id = relationship_id(&event)?;
                    let location = attribute(&event, b"location")?;
                    let (url, is_internal) = if let Some(id) = relationship_id {
                        (
                            links
                                .get(&id)
                                .ok_or_else(|| {
                                    malformed(format!("Missing hyperlink relationship: {id}"))
                                })?
                                .clone(),
                            false,
                        )
                    } else if let Some(location) = location {
                        (location, true)
                    } else {
                        return Err(malformed("Hyperlink has no target"));
                    };
                    sheet.hyperlinks.push(Hyperlink {
                        cell,
                        url,
                        text: attribute(&event, b"display")?,
                        is_internal,
                    });
                }
                b"tabColor" => {
                    sheet.tab_color = attribute(&event, b"rgb")?;
                }
                b"row" => {
                    if let Some(number) = number_attribute::<u32>(&event, b"r")? {
                        if !(1..=1_048_576).contains(&number) {
                            return Err(malformed("Row index exceeds Excel limits"));
                        }
                        if sheet.rows.len() < number as usize {
                            sheet.rows.resize_with(number as usize, Row::default);
                        }
                        let row = &mut sheet.rows[number as usize - 1];
                        row.height = number_attribute(&event, b"ht")?;
                        row.hidden = true_attribute(&event, b"hidden")?;
                        row.outline_level = number_attribute(&event, b"outlineLevel")?.unwrap_or(0);
                    }
                }
                b"col" => {
                    let first = number_attribute::<u32>(&event, b"min")?.unwrap_or(0);
                    let last = number_attribute::<u32>(&event, b"max")?.unwrap_or(0);
                    if first == 0 || last < first || last > 16_384 {
                        return Err(malformed("Column range is outside Excel limits"));
                    }
                    let width = number_attribute::<f64>(&event, b"width")?;
                    let hidden = true_attribute(&event, b"hidden")?;
                    let best_fit = true_attribute(&event, b"bestFit")?;
                    for index in first..=last {
                        sheet.columns.push(Column {
                            index: index - 1,
                            width,
                            hidden,
                            style: None,
                            best_fit,
                        });
                    }
                }
                b"pane" => {
                    let state = attribute(&event, b"state")?;
                    let split_x = number_attribute::<f64>(&event, b"xSplit")?;
                    let split_y = number_attribute::<f64>(&event, b"ySplit")?;
                    let top_left = attribute(&event, b"topLeftCell")?
                        .and_then(|reference| CellReference::from_a1(&reference));
                    let frozen = matches!(state.as_deref(), Some("frozen" | "frozenSplit"));
                    let previous_zoom = sheet.pane_state.as_ref().and_then(|pane| pane.zoom);
                    sheet.pane_state = Some(PaneState {
                        frozen,
                        freeze_col: split_x.map_or_else(
                            || top_left.as_ref().map_or(0, |cell| cell.column),
                            |value| value as u32,
                        ),
                        freeze_row: split_y.map_or_else(
                            || top_left.as_ref().map_or(0, |cell| cell.row),
                            |value| value as u32,
                        ),
                        split_x,
                        split_y,
                        zoom: previous_zoom,
                    });
                }
                b"sheetView" => {
                    if let Some(value) = attribute(&event, b"showGridLines")? {
                        sheet.page_setup.show_gridlines = matches!(value.as_str(), "1" | "true");
                    }
                    if let Some(value) = attribute(&event, b"showRowColHeaders")? {
                        sheet.page_setup.show_headings = matches!(value.as_str(), "1" | "true");
                    }
                    if let Some(zoom) = number_attribute::<u16>(&event, b"zoomScale")? {
                        let pane = sheet.pane_state.get_or_insert(PaneState {
                            frozen: false,
                            freeze_col: 0,
                            freeze_row: 0,
                            split_x: None,
                            split_y: None,
                            zoom: None,
                        });
                        pane.zoom = Some(zoom);
                    }
                }
                b"pageMargins" => {
                    sheet.page_setup.margin_top = number_attribute(&event, b"top")?;
                    sheet.page_setup.margin_bottom = number_attribute(&event, b"bottom")?;
                    sheet.page_setup.margin_left = number_attribute(&event, b"left")?;
                    sheet.page_setup.margin_right = number_attribute(&event, b"right")?;
                }
                b"pageSetup" => {
                    if let Some(orientation) = attribute(&event, b"orientation")? {
                        sheet.page_setup.orientation = match orientation.as_str() {
                            "portrait" => PageOrientation::Portrait,
                            "landscape" => PageOrientation::Landscape,
                            _ => {
                                return Err(malformed(format!(
                                    "Unknown page orientation: {orientation}"
                                )))
                            }
                        };
                    }
                    sheet.page_setup.paper_size = attribute(&event, b"paperSize")?;
                    sheet.page_setup.scale = number_attribute(&event, b"scale")?;
                    sheet.page_setup.fit_to_width = number_attribute(&event, b"fitToWidth")?;
                    sheet.page_setup.fit_to_height = number_attribute(&event, b"fitToHeight")?;
                }
                b"oddHeader" => header_or_footer = Some(b"header"),
                b"oddFooter" => header_or_footer = Some(b"footer"),
                _ => {}
            },
            Event::Text(event) => {
                if let Some(kind) = header_or_footer {
                    let value = event
                        .unescape()
                        .map_err(|e| malformed(format!("Invalid header/footer: {e}")))?
                        .into_owned();
                    if kind == b"header" {
                        sheet.page_setup.header = Some(value);
                    } else {
                        sheet.page_setup.footer = Some(value);
                    }
                }
            }
            Event::CData(event) => {
                if let Some(kind) = header_or_footer {
                    let value = String::from_utf8(event.into_inner().to_vec())
                        .map_err(|e| malformed(format!("Invalid header/footer: {e}")))?;
                    if kind == b"header" {
                        sheet.page_setup.header = Some(value);
                    } else {
                        sheet.page_setup.footer = Some(value);
                    }
                }
            }
            Event::End(event)
                if matches!(event.local_name().as_ref(), b"oddHeader" | b"oddFooter") =>
            {
                header_or_footer = None;
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(())
}

/// Add OOXML sheet metadata to a workbook whose cell values were read by calamine.
pub fn enrich_workbook(path: &Path, workbook: &mut Workbook) -> ExcliResult<()> {
    let mut archive = ZipArchive::new(std::fs::File::open(path)?)
        .map_err(|e| malformed(format!("Invalid Excel package: {e}")))?;
    let package = index(&mut archive)?;
    let workbook_xml = read_part(&mut archive, &package.workbook_path)?;
    workbook.date_system = workbook_date_system(&workbook_xml)?;
    enrich_defined_names(&workbook_xml, workbook)?;
    workbook.external_links = crate::ooxml_external_link_read::read_external_links(
        &mut archive,
        &package.workbook_path,
        &workbook_xml,
    )?;
    workbook.connections =
        crate::ooxml_connection_read::read_connections(&mut archive, &package.workbook_path)?;
    let shared_rich = match &package.shared_strings_path {
        Some(path) => crate::ooxml_rich_read::shared_rich_text(&read_part(&mut archive, path)?)?,
        None => Vec::new(),
    };
    let (styles, differential_styles) = match &package.styles_path {
        Some(path) => {
            let xml = read_part(&mut archive, path)?;
            (
                crate::ooxml_style_read::parse_styles(&xml)?,
                crate::ooxml_style_read::parse_differential_styles(&xml)?,
            )
        }
        None => (Vec::new(), Vec::new()),
    };
    let people = crate::ooxml_threaded_comment_read::people(&mut archive, &package.workbook_path)?;
    for part in &package.sheets {
        if let Some(sheet) = workbook.sheet_by_name_mut(&part.name) {
            let relation_path = rels_path(&part.path)?;
            let links = if archive.file_names().any(|name| name == relation_path) {
                hyperlink_targets(&read_part(&mut archive, &relation_path)?)?
            } else {
                HashMap::new()
            };
            let xml = read_part(&mut archive, &part.path)?;
            parse_sheet(&xml, sheet, &links)?;
            crate::ooxml_filter_read::enrich_sheet(&xml, sheet)?;
            crate::ooxml_rich_read::enrich_sheet(&xml, sheet, &shared_rich)?;
            crate::ooxml_style_read::enrich_sheet(&xml, sheet, &styles)?;
            crate::ooxml_validation_read::enrich_sheet(&xml, sheet)?;
            crate::ooxml_conditional_read::enrich_sheet(&xml, sheet, &differential_styles)?;
            crate::ooxml_table_read::enrich_sheet(&mut archive, &part.path, &xml, sheet)?;
            crate::ooxml_pivot_read::enrich_sheet(&mut archive, &part.path, &xml, sheet)?;
            crate::ooxml_comment_read::enrich_sheet(&mut archive, &part.path, sheet)?;
            crate::ooxml_threaded_comment_read::enrich_sheet(
                &mut archive,
                &part.path,
                &people,
                sheet,
            )?;
            crate::ooxml_drawing_read::enrich_sheet(&mut archive, &part.path, &xml, sheet)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use excli_core::sheet::FilterType;
    use excli_core::Workbook;
    use std::collections::HashMap;

    use crate::ooxml_edit::write_modified_parts;

    #[test]
    fn reads_1904_date_system() {
        assert_eq!(
            workbook_date_system(b"<workbook><workbookPr date1904=\"1\"/></workbook>").unwrap(),
            DateSystem::Date1904
        );
        assert_eq!(
            workbook_date_system(b"<workbook><sheets/></workbook>").unwrap(),
            DateSystem::Date1900
        );
    }

    #[test]
    fn workbook_reader_uses_1904_date_system() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dates.xlsx");
        let mut workbook = Workbook::new();
        workbook.add_sheet("Data");
        crate::writer::write_xlsx(&path, &workbook).unwrap();
        {
            let mut archive = ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
            let original =
                String::from_utf8(read_part(&mut archive, "xl/workbook.xml").unwrap()).unwrap();
            let updated = original.replace("<sheets>", "<workbookPr date1904=\"1\"/><sheets>");
            assert_ne!(updated, original);
            write_modified_parts(
                &path,
                &mut archive,
                &HashMap::from([("xl/workbook.xml".to_owned(), updated.into_bytes())]),
            )
            .unwrap();
        }
        let loaded = crate::reader::read_workbook(&path).unwrap();
        assert_eq!(loaded.date_system, DateSystem::Date1904);
    }

    #[test]
    fn reads_sheet_protection_permissions_and_password_hash() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("protected.xlsx");
        let mut workbook = Workbook::new();
        workbook.add_sheet("Data");
        crate::writer::write_xlsx(&path, &workbook).unwrap();
        let mut archive = ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
        let original =
            String::from_utf8(read_part(&mut archive, "xl/worksheets/sheet1.xml").unwrap())
                .unwrap();
        let updated = original.replace("</worksheet>", "<sheetProtection sheet=\"1\" password=\"ABCD\" sort=\"0\" autoFilter=\"false\" formatCells=\"1\" deleteRows=\"0\"/></worksheet>");
        write_modified_parts(
            &path,
            &mut archive,
            &HashMap::from([("xl/worksheets/sheet1.xml".to_owned(), updated.into_bytes())]),
        )
        .unwrap();
        let loaded = crate::reader::read_workbook(&path).unwrap();
        let protection = loaded.sheets[0].protection.as_ref().unwrap();
        assert_eq!(protection.password_hash.as_deref(), Some("ABCD"));
        assert!(protection.allow_sort);
        assert!(protection.allow_filter);
        assert!(protection.allow_delete_rows);
        assert!(!protection.allow_format_cells);
        assert!(!protection.allow_insert_rows);

        let mut sheet = Sheet::new("Unprotected");
        parse_sheet(
            b"<worksheet><sheetProtection sheet=\"0\" sort=\"1\"/></worksheet>",
            &mut sheet,
            &HashMap::new(),
        )
        .unwrap();
        assert!(sheet.protection.is_none());
    }

    #[test]
    fn reads_print_area_and_titles_from_sheet_scoped_names() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("print.xlsx");
        let mut workbook = Workbook::new();
        workbook.add_sheet("Data");
        workbook.add_sheet("Other, West");
        crate::writer::write_xlsx(&path, &workbook).unwrap();
        let mut archive = ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
        let original =
            String::from_utf8(read_part(&mut archive, "xl/workbook.xml").unwrap()).unwrap();
        let updated = original.replace("</workbook>", "<definedNames><definedName name=\"_xlnm.Print_Area\" localSheetId=\"1\">'Other, West'!$A$1:$D$20</definedName><definedName name=\"_xlnm.Print_Titles\" localSheetId=\"1\">'Other, West'!$1:$3,'Other, West'!$A:$B</definedName></definedNames></workbook>");
        write_modified_parts(
            &path,
            &mut archive,
            &HashMap::from([("xl/workbook.xml".to_owned(), updated.into_bytes())]),
        )
        .unwrap();
        let loaded = crate::reader::read_workbook(&path).unwrap();
        assert!(loaded.sheets[0].page_setup.print_area.is_none());
        let page = &loaded.sheets[1].page_setup;
        assert_eq!(page.print_area.as_deref(), Some("'Other, West'!$A$1:$D$20"));
        assert_eq!(page.print_title_rows.as_deref(), Some("$1:$3"));
        assert_eq!(page.print_title_cols.as_deref(), Some("$A:$B"));

        let mut workbook = Workbook::new();
        workbook.add_sheet("Data");
        let xml = b"<workbook><sheets><sheet name=\"Chart\"/><sheet name=\"Data\"/></sheets><definedNames><definedName name=\"_xlnm.Print_Area\" localSheetId=\"1\">Data!$B$2:$C$9</definedName></definedNames></workbook>";
        enrich_defined_names(xml, &mut workbook).unwrap();
        assert_eq!(
            workbook.sheets[0].page_setup.print_area.as_deref(),
            Some("Data!$B$2:$C$9")
        );
    }

    #[test]
    fn reads_defined_name_scope_visibility_and_comment() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("names.xlsx");
        let mut workbook = Workbook::new();
        workbook.add_sheet("Data");
        workbook.add_sheet("Sales");
        crate::writer::write_xlsx(&path, &workbook).unwrap();
        let mut archive = ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
        let original =
            String::from_utf8(read_part(&mut archive, "xl/workbook.xml").unwrap()).unwrap();
        let updated = original.replace("</workbook>", "<definedNames><definedName name=\"Global\" comment=\"Shared &amp; safe\">Data!$A$1</definedName><definedName name=\"Local\" localSheetId=\"1\" hidden=\"1\" comment=\"Draft\">Sales!$B$2</definedName></definedNames></workbook>");
        write_modified_parts(
            &path,
            &mut archive,
            &HashMap::from([("xl/workbook.xml".to_owned(), updated.into_bytes())]),
        )
        .unwrap();
        let loaded = crate::reader::read_workbook(&path).unwrap();
        assert_eq!(loaded.named_ranges.len(), 2);
        assert_eq!(loaded.named_ranges[0].name, "Global");
        assert_eq!(loaded.named_ranges[0].scope, NameScope::Workbook);
        assert_eq!(
            loaded.named_ranges[0].comment.as_deref(),
            Some("Shared & safe")
        );
        assert!(!loaded.named_ranges[0].is_hidden);
        assert_eq!(
            loaded.named_ranges[1].scope,
            NameScope::Sheet("Sales".to_owned())
        );
        assert_eq!(loaded.named_ranges[1].refers_to, "Sales!$B$2");
        assert_eq!(loaded.named_ranges[1].comment.as_deref(), Some("Draft"));
        assert!(loaded.named_ranges[1].is_hidden);
    }

    #[test]
    fn workbook_reader_populates_auto_filter_criteria() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("filtered.xlsx");
        let mut workbook = Workbook::new();
        workbook.add_sheet("Data");
        crate::writer::write_xlsx(&path, &workbook).unwrap();
        let mut archive = ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
        let original =
            String::from_utf8(read_part(&mut archive, "xl/worksheets/sheet1.xml").unwrap())
                .unwrap();
        let updated = original.replace("</worksheet>", "<autoFilter ref=\"A1:A20\"><filterColumn colId=\"0\"><filters><filter val=\"East\"/></filters></filterColumn></autoFilter></worksheet>");
        write_modified_parts(
            &path,
            &mut archive,
            &HashMap::from([("xl/worksheets/sheet1.xml".to_owned(), updated.into_bytes())]),
        )
        .unwrap();
        let loaded = crate::reader::read_workbook(&path).unwrap();
        let filter = loaded.sheets[0].auto_filter.as_ref().unwrap();
        assert_eq!(filter.range, "A1:A20");
        assert!(
            matches!(&filter.filters[0].filter_type, FilterType::Value(value) if value == "East")
        );
    }

    #[test]
    fn reads_layout_and_sheet_metadata() {
        let xml = br#"<worksheet><sheetViews><sheetView zoomScale="125"><pane xSplit="2" ySplit="3" state="frozen" topLeftCell="C4"/></sheetView></sheetViews><cols><col min="2" max="3" width="19.5" hidden="1" bestFit="1"/></cols><sheetData><row r="4" ht="24" hidden="1" outlineLevel="2"/></sheetData><autoFilter ref="A1:D20"/><mergeCells count="1"><mergeCell ref="B4:C4"/></mergeCells><pageMargins left="0.5" right="0.6" top="0.7" bottom="0.8"/><pageSetup orientation="landscape" paperSize="9" fitToWidth="1"/></worksheet>"#;
        let mut sheet = Sheet::new("Data");
        parse_sheet(xml, &mut sheet, &HashMap::new()).unwrap();
        assert_eq!(sheet.rows[3].height, Some(24.0));
        assert!(sheet.rows[3].hidden);
        assert_eq!(sheet.rows[3].outline_level, 2);
        assert_eq!(sheet.columns.len(), 2);
        assert_eq!(sheet.columns[0].index, 1);
        assert_eq!(sheet.columns[1].width, Some(19.5));
        assert!(sheet.columns[1].hidden);
        assert_eq!(sheet.merged_cells[0].start_col, 1);
        assert_eq!(sheet.merged_cells[0].end_col, 2);
        assert_eq!(sheet.auto_filter.as_ref().unwrap().range, "A1:D20");
        let pane = sheet.pane_state.as_ref().unwrap();
        assert!(pane.frozen);
        assert_eq!((pane.freeze_col, pane.freeze_row), (2, 3));
        assert_eq!(pane.zoom, Some(125));
        assert_eq!(sheet.page_setup.margin_left, Some(0.5));
        assert_eq!(sheet.page_setup.fit_to_width, Some(1));
        assert!(matches!(
            sheet.page_setup.orientation,
            PageOrientation::Landscape
        ));
    }

    #[test]
    fn reads_hyperlinks_headers_and_sheet_view_options() {
        let relations = br#"<Relationships><Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="https://example.com/report" TargetMode="External"/></Relationships>"#;
        let links = hyperlink_targets(relations).unwrap();
        let xml = br#"<worksheet xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheetPr><tabColor rgb="FF0088CC"/></sheetPr><sheetViews><sheetView showGridLines="0" showRowColHeaders="1"/></sheetViews><sheetData/><hyperlinks><hyperlink ref="A1" r:id="rId3" display="Report"/><hyperlink ref="B2" location="Summary!A1"/></hyperlinks><headerFooter><oddHeader>&amp;CQuarterly report</oddHeader><oddFooter>Page &amp;P</oddFooter></headerFooter></worksheet>"#;
        let mut sheet = Sheet::new("Data");
        parse_sheet(xml, &mut sheet, &links).unwrap();
        assert_eq!(sheet.tab_color.as_deref(), Some("FF0088CC"));
        assert!(!sheet.page_setup.show_gridlines);
        assert!(sheet.page_setup.show_headings);
        assert_eq!(sheet.hyperlinks.len(), 2);
        assert_eq!(sheet.hyperlinks[0].url, "https://example.com/report");
        assert!(!sheet.hyperlinks[0].is_internal);
        assert_eq!(sheet.hyperlinks[1].url, "Summary!A1");
        assert!(sheet.hyperlinks[1].is_internal);
        assert_eq!(
            sheet.page_setup.header.as_deref(),
            Some("&CQuarterly report")
        );
        assert_eq!(sheet.page_setup.footer.as_deref(), Some("Page &P"));
    }
}
