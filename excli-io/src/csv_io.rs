use std::io::Read;
use std::path::Path;

#[allow(unused_imports)]
use excli_core::cell::{Cell, CellValue};
use excli_core::error::{ExcliError, ExcliResult};
use excli_core::Workbook;

use crate::format_detect;

/// Read a CSV file and return a Workbook.
pub fn read_csv(path: &Path, delimiter: u8) -> ExcliResult<Workbook> {
    let mut file = std::fs::File::open(path)?;
    let mut data = Vec::new();
    file.read_to_end(&mut data)?;
    read_csv_from_bytes(&data, delimiter)
}

/// Read CSV from bytes.
pub fn read_csv_from_bytes(data: &[u8], delimiter: u8) -> ExcliResult<Workbook> {
    let encoding_info = format_detect::detect_encoding(data);
    let text = format_detect::decode_bytes(data, Some(&encoding_info.encoding))?;

    let mut wb = Workbook::new();
    let sheet_name = "Sheet1";
    let sheet = wb.add_sheet(sheet_name);

    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .has_headers(false)
        .flexible(true)
        .from_reader(text.as_bytes());

    for result in reader.records() {
        let record = result.map_err(|e| ExcliError::Parse(e.to_string()))?;
        let row_idx = sheet.row_count() as u32;
        let cells: Vec<Cell> = record
            .iter()
            .map(|field| {
                let field = field.trim();
                if field.is_empty() {
                    Cell::empty()
                } else if let Ok(n) = field.parse::<f64>() {
                    Cell::number(n)
                } else if field.eq_ignore_ascii_case("true") {
                    Cell::boolean(true)
                } else if field.eq_ignore_ascii_case("false") {
                    Cell::boolean(false)
                } else {
                    Cell::string(field)
                }
            })
            .collect();

        for (col_idx, cell) in cells.into_iter().enumerate() {
            sheet.set_cell(col_idx as u32, row_idx, cell);
        }
    }

    Ok(wb)
}

/// Parse UTF-8 CSV text passed directly to a command, retaining quoted whitespace.
pub fn parse_inline_csv(data: &str, delimiter: u8) -> ExcliResult<Vec<Vec<Cell>>> {
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .has_headers(false)
        .flexible(true)
        .from_reader(data.as_bytes());
    reader
        .records()
        .map(|record| {
            let record = record.map_err(|error| ExcliError::Parse(error.to_string()))?;
            Ok(record
                .iter()
                .map(|value| {
                    let trimmed = value.trim();
                    trimmed
                        .parse::<f64>()
                        .map_or_else(|_| Cell::string(value), Cell::number)
                })
                .collect())
        })
        .collect()
}

/// Auto-detect CSV dialect (delimiter, quote char).
pub fn detect_csv_dialect(first_lines: &str) -> CsvDialect {
    let delimiters = [(b',', 0u32), (b';', 0), (b'\t', 0), (b'|', 0)];

    let mut scores: Vec<(u8, u32)> = delimiters.iter().map(|&(d, _)| (d, 0)).collect();

    for line in first_lines.lines().take(5) {
        for (delim, score) in &mut scores {
            let count = line.bytes().filter(|&b| b == *delim).count();
            *score += count as u32;
        }
    }

    // Pick delimiter with most occurrences and consistent counts
    scores.sort_by_key(|&(_, s)| -(s as i32));
    let best_delim = if scores.is_empty() || scores[0].1 == 0 {
        b','
    } else {
        scores[0].0
    };

    // Detect quote character
    let has_double_quote = first_lines.lines().any(|l| l.contains('"'));
    let quote = if has_double_quote { b'"' } else { b'\0' };

    CsvDialect {
        delimiter: best_delim,
        quote,
        has_header: true, // Default assumption
    }
}

#[derive(Debug, Clone)]
pub struct CsvDialect {
    pub delimiter: u8,
    pub quote: u8,
    pub has_header: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_csv_comma() {
        let data = b"Name,Age,Score\nAlice,30,95.5\nBob,25,87.3\n";
        let wb = read_csv_from_bytes(data, b',').unwrap();
        let sheet = wb.sheet_by_index(0).unwrap();
        assert_eq!(sheet.row_count(), 3);
        assert_eq!(sheet.col_count(), 3);
    }

    #[test]
    fn test_read_csv_tab() {
        let data = b"A\tB\tC\n1\t2\t3\n";
        let wb = read_csv_from_bytes(data, b'\t').unwrap();
        let sheet = wb.sheet_by_index(0).unwrap();
        assert_eq!(sheet.row_count(), 2);
    }

    #[test]
    fn test_read_csv_empty_fields() {
        let data = b"a,,c\n1,2,\n";
        let wb = read_csv_from_bytes(data, b',').unwrap();
        let sheet = wb.sheet_by_index(0).unwrap();
        assert_eq!(sheet.row_count(), 2);
    }

    #[test]
    fn test_read_csv_quoted() {
        let data = b"\"Name\",\"Description\"\n\"Alice\",\"Hello, World\"\n";
        let wb = read_csv_from_bytes(data, b',').unwrap();
        let sheet = wb.sheet_by_index(0).unwrap();
        assert_eq!(sheet.row_count(), 2);
    }

    #[test]
    fn test_detect_csv_dialect_comma() {
        let text = "Name,Age,Score\nAlice,30,95\nBob,25,87\n";
        let dialect = detect_csv_dialect(text);
        assert_eq!(dialect.delimiter, b',');
    }

    #[test]
    fn test_detect_csv_dialect_tab() {
        let text = "Name\tAge\tScore\nAlice\t30\t95\n";
        let dialect = detect_csv_dialect(text);
        assert_eq!(dialect.delimiter, b'\t');
    }

    #[test]
    fn test_number_detection() {
        let data = b"Values\n123\n-45.67\n0\n";
        let wb = read_csv_from_bytes(data, b',').unwrap();
        let sheet = wb.sheet_by_index(0).unwrap();
        assert!(matches!(
            sheet.get_cell(0, 1).unwrap().value,
            CellValue::Number(_)
        ));
    }

    #[test]
    fn test_read_csv_semicolon_delimiter() {
        let data = b"a;b;c\n1;2;3\n";
        let wb = read_csv_from_bytes(data, b';').unwrap();
        let sheet = wb.sheet_by_index(0).unwrap();
        assert_eq!(sheet.col_count(), 3);
    }

    #[test]
    fn test_read_csv_single_column() {
        let data = b"Value\n1\n2\n3\n";
        let wb = read_csv_from_bytes(data, b',').unwrap();
        let sheet = wb.sheet_by_index(0).unwrap();
        assert_eq!(sheet.row_count(), 4);
        assert_eq!(sheet.col_count(), 1);
    }

    #[test]
    fn test_detect_csv_dialect_semicolon() {
        let text = "a;b;c\n1;2;3\n";
        let dialect = detect_csv_dialect(text);
        assert_eq!(dialect.delimiter, b';');
    }

    #[test]
    fn test_write_csv_and_read_back() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let mut wb = excli_core::Workbook::new();
        let sheet = wb.add_sheet("Sheet1");
        sheet.set_cell(0, 0, excli_core::cell::Cell::string("Name"));
        sheet.set_cell(1, 0, excli_core::cell::Cell::string("Age"));
        sheet.set_cell(0, 1, excli_core::cell::Cell::string("Alice"));
        sheet.set_cell(1, 1, excli_core::cell::Cell::number(30.0));

        crate::writer::write_csv(tmp.path(), &wb, b',').unwrap();
        let wb2 = read_csv(tmp.path(), b',').unwrap();
        let sheet2 = wb2.sheet_by_index(0).unwrap();
        assert_eq!(sheet2.row_count(), 2);
    }

    #[test]
    fn test_read_csv_boolean_values() {
        let data = b"Flag\ntrue\nfalse\nTRUE\n";
        let wb = read_csv_from_bytes(data, b',').unwrap();
        let sheet = wb.sheet_by_index(0).unwrap();
        assert!(matches!(
            sheet.get_cell(0, 1).unwrap().value,
            CellValue::Boolean(_)
        ));
    }

    #[test]
    fn test_read_csv_empty_fields_2() {
        let data = b"A,B,C\n1,,3\n,2,\n";
        let wb = read_csv_from_bytes(data, b',').unwrap();
        let sheet = wb.sheet_by_index(0).unwrap();
        assert_eq!(sheet.row_count(), 3); // header + 2 data rows
    }

    #[test]
    fn test_read_csv_quoted_fields() {
        let data = b"Name,Note\n\"Smith, John\",\"says hi\"\n";
        let wb = read_csv_from_bytes(data, b',').unwrap();
        let sheet = wb.sheet_by_index(0).unwrap();
        let name = sheet.get_cell(0, 1).unwrap().value.as_string().unwrap();
        assert_eq!(name, "Smith, John");
    }

    #[test]
    fn test_detect_csv_dialect_tab_new() {
        let text = "a\tb\tc\n1\t2\t3\n";
        let dialect = detect_csv_dialect(text);
        assert_eq!(dialect.delimiter, b'\t');
    }

    #[test]
    fn test_read_csv_number_parsing() {
        let data = b"Amount\n123.45\n-67.89\n0\n";
        let wb = read_csv_from_bytes(data, b',').unwrap();
        let sheet = wb.sheet_by_index(0).unwrap();
        assert!(matches!(
            sheet.get_cell(0, 1).unwrap().value,
            CellValue::Number(_)
        ));
    }
}
