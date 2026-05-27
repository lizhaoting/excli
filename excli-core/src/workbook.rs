use serde::{Deserialize, Serialize};

use crate::error::{ExcliError, ExcliResult};
use crate::sheet::Sheet;
use crate::types::{DateSystem, DocumentProperties, FileFormat, Stats};

/// Represents a complete Excel workbook.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workbook {
    pub sheets: Vec<Sheet>,
    pub named_ranges: Vec<NamedRange>,
    pub external_links: Vec<ExternalLink>,
    pub properties: DocumentProperties,
    pub vba_project: Option<VbaProject>,
    pub connections: Vec<DataConnection>,
    pub date_system: DateSystem,
    #[serde(skip)]
    pub file_format: Option<FileFormat>,
    #[serde(skip)]
    pub file_size: u64,
}

impl Default for Workbook {
    fn default() -> Self {
        Workbook {
            sheets: Vec::new(),
            named_ranges: Vec::new(),
            external_links: Vec::new(),
            properties: DocumentProperties::default(),
            vba_project: None,
            connections: Vec::new(),
            date_system: DateSystem::default(),
            file_format: None,
            file_size: 0,
        }
    }
}

impl Workbook {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_sheet(mut self, sheet: Sheet) -> Self {
        self.sheets.push(sheet);
        self
    }

    /// Get a sheet by name.
    pub fn sheet_by_name(&self, name: &str) -> Option<&Sheet> {
        self.sheets.iter().find(|s| s.name == name)
    }

    /// Get a mutable reference to a sheet by name.
    pub fn sheet_by_name_mut(&mut self, name: &str) -> Option<&mut Sheet> {
        self.sheets.iter_mut().find(|s| s.name == name)
    }

    /// Get a sheet by index.
    pub fn sheet_by_index(&self, index: usize) -> Option<&Sheet> {
        self.sheets.get(index)
    }

    /// Add a new sheet.
    pub fn add_sheet(&mut self, name: impl Into<String>) -> &mut Sheet {
        let sheet = Sheet::new(name);
        self.sheets.push(sheet);
        self.sheets.last_mut().unwrap()
    }

    /// Remove a sheet by name.
    pub fn remove_sheet(&mut self, name: &str) -> ExcliResult<()> {
        let idx = self
            .sheets
            .iter()
            .position(|s| s.name == name)
            .ok_or_else(|| ExcliError::sheet_not_found(name))?;
        self.sheets.remove(idx);
        Ok(())
    }

    /// Rename a sheet.
    pub fn rename_sheet(&mut self, old_name: &str, new_name: &str) -> ExcliResult<()> {
        let sheet = self
            .sheet_by_name_mut(old_name)
            .ok_or_else(|| ExcliError::sheet_not_found(old_name))?;
        sheet.name = new_name.to_string();
        Ok(())
    }

    /// Copy a sheet to a new name.
    pub fn copy_sheet(&mut self, source: &str, dest: impl Into<String>) -> ExcliResult<()> {
        let dest_name = dest.into();
        let sheet = self.sheet_by_name(source)
            .ok_or_else(|| ExcliError::sheet_not_found(source))?
            .clone();
        let mut copy = sheet;
        copy.name = dest_name;
        self.sheets.push(copy);
        Ok(())
    }

    /// Move a sheet to a new position (0-based index).
    pub fn move_sheet(&mut self, name: &str, new_index: usize) -> ExcliResult<()> {
        let pos = self.sheets.iter().position(|s| s.name == name)
            .ok_or_else(|| ExcliError::sheet_not_found(name))?;
        let sheet = self.sheets.remove(pos);
        let insert_at = new_index.min(self.sheets.len());
        self.sheets.insert(insert_at, sheet);
        Ok(())
    }

    /// Hide a sheet by name.
    pub fn hide_sheet(&mut self, name: &str) -> ExcliResult<()> {
        let sheet = self.sheet_by_name_mut(name)
            .ok_or_else(|| ExcliError::sheet_not_found(name))?;
        sheet.state = crate::sheet::SheetState::Hidden;
        Ok(())
    }

    /// Unhide a sheet by name.
    pub fn unhide_sheet(&mut self, name: &str) -> ExcliResult<()> {
        let sheet = self.sheet_by_name_mut(name)
            .ok_or_else(|| ExcliError::sheet_not_found(name))?;
        sheet.state = crate::sheet::SheetState::Visible;
        Ok(())
    }

    /// Sheet count.
    pub fn sheet_count(&self) -> usize {
        self.sheets.len()
    }

    /// Get statistics for the workbook.
    pub fn stats(&self) -> Stats {
        let mut stats = Stats {
            sheet_count: self.sheets.len(),
            file_size_bytes: self.file_size,
            ..Default::default()
        };

        for sheet in &self.sheets {
            let s = sheet.stats();
            stats.row_count += s.row_count;
            stats.col_count = stats.col_count.max(s.col_count);
            stats.formula_count += s.formula_count;
            stats.style_count += s.style_count;
        }

        stats
    }

    /// Detect date system from workbook properties.
    pub fn detect_date_system(&self) -> DateSystem {
        self.date_system
    }
}

/// A named range definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NamedRange {
    pub name: String,
    pub refers_to: String,
    pub scope: NameScope,
    pub is_hidden: bool,
    pub comment: Option<String>,
}

impl NamedRange {
    pub fn new(name: impl Into<String>, refers_to: impl Into<String>) -> Self {
        NamedRange {
            name: name.into(),
            refers_to: refers_to.into(),
            scope: NameScope::Workbook,
            is_hidden: false,
            comment: None,
        }
    }
}

/// Scope of a named range.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NameScope {
    Workbook,
    Sheet(String),
}

/// An external workbook link.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalLink {
    pub path: String,
    pub is_relative: bool,
    pub sheet_names: Vec<String>,
}

/// VBA project metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VbaProject {
    pub modules: Vec<VbaModule>,
    pub references: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VbaModule {
    pub name: String,
    pub module_type: VbaModuleType,
    pub code: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VbaModuleType {
    Module,
    Class,
    Form,
    Document,
}

/// A data connection to external sources.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataConnection {
    pub name: String,
    pub connection_type: ConnectionType,
    pub connection_string: Option<String>,
    pub command_text: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionType {
    Odbc,
    OleDb,
    Web,
    Text,
    Other,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cell::Cell;

    #[test]
    fn test_workbook_creation() {
        let wb = Workbook::new();
        assert_eq!(wb.sheet_count(), 0);
    }

    #[test]
    fn test_add_and_get_sheet() {
        let mut wb = Workbook::new();
        wb.add_sheet("Data");
        wb.add_sheet("Summary");

        assert_eq!(wb.sheet_count(), 2);
        assert!(wb.sheet_by_name("Data").is_some());
        assert!(wb.sheet_by_name("Summary").is_some());
        assert!(wb.sheet_by_name("Missing").is_none());
    }

    #[test]
    fn test_remove_sheet() {
        let mut wb = Workbook::new();
        wb.add_sheet("Temp");
        assert!(wb.remove_sheet("Temp").is_ok());
        assert_eq!(wb.sheet_count(), 0);
    }

    #[test]
    fn test_rename_sheet() {
        let mut wb = Workbook::new();
        wb.add_sheet("Old");
        assert!(wb.rename_sheet("Old", "New").is_ok());
        assert!(wb.sheet_by_name("New").is_some());
        assert!(wb.sheet_by_name("Old").is_none());
    }

    #[test]
    fn test_workbook_with_data() {
        let mut wb = Workbook::new();
        let sheet = wb.add_sheet("Data");
        sheet.set_cell(0, 0, Cell::number(1.0));
        sheet.set_cell(1, 0, Cell::number(2.0));

        assert_eq!(wb.stats().row_count, 1);
        assert_eq!(wb.stats().col_count, 2);
    }

    #[test]
    fn test_named_range() {
        let nr = NamedRange::new("MyRange", "=Sheet1!$A$1:$B$10");
        assert_eq!(nr.name, "MyRange");
        assert_eq!(nr.scope, NameScope::Workbook);
    }

    #[test]
    fn test_workbook_sheet_by_index() {
        let mut wb = Workbook::new();
        wb.add_sheet("First");
        wb.add_sheet("Second");

        assert_eq!(wb.sheet_by_index(0).unwrap().name, "First");
        assert_eq!(wb.sheet_by_index(1).unwrap().name, "Second");
        assert!(wb.sheet_by_index(2).is_none());
    }

    #[test]
    fn test_workbook_copy_sheet() {
        let mut wb = Workbook::new();
        let sheet = wb.add_sheet("Original");
        sheet.set_cell(0, 0, Cell::string("Hello"));

        let result = wb.copy_sheet("Original", "Copy");
        assert!(result.is_ok());
        assert_eq!(wb.sheet_count(), 2);
        let copy = wb.sheet_by_name("Copy").unwrap();
        assert_eq!(copy.get_cell(0, 0).unwrap().value.to_string(), "Hello");
    }

    #[test]
    fn test_workbook_move_sheet() {
        let mut wb = Workbook::new();
        wb.add_sheet("A");
        wb.add_sheet("B");
        wb.add_sheet("C");

        assert!(wb.move_sheet("C", 0).is_ok());
        assert_eq!(wb.sheet_by_index(0).unwrap().name, "C");
    }

    #[test]
    fn test_workbook_hide_show_sheet() {
        let mut wb = Workbook::new();
        wb.add_sheet("Visible");
        assert!(wb.hide_sheet("Visible").is_ok());
        assert!(wb.unhide_sheet("Visible").is_ok());
    }

    #[test]
    fn test_workbook_remove_nonexistent_sheet() {
        let mut wb = Workbook::new();
        assert!(wb.remove_sheet("DoesNotExist").is_err());
    }

    #[test]
    fn test_workbook_stats_multiple_sheets() {
        let mut wb = Workbook::new();
        let s1 = wb.add_sheet("Sheet1");
        s1.set_cell(0, 0, Cell::number(1.0));
        s1.set_cell(0, 1, Cell::number(2.0));
        let s2 = wb.add_sheet("Sheet2");
        s2.set_cell(0, 0, Cell::number(3.0));

        let stats = wb.stats();
        assert_eq!(stats.sheet_count, 2);
        // row_count sums across sheets
        assert!(stats.row_count >= 2);
    }
}
