/// Information about detected language in text.
#[derive(Debug, Clone)]
pub struct LanguageInfo {
    pub language: String,
    pub confidence: f64,
    pub script: Option<String>,
}

/// Detect the language of text using character-based heuristics.
pub fn detect_language(text: &str) -> LanguageInfo {
    if text.is_empty() {
        return LanguageInfo {
            language: "und".to_string(),
            confidence: 0.0,
            script: None,
        };
    }

    let chars: Vec<char> = text.chars().collect();
    let total = chars.len() as f64;
    if total == 0.0 {
        return LanguageInfo {
            language: "und".to_string(),
            confidence: 0.0,
            script: None,
        };
    }

    let mut cjk = 0;
    let mut hiragana = 0;
    let mut katakana = 0;
    let mut hangul = 0;
    let mut arabic = 0;
    let mut cyrillic = 0;
    let mut thai = 0;
    let mut latin = 0;

    for &c in &chars {
        match c {
            '\u{4E00}'..='\u{9FFF}' | '\u{3400}'..='\u{4DBF}' | '\u{20000}'..='\u{2A6DF}' => {
                cjk += 1;
            }
            '\u{3040}'..='\u{309F}' => hiragana += 1,
            '\u{30A0}'..='\u{30FF}' | '\u{FF65}'..='\u{FF9F}' => katakana += 1,
            '\u{AC00}'..='\u{D7AF}' => hangul += 1,
            '\u{0600}'..='\u{06FF}' | '\u{0750}'..='\u{077F}' => arabic += 1,
            '\u{0400}'..='\u{04FF}' | '\u{0500}'..='\u{052F}' => cyrillic += 1,
            '\u{0E00}'..='\u{0E7F}' => thai += 1,
            '\u{0041}'..='\u{005A}' | '\u{0061}'..='\u{007A}' => latin += 1,
            _ => {}
        }
    }

    let cjk_ratio = cjk as f64 / total;
    let hiragana_ratio = hiragana as f64 / total;
    let katakana_ratio = katakana as f64 / total;
    let hangul_ratio = hangul as f64 / total;
    let arabic_ratio = arabic as f64 / total;
    let cyrillic_ratio = cyrillic as f64 / total;
    let thai_ratio = thai as f64 / total;

    if hiragana_ratio > 0.1 || katakana_ratio > 0.1 {
        LanguageInfo {
            language: "ja".to_string(),
            confidence: ((hiragana_ratio + katakana_ratio) * 2.0).min(1.0),
            script: Some("Hiragana/Katakana".to_string()),
        }
    } else if hangul_ratio > 0.3 {
        LanguageInfo {
            language: "ko".to_string(),
            confidence: hangul_ratio,
            script: Some("Hangul".to_string()),
        }
    } else if arabic_ratio > 0.3 {
        LanguageInfo {
            language: "ar".to_string(),
            confidence: arabic_ratio,
            script: Some("Arabic".to_string()),
        }
    } else if cyrillic_ratio > 0.3 {
        LanguageInfo {
            language: "ru".to_string(),
            confidence: cyrillic_ratio,
            script: Some("Cyrillic".to_string()),
        }
    } else if thai_ratio > 0.3 {
        LanguageInfo {
            language: "th".to_string(),
            confidence: thai_ratio,
            script: Some("Thai".to_string()),
        }
    } else if cjk_ratio > 0.3 {
        // Can't easily distinguish zh/ja from pure CJK without kana
        LanguageInfo {
            language: "zh".to_string(),
            confidence: cjk_ratio * 0.8,
            script: Some("CJK".to_string()),
        }
    } else if latin > 0 {
        LanguageInfo {
            language: "en".to_string(),
            confidence: (latin as f64 / total).min(0.9),
            script: Some("Latin".to_string()),
        }
    } else {
        LanguageInfo {
            language: "und".to_string(),
            confidence: 0.0,
            script: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_chinese() {
        let info = detect_language("你好世界这是一个测试");
        assert_eq!(info.language, "zh");
        assert!(info.confidence > 0.3);
    }

    #[test]
    fn test_detect_japanese() {
        let info = detect_language("こんにちは世界");
        assert_eq!(info.language, "ja");
        assert!(info.confidence > 0.1);
    }

    #[test]
    fn test_detect_korean() {
        let info = detect_language("안녕하세요");
        assert_eq!(info.language, "ko");
        assert!(info.confidence > 0.3);
    }

    #[test]
    fn test_detect_english() {
        let info = detect_language("Hello World");
        assert_eq!(info.language, "en");
        assert!(info.confidence > 0.0);
    }

    #[test]
    fn test_detect_arabic() {
        let info = detect_language("مرحبا بالعالم");
        assert_eq!(info.language, "ar");
        assert!(info.confidence > 0.3);
    }

    #[test]
    fn test_detect_empty() {
        let info = detect_language("");
        assert_eq!(info.language, "und");
        assert_eq!(info.confidence, 0.0);
    }

    #[test]
    fn test_detect_cyrillic() {
        let info = detect_language("Привет мир");
        assert_eq!(info.language, "ru");
        assert!(info.confidence > 0.3);
    }

    #[test]
    fn test_mixed_language() {
        // Mixed CJK + Latin text — should detect based on dominant script
        let info = detect_language("Hello world 你好");
        // Latin dominates, so should be "en"
        assert!(info.language == "en" || info.language == "zh");
    }
}
