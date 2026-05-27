use std::path::Path;

use excli_core::error::ExcliResult;
use excli_core::Workbook;

/// Read an ODS file. Currently delegates to calamine via excel_io.
pub fn read_ods(path: &Path) -> ExcliResult<Workbook> {
    // calamine supports ODS natively
    crate::excel_io::read_excel(path)
}

/// Read ODS from bytes.
pub fn read_ods_from_bytes(data: &[u8]) -> ExcliResult<Workbook> {
    crate::excel_io::read_excel_from_bytes(data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_read_ods_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing.ods");
        let r = read_ods(&path);
        assert!(r.is_err());
    }

    #[test]
    fn test_read_ods_invalid_file_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("invalid.ods");
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(b"This is not an ODS file").unwrap();
        drop(f);
        let r = read_ods(&path);
        assert!(r.is_err());
    }

    #[test]
    fn test_read_ods_from_invalid_bytes() {
        let r = read_ods_from_bytes(b"invalid ods data");
        assert!(r.is_err());
    }

    #[test]
    fn test_read_ods_from_empty_bytes() {
        let r = read_ods_from_bytes(b"");
        assert!(r.is_err());
    }

    #[test]
    fn test_read_ods_delegates_to_excel_io() {
        // ODS format uses same calamine auto-detect as excel
        // This test simply verifies the delegation works (error handling)
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("notreal.ods");
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(b"Not a real ODS file").unwrap();
        drop(f);
        // Should fail on invalid format, not panic
        let result = read_ods(&path);
        assert!(result.is_err());
    }
}
