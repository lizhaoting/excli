use excli_core::style::{DataValidation, ValidationType};
use excli_core::Sheet;

/// Manager for data validation rules.
pub struct ValidationManager;

impl ValidationManager {
    pub fn list(sheet: &Sheet) -> &[DataValidation] {
        &sheet.data_validations
    }

    pub fn add_list(sheet: &mut Sheet, range: String, source: String) {
        sheet.data_validations.push(DataValidation {
            range,
            validation_type: ValidationType::List { source },
            allow_blank: true,
            show_error: true,
            error_title: Some("Invalid Value".to_string()),
            error_message: Some("Please select a value from the dropdown list.".to_string()),
            prompt_title: None,
            prompt_message: None,
        });
    }

    pub fn add_whole(sheet: &mut Sheet, range: String, min: i64, max: i64) {
        sheet.data_validations.push(DataValidation {
            range,
            validation_type: ValidationType::Whole {
                between: Some((min, max)),
                min: None,
                max: None,
            },
            allow_blank: true,
            show_error: true,
            error_title: Some("Invalid Number".to_string()),
            error_message: Some(format!("Please enter a whole number between {} and {}.", min, max)),
            prompt_title: None,
            prompt_message: None,
        });
    }

    pub fn add_decimal(sheet: &mut Sheet, range: String, min: f64, max: f64) {
        sheet.data_validations.push(DataValidation {
            range,
            validation_type: ValidationType::Decimal {
                between: Some((min, max)),
                min: None,
                max: None,
            },
            allow_blank: true,
            show_error: true,
            error_title: Some("Invalid Number".to_string()),
            error_message: Some(format!(
                "Please enter a decimal number between {} and {}.",
                min, max
            )),
            prompt_title: None,
            prompt_message: None,
        });
    }

    pub fn add_text_length(sheet: &mut Sheet, range: String, min: u32, max: u32) {
        sheet.data_validations.push(DataValidation {
            range,
            validation_type: ValidationType::TextLength {
                min: Some(min),
                max: Some(max),
            },
            allow_blank: true,
            show_error: true,
            error_title: Some("Invalid Text Length".to_string()),
            error_message: Some(format!(
                "Text must be between {} and {} characters.",
                min, max
            )),
            prompt_title: None,
            prompt_message: None,
        });
    }

    pub fn add_date(sheet: &mut Sheet, range: String, min: &str, max: &str) {
        sheet.data_validations.push(DataValidation {
            range,
            validation_type: ValidationType::Date {
                min: Some(min.to_string()),
                max: Some(max.to_string()),
            },
            allow_blank: true,
            show_error: true,
            error_title: Some("Invalid Date".to_string()),
            error_message: Some(format!("Date must be between {} and {}.", min, max)),
            prompt_title: None,
            prompt_message: None,
        });
    }

    pub fn add_custom(sheet: &mut Sheet, range: String, formula: String) {
        sheet.data_validations.push(DataValidation {
            range,
            validation_type: ValidationType::Custom { formula },
            allow_blank: true,
            show_error: true,
            error_title: Some("Invalid Entry".to_string()),
            error_message: Some("This value does not meet the validation criteria.".to_string()),
            prompt_title: None,
            prompt_message: None,
        });
    }

    pub fn remove(sheet: &mut Sheet, index: usize) -> bool {
        if index < sheet.data_validations.len() {
            sheet.data_validations.remove(index);
            true
        } else {
            false
        }
    }

    pub fn clear(sheet: &mut Sheet) {
        sheet.data_validations.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_list_validation() {
        let mut sheet = Sheet::new("Test");
        ValidationManager::add_list(
            &mut sheet,
            "A1:A100".to_string(),
            "苹果,香蕉,橘子".to_string(),
        );
        assert_eq!(sheet.data_validations.len(), 1);
    }

    #[test]
    fn test_add_whole_validation() {
        let mut sheet = Sheet::new("Test");
        ValidationManager::add_whole(&mut sheet, "B1:B100".to_string(), 1, 100);
        assert_eq!(sheet.data_validations.len(), 1);
    }

    #[test]
    fn test_add_custom_validation() {
        let mut sheet = Sheet::new("Test");
        ValidationManager::add_custom(
            &mut sheet,
            "C1:C100".to_string(),
            "=LEN(C1)<=50".to_string(),
        );
        assert_eq!(sheet.data_validations.len(), 1);
    }

    #[test]
    fn test_remove_validation() {
        let mut sheet = Sheet::new("Test");
        ValidationManager::add_list(&mut sheet, "A1:A10".to_string(), "A,B,C".to_string());
        assert!(ValidationManager::remove(&mut sheet, 0));
        assert_eq!(sheet.data_validations.len(), 0);
    }

    #[test]
    fn test_add_decimal_validation() {
        let mut sheet = Sheet::new("Test");
        ValidationManager::add_decimal(&mut sheet, "D1:D100".to_string(), 0.0, 100.0);
        assert_eq!(sheet.data_validations.len(), 1);
    }

    #[test]
    fn test_add_text_length_validation() {
        let mut sheet = Sheet::new("Test");
        ValidationManager::add_text_length(&mut sheet, "E1:E100".to_string(), 1, 50);
        assert_eq!(sheet.data_validations.len(), 1);
    }

    #[test]
    fn test_add_date_validation() {
        let mut sheet = Sheet::new("Test");
        ValidationManager::add_date(&mut sheet, "F1:F100".to_string(), "2020-01-01", "2025-12-31");
        assert_eq!(sheet.data_validations.len(), 1);
    }

    #[test]
    fn test_clear_validations() {
        let mut sheet = Sheet::new("Test");
        ValidationManager::add_list(&mut sheet, "A1:A10".to_string(), "X,Y,Z".to_string());
        ValidationManager::add_whole(&mut sheet, "B1:B10".to_string(), 1, 10);
        ValidationManager::clear(&mut sheet);
        assert_eq!(sheet.data_validations.len(), 0);
    }

    #[test]
    fn test_remove_nonexistent_validation() {
        let mut sheet = Sheet::new("Test");
        assert!(!ValidationManager::remove(&mut sheet, 0));
    }
}
