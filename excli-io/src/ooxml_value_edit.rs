//! Typed cell edits in existing XLSX/XLSM packages without rebuilding other parts.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

use excli_core::cell::CellValue;
use excli_core::error::{ErrorCode, ExcliError, ExcliResult};
use excli_core::reference::CellReference;
use excli_core::types::FileFormat;
use zip::ZipArchive;

use crate::format_detect::detect_format;
use crate::ooxml_edit::{
    ensure_editable, read_shared_strings, recalculation_xml, write_modified_parts, xml_escape,
    SharedString,
};
use crate::ooxml_formula_edit::{next_append_row, patch_sheet_with, safe_existing_opening};
use crate::ooxml_package::{index, read_part};

fn unsupported(message: impl Into<String>) -> ExcliError {
    ExcliError::new(ErrorCode::NotImplemented, message)
}

fn validate_value(value: &CellValue) -> ExcliResult<()> {
    match value {
        CellValue::Empty | CellValue::Boolean(_) | CellValue::Error(_) => Ok(()),
        CellValue::Number(number) if number.is_finite() => Ok(()),
        CellValue::InlineString(value)
            if value.chars().all(|ch| {
                matches!(ch as u32, 0x9 | 0xA | 0xD | 0x20..=0xD7FF | 0xE000..=0xFFFD | 0x10000..=0x10FFFF)
            }) => Ok(()),
        CellValue::Number(_) => Err(ExcliError::new(
            ErrorCode::WriteError,
            "Excel cells cannot contain NaN or infinity",
        )),
        CellValue::InlineString(_) => Err(ExcliError::new(
            ErrorCode::WriteError,
            "Cell text contains an invalid XML character",
        )),
        _ => Err(unsupported(
            "Cell values with shared-string indexes, rich text, or range arrays need dedicated editing",
        )),
    }
}

fn render_cell(
    column: u32,
    row: u32,
    existing: Option<&[u8]>,
    prefix: &str,
    value: &CellValue,
    shared_strings: &[SharedString],
) -> ExcliResult<Vec<u8>> {
    let (opening, cell_tag) = match existing {
        Some(xml) => safe_existing_opening(xml, shared_strings)?,
        None => (
            format!(
                "<{prefix}c r=\"{}\"",
                CellReference::new(column, row).to_a1()
            ),
            format!("{prefix}c"),
        ),
    };
    let prefix = cell_tag
        .rsplit_once(':')
        .map_or(String::new(), |(prefix, _)| format!("{prefix}:"));
    let result = match value {
        CellValue::Empty => format!("{opening}/>"),
        CellValue::Boolean(value) => {
            format!("{opening} t=\"b\"><{prefix}v>{}</{prefix}v></{cell_tag}>", u8::from(*value))
        }
        CellValue::Number(value) => format!("{opening}><{prefix}v>{value}</{prefix}v></{cell_tag}>"),
        CellValue::InlineString(value) => format!(
            "{opening} t=\"inlineStr\"><{prefix}is><{prefix}t xml:space=\"preserve\">{}</{prefix}t></{prefix}is></{cell_tag}>",
            xml_escape(value)
        ),
        CellValue::Error(value) => {
            format!("{opening} t=\"e\"><{prefix}v>{value}</{prefix}v></{cell_tag}>")
        }
        _ => return Err(unsupported("This cell value cannot be written to OOXML")),
    };
    Ok(result.into_bytes())
}

/// Return the selected worksheet name and the next zero-based row after its last row element.
pub fn append_position(path: &Path, sheet_name: Option<&str>) -> ExcliResult<(String, u32)> {
    if !matches!(detect_format(path)?, FileFormat::Xlsx | FileFormat::Xlsm) {
        return Err(unsupported(
            "Package-preserving append supports XLSX and XLSM",
        ));
    }
    let mut archive = ZipArchive::new(std::fs::File::open(path)?).map_err(|e| {
        ExcliError::new(
            ErrorCode::FileCorrupted,
            format!("Invalid Excel package: {e}"),
        )
    })?;
    let package = index(&mut archive)?;
    let sheet = if let Some(name) = sheet_name {
        package
            .sheets
            .iter()
            .find(|sheet| sheet.name == name)
            .ok_or_else(|| ExcliError::sheet_not_found(name))?
    } else {
        package.sheets.first().ok_or_else(|| {
            ExcliError::new(ErrorCode::FileCorrupted, "Workbook has no worksheets")
        })?
    };
    let row = next_append_row(&read_part(&mut archive, &sheet.path)?)?;
    Ok((sheet.name.clone(), row))
}

/// Set typed values in one worksheet while retaining all untouched ZIP parts.
/// Returns the number of distinct cells changed.
pub fn set_values(
    path: &Path,
    sheet_name: Option<&str>,
    values: &[((u32, u32), CellValue)],
) -> ExcliResult<usize> {
    if !matches!(detect_format(path)?, FileFormat::Xlsx | FileFormat::Xlsm) {
        return Err(unsupported(
            "Package-preserving value edits support XLSX and XLSM",
        ));
    }
    let mut by_cell = BTreeMap::new();
    let mut targets: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
    for &((column, row), ref value) in values {
        if column >= 16_384 || row >= 1_048_576 {
            return Err(ExcliError::new(
                ErrorCode::RangeInvalid,
                "Cell is outside Excel limits",
            ));
        }
        validate_value(value)?;
        by_cell.insert((column, row), value.clone());
        targets.entry(row).or_default().insert(column);
    }
    if by_cell.is_empty() {
        return Ok(0);
    }

    let mut archive = ZipArchive::new(std::fs::File::open(path)?).map_err(|e| {
        ExcliError::new(
            ErrorCode::FileCorrupted,
            format!("Invalid Excel package: {e}"),
        )
    })?;
    ensure_editable(&mut archive)?;
    let package = index(&mut archive)?;
    let sheet = if let Some(name) = sheet_name {
        package
            .sheets
            .iter()
            .find(|sheet| sheet.name == name)
            .ok_or_else(|| ExcliError::sheet_not_found(name))?
    } else {
        package.sheets.first().ok_or_else(|| {
            ExcliError::new(ErrorCode::FileCorrupted, "Workbook has no worksheets")
        })?
    };
    let shared_strings = match &package.shared_strings_path {
        Some(path) => read_shared_strings(&read_part(&mut archive, path)?)?,
        None => Vec::new(),
    };
    let original_sheet = read_part(&mut archive, &sheet.path)?;
    let patched = patch_sheet_with(
        &original_sheet,
        &targets,
        |column, row, existing, prefix| {
            render_cell(
                column,
                row,
                existing,
                prefix,
                by_cell.get(&(column, row)).ok_or_else(|| {
                    ExcliError::new(ErrorCode::FileCorrupted, "Cell edit target has no value")
                })?,
                &shared_strings,
            )
        },
    )?;
    let workbook_xml = read_part(&mut archive, &package.workbook_path)?;
    let changed = HashMap::from([
        (sheet.path.clone(), patched),
        (package.workbook_path, recalculation_xml(&workbook_xml)?),
    ]);
    write_modified_parts(path, &mut archive, &changed)?;
    Ok(by_cell.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use excli_core::cell::Cell;
    use excli_core::Workbook;

    #[test]
    fn edits_sparse_values_without_rebuilding_other_sheets() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("values.xlsx");
        let mut workbook = Workbook::new();
        workbook
            .add_sheet("Data")
            .set_cell(0, 0, Cell::number(10.0));
        workbook
            .add_sheet("Other")
            .set_cell(0, 0, Cell::string("keep"));
        crate::writer::write_xlsx(&path, &workbook).unwrap();
        assert_eq!(
            append_position(&path, Some("Data")).unwrap(),
            ("Data".to_owned(), 1)
        );
        let other_before = read_part(
            &mut ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap(),
            "xl/worksheets/sheet2.xml",
        )
        .unwrap();

        assert_eq!(
            set_values(
                &path,
                Some("Data"),
                &[
                    ((0, 0), CellValue::Number(12.5)),
                    ((1, 1), CellValue::Boolean(true)),
                    ((2, 2), CellValue::InlineString(" a & b ".into())),
                ]
            )
            .unwrap(),
            3
        );
        let loaded = crate::reader::read_workbook(&path).unwrap();
        let data = loaded.sheet_by_name("Data").unwrap();
        assert_eq!(data.get_cell(0, 0).unwrap().value.to_string(), "12.5");
        assert_eq!(data.get_cell(1, 1).unwrap().value.to_string(), "TRUE");
        assert_eq!(data.get_cell(2, 2).unwrap().value.to_string(), " a & b ");
        let other_after = read_part(
            &mut ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap(),
            "xl/worksheets/sheet2.xml",
        )
        .unwrap();
        assert_eq!(other_after, other_before);
    }

    #[test]
    fn invalid_value_does_not_change_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("values.xlsx");
        let mut workbook = Workbook::new();
        workbook
            .add_sheet("Data")
            .set_cell(0, 0, Cell::number(10.0));
        crate::writer::write_xlsx(&path, &workbook).unwrap();
        let before = std::fs::read(&path).unwrap();
        assert!(set_values(&path, None, &[((0, 0), CellValue::Number(f64::NAN))]).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }

    #[test]
    fn existing_cell_style_survives_value_change() {
        let cell = render_cell(
            0,
            0,
            Some(b"<c r=\"A1\" s=\"7\" t=\"s\"><v>0</v></c>"),
            "",
            &CellValue::Number(42.0),
            &[SharedString {
                text: "old".into(),
                rich: false,
                raw_xml: None,
            }],
        )
        .unwrap();
        assert_eq!(cell, b"<c r=\"A1\" s=\"7\"><v>42</v></c>");
    }
}
