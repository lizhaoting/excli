/// Built-in Excel table style themes.
pub const TABLE_STYLES: &[&str] = &[
    "TableStyleLight1",
    "TableStyleLight2",
    "TableStyleLight3",
    "TableStyleLight4",
    "TableStyleLight5",
    "TableStyleLight6",
    "TableStyleLight7",
    "TableStyleLight8",
    "TableStyleLight9",
    "TableStyleLight10",
    "TableStyleLight11",
    "TableStyleLight12",
    "TableStyleLight13",
    "TableStyleLight14",
    "TableStyleLight15",
    "TableStyleLight16",
    "TableStyleLight17",
    "TableStyleLight18",
    "TableStyleLight19",
    "TableStyleLight20",
    "TableStyleLight21",
    "TableStyleMedium1",
    "TableStyleMedium2",
    "TableStyleMedium3",
    "TableStyleMedium4",
    "TableStyleMedium5",
    "TableStyleMedium6",
    "TableStyleMedium7",
    "TableStyleMedium8",
    "TableStyleMedium9",
    "TableStyleMedium10",
    "TableStyleMedium11",
    "TableStyleMedium12",
    "TableStyleMedium13",
    "TableStyleMedium14",
    "TableStyleMedium15",
    "TableStyleMedium16",
    "TableStyleMedium17",
    "TableStyleMedium18",
    "TableStyleMedium19",
    "TableStyleMedium20",
    "TableStyleMedium21",
    "TableStyleMedium22",
    "TableStyleMedium23",
    "TableStyleMedium24",
    "TableStyleMedium25",
    "TableStyleMedium26",
    "TableStyleMedium27",
    "TableStyleMedium28",
    "TableStyleDark1",
    "TableStyleDark2",
    "TableStyleDark3",
    "TableStyleDark4",
    "TableStyleDark5",
    "TableStyleDark6",
    "TableStyleDark7",
    "TableStyleDark8",
    "TableStyleDark9",
    "TableStyleDark10",
    "TableStyleDark11",
];

/// Built-in icon sets for conditional formatting.
pub const ICON_SETS: &[&str] = &[
    "3-arrows",
    "3-arrows-gray",
    "3-flags",
    "3-traffic-lights",
    "3-traffic-lights-rimmed",
    "3-signs",
    "3-symbols",
    "3-symbols-2",
    "3-stars",
    "3-triangles",
    "4-arrows",
    "4-arrows-gray",
    "4-ratings",
    "4-red-to-black",
    "4-traffic-lights",
    "5-arrows",
    "5-arrows-gray",
    "5-ratings",
    "5-quarters",
    "5-boxes",
];

/// Check if a table style name is valid (built-in).
pub fn is_valid_table_style(name: &str) -> bool {
    TABLE_STYLES.contains(&name)
}

/// Check if an icon set name is valid.
pub fn is_valid_icon_set(name: &str) -> bool {
    ICON_SETS.contains(&name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_table_style_count() {
        assert_eq!(TABLE_STYLES.len(), 60); // 21 light + 28 medium + 11 dark
    }

    #[test]
    fn test_valid_style() {
        assert!(is_valid_table_style("TableStyleMedium2"));
        assert!(!is_valid_table_style("InvalidStyle"));
    }

    #[test]
    fn test_valid_icon_set() {
        assert!(is_valid_icon_set("3-traffic-lights"));
        assert!(!is_valid_icon_set("invalid-icons"));
    }

    #[test]
    fn test_all_table_styles_valid() {
        for style in TABLE_STYLES {
            assert!(is_valid_table_style(style), "{} should be valid", style);
        }
    }

    #[test]
    fn test_all_icon_sets_valid() {
        for icon in ICON_SETS {
            assert!(is_valid_icon_set(icon), "{} should be valid", icon);
        }
    }

    #[test]
    fn test_table_style_categories() {
        // At least one from each category
        assert!(is_valid_table_style("TableStyleLight1"));
        assert!(is_valid_table_style("TableStyleMedium1"));
        assert!(is_valid_table_style("TableStyleDark1"));
    }
}
