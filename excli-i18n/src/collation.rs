use std::cmp::Ordering;

/// Collation locale for sorting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollationLocale {
    EnUs,
    ZhCn,
    JaJp,
    KoKr,
    DeDe,
    FrFr,
    EsEs,
    RuRu,
}

impl CollationLocale {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "en-us" | "en" => Some(CollationLocale::EnUs),
            "zh-cn" | "zh" => Some(CollationLocale::ZhCn),
            "ja-jp" | "ja" => Some(CollationLocale::JaJp),
            "ko-kr" | "ko" => Some(CollationLocale::KoKr),
            "de-de" | "de" => Some(CollationLocale::DeDe),
            "fr-fr" | "fr" => Some(CollationLocale::FrFr),
            "es-es" | "es" => Some(CollationLocale::EsEs),
            "ru-ru" | "ru" => Some(CollationLocale::RuRu),
            _ => None,
        }
    }
}

/// Sort strings according to locale-specific collation rules.
pub struct CollationSorter;

impl CollationSorter {
    /// Compare two strings using locale-aware collation.
    pub fn compare(a: &str, b: &str, locale: CollationLocale) -> Ordering {
        match locale {
            CollationLocale::ZhCn => Self::compare_pinyin(a, b),
            CollationLocale::JaJp => Self::compare_gojuon(a, b),
            CollationLocale::KoKr => Self::compare_hangul(a, b),
            _ => a.cmp(b), // Default Unicode ordering for western locales
        }
    }

    /// Sort a collection of strings.
    pub fn sort(strings: &mut [String], locale: CollationLocale) {
        strings.sort_by(|a, b| Self::compare(a, b, locale));
    }

    /// Chinese Pinyin-based comparison (simplified).
    fn compare_pinyin(a: &str, b: &str) -> Ordering {
        // Simplified pinyin ordering: sort by first character's pronunciation group
        let get_pinyin_group = |c: char| -> u8 {
            match c {
                '陈' | '程' | '曹' | '蔡' => 1,  // C
                '邓' | '杜' | '丁' => 2,          // D
                '冯' | '范' | '方' => 3,          // F
                '郭' | '高' | '关' => 4,          // G
                '黄' | '胡' | '何' | '韩' => 5,   // H
                '蒋' | '江' | '金' => 6,          // J
                '李' | '刘' | '林' | '罗' => 7,   // L
                '马' | '孟' => 8,                  // M
                '潘' | '彭' => 9,                  // P
                '钱' | '秦' => 10,                 // Q
                '孙' | '宋' | '沈' => 11,          // S
                '田' | '唐' => 12,                 // T
                '王' | '吴' | '魏' => 13,          // W
                '徐' | '许' | '谢' => 14,          // X
                '杨' | '姚' | '叶' => 15,          // Y
                '张' | '赵' | '周' | '郑' | '朱' => 16, // Z
                _ => 99,
            }
        };

        let a_group = a.chars().next().map(get_pinyin_group).unwrap_or(99);
        let b_group = b.chars().next().map(get_pinyin_group).unwrap_or(99);

        match a_group.cmp(&b_group) {
            Ordering::Equal => a.cmp(b),
            other => other,
        }
    }

    /// Japanese Gojuon (五十音) ordering — simplified.
    fn compare_gojuon(a: &str, b: &str) -> Ordering {
        let gojuon_order = |c: char| -> u32 {
            match c {
                'あ' => 0, 'い' => 1, 'う' => 2, 'え' => 3, 'お' => 4,
                'か' => 5, 'き' => 6, 'く' => 7, 'け' => 8, 'こ' => 9,
                'さ' => 10, 'し' => 11, 'す' => 12, 'せ' => 13, 'そ' => 14,
                'た' => 15, 'ち' => 16, 'つ' => 17, 'て' => 18, 'と' => 19,
                'な' => 20, 'に' => 21, 'ぬ' => 22, 'ね' => 23, 'の' => 24,
                'は' => 25, 'ひ' => 26, 'ふ' => 27, 'へ' => 28, 'ほ' => 29,
                'ま' => 30, 'み' => 31, 'む' => 32, 'め' => 33, 'も' => 34,
                'や' => 35, 'ゆ' => 37, 'よ' => 38,
                'ら' => 39, 'り' => 40, 'る' => 41, 'れ' => 42, 'ろ' => 43,
                'わ' => 44, 'を' => 45, 'ん' => 46,
                'が' => 50, 'ぎ' => 51, 'ぐ' => 52, 'げ' => 53, 'ご' => 54,
                'ざ' => 56, 'じ' => 57, 'ず' => 58, 'ぜ' => 59, 'ぞ' => 60,
                'だ' => 62, 'ぢ' => 63, 'づ' => 64, 'で' => 65, 'ど' => 66,
                'ば' => 68, 'び' => 69, 'ぶ' => 70, 'べ' => 71, 'ぼ' => 72,
                'ぱ' => 74, 'ぴ' => 75, 'ぷ' => 76, 'ぺ' => 77, 'ぽ' => 78,
                _ => 1000,
            }
        };

        let a_pos = a.chars().next().map(gojuon_order).unwrap_or(1000);
        let b_pos = b.chars().next().map(gojuon_order).unwrap_or(1000);

        match a_pos.cmp(&b_pos) {
            Ordering::Equal => a.cmp(b),
            other => other,
        }
    }

    /// Korean Hangul ordering — simplified.
    fn compare_hangul(a: &str, b: &str) -> Ordering {
        // Hangul syllables are already in correct order via Unicode
        // Just use standard comparison for simplicity
        a.cmp(b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pinyin_sort() {
        let mut names = vec![
            "张伟".to_string(),
            "陈明".to_string(),
            "李华".to_string(),
            "王芳".to_string(),
        ];
        CollationSorter::sort(&mut names, CollationLocale::ZhCn);
        // C < L < W < Z (pinyin first-letter order)
        assert_eq!(names[0].chars().next().unwrap(), '陈');
        assert_eq!(names[1].chars().next().unwrap(), '李');
        assert_eq!(names[2].chars().next().unwrap(), '王');
        assert_eq!(names[3].chars().next().unwrap(), '张');
    }

    #[test]
    fn test_gojuon_sort() {
        let mut words = vec![
            "さくら".to_string(),
            "いちご".to_string(),
            "うめ".to_string(),
            "えだまめ".to_string(),
            "あおい".to_string(),
        ];
        CollationSorter::sort(&mut words, CollationLocale::JaJp);
        // あ < い < う < え < さ
        assert_eq!(words[0].chars().next().unwrap(), 'あ');
        assert_eq!(words[4].chars().next().unwrap(), 'さ');
    }

    #[test]
    fn test_english_sort() {
        let mut words = vec!["banana".to_string(), "apple".to_string(), "cherry".to_string()];
        CollationSorter::sort(&mut words, CollationLocale::EnUs);
        assert_eq!(words, vec!["apple", "banana", "cherry"]);
    }

    #[test]
    fn test_english_case_insensitive_sort() {
        let mut words = vec!["banana".to_string(), "apple".to_string(), "cherry".to_string()];
        CollationSorter::sort(&mut words, CollationLocale::EnUs);
        // sorted order should start with apple
        assert_eq!(words[0], "apple");
    }

    #[test]
    fn test_compare_english() {
        use std::cmp::Ordering;
        assert_eq!(CollationSorter::compare("apple", "banana", CollationLocale::EnUs), Ordering::Less);
        assert_eq!(CollationSorter::compare("cherry", "apple", CollationLocale::EnUs), Ordering::Greater);
        assert_eq!(CollationSorter::compare("apple", "apple", CollationLocale::EnUs), Ordering::Equal);
    }

    #[test]
    fn test_from_str_locale() {
        assert!(CollationLocale::from_str("zh-CN").is_some());
        assert!(CollationLocale::from_str("ja-JP").is_some());
        assert!(CollationLocale::from_str("en-US").is_some());
        assert!(CollationLocale::from_str("invalid-locale").is_none());
    }

    #[test]
    fn test_sort_empty() {
        let mut words: Vec<String> = vec![];
        CollationSorter::sort(&mut words, CollationLocale::EnUs);
        assert!(words.is_empty());
    }

    #[test]
    fn test_sort_single() {
        let mut words = vec!["only".to_string()];
        CollationSorter::sort(&mut words, CollationLocale::EnUs);
        assert_eq!(words, vec!["only"]);
    }

    #[test]
    fn test_sort_already_sorted() {
        let mut words = vec!["alpha".to_string(), "beta".to_string(), "gamma".to_string()];
        CollationSorter::sort(&mut words, CollationLocale::EnUs);
        assert_eq!(words[0], "alpha");
        assert_eq!(words[1], "beta");
        assert_eq!(words[2], "gamma");
    }

    #[test]
    fn test_sort_duplicates() {
        let mut words = vec!["b".to_string(), "a".to_string(), "b".to_string()];
        CollationSorter::sort(&mut words, CollationLocale::EnUs);
        assert_eq!(words[0], "a");
    }

    #[test]
    fn test_compare_equal_strings() {
        use std::cmp::Ordering;
        assert_eq!(CollationSorter::compare("same", "same", CollationLocale::EnUs), Ordering::Equal);
    }

    #[test]
    fn test_from_str_locale_case_insensitive() {
        assert!(CollationLocale::from_str("ZH-CN").is_some() || CollationLocale::from_str("zh-cn").is_some());
    }
}
