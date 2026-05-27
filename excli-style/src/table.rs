use excli_core::sheet::{Table, TableColumn};
use excli_core::Sheet;
use excli_core::range::CellRange;

/// Manager for Excel tables (ListObjects).
pub struct TableManager;

impl TableManager {
    pub fn list(sheet: &Sheet) -> &[Table] {
        &sheet.tables
    }

    pub fn get<'a>(sheet: &'a Sheet, name: &str) -> Option<&'a Table> {
        sheet.tables.iter().find(|t| t.name == name || t.display_name == name)
    }

    pub fn create(sheet: &mut Sheet, range: &CellRange, name: &str) -> Table {
        let table = Table {
            name: name.to_string(),
            display_name: name.to_string(),
            range: range.to_a1(),
            columns: vec![],
            style_name: Some("TableStyleMedium2".to_string()),
            show_header: true,
            show_totals: false,
            auto_filter: true,
        };
        sheet.tables.push(table.clone());
        table
    }

    pub fn convert_range_to_table(
        sheet: &mut Sheet,
        range: &CellRange,
        name: &str,
    ) -> Table {
        Self::create(sheet, range, name)
    }

    pub fn delete(sheet: &mut Sheet, name: &str) -> bool {
        if let Some(idx) = sheet.tables.iter().position(|t| t.name == name) {
            sheet.tables.remove(idx);
            true
        } else {
            false
        }
    }

    pub fn resize(sheet: &mut Sheet, name: &str, new_range: &CellRange) -> bool {
        if let Some(table) = sheet.tables.iter_mut().find(|t| t.name == name) {
            table.range = new_range.to_a1();
            true
        } else {
            false
        }
    }

    pub fn set_style(sheet: &mut Sheet, name: &str, style_name: &str) -> bool {
        if let Some(table) = sheet.tables.iter_mut().find(|t| t.name == name) {
            table.style_name = Some(style_name.to_string());
            true
        } else {
            false
        }
    }

    pub fn rename(sheet: &mut Sheet, old_name: &str, new_name: &str) -> bool {
        if let Some(table) = sheet.tables.iter_mut().find(|t| t.name == old_name) {
            table.name = new_name.to_string();
            table.display_name = new_name.to_string();
            true
        } else {
            false
        }
    }

    pub fn add_column(
        sheet: &mut Sheet,
        table_name: &str,
        col_name: &str,
        formula: Option<&str>,
    ) -> bool {
        if let Some(table) = sheet.tables.iter_mut().find(|t| t.name == table_name) {
            table.columns.push(TableColumn {
                name: col_name.to_string(),
                formula: formula.map(|s| s.to_string()),
                totals_function: None,
            });
            true
        } else {
            false
        }
    }

    pub fn remove_column(sheet: &mut Sheet, table_name: &str, col_name: &str) -> bool {
        if let Some(table) = sheet.tables.iter_mut().find(|t| t.name == table_name) {
            if let Some(idx) = table.columns.iter().position(|c| c.name == col_name) {
                table.columns.remove(idx);
                return true;
            }
        }
        false
    }

    pub fn convert_to_range(sheet: &mut Sheet, name: &str) -> bool {
        // Remove table but preserve data (data stays in cells)
        Self::delete(sheet, name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_table() {
        let mut sheet = Sheet::new("Test");
        let range = CellRange::new(0, 0, 3, 10);
        TableManager::create(&mut sheet, &range, "SalesData");
        assert_eq!(sheet.tables.len(), 1);
        assert_eq!(sheet.tables[0].name, "SalesData");
    }

    #[test]
    fn test_delete_table() {
        let mut sheet = Sheet::new("Test");
        let range = CellRange::new(0, 0, 3, 10);
        TableManager::create(&mut sheet, &range, "Temp");
        assert!(TableManager::delete(&mut sheet, "Temp"));
        assert_eq!(sheet.tables.len(), 0);
    }

    #[test]
    fn test_rename_table() {
        let mut sheet = Sheet::new("Test");
        let range = CellRange::new(0, 0, 3, 10);
        TableManager::create(&mut sheet, &range, "Old");
        assert!(TableManager::rename(&mut sheet, "Old", "New"));
        assert!(TableManager::get(&sheet, "New").is_some());
        assert!(TableManager::get(&sheet, "Old").is_none());
    }

    #[test]
    fn test_add_column() {
        let mut sheet = Sheet::new("Test");
        let range = CellRange::new(0, 0, 2, 10);
        TableManager::create(&mut sheet, &range, "Sales");
        assert!(TableManager::add_column(&mut sheet, "Sales", "Total", Some("=[Price]*[Qty]")));
        assert_eq!(sheet.tables[0].columns.len(), 1);
    }

    #[test]
    fn test_resize_table() {
        let mut sheet = Sheet::new("Test");
        let range = CellRange::new(0, 0, 3, 10);
        TableManager::create(&mut sheet, &range, "Data");
        let new_range = CellRange::new(0, 0, 5, 20);
        assert!(TableManager::resize(&mut sheet, "Data", &new_range));
    }

    #[test]
    fn test_convert_to_range() {
        let mut sheet = Sheet::new("Test");
        // Add data before creating table
        sheet.set_cell(0, 0, excli_core::cell::Cell::string("Header"));
        sheet.set_cell(0, 1, excli_core::cell::Cell::string("Value"));
        let range = CellRange::new(0, 0, 3, 10);
        TableManager::create(&mut sheet, &range, "Data");
        assert!(TableManager::convert_to_range(&mut sheet, "Data"));
        assert!(sheet.tables.is_empty());
        assert!(!sheet.rows.is_empty()); // Data preserved
    }
}
