use std::path::Path;

use excli_core::error::{ErrorCode, ExcliError, ExcliResult};
use excli_core::types::FileFormat;

/// Detected encoding info.
#[derive(Debug, Clone)]
pub struct EncodingInfo {
    pub encoding: String,
    pub confidence: f64,
}

/// Detect file format from extension and/or magic bytes.
pub fn detect_format(path: &Path) -> ExcliResult<FileFormat> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

    FileFormat::from_extension(ext)
        .ok_or_else(|| ExcliError::format_not_supported(format!("Unknown extension: .{}", ext)))
}

/// Detect text encoding from raw bytes.
pub fn detect_encoding(data: &[u8]) -> EncodingInfo {
    // Check BOM first
    if data.len() >= 3 && data[0] == 0xEF && data[1] == 0xBB && data[2] == 0xBF {
        return EncodingInfo {
            encoding: "utf-8".to_string(),
            confidence: 1.0,
        };
    }
    if data.len() >= 2 && data[0] == 0xFF && data[1] == 0xFE {
        return EncodingInfo {
            encoding: "utf-16le".to_string(),
            confidence: 1.0,
        };
    }
    if data.len() >= 2 && data[0] == 0xFE && data[1] == 0xFF {
        return EncodingInfo {
            encoding: "utf-16be".to_string(),
            confidence: 1.0,
        };
    }

    // Use chardetng for statistical detection
    let mut detector = chardetng::EncodingDetector::new();
    detector.feed(data, data.len() < 4096);
    let encoding = detector.guess(None, data.len() >= 4096);
    let confidence = if detector.guess_assess(None, data.len() >= 4096).1 {
        0.9
    } else {
        0.5
    };

    EncodingInfo {
        encoding: encoding.name().to_string(),
        confidence,
    }
}

/// Decode bytes to string using the detected encoding.
pub fn decode_bytes(data: &[u8], encoding_hint: Option<&str>) -> ExcliResult<String> {
    // Check BOM
    if data.len() >= 3 && data[0] == 0xEF && data[1] == 0xBB && data[2] == 0xBF {
        return String::from_utf8(data[3..].to_vec())
            .map_err(|e| ExcliError::new(ErrorCode::EncodingNotSupported, e.to_string()));
    }

    let encoding_name = if let Some(hint) = encoding_hint {
        hint.to_string()
    } else {
        detect_encoding(data).encoding
    };

    let encoding =
        encoding_rs::Encoding::for_label(encoding_name.as_bytes()).unwrap_or(encoding_rs::UTF_8);

    let (cow, _actual_encoding, had_errors) = encoding.decode(data);
    if had_errors {
        String::from_utf8(data.to_vec()).map_err(|_| {
            ExcliError::new(
                ErrorCode::EncodingNotSupported,
                "Failed to decode with specified encoding",
            )
        })
    } else {
        Ok(cow.into_owned())
    }
}

/// Encode string to bytes with the specified encoding.
pub fn encode_bytes(text: &str, encoding: &str, add_bom: bool) -> ExcliResult<Vec<u8>> {
    let enc = encoding_rs::Encoding::for_label(encoding.as_bytes()).unwrap_or(encoding_rs::UTF_8);

    let (result, _actual_encoding, _had_errors) = enc.encode(text);

    let mut bytes = Vec::new();
    if add_bom {
        if enc == encoding_rs::UTF_8 {
            bytes.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
        } else if enc == encoding_rs::UTF_16LE {
            bytes.extend_from_slice(&[0xFF, 0xFE]);
        }
    }
    bytes.extend_from_slice(&result);
    Ok(bytes)
}

/// List all possible encodings with their confidence scores.
pub fn detect_encoding_all(data: &[u8]) -> Vec<EncodingInfo> {
    let primary = detect_encoding(data);
    let mut results = vec![primary];

    let candidates = [
        "utf-8",
        "gbk",
        "big5",
        "shift_jis",
        "euc-jp",
        "euc-kr",
        "windows-1252",
        "windows-1251",
        "windows-1256",
        "windows-874",
    ];

    for enc_name in &candidates {
        if let Some(enc) = encoding_rs::Encoding::for_label(enc_name.as_bytes()) {
            let (_, _, had_errors) = enc.decode(data);
            let confidence = if had_errors { 0.1 } else { 0.8 };
            if !results.iter().any(|r| r.encoding == *enc_name) {
                results.push(EncodingInfo {
                    encoding: enc_name.to_string(),
                    confidence,
                });
            }
        }
    }

    results.sort_by(|a, b| {
        b.confidence
            .partial_cmp(&a.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_format_xlsx() {
        let fmt = detect_format(Path::new("test.xlsx")).unwrap();
        assert_eq!(fmt, FileFormat::Xlsx);
    }

    #[test]
    fn test_detect_format_csv() {
        let fmt = detect_format(Path::new("data.csv")).unwrap();
        assert_eq!(fmt, FileFormat::Csv);
    }

    #[test]
    fn test_detect_format_unknown() {
        let result = detect_format(Path::new("file.xyz"));
        assert!(result.is_err());
    }

    #[test]
    fn test_detect_utf8_bom() {
        let data = b"\xEF\xBB\xBFHello, World!";
        let info = detect_encoding(data);
        assert_eq!(info.encoding, "utf-8");
        assert_eq!(info.confidence, 1.0);
    }

    #[test]
    fn test_detect_utf16le_bom() {
        let data = b"\xFF\xFEH\x00e\x00l\x00l\x00o\x00";
        let info = detect_encoding(data);
        assert_eq!(info.encoding, "utf-16le");
    }

    #[test]
    fn test_decode_utf8() {
        let data = "Hello, World!".as_bytes();
        let result = decode_bytes(data, Some("utf-8")).unwrap();
        assert_eq!(result, "Hello, World!");
    }

    #[test]
    fn test_detect_format_csv_ext() {
        let result = detect_format(Path::new("data.csv"));
        assert!(result.is_ok());
    }

    #[test]
    fn test_detect_format_xlsx_ext() {
        let result = detect_format(Path::new("workbook.xlsx"));
        assert!(result.is_ok());
    }

    #[test]
    fn test_detect_format_ods() {
        let result = detect_format(Path::new("sheet.ods"));
        assert!(result.is_ok());
    }

    #[test]
    fn test_detect_format_tsv() {
        let result = detect_format(Path::new("data.tsv"));
        assert!(result.is_ok());
    }

    #[test]
    fn test_detect_encoding_utf16be() {
        let data = b"\xFE\xFF\x00H\x00e\x00l\x00l\x00o";
        let info = detect_encoding(data);
        assert_eq!(info.encoding, "utf-16be");
    }

    #[test]
    fn test_decode_utf8_with_auto_detection() {
        let data = "Simple ASCII".as_bytes();
        let result = decode_bytes(data, None).unwrap();
        assert_eq!(result, "Simple ASCII");
    }
}
