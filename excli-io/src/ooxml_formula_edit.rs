//! Formula edits in existing XLSX/XLSM worksheets without rebuilding the package.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

use excli_core::error::{ErrorCode, ExcliError, ExcliResult};
use excli_core::reference::{CellReference, RangeReference};
use excli_core::types::FileFormat;
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use zip::ZipArchive;

use crate::format_detect::detect_format;
use crate::ooxml_edit::{
    ensure_editable, read_shared_strings, recalculation_xml, write_modified_parts, xml_escape,
    SharedString,
};
use crate::ooxml_package::{index, read_part};

#[derive(Debug)]
struct CellSpan {
    column: u32,
    from: usize,
    to: usize,
}

#[derive(Debug)]
struct RowSpan {
    number: u32,
    prefix: String,
    from: usize,
    to: usize,
    end_start: usize,
    empty: bool,
    cells: Vec<CellSpan>,
}

#[derive(Debug)]
struct SheetDataSpan {
    prefix: String,
    from: usize,
    to: usize,
    end_start: usize,
    empty: bool,
}

fn invalid(message: impl Into<String>) -> ExcliError {
    ExcliError::new(ErrorCode::FileCorrupted, message)
}

fn unsupported(message: impl Into<String>) -> ExcliError {
    ExcliError::new(ErrorCode::NotImplemented, message)
}

fn attr(event: &BytesStart<'_>, key: &[u8]) -> ExcliResult<Option<String>> {
    for attribute in event.attributes() {
        let attribute = attribute.map_err(|e| invalid(format!("Invalid cell attribute: {e}")))?;
        if attribute.key.as_ref() == key {
            return Ok(Some(
                attribute
                    .unescape_value()
                    .map_err(|e| invalid(format!("Invalid cell attribute value: {e}")))?
                    .into_owned(),
            ));
        }
    }
    Ok(None)
}

fn element_prefix(event: &BytesStart<'_>) -> ExcliResult<String> {
    let qualified = event.name();
    let name = std::str::from_utf8(qualified.as_ref())
        .map_err(|_| invalid("Invalid worksheet element name"))?;
    Ok(name
        .rsplit_once(':')
        .map_or(String::new(), |(prefix, _)| format!("{prefix}:")))
}

fn parse_spans(xml: &[u8]) -> ExcliResult<(SheetDataSpan, Vec<RowSpan>)> {
    let mut reader = Reader::from_reader(xml);
    let mut sheet_data = None;
    let mut rows = Vec::new();
    let mut row: Option<RowSpan> = None;
    let mut cell: Option<(u32, usize)> = None;
    let mut last_row = 0u32;

    loop {
        let from = reader.buffer_position() as usize;
        let event = reader
            .read_event()
            .map_err(|e| invalid(format!("Invalid worksheet XML: {e}")))?;
        let to = reader.buffer_position() as usize;
        match event {
            Event::Start(event) if event.local_name().as_ref() == b"sheetData" => {
                sheet_data = Some(SheetDataSpan {
                    prefix: element_prefix(&event)?,
                    from,
                    to,
                    end_start: 0,
                    empty: false,
                });
            }
            Event::Empty(event) if event.local_name().as_ref() == b"sheetData" => {
                sheet_data = Some(SheetDataSpan {
                    prefix: element_prefix(&event)?,
                    from,
                    to,
                    end_start: to,
                    empty: true,
                });
            }
            Event::End(event) if event.local_name().as_ref() == b"sheetData" => {
                if let Some(span) = &mut sheet_data {
                    span.end_start = from;
                    span.to = to;
                }
            }
            Event::Start(event) | Event::Empty(event) if event.local_name().as_ref() == b"row" => {
                let prefix = element_prefix(&event)?;
                let number = attr(&event, b"r")?
                    .map(|value| {
                        value
                            .parse::<u32>()
                            .map_err(|_| invalid("Invalid row number"))
                    })
                    .transpose()?
                    .unwrap_or(last_row + 1);
                if number == 0 || number > 1_048_576 {
                    return Err(invalid("Row number outside Excel limits"));
                }
                last_row = number;
                let empty = xml[from..to].ends_with(b"/>");
                let span = RowSpan {
                    number,
                    prefix,
                    from,
                    to,
                    end_start: to,
                    empty,
                    cells: Vec::new(),
                };
                if empty {
                    rows.push(span);
                } else {
                    row = Some(span);
                }
            }
            Event::End(event) if event.local_name().as_ref() == b"row" => {
                let mut span = row
                    .take()
                    .ok_or_else(|| invalid("Unmatched row closing tag"))?;
                span.end_start = from;
                span.to = to;
                rows.push(span);
            }
            Event::Start(event) | Event::Empty(event)
                if event.local_name().as_ref() == b"c" && row.is_some() =>
            {
                let current_row = row.as_mut().unwrap();
                let column = if let Some(reference) = attr(&event, b"r")? {
                    CellReference::from_a1(&reference)
                        .ok_or_else(|| invalid(format!("Invalid cell reference: {reference}")))?
                        .column
                } else {
                    current_row
                        .cells
                        .last()
                        .map_or(0, |previous| previous.column + 1)
                };
                if xml[from..to].ends_with(b"/>") {
                    current_row.cells.push(CellSpan { column, from, to });
                } else {
                    cell = Some((column, from));
                }
            }
            Event::End(event) if event.local_name().as_ref() == b"c" => {
                let (column, start) = cell
                    .take()
                    .ok_or_else(|| invalid("Unmatched cell closing tag"))?;
                if let Some(current_row) = &mut row {
                    current_row.cells.push(CellSpan {
                        column,
                        from: start,
                        to,
                    });
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    let sheet_data = sheet_data.ok_or_else(|| invalid("Worksheet has no sheetData element"))?;
    if !sheet_data.empty && sheet_data.end_start == 0 {
        return Err(invalid("Worksheet sheetData is not closed"));
    }
    Ok((sheet_data, rows))
}

pub(crate) fn next_append_row(xml: &[u8]) -> ExcliResult<u32> {
    let (_, rows) = parse_spans(xml)?;
    Ok(rows.iter().map(|row| row.number).max().unwrap_or(0))
}

fn check_formula_groups(xml: &[u8], targets: &BTreeMap<u32, BTreeSet<u32>>) -> ExcliResult<()> {
    let mut reader = Reader::from_reader(xml);
    loop {
        match reader
            .read_event()
            .map_err(|e| invalid(format!("Invalid worksheet XML: {e}")))?
        {
            Event::Start(event) | Event::Empty(event) if event.local_name().as_ref() == b"f" => {
                if matches!(
                    attr(&event, b"t")?.as_deref(),
                    Some("array" | "shared" | "dataTable")
                ) {
                    if let Some(reference) = attr(&event, b"ref")? {
                        let range = RangeReference::from_a1(&reference).ok_or_else(|| {
                            invalid(format!("Invalid formula group range: {reference}"))
                        })?;
                        if targets
                            .range(range.start.row..=range.end.row)
                            .any(|(_, columns)| {
                                columns
                                    .range(range.start.column..=range.end.column)
                                    .next()
                                    .is_some()
                            })
                        {
                            return Err(unsupported(
                                "Shared, array, and data-table formulas require group-aware editing",
                            ));
                        }
                    }
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(())
}

fn check_merged_targets(xml: &[u8], targets: &BTreeMap<u32, BTreeSet<u32>>) -> ExcliResult<()> {
    let mut reader = Reader::from_reader(xml);
    loop {
        match reader
            .read_event()
            .map_err(|e| invalid(format!("Invalid worksheet XML: {e}")))?
        {
            Event::Start(event) | Event::Empty(event)
                if event.local_name().as_ref() == b"mergeCell" =>
            {
                let reference =
                    attr(&event, b"ref")?.ok_or_else(|| invalid("Merged cell has no range"))?;
                let range = RangeReference::from_a1(&reference)
                    .ok_or_else(|| invalid(format!("Invalid merged range: {reference}")))?;
                for (&row, columns) in targets.range(range.start.row..=range.end.row) {
                    for &column in columns.range(range.start.column..=range.end.column) {
                        if row != range.start.row || column != range.start.column {
                            return Err(unsupported(
                                "Formula edit cannot target a covered cell in a merged range",
                            ));
                        }
                    }
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(())
}

fn formula_cell(column: u32, row: u32, expression: &str, prefix: &str) -> Vec<u8> {
    let reference = CellReference::new(column, row).to_a1();
    format!(
        "<{prefix}c r=\"{reference}\"><{prefix}f>{}</{prefix}f></{prefix}c>",
        xml_escape(expression)
    )
    .into_bytes()
}

pub(crate) fn safe_existing_opening(
    xml: &[u8],
    shared_strings: &[SharedString],
) -> ExcliResult<(String, String)> {
    let mut reader = Reader::from_reader(xml);
    let mut opening = None;
    let mut cell_tag = None;
    let mut cell_type = None;
    let mut in_value = false;
    let mut value = String::new();
    loop {
        match reader
            .read_event()
            .map_err(|e| invalid(format!("Invalid cell XML: {e}")))?
        {
            Event::Start(event) | Event::Empty(event) if event.local_name().as_ref() == b"c" => {
                cell_type = attr(&event, b"t")?;
                let qualified = std::str::from_utf8(event.name().as_ref())
                    .map_err(|_| invalid("Invalid cell element name"))?
                    .to_owned();
                let mut tag = format!("<{qualified}");
                for attribute in event.attributes() {
                    let attribute =
                        attribute.map_err(|e| invalid(format!("Invalid cell attribute: {e}")))?;
                    let key = std::str::from_utf8(attribute.key.as_ref())
                        .map_err(|e| invalid(format!("Invalid cell attribute name: {e}")))?;
                    if matches!(key, "cm" | "vm") {
                        return Err(unsupported(
                            "Cell edit cannot replace cells with extension metadata",
                        ));
                    }
                    if key != "t" {
                        let value = attribute
                            .unescape_value()
                            .map_err(|e| invalid(format!("Invalid cell attribute: {e}")))?;
                        tag.push_str(&format!(" {key}=\"{}\"", xml_escape(&value)));
                    }
                }
                opening = Some(tag);
                cell_tag = Some(qualified);
            }
            Event::Start(event) if event.local_name().as_ref() == b"v" => in_value = true,
            Event::End(event) if event.local_name().as_ref() == b"v" => in_value = false,
            Event::Text(event) if in_value => value.push_str(
                &event
                    .unescape()
                    .map_err(|e| invalid(format!("Invalid cell value: {e}")))?,
            ),
            Event::Start(event) | Event::Empty(event) if event.local_name().as_ref() == b"f" => {
                if attr(&event, b"t")?.is_some() || attr(&event, b"ref")?.is_some() {
                    return Err(unsupported(
                        "Shared and array formulas require group-aware editing",
                    ));
                }
            }
            Event::Start(event) | Event::Empty(event)
                if matches!(event.local_name().as_ref(), b"r" | b"rPh" | b"extLst") =>
            {
                return Err(unsupported(
                    "Cell edit cannot replace rich text or cell extensions",
                ));
            }
            Event::Start(event) | Event::Empty(event)
                if !matches!(event.local_name().as_ref(), b"is" | b"t" | b"v") =>
            {
                return Err(unsupported(
                    "Cell edit cannot replace an unknown cell feature",
                ));
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if cell_type.as_deref() == Some("s") {
        let index: usize = value
            .parse()
            .map_err(|_| invalid("Invalid shared string index"))?;
        if shared_strings
            .get(index)
            .ok_or_else(|| invalid("Shared string index out of range"))?
            .rich
        {
            return Err(unsupported("Cell edit cannot replace a rich-text cell"));
        }
    }
    Ok((
        opening.ok_or_else(|| invalid("Missing cell opening tag"))?,
        cell_tag.ok_or_else(|| invalid("Missing cell element name"))?,
    ))
}

fn replace_existing_cell(
    xml: &[u8],
    expression: &str,
    shared_strings: &[SharedString],
) -> ExcliResult<Vec<u8>> {
    let (mut result, cell_tag) = safe_existing_opening(xml, shared_strings)?;
    let prefix = cell_tag.rsplit_once(':').map_or("", |(prefix, _)| prefix);
    let prefix = if prefix.is_empty() {
        String::new()
    } else {
        format!("{prefix}:")
    };
    result.push_str(&format!("><{prefix}f>"));
    result.push_str(&xml_escape(expression));
    result.push_str(&format!("</{prefix}f></{cell_tag}>"));
    Ok(result.into_bytes())
}

fn expand_empty_tag(xml: &[u8], from: usize, to: usize) -> ExcliResult<(Vec<u8>, Vec<u8>)> {
    let mut opening = xml[from..to].to_vec();
    if !opening.ends_with(b"/>") {
        return Err(invalid("Expected an empty XML element"));
    }
    opening.truncate(opening.len() - 2);
    opening.push(b'>');
    let name_end = xml[from + 1..to]
        .iter()
        .position(|byte| byte.is_ascii_whitespace() || *byte == b'/' || *byte == b'>')
        .ok_or_else(|| invalid("Invalid empty XML element"))?;
    let name = &xml[from + 1..from + 1 + name_end];
    let mut closing = b"</".to_vec();
    closing.extend_from_slice(name);
    closing.push(b'>');
    Ok((opening, closing))
}

fn expand_dimension(xml: Vec<u8>, targets: &BTreeMap<u32, BTreeSet<u32>>) -> ExcliResult<Vec<u8>> {
    let mut reader = Reader::from_reader(xml.as_slice());
    loop {
        let from = reader.buffer_position() as usize;
        let event = reader
            .read_event()
            .map_err(|e| invalid(format!("Invalid worksheet XML: {e}")))?;
        let to = reader.buffer_position() as usize;
        match event {
            Event::Empty(event) if event.local_name().as_ref() == b"dimension" => {
                let reference = attr(&event, b"ref")?
                    .ok_or_else(|| invalid("Worksheet dimension has no reference"))?;
                let (mut first, mut last) = if let Some(range) = RangeReference::from_a1(&reference)
                {
                    (range.start, range.end)
                } else {
                    let cell = CellReference::from_a1(&reference).ok_or_else(|| {
                        invalid(format!("Invalid worksheet dimension: {reference}"))
                    })?;
                    (cell.clone(), cell)
                };
                for (&row, columns) in targets {
                    if let (Some(&min_column), Some(&max_column)) =
                        (columns.first(), columns.last())
                    {
                        first.row = first.row.min(row);
                        first.column = first.column.min(min_column);
                        last.row = last.row.max(row);
                        last.column = last.column.max(max_column);
                    }
                }
                let expanded = if first == last {
                    first.to_a1()
                } else {
                    RangeReference::new(first, last).to_a1()
                };
                if expanded == reference {
                    return Ok(xml);
                }
                let name = std::str::from_utf8(event.name().as_ref())
                    .map_err(|e| invalid(format!("Invalid dimension name: {e}")))?
                    .to_owned();
                let mut replacement = format!("<{name}");
                for attribute in event.attributes() {
                    let attribute = attribute
                        .map_err(|e| invalid(format!("Invalid dimension attribute: {e}")))?;
                    let key = std::str::from_utf8(attribute.key.as_ref())
                        .map_err(|e| invalid(format!("Invalid dimension attribute: {e}")))?;
                    let value = if key == "ref" {
                        expanded.as_str().into()
                    } else {
                        attribute
                            .unescape_value()
                            .map_err(|e| invalid(format!("Invalid dimension attribute: {e}")))?
                    };
                    replacement.push_str(&format!(" {key}=\"{}\"", xml_escape(&value)));
                }
                replacement.push_str("/>");
                let mut updated = Vec::with_capacity(xml.len() + replacement.len());
                updated.extend_from_slice(&xml[..from]);
                updated.extend_from_slice(replacement.as_bytes());
                updated.extend_from_slice(&xml[to..]);
                return Ok(updated);
            }
            Event::Eof => return Ok(xml),
            _ => {}
        }
    }
}

pub(crate) fn patch_sheet_with<F>(
    xml: &[u8],
    targets: &BTreeMap<u32, BTreeSet<u32>>,
    render: F,
) -> ExcliResult<Vec<u8>>
where
    F: Fn(u32, u32, Option<&[u8]>, &str) -> ExcliResult<Vec<u8>>,
{
    check_formula_groups(xml, targets)?;
    check_merged_targets(xml, targets)?;
    let (sheet_data, rows) = parse_spans(xml)?;
    let mut edits: Vec<(usize, usize, Vec<u8>)> = Vec::new();

    if sheet_data.empty {
        let (opening, closing) = expand_empty_tag(xml, sheet_data.from, sheet_data.to)?;
        let mut replacement = opening;
        for (&row, columns) in targets {
            replacement.extend_from_slice(
                format!("<{}row r=\"{}\">", sheet_data.prefix, row + 1).as_bytes(),
            );
            for &column in columns {
                replacement.extend_from_slice(&render(column, row, None, &sheet_data.prefix)?);
            }
            replacement.extend_from_slice(format!("</{}row>", sheet_data.prefix).as_bytes());
        }
        replacement.extend_from_slice(&closing);
        edits.push((sheet_data.from, sheet_data.to, replacement));
    } else {
        for (&row_number, columns) in targets {
            if let Some(row) = rows
                .iter()
                .find(|candidate| candidate.number == row_number + 1)
            {
                if row.empty {
                    let (opening, closing) = expand_empty_tag(xml, row.from, row.to)?;
                    let mut replacement = opening;
                    for &column in columns {
                        replacement.extend_from_slice(&render(
                            column,
                            row_number,
                            None,
                            &row.prefix,
                        )?);
                    }
                    replacement.extend_from_slice(&closing);
                    edits.push((row.from, row.to, replacement));
                } else {
                    let mut insertions: BTreeMap<usize, Vec<u8>> = BTreeMap::new();
                    for &column in columns {
                        if let Some(cell) = row.cells.iter().find(|cell| cell.column == column) {
                            edits.push((
                                cell.from,
                                cell.to,
                                render(
                                    column,
                                    row_number,
                                    Some(&xml[cell.from..cell.to]),
                                    &row.prefix,
                                )?,
                            ));
                        } else {
                            let at = row
                                .cells
                                .iter()
                                .find(|cell| cell.column > column)
                                .map_or(row.end_start, |cell| cell.from);
                            insertions.entry(at).or_default().extend_from_slice(&render(
                                column,
                                row_number,
                                None,
                                &row.prefix,
                            )?);
                        }
                    }
                    edits.extend(insertions.into_iter().map(|(at, bytes)| (at, at, bytes)));
                }
            } else {
                let at = rows
                    .iter()
                    .find(|row| row.number > row_number + 1)
                    .map_or(sheet_data.end_start, |row| row.from);
                let mut replacement =
                    format!("<{}row r=\"{}\">", sheet_data.prefix, row_number + 1).into_bytes();
                for &column in columns {
                    replacement.extend_from_slice(&render(
                        column,
                        row_number,
                        None,
                        &sheet_data.prefix,
                    )?);
                }
                replacement.extend_from_slice(format!("</{}row>", sheet_data.prefix).as_bytes());
                edits.push((at, at, replacement));
            }
        }
    }

    edits.sort_by_key(|(from, to, _)| (*from, *to));
    let mut output =
        Vec::with_capacity(xml.len() + edits.iter().map(|(_, _, data)| data.len()).sum::<usize>());
    let mut cursor = 0;
    for (from, to, replacement) in edits {
        if from < cursor {
            return Err(invalid("Overlapping worksheet edits"));
        }
        output.extend_from_slice(&xml[cursor..from]);
        output.extend_from_slice(&replacement);
        cursor = to;
    }
    output.extend_from_slice(&xml[cursor..]);
    expand_dimension(output, targets)
}

fn patch_sheet(
    xml: &[u8],
    targets: &BTreeMap<u32, BTreeSet<u32>>,
    expression: &str,
    shared_strings: &[SharedString],
) -> ExcliResult<Vec<u8>> {
    patch_sheet_with(
        xml,
        targets,
        |column, row, existing, prefix| match existing {
            Some(bytes) => replace_existing_cell(bytes, expression, shared_strings),
            None => Ok(formula_cell(column, row, expression, prefix)),
        },
    )
}

/// Set the same formula in one or more cells of an existing XLSX/XLSM sheet.
/// Other package parts are retained and Excel is asked to recalculate on open.
pub fn set_formulas(
    path: &Path,
    sheet_name: Option<&str>,
    cells: &[(u32, u32)],
    formula: &str,
) -> ExcliResult<usize> {
    if !matches!(detect_format(path)?, FileFormat::Xlsx | FileFormat::Xlsm) {
        return Err(unsupported(
            "Package-preserving formula edits support XLSX and XLSM",
        ));
    }
    let expression = formula.strip_prefix('=').unwrap_or(formula);
    if expression.is_empty() || expression.chars().any(|ch| !matches!(ch as u32, 0x9 | 0xA | 0xD | 0x20..=0xD7FF | 0xE000..=0xFFFD | 0x10000..=0x10FFFF)) {
        return Err(ExcliError::new(ErrorCode::FormulaParseError, "Formula is empty or contains an invalid XML character"));
    }
    let mut targets: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
    for &(column, row) in cells {
        if column >= 16_384 || row >= 1_048_576 {
            return Err(ExcliError::new(
                ErrorCode::RangeInvalid,
                "Cell is outside Excel limits",
            ));
        }
        targets.entry(row).or_default().insert(column);
    }
    if targets.is_empty() {
        return Err(ExcliError::new(
            ErrorCode::RangeInvalid,
            "No target cells were specified",
        ));
    }

    let mut archive = ZipArchive::new(std::fs::File::open(path)?)
        .map_err(|e| invalid(format!("Invalid Excel package: {e}")))?;
    ensure_editable(&mut archive)?;
    let package = index(&mut archive)?;
    let sheet = if let Some(name) = sheet_name {
        package
            .sheets
            .iter()
            .find(|sheet| sheet.name == name)
            .ok_or_else(|| ExcliError::sheet_not_found(name))?
    } else {
        package
            .sheets
            .first()
            .ok_or_else(|| invalid("Workbook has no worksheets"))?
    };
    let shared_strings = match &package.shared_strings_path {
        Some(part) => read_shared_strings(&read_part(&mut archive, part)?)?,
        None => Vec::new(),
    };
    let worksheet_xml = read_part(&mut archive, &sheet.path)?;
    let updated_sheet = patch_sheet(&worksheet_xml, &targets, expression, &shared_strings)?;
    let workbook_xml = read_part(&mut archive, &package.workbook_path)?;
    let mut changed = HashMap::new();
    changed.insert(sheet.path.clone(), updated_sheet);
    changed.insert(package.workbook_path, recalculation_xml(&workbook_xml)?);
    write_modified_parts(path, &mut archive, &changed)?;
    Ok(targets.values().map(BTreeSet::len).sum())
}

#[cfg(test)]
mod tests {
    use super::*;
    use excli_core::cell::Cell;
    use excli_core::Workbook;

    #[test]
    fn inserts_sparse_formulas_and_replaces_a_plain_cell() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("formulas.xlsx");
        let mut workbook = Workbook::new();
        let sheet = workbook.add_sheet("Data");
        sheet.set_cell(0, 0, Cell::number(10.0));
        sheet.set_cell(2, 2, Cell::string("old"));
        workbook
            .add_sheet("Other")
            .set_cell(0, 0, Cell::string("untouched"));
        crate::writer::write_xlsx(&path, &workbook).unwrap();
        let other_before = read_part(
            &mut ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap(),
            "xl/worksheets/sheet2.xml",
        )
        .unwrap();

        assert_eq!(
            set_formulas(&path, Some("Data"), &[(1, 1), (2, 2), (4, 4)], "=SUM(A1,1)").unwrap(),
            3
        );
        let loaded = crate::reader::read_workbook(&path).unwrap();
        let sheet = loaded.sheet_by_name("Data").unwrap();
        for (column, row) in [(1, 1), (2, 2), (4, 4)] {
            assert_eq!(
                sheet
                    .get_cell(column, row)
                    .unwrap()
                    .formula
                    .as_ref()
                    .unwrap()
                    .raw,
                "=SUM(A1,1)"
            );
        }
        assert_eq!(sheet.get_cell(0, 0).unwrap().value.to_string(), "10");
        let other_after = read_part(
            &mut ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap(),
            "xl/worksheets/sheet2.xml",
        )
        .unwrap();
        assert_eq!(other_after, other_before);
    }

    #[test]
    fn handles_empty_sheet_data_and_empty_row() {
        let xml = b"<worksheet><sheetData/></worksheet>";
        let targets = BTreeMap::from([(2, BTreeSet::from([3]))]);
        let patched = patch_sheet(xml, &targets, "1+2", &[]).unwrap();
        assert!(String::from_utf8(patched)
            .unwrap()
            .contains("<row r=\"3\"><c r=\"D3\"><f>1+2</f></c></row>"));

        let xml = b"<worksheet><sheetData><row r=\"3\"/></sheetData></worksheet>";
        let patched = patch_sheet(xml, &targets, "1+2", &[]).unwrap();
        assert!(String::from_utf8(patched)
            .unwrap()
            .contains("<row r=\"3\"><c r=\"D3\"><f>1+2</f></c></row>"));
    }

    #[test]
    fn refuses_shared_formulas_without_changing_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("shared.xlsx");
        let mut workbook = Workbook::new();
        workbook.add_sheet("Data").set_cell(
            0,
            0,
            Cell::number(3.0).with_formula(excli_core::formula::Formula::new("=1+2")),
        );
        crate::writer::write_xlsx(&path, &workbook).unwrap();
        {
            let mut archive = ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
            let original_sheet =
                String::from_utf8(read_part(&mut archive, "xl/worksheets/sheet1.xml").unwrap())
                    .unwrap();
            let shared_sheet = original_sheet.replace(
                "<f>1+2</f>",
                "<f t=\"shared\" si=\"0\" ref=\"A1:B2\">1+2</f>",
            );
            assert_ne!(shared_sheet, original_sheet);
            write_modified_parts(
                &path,
                &mut archive,
                &HashMap::from([(
                    "xl/worksheets/sheet1.xml".to_owned(),
                    shared_sheet.into_bytes(),
                )]),
            )
            .unwrap();
        }
        let original = std::fs::read(&path).unwrap();
        assert!(set_formulas(&path, Some("Data"), &[(1, 1)], "=4+5").is_err());
        assert_eq!(std::fs::read(&path).unwrap(), original);
    }

    #[test]
    fn preserves_style_and_refuses_group_spill_cells() {
        let replaced =
            replace_existing_cell(b"<c r=\"A1\" s=\"7\" t=\"n\"><v>42</v></c>", "1+2", &[])
                .unwrap();
        assert_eq!(replaced, b"<c r=\"A1\" s=\"7\"><f>1+2</f></c>");

        let xml = b"<worksheet><sheetData><row r=\"1\"><c r=\"A1\"><f t=\"array\" ref=\"A1:B2\">1+2</f></c></row></sheetData></worksheet>";
        let targets = BTreeMap::from([(1, BTreeSet::from([1]))]);
        assert!(patch_sheet(xml, &targets, "3+4", &[]).is_err());
        let prefixed = b"<x:worksheet xmlns:x=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><x:sheetData/></x:worksheet>";
        let updated =
            String::from_utf8(patch_sheet(prefixed, &targets, "3+4", &[]).unwrap()).unwrap();
        assert!(updated.contains(
            "<x:sheetData><x:row r=\"2\"><x:c r=\"B2\"><x:f>3+4</x:f></x:c></x:row></x:sheetData>"
        ));
        let existing =
            replace_existing_cell(b"<x:c r=\"A1\" s=\"7\"><x:v>42</x:v></x:c>", "1+2", &[])
                .unwrap();
        assert_eq!(existing, b"<x:c r=\"A1\" s=\"7\"><x:f>1+2</x:f></x:c>");
    }

    #[test]
    fn formula_and_value_edits_preserve_prefixed_worksheet_namespace() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("prefixed.xlsx");
        let mut workbook = Workbook::new();
        workbook.add_sheet("Data");
        crate::writer::write_xlsx(&path, &workbook).unwrap();
        let mut archive = ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
        let sheet = br#"<x:worksheet xmlns:x="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><x:sheetData><x:row r="1"><x:c r="A1"><x:v>1</x:v></x:c></x:row></x:sheetData></x:worksheet>"#;
        crate::ooxml_edit::write_modified_parts(
            &path,
            &mut archive,
            &HashMap::from([("xl/worksheets/sheet1.xml".to_owned(), sheet.to_vec())]),
        )
        .unwrap();
        assert_eq!(
            set_formulas(&path, Some("Data"), &[(1, 0)], "=A1+1").unwrap(),
            1
        );
        assert_eq!(
            crate::ooxml_value_edit::set_values(
                &path,
                Some("Data"),
                &[
                    ((0, 0), excli_core::cell::CellValue::Number(2.0)),
                    (
                        (3, 3),
                        excli_core::cell::CellValue::InlineString("new".into())
                    ),
                ],
            )
            .unwrap(),
            2
        );
        let mut archive = ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
        let edited =
            String::from_utf8(read_part(&mut archive, "xl/worksheets/sheet1.xml").unwrap())
                .unwrap();
        assert!(edited.contains("<x:c r=\"A1\"><x:v>2</x:v></x:c>"));
        assert!(edited.contains("<x:c r=\"B1\"><x:f>A1+1</x:f></x:c>"));
        assert!(edited.contains("<x:row r=\"4\"><x:c r=\"D4\" t=\"inlineStr\"><x:is><x:t xml:space=\"preserve\">new</x:t></x:is></x:c></x:row>"));
        let loaded = crate::reader::read_workbook(&path).unwrap();
        assert_eq!(
            loaded.sheets[0].get_cell(0, 0).unwrap().value.to_string(),
            "2"
        );
        assert_eq!(
            loaded.sheets[0].get_cell(3, 3).unwrap().value.to_string(),
            "new"
        );
    }

    #[test]
    fn expands_existing_worksheet_dimension() {
        let xml = b"<worksheet><dimension ref=\"B2:C3\"/><sheetData><row r=\"2\"><c r=\"B2\"><v>1</v></c></row></sheetData></worksheet>";
        let targets = BTreeMap::from([(4, BTreeSet::from([4]))]);
        let updated = String::from_utf8(patch_sheet(xml, &targets, "1+2", &[]).unwrap()).unwrap();
        assert!(updated.contains("<dimension ref=\"B2:E5\"/>"));
        assert!(updated.contains("<c r=\"E5\"><f>1+2</f></c>"));
    }

    #[test]
    fn rejects_covered_merged_cells() {
        let xml = b"<worksheet><sheetData/><mergeCells><mergeCell ref=\"A1:B2\"/></mergeCells></worksheet>";
        let covered = BTreeMap::from([(1, BTreeSet::from([1]))]);
        assert!(patch_sheet(xml, &covered, "1+2", &[]).is_err());
        let anchor = BTreeMap::from([(0, BTreeSet::from([0]))]);
        assert!(patch_sheet(xml, &anchor, "1+2", &[]).is_ok());
    }
}
