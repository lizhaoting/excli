//! Recover OOXML rich-text runs that calamine flattens into plain strings.

use excli_core::cell::{Cell, CellValue, RichTextRun};
use excli_core::error::{ErrorCode, ExcliError, ExcliResult};
use excli_core::reference::CellReference;
use excli_core::sheet::Sheet;
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

fn malformed(message: impl Into<String>) -> ExcliError {
    ExcliError::new(ErrorCode::FileCorrupted, message)
}

fn attribute(event: &BytesStart<'_>, key: &[u8]) -> ExcliResult<Option<String>> {
    for item in event.attributes() {
        let item =
            item.map_err(|error| malformed(format!("Invalid rich-text attribute: {error}")))?;
        if item.key.as_ref() == key {
            return Ok(Some(
                item.unescape_value()
                    .map_err(|error| malformed(format!("Invalid rich-text value: {error}")))?
                    .into_owned(),
            ));
        }
    }
    Ok(None)
}

fn empty_run() -> RichTextRun {
    RichTextRun {
        text: String::new(),
        bold: None,
        italic: None,
        underline: None,
        strikethrough: None,
        font_name: None,
        font_size: None,
        color: None,
    }
}

fn flag(event: &BytesStart<'_>) -> ExcliResult<bool> {
    Ok(!matches!(
        attribute(event, b"val")?.as_deref(),
        Some("0" | "false" | "none")
    ))
}

fn run_property(event: &BytesStart<'_>, run: &mut RichTextRun) -> ExcliResult<()> {
    match event.local_name().as_ref() {
        b"b" => run.bold = Some(flag(event)?),
        b"i" => run.italic = Some(flag(event)?),
        b"u" => run.underline = Some(flag(event)?),
        b"strike" => run.strikethrough = Some(flag(event)?),
        b"rFont" => run.font_name = attribute(event, b"val")?,
        b"sz" => {
            run.font_size = attribute(event, b"val")?
                .map(|value| {
                    value
                        .parse::<f64>()
                        .map_err(|_| malformed("Invalid rich-text font size"))
                })
                .transpose()?;
        }
        b"color" => run.color = attribute(event, b"rgb")?,
        _ => {}
    }
    Ok(())
}

fn parse_entries(xml: &[u8], item_name: &[u8]) -> ExcliResult<Vec<Option<Vec<RichTextRun>>>> {
    let mut reader = Reader::from_reader(xml);
    let mut entries = Vec::new();
    let mut in_item = false;
    let mut in_phonetic = false;
    let mut in_properties = false;
    let mut in_text = false;
    let mut runs = Vec::new();
    let mut run: Option<RichTextRun> = None;
    loop {
        match reader
            .read_event()
            .map_err(|error| malformed(format!("Invalid rich-text XML: {error}")))?
        {
            Event::Start(event) if event.local_name().as_ref() == item_name => {
                in_item = true;
                runs.clear();
            }
            Event::Empty(event) if event.local_name().as_ref() == item_name => entries.push(None),
            Event::Start(event) if in_item && event.local_name().as_ref() == b"rPh" => {
                in_phonetic = true
            }
            Event::Start(event)
                if in_item && !in_phonetic && event.local_name().as_ref() == b"r" =>
            {
                run = Some(empty_run());
            }
            Event::Start(event) if run.is_some() && event.local_name().as_ref() == b"rPr" => {
                in_properties = true;
            }
            Event::Start(event) | Event::Empty(event) if in_properties => {
                if let Some(run) = &mut run {
                    run_property(&event, run)?;
                }
            }
            Event::Start(event) if run.is_some() && event.local_name().as_ref() == b"t" => {
                in_text = true;
            }
            Event::Text(event) if in_text => {
                if let Some(run) = &mut run {
                    run.text.push_str(
                        &event
                            .unescape()
                            .map_err(|error| malformed(format!("Invalid rich text: {error}")))?,
                    );
                }
            }
            Event::CData(event) if in_text => {
                if let Some(run) = &mut run {
                    run.text.push_str(
                        &String::from_utf8(event.into_inner().to_vec())
                            .map_err(|error| malformed(format!("Invalid rich text: {error}")))?,
                    );
                }
            }
            Event::End(event) if event.local_name().as_ref() == b"t" => in_text = false,
            Event::End(event) if event.local_name().as_ref() == b"rPr" => in_properties = false,
            Event::End(event) if event.local_name().as_ref() == b"r" && !in_phonetic => {
                if let Some(run) = run.take() {
                    runs.push(run);
                }
            }
            Event::End(event) if event.local_name().as_ref() == b"rPh" => in_phonetic = false,
            Event::End(event) if event.local_name().as_ref() == item_name => {
                in_item = false;
                entries.push((!runs.is_empty()).then(|| std::mem::take(&mut runs)));
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(entries)
}

pub(crate) fn shared_rich_text(xml: &[u8]) -> ExcliResult<Vec<Option<Vec<RichTextRun>>>> {
    parse_entries(xml, b"si")
}

pub(crate) fn enrich_sheet(
    xml: &[u8],
    sheet: &mut Sheet,
    shared: &[Option<Vec<RichTextRun>>],
) -> ExcliResult<()> {
    let mut reader = Reader::from_reader(xml);
    let mut cell: Option<(CellReference, String, usize)> = None;
    let mut current_row = None;
    let mut last_row = 0u32;
    let mut next_column = 0u32;
    let mut in_value = false;
    let mut value = String::new();
    loop {
        let from = reader.buffer_position() as usize;
        let event = reader
            .read_event()
            .map_err(|error| malformed(format!("Invalid worksheet XML: {error}")))?;
        let to = reader.buffer_position() as usize;
        match event {
            Event::Start(event) | Event::Empty(event) if event.local_name().as_ref() == b"row" => {
                let row = attribute(&event, b"r")?
                    .map(|number| {
                        number
                            .parse::<u32>()
                            .map_err(|_| malformed("Invalid row number"))
                    })
                    .transpose()?
                    .unwrap_or(last_row + 1);
                if row == 0 || row > 1_048_576 {
                    return Err(malformed("Row number outside Excel limits"));
                }
                last_row = row;
                current_row = Some(row - 1);
                next_column = 0;
            }
            Event::Start(event) if event.local_name().as_ref() == b"c" => {
                let reference = if let Some(reference) = attribute(&event, b"r")? {
                    CellReference::from_a1(&reference)
                        .ok_or_else(|| malformed(format!("Invalid cell reference: {reference}")))?
                } else {
                    CellReference::new(
                        next_column,
                        current_row.ok_or_else(|| malformed("Cell has no row context"))?,
                    )
                };
                if reference.column >= 16_384 || reference.row >= 1_048_576 {
                    return Err(malformed("Cell reference outside Excel limits"));
                }
                next_column = reference.column + 1;
                cell = Some((
                    reference,
                    attribute(&event, b"t")?.unwrap_or_default(),
                    from,
                ));
                value.clear();
            }
            Event::Empty(event) if event.local_name().as_ref() == b"c" => {
                let column = if let Some(reference) = attribute(&event, b"r")? {
                    CellReference::from_a1(&reference)
                        .ok_or_else(|| malformed(format!("Invalid cell reference: {reference}")))?
                        .column
                } else {
                    next_column
                };
                if column >= 16_384 {
                    return Err(malformed("Cell column outside Excel limits"));
                }
                next_column = column + 1;
            }
            Event::Start(event) if cell.is_some() && event.local_name().as_ref() == b"v" => {
                in_value = true
            }
            Event::Text(event) if in_value => {
                value.push_str(
                    &event.unescape().map_err(|error| {
                        malformed(format!("Invalid shared-string index: {error}"))
                    })?,
                );
            }
            Event::End(event) if event.local_name().as_ref() == b"v" => in_value = false,
            Event::End(event) if event.local_name().as_ref() == b"c" => {
                if let Some((reference, kind, start)) = cell.take() {
                    let rich = if kind == "s" {
                        let index = value
                            .parse::<usize>()
                            .map_err(|_| malformed("Invalid shared-string index"))?;
                        shared
                            .get(index)
                            .ok_or_else(|| malformed("Shared-string index out of range"))?
                            .clone()
                    } else if kind == "inlineStr" {
                        parse_entries(&xml[start..to], b"is")?
                            .into_iter()
                            .next()
                            .flatten()
                    } else {
                        None
                    };
                    if let Some(runs) = rich {
                        let mut existing = sheet
                            .get_cell(reference.column, reference.row)
                            .cloned()
                            .unwrap_or_else(Cell::empty);
                        let rich_value = CellValue::RichText(runs);
                        if let Some(formula) = &mut existing.formula {
                            formula.cached_value = Some(rich_value.clone());
                        }
                        existing.value = rich_value;
                        sheet.set_cell(reference.column, reference.row, existing);
                    }
                }
            }
            Event::End(event) if event.local_name().as_ref() == b"row" => current_row = None,
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use excli_core::Workbook;
    use std::collections::HashMap;
    use zip::ZipArchive;

    use crate::ooxml_edit::write_modified_parts;
    use crate::ooxml_package::read_part;

    #[test]
    fn parses_shared_and_inline_rich_text() {
        let shared = br#"<sst><si><t>plain</t></si><si><r><rPr><b/><rFont val="Arial"/><sz val="12"/><color rgb="FFFF0000"/></rPr><t xml:space="preserve"> Red </t></r><r><rPr><i/></rPr><t>blue</t></r></si></sst>"#;
        let entries = shared_rich_text(shared).unwrap();
        assert!(entries[0].is_none());
        let runs = entries[1].as_ref().unwrap();
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].text, " Red ");
        assert_eq!(runs[0].bold, Some(true));
        assert_eq!(runs[0].font_name.as_deref(), Some("Arial"));
        assert_eq!(runs[0].font_size, Some(12.0));
        assert_eq!(runs[1].italic, Some(true));

        let xml = br#"<worksheet><sheetData><row r="1"><c r="A1" t="s"><v>1</v></c><c t="inlineStr"><is><r><rPr><u/></rPr><t>Hi</t></r></is></c></row></sheetData></worksheet>"#;
        let mut sheet = Sheet::new("Data");
        enrich_sheet(xml, &mut sheet, &entries).unwrap();
        assert!(matches!(
            sheet.get_cell(0, 0).unwrap().value,
            CellValue::RichText(_)
        ));
        let CellValue::RichText(inline) = &sheet.get_cell(1, 0).unwrap().value else {
            panic!("Expected rich text");
        };
        assert_eq!(inline[0].text, "Hi");
        assert_eq!(inline[0].underline, Some(true));
    }

    #[test]
    fn workbook_reader_exposes_rich_shared_string_runs() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rich.xlsx");
        let mut workbook = Workbook::new();
        workbook
            .add_sheet("Data")
            .set_cell(0, 0, Cell::string("rich"));
        crate::writer::write_xlsx(&path, &workbook).unwrap();
        {
            let mut archive = ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
            let original =
                String::from_utf8(read_part(&mut archive, "xl/sharedStrings.xml").unwrap())
                    .unwrap();
            let rich = original.replace(
                "<si><t>rich</t></si>",
                "<si><r><rPr><b/></rPr><t>ri</t></r><r><rPr><i/></rPr><t>ch</t></r></si>",
            );
            assert_ne!(rich, original);
            write_modified_parts(
                &path,
                &mut archive,
                &HashMap::from([("xl/sharedStrings.xml".to_owned(), rich.into_bytes())]),
            )
            .unwrap();
        }
        let loaded = crate::reader::read_workbook(&path).unwrap();
        let CellValue::RichText(runs) = &loaded
            .sheet_by_name("Data")
            .unwrap()
            .get_cell(0, 0)
            .unwrap()
            .value
        else {
            panic!("Expected rich text from workbook read");
        };
        assert_eq!(
            runs.iter().map(|run| run.text.as_str()).collect::<String>(),
            "rich"
        );
        assert_eq!(runs[0].bold, Some(true));
        assert_eq!(runs[1].italic, Some(true));
    }
}
