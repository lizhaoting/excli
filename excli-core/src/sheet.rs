use serde::{Deserialize, Serialize};

use crate::cell::Cell;
use crate::style::{CellStyle, ConditionalFormat, ConditionalFormatExtension, DataValidation};
use crate::types::Stats;

/// Represents a single worksheet in a workbook.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sheet {
    pub name: String,
    pub state: SheetState,
    pub columns: Vec<Column>,
    pub rows: Vec<Row>,
    pub merged_cells: Vec<MergedRegion>,
    pub tables: Vec<Table>,
    pub pivot_tables: Vec<PivotTable>,
    pub charts: Vec<Chart>,
    pub images: Vec<Image>,
    pub shapes: Vec<Shape>,
    pub hyperlinks: Vec<Hyperlink>,
    pub comments: Vec<Comment>,
    #[serde(default)]
    pub threaded_comments: Vec<ThreadedComment>,
    pub data_validations: Vec<DataValidation>,
    pub conditional_formats: Vec<ConditionalFormat>,
    #[serde(default)]
    pub conditional_format_extensions: Vec<ConditionalFormatExtension>,
    pub auto_filter: Option<AutoFilter>,
    pub page_setup: PageSetup,
    pub pane_state: Option<PaneState>,
    pub protection: Option<SheetProtection>,
    pub outlines: OutlineState,
    pub tab_color: Option<String>,
}

impl Sheet {
    pub fn new(name: impl Into<String>) -> Self {
        Sheet {
            name: name.into(),
            state: SheetState::Visible,
            columns: Vec::new(),
            rows: Vec::new(),
            merged_cells: Vec::new(),
            tables: Vec::new(),
            pivot_tables: Vec::new(),
            charts: Vec::new(),
            images: Vec::new(),
            shapes: Vec::new(),
            hyperlinks: Vec::new(),
            comments: Vec::new(),
            threaded_comments: Vec::new(),
            data_validations: Vec::new(),
            conditional_formats: Vec::new(),
            conditional_format_extensions: Vec::new(),
            auto_filter: None,
            page_setup: PageSetup::default(),
            pane_state: None,
            protection: None,
            outlines: OutlineState::default(),
            tab_color: None,
        }
    }

    /// Get cell at given column and row (0-based).
    pub fn get_cell(&self, col: u32, row: u32) -> Option<&Cell> {
        self.rows
            .get(row as usize)
            .and_then(|r| r.cells.get(col as usize))
    }

    /// Get mutable reference to a cell.
    pub fn get_cell_mut(&mut self, col: u32, row: u32) -> Option<&mut Cell> {
        self.ensure_row(row);
        self.rows
            .get_mut(row as usize)
            .and_then(|r| r.cells.get_mut(col as usize))
    }

    /// Set a cell value, creating rows and cells as needed.
    pub fn set_cell(&mut self, col: u32, row: u32, cell: Cell) {
        self.ensure_row(row);
        let row_data = &mut self.rows[row as usize];
        row_data.ensure_column(col);
        row_data.cells[col as usize] = cell;
    }

    /// Get row count.
    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    /// Get column count (from row with most cells).
    pub fn col_count(&self) -> usize {
        self.rows.iter().map(|r| r.cells.len()).max().unwrap_or(0)
    }

    /// Get statistics for the sheet.
    pub fn stats(&self) -> Stats {
        let mut stats = Stats {
            row_count: self.row_count(),
            col_count: self.col_count(),
            ..Default::default()
        };

        for row in &self.rows {
            for cell in &row.cells {
                if cell.formula.is_some() {
                    stats.formula_count += 1;
                }
                if cell.style.is_some() {
                    stats.style_count += 1;
                }
            }
        }

        stats
    }

    fn ensure_row(&mut self, row: u32) {
        while self.rows.len() <= row as usize {
            self.rows.push(Row::default());
        }
    }
}

/// Sheet visibility state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SheetState {
    Visible,
    Hidden,
    VeryHidden,
}

/// A column definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Column {
    pub index: u32,
    pub width: Option<f64>,
    pub hidden: bool,
    pub style: Option<CellStyle>,
    pub best_fit: bool,
}

impl Column {
    pub fn new(index: u32, width: f64) -> Self {
        Column {
            index,
            width: Some(width),
            hidden: false,
            style: None,
            best_fit: false,
        }
    }
}

/// A row definition.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Row {
    pub cells: Vec<Cell>,
    pub height: Option<f64>,
    pub hidden: bool,
    pub style: Option<CellStyle>,
    pub outline_level: u8,
}

impl Row {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_cells(cells: Vec<Cell>) -> Self {
        Row {
            cells,
            ..Default::default()
        }
    }

    pub fn ensure_column(&mut self, col: u32) {
        while self.cells.len() <= col as usize {
            self.cells.push(Cell::empty());
        }
    }
}

/// A merged cell region.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergedRegion {
    pub start_col: u32,
    pub start_row: u32,
    pub end_col: u32,
    pub end_row: u32,
}

/// A table (list object) in a worksheet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Table {
    pub name: String,
    pub display_name: String,
    pub range: String,
    pub columns: Vec<TableColumn>,
    pub style_name: Option<String>,
    pub show_header: bool,
    pub show_totals: bool,
    pub auto_filter: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableColumn {
    pub name: String,
    pub formula: Option<String>,
    pub totals_function: Option<String>,
}

/// A pivot table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PivotTable {
    pub name: String,
    pub source_range: String,
    pub location: String,
    pub rows: Vec<String>,
    pub cols: Vec<String>,
    pub values: Vec<PivotValue>,
    pub filters: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PivotValue {
    pub field: String,
    pub aggregate: PivotAggregate,
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PivotAggregate {
    Sum,
    Count,
    Average,
    Min,
    Max,
    Product,
    StdDev,
    Var,
}

/// A chart in a sheet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chart {
    pub title: Option<String>,
    pub chart_type: ChartType,
    pub source_range: String,
    pub position: String,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChartType {
    Bar,
    Line,
    Pie,
    Scatter,
    Area,
    Column,
    Radar,
}

/// An image in a sheet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Image {
    pub index: u32,
    pub position: String,
    pub width: u32,
    pub height: u32,
    pub description: Option<String>,
    #[serde(default)]
    pub media_path: Option<String>,
    #[serde(default)]
    pub data: Option<Vec<u8>>,
}

/// A shape in a sheet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Shape {
    pub shape_type: String,
    pub position: String,
    pub text: Option<String>,
}

/// A hyperlink in a sheet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hyperlink {
    pub cell: String,
    pub url: String,
    pub text: Option<String>,
    pub is_internal: bool,
}

/// A comment in a sheet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Comment {
    pub cell: String,
    pub text: String,
    pub author: Option<String>,
}

/// A modern Excel comment, including replies within a thread.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadedComment {
    pub id: String,
    pub parent_id: Option<String>,
    pub cell: String,
    pub text: String,
    pub person_id: String,
    pub author: Option<String>,
    pub created_at: Option<String>,
    pub done: bool,
    #[serde(default)]
    pub mentions: Vec<ThreadedMention>,
}

/// A person mentioned at a position in a threaded comment's text.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadedMention {
    pub id: String,
    pub person_id: String,
    pub person_name: Option<String>,
    pub start_index: u32,
    pub length: u32,
}

/// AutoFilter definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoFilter {
    pub range: String,
    pub filters: Vec<FilterColumn>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterColumn {
    pub column: u32,
    pub filter_type: FilterType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FilterType {
    Value(String),
    TopN(u32),
    /// Custom criterion; OOXML reads use the original filterColumn XML for complex rules.
    Custom(String),
    Dynamic(String),
}

/// Page setup / print settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageSetup {
    pub orientation: PageOrientation,
    pub paper_size: Option<String>,
    pub margin_top: Option<f64>,
    pub margin_bottom: Option<f64>,
    pub margin_left: Option<f64>,
    pub margin_right: Option<f64>,
    pub header: Option<String>,
    pub footer: Option<String>,
    pub print_area: Option<String>,
    pub print_title_rows: Option<String>,
    pub print_title_cols: Option<String>,
    pub fit_to_width: Option<u16>,
    pub fit_to_height: Option<u16>,
    pub show_gridlines: bool,
    pub show_headings: bool,
    pub scale: Option<u16>,
}

impl Default for PageSetup {
    fn default() -> Self {
        PageSetup {
            orientation: PageOrientation::Portrait,
            paper_size: None,
            margin_top: None,
            margin_bottom: None,
            margin_left: None,
            margin_right: None,
            header: None,
            footer: None,
            print_area: None,
            print_title_rows: None,
            print_title_cols: None,
            fit_to_width: None,
            fit_to_height: None,
            show_gridlines: false,
            show_headings: false,
            scale: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PageOrientation {
    Portrait,
    Landscape,
}

/// Pane state (frozen panes, split windows).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaneState {
    pub frozen: bool,
    pub freeze_col: u32,
    pub freeze_row: u32,
    pub split_x: Option<f64>,
    pub split_y: Option<f64>,
    pub zoom: Option<u16>,
}

/// Sheet protection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SheetProtection {
    pub password_hash: Option<String>,
    pub allow_sort: bool,
    pub allow_filter: bool,
    pub allow_format_cells: bool,
    pub allow_format_columns: bool,
    pub allow_format_rows: bool,
    pub allow_insert_rows: bool,
    pub allow_insert_columns: bool,
    pub allow_insert_hyperlinks: bool,
    pub allow_delete_rows: bool,
    pub allow_delete_columns: bool,
}

/// Outline / grouping state.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OutlineState {
    pub row_groups: Vec<OutlineGroup>,
    pub col_groups: Vec<OutlineGroup>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutlineGroup {
    pub start: u32,
    pub end: u32,
    pub level: u8,
    pub collapsed: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cell::{Cell, CellValue};

    #[test]
    fn test_sheet_creation() {
        let sheet = Sheet::new("Sheet1");
        assert_eq!(sheet.name, "Sheet1");
        assert_eq!(sheet.state, SheetState::Visible);
        assert_eq!(sheet.row_count(), 0);
    }

    #[test]
    fn test_sheet_set_and_get_cell() {
        let mut sheet = Sheet::new("Test");
        sheet.set_cell(0, 0, Cell::number(42.0));
        sheet.set_cell(1, 5, Cell::string("hello"));

        assert_eq!(sheet.get_cell(0, 0).unwrap().value, CellValue::Number(42.0));
        assert_eq!(
            sheet.get_cell(1, 5).unwrap().value,
            CellValue::InlineString("hello".to_string())
        );
        assert!(sheet.get_cell(0, 10).is_none());
    }

    #[test]
    fn test_sheet_stats() {
        let mut sheet = Sheet::new("Stats");
        sheet.set_cell(0, 0, Cell::number(1.0));
        sheet.set_cell(1, 0, Cell::number(2.0));
        sheet.set_cell(2, 0, Cell::number(3.0));

        let stats = sheet.stats();
        assert_eq!(stats.row_count, 1);
        assert_eq!(stats.col_count, 3);
    }

    #[test]
    fn test_sheet_row_col_count() {
        let mut sheet = Sheet::new("Count");
        sheet.set_cell(0, 0, Cell::empty());
        sheet.set_cell(3, 2, Cell::empty());

        assert_eq!(sheet.row_count(), 3);
        assert_eq!(sheet.col_count(), 4);
    }

    #[test]
    fn test_sheet_overwrite_cell() {
        let mut sheet = Sheet::new("Overwrite");
        sheet.set_cell(0, 0, Cell::string("old"));
        sheet.set_cell(0, 0, Cell::string("new"));
        assert_eq!(
            sheet.get_cell(0, 0).unwrap().value.as_string().unwrap(),
            "new"
        );
    }

    #[test]
    fn test_sheet_clear_cell() {
        let mut sheet = Sheet::new("Clear");
        sheet.set_cell(0, 0, Cell::number(42.0));
        sheet.set_cell(0, 0, Cell::empty());
        assert!(matches!(
            sheet.get_cell(0, 0).unwrap().value,
            CellValue::Empty
        ));
    }

    #[test]
    fn test_sheet_multiple_rows() {
        let mut sheet = Sheet::new("Rows");
        for row in 0..5u32 {
            for col in 0..3u32 {
                sheet.set_cell(col, row, Cell::number((row * 3 + col) as f64));
            }
        }
        assert_eq!(sheet.row_count(), 5);
        assert_eq!(sheet.col_count(), 3);
    }

    #[test]
    fn test_sheet_get_nonexistent_cell() {
        let sheet = Sheet::new("Empty");
        assert!(sheet.get_cell(100, 100).is_none());
    }
}
