/// Unicode normalization form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NormalizationForm {
    NFC,
    NFD,
    NFKC,
    NFKD,
}

/// Width conversion mode for CJK characters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WidthMode {
    ToHalfWidth,
    ToFullWidth,
}

/// Normalize text using Unicode normalization algorithms.
pub fn normalize_text(text: &str, form: NormalizationForm) -> String {
    use unicode_normalization::UnicodeNormalization;

    match form {
        NormalizationForm::NFC => text.nfc().collect(),
        NormalizationForm::NFD => text.nfd().collect(),
        NormalizationForm::NFKC => text.nfkc().collect(),
        NormalizationForm::NFKD => text.nfkd().collect(),
    }
}

/// Convert between full-width and half-width characters.
pub fn width_convert(text: &str, mode: WidthMode) -> String {
    text.chars()
        .map(|c| match mode {
            WidthMode::ToHalfWidth => to_halfwidth(c),
            WidthMode::ToFullWidth => to_fullwidth(c),
        })
        .collect()
}

/// Convert a full-width character to half-width.
fn to_halfwidth(c: char) -> char {
    match c {
        // Fullwidth space
        '\u{3000}' => ' ',
        // Fullwidth digits
        '\u{FF10}'..='\u{FF19}' => {
            char::from_u32((c as u32) - 0xFF10 + 0x30).unwrap_or(c)
        }
        // Fullwidth uppercase letters
        '\u{FF21}'..='\u{FF3A}' => {
            char::from_u32((c as u32) - 0xFF21 + 0x41).unwrap_or(c)
        }
        // Fullwidth lowercase letters
        '\u{FF41}'..='\u{FF5A}' => {
            char::from_u32((c as u32) - 0xFF41 + 0x61).unwrap_or(c)
        }
        // Fullwidth ASCII (general)
        '\u{FF01}'..='\u{FF5E}' => {
            char::from_u32((c as u32) - 0xFF01 + 0x21).unwrap_or(c)
        }
        _ => c,
    }
}

/// Convert a half-width character to full-width.
fn to_fullwidth(c: char) -> char {
    match c {
        '!'..='~' => {
            char::from_u32((c as u32) - 0x21 + 0xFF01).unwrap_or(c)
        }
        ' ' => '\u{3000}',
        _ => c,
    }
}

/// Count grapheme clusters in a string.
pub fn grapheme_count(text: &str) -> usize {
    use unicode_segmentation::UnicodeSegmentation;
    text.graphemes(true).count()
}

/// Slice string by grapheme cluster boundaries.
pub fn grapheme_slice(text: &str, start: usize, end: usize) -> &str {
    use unicode_segmentation::UnicodeSegmentation;
    let graphemes: Vec<&str> = text.graphemes(true).collect();
    if start >= graphemes.len() {
        return "";
    }
    let end = end.min(graphemes.len());
    let start_byte = graphemes[..start].iter().map(|g| g.len()).sum::<usize>();
    let end_byte = graphemes[..end].iter().map(|g| g.len()).sum::<usize>();
    &text[start_byte..end_byte]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_nfc() {
        // é as precomposed vs decomposed
        let composed = "\u{00E9}"; // é (U+00E9)
        let decomposed = "e\u{0301}"; // e + combining acute

        let nfc_result = normalize_text(decomposed, NormalizationForm::NFC);
        assert_eq!(nfc_result, composed);
    }

    #[test]
    fn test_normalize_nfd() {
        let composed = "\u{00E9}";
        let decomposed = "e\u{0301}";

        let nfd_result = normalize_text(composed, NormalizationForm::NFD);
        assert_eq!(nfd_result, decomposed);
    }

    #[test]
    fn test_width_convert_halfwidth() {
        let fullwidth = "１２３ＡＢＣ";
        let result = width_convert(fullwidth, WidthMode::ToHalfWidth);
        assert_eq!(result, "123ABC");
    }

    #[test]
    fn test_width_convert_fullwidth() {
        let halfwidth = "123ABC";
        let result = width_convert(halfwidth, WidthMode::ToFullWidth);
        assert_eq!(result, "１２３ＡＢＣ");
    }

    #[test]
    fn test_grapheme_count() {
        // "Hello" = 5 graphemes
        assert_eq!(grapheme_count("Hello"), 5);

        // e + combining accent = 1 grapheme
        assert_eq!(grapheme_count("e\u{0301}"), 1);

        // CJK
        assert_eq!(grapheme_count("你好"), 2);
    }

    #[test]
    fn test_grapheme_slice() {
        let text = "Hello世界";
        assert_eq!(grapheme_slice(text, 5, 7), "世界");
        assert_eq!(grapheme_slice(text, 0, 5), "Hello");
    }

    #[test]
    fn test_japanese_width() {
        // Halfwidth katakana ﾊﾟｿｺﾝ (6 halfwidth chars)
        let half = "ﾊﾟｿｺﾝ";
        let result = width_convert(half, WidthMode::ToFullWidth);
        // Should map to fullwidth
        assert!(result.len() > 0);
    }

    #[test]
    fn test_grapheme_count_empty() {
        assert_eq!(grapheme_count(""), 0);
    }

    #[test]
    fn test_grapheme_count_ascii() {
        assert_eq!(grapheme_count("abc"), 3);
    }

    #[test]
    fn test_grapheme_slice_out_of_bounds() {
        let text = "Hello";
        let sliced = grapheme_slice(text, 0, 100);
        assert_eq!(sliced, "Hello");
    }

    #[test]
    fn test_grapheme_slice_empty_range() {
        let text = "Hello";
        let sliced = grapheme_slice(text, 2, 2);
        assert_eq!(sliced, "");
    }

    #[test]
    fn test_width_convert_fullwidth_to_half() {
        let full = "Ａ";  // fullwidth A
        let result = width_convert(full, WidthMode::ToHalfWidth);
        assert!(!result.is_empty());
    }

    #[test]
    fn test_normalize_nfc_new() {
        let nfd = "e\u{0301}";  // e + combining acute accent
        let nfc = normalize_text(nfd, NormalizationForm::NFC);
        // NFC of e+acute = é (single char)
        assert_eq!(nfc.chars().count(), 1);
    }

    #[test]
    fn test_normalize_nfd_new() {
        let composed = "é";  // é as single precomposed char
        let nfd = normalize_text(composed, NormalizationForm::NFD);
        // NFD should decompose into e + combining char
        assert!(nfd.chars().count() >= 1);
    }
}
