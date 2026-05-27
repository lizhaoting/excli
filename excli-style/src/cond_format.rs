use excli_core::style::{
    ConditionalFormat, ConditionalFormatRule, ConditionalOperator, CellStyle, ScalePoint,
};
use excli_core::Sheet;

/// Manager for conditional formatting rules.
pub struct CondFormatManager;

impl CondFormatManager {
    /// List all conditional formats in a sheet.
    pub fn list(sheet: &Sheet) -> &[ConditionalFormat] {
        &sheet.conditional_formats
    }

    /// Add a cell value condition.
    pub fn add_cell_value(
        sheet: &mut Sheet,
        range: String,
        operator: ConditionalOperator,
        value: String,
        style: CellStyle,
    ) {
        let cf = ConditionalFormat {
            range,
            priority: (sheet.conditional_formats.len() + 1) as u32,
            rule: ConditionalFormatRule::CellValue {
                operator,
                value,
                style,
            },
        };
        sheet.conditional_formats.push(cf);
    }

    /// Add a formula condition.
    pub fn add_formula(
        sheet: &mut Sheet,
        range: String,
        formula: String,
        style: CellStyle,
    ) {
        let cf = ConditionalFormat {
            range,
            priority: (sheet.conditional_formats.len() + 1) as u32,
            rule: ConditionalFormatRule::Formula { formula, style },
        };
        sheet.conditional_formats.push(cf);
    }

    /// Add a color scale rule.
    pub fn add_color_scale(
        sheet: &mut Sheet,
        range: String,
        min_type: ScalePoint,
        max_type: ScalePoint,
        colors: (String, Option<String>, String),
    ) {
        let cf = ConditionalFormat {
            range,
            priority: (sheet.conditional_formats.len() + 1) as u32,
            rule: ConditionalFormatRule::ColorScale {
                min_type,
                mid_type: None,
                max_type,
                colors,
            },
        };
        sheet.conditional_formats.push(cf);
    }

    /// Add a data bar rule.
    pub fn add_data_bar(sheet: &mut Sheet, range: String, color: String, show_value: bool) {
        let cf = ConditionalFormat {
            range,
            priority: (sheet.conditional_formats.len() + 1) as u32,
            rule: ConditionalFormatRule::DataBar { color, show_value },
        };
        sheet.conditional_formats.push(cf);
    }

    /// Add an icon set rule.
    pub fn add_icon_set(
        sheet: &mut Sheet,
        range: String,
        icon_set: String,
        reverse: bool,
    ) {
        let cf = ConditionalFormat {
            range,
            priority: (sheet.conditional_formats.len() + 1) as u32,
            rule: ConditionalFormatRule::IconSet {
                icon_set,
                reverse,
                show_value: true,
            },
        };
        sheet.conditional_formats.push(cf);
    }

    /// Remove a conditional format by index.
    pub fn remove(sheet: &mut Sheet, index: usize) -> bool {
        if index < sheet.conditional_formats.len() {
            sheet.conditional_formats.remove(index);
            true
        } else {
            false
        }
    }

    /// Clear all conditional formats.
    pub fn clear(sheet: &mut Sheet) {
        sheet.conditional_formats.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_cell_value_condition() {
        let mut sheet = Sheet::new("Test");
        CondFormatManager::add_cell_value(
            &mut sheet,
            "A1:A100".to_string(),
            ConditionalOperator::GreaterThan,
            "100".to_string(),
            CellStyle::bold(),
        );
        assert_eq!(sheet.conditional_formats.len(), 1);
        assert_eq!(sheet.conditional_formats[0].priority, 1);
    }

    #[test]
    fn test_add_color_scale() {
        let mut sheet = Sheet::new("Test");
        CondFormatManager::add_color_scale(
            &mut sheet,
            "B1:B100".to_string(),
            ScalePoint::Min,
            ScalePoint::Max,
            ("#63BE7B".to_string(), Some("#FFEB84".to_string()), "#F8696B".to_string()),
        );
        assert_eq!(sheet.conditional_formats.len(), 1);
    }

    #[test]
    fn test_remove_condition() {
        let mut sheet = Sheet::new("Test");
        CondFormatManager::add_cell_value(
            &mut sheet,
            "A1:A100".to_string(),
            ConditionalOperator::GreaterThan,
            "100".to_string(),
            CellStyle::bold(),
        );
        assert!(CondFormatManager::remove(&mut sheet, 0));
        assert_eq!(sheet.conditional_formats.len(), 0);
    }

    #[test]
    fn test_clear_conditions() {
        let mut sheet = Sheet::new("Test");
        CondFormatManager::add_cell_value(
            &mut sheet,
            "A1:A100".to_string(),
            ConditionalOperator::GreaterThan,
            "100".to_string(),
            CellStyle::bold(),
        );
        CondFormatManager::clear(&mut sheet);
        assert_eq!(sheet.conditional_formats.len(), 0);
    }

    #[test]
    fn test_add_data_bar() {
        let mut sheet = Sheet::new("Test");
        CondFormatManager::add_data_bar(
            &mut sheet,
            "B1:B20".to_string(),
            "#4472C4".to_string(),
            true,
        );
        assert_eq!(sheet.conditional_formats.len(), 1);
    }

    #[test]
    fn test_add_icon_set() {
        let mut sheet = Sheet::new("Test");
        CondFormatManager::add_icon_set(
            &mut sheet,
            "C1:C30".to_string(),
            "3Arrows".to_string(),
            false,
        );
        assert_eq!(sheet.conditional_formats.len(), 1);
    }

    #[test]
    fn test_multiple_conditions_same_range() {
        let mut sheet = Sheet::new("Test");
        CondFormatManager::add_cell_value(
            &mut sheet,
            "A1:A10".to_string(),
            ConditionalOperator::GreaterThan,
            "50".to_string(),
            CellStyle::bold(),
        );
        CondFormatManager::add_cell_value(
            &mut sheet,
            "A1:A10".to_string(),
            ConditionalOperator::LessThan,
            "10".to_string(),
            CellStyle::default(),
        );
        assert_eq!(sheet.conditional_formats.len(), 2);
    }
}
