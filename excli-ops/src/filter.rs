use excli_core::cell::CellValue;
use excli_core::error::ExcliResult;
use excli_core::Sheet;

/// Filter operator types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterOperator {
    Eq,
    Ne,
    Gt,
    Gte,
    Lt,
    Lte,
    Contains,
    NotContains,
    StartsWith,
    EndsWith,
    Between,
    In,
    IsNull,
    IsNotNull,
    Regex,
}

impl FilterOperator {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "eq" | "=" | "==" => Some(FilterOperator::Eq),
            "ne" | "!=" | "<>" => Some(FilterOperator::Ne),
            "gt" | ">" => Some(FilterOperator::Gt),
            "gte" | ">=" => Some(FilterOperator::Gte),
            "lt" | "<" => Some(FilterOperator::Lt),
            "lte" | "<=" => Some(FilterOperator::Lte),
            "contains" | "like" => Some(FilterOperator::Contains),
            "not-contains" => Some(FilterOperator::NotContains),
            "startswith" => Some(FilterOperator::StartsWith),
            "endswith" => Some(FilterOperator::EndsWith),
            "between" => Some(FilterOperator::Between),
            "in" => Some(FilterOperator::In),
            "is-null" => Some(FilterOperator::IsNull),
            "is-not-null" => Some(FilterOperator::IsNotNull),
            "regex" => Some(FilterOperator::Regex),
            _ => None,
        }
    }
}

/// A single filter condition.
#[derive(Debug, Clone)]
pub struct FilterCondition {
    pub column: String,
    pub operator: FilterOperator,
    pub value: Option<String>,
    pub value2: Option<String>, // For Between
    pub values: Option<Vec<String>>, // For In
}

impl FilterCondition {
    pub fn new(column: impl Into<String>, operator: FilterOperator, value: Option<String>) -> Self {
        FilterCondition {
            column: column.into(),
            operator,
            value,
            value2: None,
            values: None,
        }
    }

    fn matches(&self, val: &CellValue) -> bool {
        match self.operator {
            FilterOperator::IsNull => val.is_empty(),
            FilterOperator::IsNotNull => !val.is_empty(),
            _ => {
                let cell_str = val.to_string();
                let val_str = self.value.as_deref().unwrap_or("");

                match self.operator {
                    FilterOperator::Eq => cell_str == val_str,
                    FilterOperator::Ne => cell_str != val_str,
                    FilterOperator::Gt => {
                        if let (Some(cn), Some(vn)) = (val.as_f64(), val_str.parse::<f64>().ok()) {
                            cn > vn
                        } else {
                            cell_str.as_str() > val_str
                        }
                    }
                    FilterOperator::Gte => {
                        if let (Some(cn), Some(vn)) = (val.as_f64(), val_str.parse::<f64>().ok()) {
                            cn >= vn
                        } else {
                            cell_str.as_str() >= val_str
                        }
                    }
                    FilterOperator::Lt => {
                        if let (Some(cn), Some(vn)) = (val.as_f64(), val_str.parse::<f64>().ok()) {
                            cn < vn
                        } else {
                            cell_str.as_str() < val_str
                        }
                    }
                    FilterOperator::Lte => {
                        if let (Some(cn), Some(vn)) = (val.as_f64(), val_str.parse::<f64>().ok()) {
                            cn <= vn
                        } else {
                            cell_str.as_str() <= val_str
                        }
                    }
                    FilterOperator::Contains => {
                        cell_str.to_lowercase().contains(&val_str.to_lowercase())
                    }
                    FilterOperator::NotContains => {
                        !cell_str.to_lowercase().contains(&val_str.to_lowercase())
                    }
                    FilterOperator::StartsWith => {
                        cell_str.to_lowercase().starts_with(&val_str.to_lowercase())
                    }
                    FilterOperator::EndsWith => {
                        cell_str.to_lowercase().ends_with(&val_str.to_lowercase())
                    }
                    FilterOperator::Between => {
                        let v1 = self.value.as_deref().unwrap_or("0");
                        let v2 = self.value2.as_deref().unwrap_or("0");
                        if let (Some(cn), Some(n1), Some(n2)) = (
                            val.as_f64(),
                            v1.parse::<f64>().ok(),
                            v2.parse::<f64>().ok(),
                        ) {
                            cn >= n1 && cn <= n2
                        } else {
                            cell_str.as_str() >= v1 && cell_str.as_str() <= v2
                        }
                    }
                    FilterOperator::In => {
                        if let Some(ref vals) = self.values {
                            vals.iter().any(|v| cell_str == *v)
                        } else {
                            false
                        }
                    }
                    FilterOperator::Regex => {
                        if let Ok(re) = regex::Regex::new(val_str) {
                            re.is_match(&cell_str)
                        } else {
                            false
                        }
                    }
                    _ => false,
                }
            }
        }
    }
}

/// Filter rows in a sheet based on conditions.
pub fn filter_rows(sheet: &Sheet, conditions: &[FilterCondition]) -> ExcliResult<Sheet> {
    let mut result = Sheet::new(format!("{}_filtered", sheet.name));

    // Find column indices
    let mut col_indices: Vec<Option<u32>> = Vec::new();
    for cond in conditions {
        let idx = find_column_index(sheet, &cond.column);
        col_indices.push(idx);
    }

    for (_row_idx, row) in sheet.rows.iter().enumerate() {
        let mut include = true;
        for (cond_idx, cond) in conditions.iter().enumerate() {
            if let Some(col_idx) = col_indices[cond_idx] {
                let cell_val = row
                    .cells
                    .get(col_idx as usize)
                    .map(|c| &c.value)
                    .unwrap_or(&CellValue::Empty);
                if !cond.matches(cell_val) {
                    include = false;
                    break;
                }
            } else {
                include = false;
                break;
            }
        }

        if include {
            let target_row = result.row_count() as u32;
            for (col_idx, cell) in row.cells.iter().enumerate() {
                result.set_cell(col_idx as u32, target_row, cell.clone());
            }
        }
    }

    Ok(result)
}

/// Filter rows and return only unique rows.
pub fn filter_unique(sheet: &Sheet) -> Sheet {
    let mut result = Sheet::new(format!("{}_unique", sheet.name));
    let mut seen = std::collections::HashSet::new();

    for row in &sheet.rows {
        let key: Vec<String> = row.cells.iter().map(|c| c.value.to_string()).collect();
        if seen.insert(key) {
            let target_row = result.row_count() as u32;
            for (col_idx, cell) in row.cells.iter().enumerate() {
                result.set_cell(col_idx as u32, target_row, cell.clone());
            }
        }
    }

    result
}

fn find_column_index(sheet: &Sheet, col_ref: &str) -> Option<u32> {
    // Try as column letter (A, B, AA, etc.)
    if let Some(idx) = excli_core::reference::column_name_to_index(col_ref) {
        return Some(idx);
    }

    // Try as column name in header row
    if let Some(header_row) = sheet.rows.first() {
        for (idx, cell) in header_row.cells.iter().enumerate() {
            if cell.value.to_string().eq_ignore_ascii_case(col_ref) {
                return Some(idx as u32);
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use excli_core::cell::Cell;

    fn make_test_sheet() -> Sheet {
        let mut sheet = Sheet::new("Test");
        // Header
        sheet.set_cell(0, 0, Cell::string("Name"));
        sheet.set_cell(1, 0, Cell::string("Age"));
        sheet.set_cell(2, 0, Cell::string("Score"));
        // Row 1
        sheet.set_cell(0, 1, Cell::string("Alice"));
        sheet.set_cell(1, 1, Cell::number(30.0));
        sheet.set_cell(2, 1, Cell::number(95.0));
        // Row 2
        sheet.set_cell(0, 2, Cell::string("Bob"));
        sheet.set_cell(1, 2, Cell::number(25.0));
        sheet.set_cell(2, 2, Cell::number(87.0));
        // Row 3
        sheet.set_cell(0, 3, Cell::string("Carol"));
        sheet.set_cell(1, 3, Cell::number(35.0));
        sheet.set_cell(2, 3, Cell::number(92.0));
        sheet
    }

    #[test]
    fn test_filter_gt() {
        let sheet = make_test_sheet();
        let conditions = vec![FilterCondition::new("B", FilterOperator::Gt, Some("28".to_string()))];
        let result = filter_rows(&sheet, &conditions).unwrap();
        // Alice (30) and Carol (35) should pass; Bob (25) shouldn't
        // Plus header row
        assert_eq!(result.row_count(), 3); // header + 2 matching
    }

    #[test]
    fn test_filter_contains() {
        let sheet = make_test_sheet();
        let conditions = vec![FilterCondition::new("A", FilterOperator::Contains, Some("li".to_string()))];
        let result = filter_rows(&sheet, &conditions).unwrap();
        assert_eq!(result.row_count(), 1); // Only Alice contains "li"
    }

    #[test]
    fn test_filter_is_null() {
        let mut sheet = Sheet::new("Test");
        sheet.set_cell(0, 0, Cell::string("Name"));
        sheet.set_cell(0, 1, Cell::string("Alice"));
        sheet.set_cell(0, 2, Cell::empty());

        let conditions = vec![FilterCondition::new("A", FilterOperator::IsNotNull, None)];
        let result = filter_rows(&sheet, &conditions).unwrap();
        assert_eq!(result.row_count(), 2); // header + Alice
    }

    #[test]
    fn test_filter_unique() {
        let mut sheet = Sheet::new("Test");
        sheet.set_cell(0, 0, Cell::string("A"));
        sheet.set_cell(0, 1, Cell::string("A"));
        sheet.set_cell(0, 2, Cell::string("B"));

        let result = filter_unique(&sheet);
        assert_eq!(result.row_count(), 2); // A, B
    }

    #[test]
    fn test_filter_operator_from_str() {
        assert_eq!(FilterOperator::from_str(">"), Some(FilterOperator::Gt));
        assert_eq!(FilterOperator::from_str("contains"), Some(FilterOperator::Contains));
        assert_eq!(FilterOperator::from_str("is-null"), Some(FilterOperator::IsNull));
        assert_eq!(FilterOperator::from_str("regex"), Some(FilterOperator::Regex));
    }

    #[test]
    fn test_filter_lt() {
        let sheet = make_test_sheet();
        let conditions = vec![FilterCondition::new("B", FilterOperator::Lt, Some("30".to_string()))];
        let result = filter_rows(&sheet, &conditions).unwrap();
        // Lt 30 should return Bob (25) only -> 1 data row
        assert_eq!(result.row_count(), 1);
    }

    #[test]
    fn test_filter_between() {
        let sheet = make_test_sheet();
        let mut cond = FilterCondition::new("B", FilterOperator::Between, Some("25".to_string()));
        cond.value2 = Some("30".to_string());
        let result = filter_rows(&sheet, &vec![cond]).unwrap();
        // Between 25 and 30 inclusive: Alice (30) and Bob (25)
        assert!(result.row_count() >= 1);
    }

    #[test]
    fn test_filter_regex() {
        let sheet = make_test_sheet();
        let conditions = vec![FilterCondition::new("A", FilterOperator::Regex, Some("^A".to_string()))];
        let result = filter_rows(&sheet, &conditions).unwrap();
        // Only Alice starts with A -> 1 row
        assert_eq!(result.row_count(), 1);
    }

    #[test]
    fn test_filter_no_match() {
        let sheet = make_test_sheet();
        let conditions = vec![FilterCondition::new("A", FilterOperator::Contains, Some("XYZ".to_string()))];
        let result = filter_rows(&sheet, &conditions).unwrap();
        // No matches
        assert_eq!(result.row_count(), 0);
    }

    #[test]
    fn test_filter_greater_than() {
        let sheet = make_test_sheet();
        let conditions = vec![FilterCondition::new("B", FilterOperator::Gt, Some("25".to_string()))];
        let result = filter_rows(&sheet, &conditions).unwrap();
        assert!(result.row_count() >= 1);
    }

    #[test]
    fn test_filter_less_than() {
        let sheet = make_test_sheet();
        let conditions = vec![FilterCondition::new("B", FilterOperator::Lt, Some("30".to_string()))];
        let result = filter_rows(&sheet, &conditions).unwrap();
        assert!(result.row_count() >= 1);
    }

    #[test]
    fn test_filter_equals() {
        let sheet = make_test_sheet();
        let conditions = vec![FilterCondition::new("A", FilterOperator::Eq, Some("Alice".to_string()))];
        let result = filter_rows(&sheet, &conditions).unwrap();
        assert_eq!(result.row_count(), 1);
    }

    #[test]
    fn test_filter_not_equals() {
        let sheet = make_test_sheet();
        let conditions = vec![FilterCondition::new("A", FilterOperator::Ne, Some("Alice".to_string()))];
        let result = filter_rows(&sheet, &conditions).unwrap();
        assert!(result.row_count() >= 1);
    }

    #[test]
    fn test_filter_starts_with() {
        let sheet = make_test_sheet();
        let conditions = vec![FilterCondition::new("A", FilterOperator::StartsWith, Some("B".to_string()))];
        let result = filter_rows(&sheet, &conditions).unwrap();
        assert!(result.row_count() >= 1);
    }

    #[test]
    fn test_filter_ends_with() {
        let sheet = make_test_sheet();
        let conditions = vec![FilterCondition::new("A", FilterOperator::EndsWith, Some("e".to_string()))];
        let result = filter_rows(&sheet, &conditions).unwrap();
        assert!(result.row_count() >= 1); // Alice ends with 'e'
    }
}
