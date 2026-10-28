use std::path::Path;

use excli_core::error::ExcliResult;
use excli_core::types::FileFormat;
use excli_core::Workbook;

use crate::csv_io;
use crate::excel_io;
use crate::format_detect::detect_format;

/// Read a workbook from a file, auto-detecting format.
pub fn read_workbook(path: &Path) -> ExcliResult<Workbook> {
    let format = detect_format(path)?;
    read_workbook_with_format(path, format)
}

/// Read a workbook from a file with explicit format.
pub fn read_workbook_with_format(path: &Path, format: FileFormat) -> ExcliResult<Workbook> {
    let mut workbook = match format {
        FileFormat::Xlsx | FileFormat::Xlsm | FileFormat::Xlsb | FileFormat::Xls => {
            excel_io::read_excel(path)
        }
        FileFormat::Csv | FileFormat::Tsv => {
            let delimiter = if format == FileFormat::Tsv {
                b'\t'
            } else {
                b','
            };
            csv_io::read_csv(path, delimiter)
        }
        FileFormat::Ods => {
            excel_io::read_excel(path) // calamine supports ODS too
        }
        _ => {
            // Fallback: try calamine for all spreadsheet formats
            excel_io::read_excel(path)
        }
    }?;
    workbook.file_format = Some(format);
    Ok(workbook)
}

/// Read a workbook from bytes (in-memory).
pub fn read_workbook_from_bytes(data: &[u8], format: FileFormat) -> ExcliResult<Workbook> {
    let mut workbook = match format {
        FileFormat::Csv | FileFormat::Tsv => {
            let delimiter = if format == FileFormat::Tsv {
                b'\t'
            } else {
                b','
            };
            csv_io::read_csv_from_bytes(data, delimiter)
        }
        _ => excel_io::read_excel_from_bytes_with_format(data, format),
    }?;
    workbook.file_format = Some(format);
    Ok(workbook)
}

/// Read only metadata (sheet names, properties) without loading all data.
pub fn read_info(path: &Path) -> ExcliResult<Workbook> {
    let format = detect_format(path)?;
    let mut wb = read_workbook_with_format(path, format)?;
    wb.file_format = Some(format);

    if let Ok(meta) = std::fs::metadata(path) {
        wb.file_size = meta.len();
    }

    Ok(wb)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_read_csv() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.csv");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, "Name,Age,Score").unwrap();
        writeln!(f, "Alice,30,95.5").unwrap();
        writeln!(f, "Bob,25,87.3").unwrap();

        let wb = read_workbook(&path).unwrap();
        assert_eq!(wb.sheet_count(), 1);
        let sheet = wb.sheet_by_index(0).unwrap();
        assert_eq!(sheet.row_count(), 3);
    }

    #[test]
    fn test_read_tsv() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.tsv");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, "A\tB\tC").unwrap();
        writeln!(f, "1\t2\t3").unwrap();

        let wb = read_workbook(&path).unwrap();
        assert_eq!(wb.sheet_count(), 1);
    }

    #[test]
    fn test_read_csv_single_column() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("single.csv");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, "Name").unwrap();
        writeln!(f, "Alice").unwrap();
        writeln!(f, "Bob").unwrap();

        let wb = read_workbook(&path).unwrap();
        let sheet = wb.sheet_by_index(0).unwrap();
        assert_eq!(sheet.col_count(), 1);
        assert_eq!(sheet.row_count(), 3);
    }

    #[test]
    fn test_read_csv_with_spaces() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("spaces.csv");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, "a,b,c").unwrap();
        writeln!(f, "1, 2 , 3").unwrap();

        let wb = read_workbook(&path).unwrap();
        let sheet = wb.sheet_by_index(0).unwrap();
        assert!(sheet.row_count() >= 2);
    }

    #[test]
    fn test_read_csv_multiple_rows() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("multi.csv");
        let mut f = std::fs::File::create(&path).unwrap();
        for i in 0..10 {
            writeln!(f, "{},{},{}", i, i * 2, i * 3).unwrap();
        }

        let wb = read_workbook(&path).unwrap();
        let sheet = wb.sheet_by_index(0).unwrap();
        assert_eq!(sheet.row_count(), 10);
    }
}
