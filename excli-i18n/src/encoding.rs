/// Information about detected encoding.
#[derive(Debug, Clone)]
pub struct EncodingInfo {
    pub encoding: String,
    pub confidence: f64,
    pub has_bom: bool,
    pub byte_order_mark: Option<String>,
}

/// Detect encoding from raw bytes.
pub struct EncodingDetector;

impl EncodingDetector {
    pub fn new() -> Self {
        EncodingDetector
    }

    pub fn detect(&self, data: &[u8]) -> EncodingInfo {
        // Check BOM first
        if data.len() >= 3 && data[0] == 0xEF && data[1] == 0xBB && data[2] == 0xBF {
            return EncodingInfo {
                encoding: "utf-8".to_string(),
                confidence: 1.0,
                has_bom: true,
                byte_order_mark: Some("EF BB BF".to_string()),
            };
        }
        if data.len() >= 2 && data[0] == 0xFF && data[1] == 0xFE {
            return EncodingInfo {
                encoding: "utf-16le".to_string(),
                confidence: 1.0,
                has_bom: true,
                byte_order_mark: Some("FF FE".to_string()),
            };
        }
        if data.len() >= 2 && data[0] == 0xFE && data[1] == 0xFF {
            return EncodingInfo {
                encoding: "utf-16be".to_string(),
                confidence: 1.0,
                has_bom: true,
                byte_order_mark: Some("FE FF".to_string()),
            };
        }

        // Try UTF-8 first
        if std::str::from_utf8(data).is_ok() {
            return EncodingInfo {
                encoding: "utf-8".to_string(),
                confidence: 0.95,
                has_bom: false,
                byte_order_mark: None,
            };
        }

        // Try common CJK encodings using encoding_rs
        let candidates = [
            ("gbk", "gbk"),
            ("gb18030", "gb18030"),
            ("big5", "big5"),
            ("shift_jis", "shift_jis"),
            ("euc-jp", "euc-jp"),
            ("euc-kr", "euc-kr"),
            ("windows-1252", "windows-1252"),
            ("windows-1251", "windows-1251"),
            ("windows-1256", "windows-1256"),
            ("windows-874", "windows-874"),
        ];

        let mut best_encoding = "utf-8".to_string();
        let mut best_confidence = 0.5;

        for (enc_name, label) in &candidates {
            if let Some(enc) = encoding_rs::Encoding::for_label(label.as_bytes()) {
                let (decoded, _, had_errors) = enc.decode(data);
                if !had_errors && !decoded.is_empty() {
                    let conf = 0.8;
                    if conf > best_confidence {
                        best_confidence = conf;
                        best_encoding = enc_name.to_string();
                    }
                }
            }
        }

        EncodingInfo {
            encoding: best_encoding,
            confidence: best_confidence,
            has_bom: false,
            byte_order_mark: None,
        }
    }

    pub fn detect_all(&self, data: &[u8]) -> Vec<EncodingInfo> {
        let primary = self.detect(data);
        let mut results = vec![primary];

        let candidates = [
            "utf-8", "gbk", "gb18030", "big5", "shift_jis", "euc-jp",
            "euc-kr", "windows-1252", "windows-1251", "windows-1256",
            "windows-874",
        ];

        for enc_name in &candidates {
            if let Some(enc) = encoding_rs::Encoding::for_label(enc_name.as_bytes()) {
                let (_, _, had_errors) = enc.decode(data);
                let confidence = if had_errors { 0.1 } else { 0.85 };
                if !results.iter().any(|r| r.encoding == *enc_name) {
                    results.push(EncodingInfo {
                        encoding: enc_name.to_string(),
                        confidence,
                        has_bom: false,
                        byte_order_mark: None,
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
}

impl Default for EncodingDetector {
    fn default() -> Self {
        Self::new()
    }
}

/// Convenience function for detecting encoding.
pub fn detect_encoding(data: &[u8]) -> EncodingInfo {
    EncodingDetector::new().detect(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_utf8_bom() {
        let data = b"\xEF\xBB\xBFHello";
        let info = EncodingDetector::new().detect(data);
        assert_eq!(info.encoding, "utf-8");
        assert!(info.has_bom);
    }

    #[test]
    fn test_detect_utf16le_bom() {
        let data = b"\xFF\xFE\x48\x00";
        let info = EncodingDetector::new().detect(data);
        assert_eq!(info.encoding, "utf-16le");
    }

    #[test]
    fn test_detect_utf8() {
        let data = b"Hello World!";
        let info = EncodingDetector::new().detect(data);
        assert_eq!(info.encoding, "utf-8");
    }

    #[test]
    fn test_detect_all() {
        let data = "Hello World!".as_bytes();
        let results = EncodingDetector::new().detect_all(data);
        assert!(!results.is_empty());
    }

    #[test]
    fn test_detect_empty() {
        let data = b"";
        let info = EncodingDetector::new().detect(data);
        assert!(!info.encoding.is_empty());
    }

    #[test]
    fn test_detect_utf16be_bom() {
        let data = b"\xFE\xFF\x00\x48";
        let info = EncodingDetector::new().detect(data);
        assert_eq!(info.encoding, "utf-16be");
    }

    #[test]
    fn test_detect_all_utf8() {
        let data = "Hello, World! 123".as_bytes();
        let results = EncodingDetector::new().detect_all(data);
        assert!(results.iter().any(|r| r.encoding == "utf-8"));
    }

    #[test]
    fn test_detect_global_function() {
        let data = b"Simple ASCII text";
        let info = detect_encoding(data);
        assert_eq!(info.encoding, "utf-8");
    }
}
