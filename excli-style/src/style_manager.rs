use excli_core::cell::Cell;
use excli_core::range::CellRange;
use excli_core::style::CellStyle;
use excli_core::Sheet;

/// Manages cell styles within a sheet.
pub struct StyleManager;

impl StyleManager {
    /// Read style information for cells in a range.
    pub fn read_styles(sheet: &Sheet, range: Option<&CellRange>) -> Vec<StyleInfo> {
        let mut results = Vec::new();

        if let Some(range) = range {
            for (col, row) in range.iter_cells() {
                if let Some(cell) = sheet.get_cell(col, row) {
                    if let Some(ref style) = cell.style {
                        results.push(StyleInfo {
                            cell_ref: excli_core::reference::CellReference::new(col, row).to_a1(),
                            style: style.clone(),
                        });
                    }
                }
            }
        } else {
            for (row_idx, row) in sheet.rows.iter().enumerate() {
                for (col_idx, cell) in row.cells.iter().enumerate() {
                    if let Some(ref style) = cell.style {
                        results.push(StyleInfo {
                            cell_ref: excli_core::reference::CellReference::new(
                                col_idx as u32,
                                row_idx as u32,
                            )
                            .to_a1(),
                            style: style.clone(),
                        });
                    }
                }
            }
        }

        results
    }

    /// Set styles for cells in a range.
    pub fn set_styles(sheet: &mut Sheet, range: &CellRange, style: &CellStyle) -> usize {
        let mut count = 0;
        for (col, row) in range.iter_cells() {
            sheet.set_cell(col, row, excli_core::cell::Cell::empty().with_style(style.clone()));
            count += 1;
        }
        count
    }

    /// Copy style from one cell to another range (format painter).
    pub fn copy_style(sheet: &mut Sheet, from: (&str, u32, u32), to_range: &CellRange) -> usize {
        let from_col = from.1;
        let from_row = from.2;

        let source_style = sheet
            .get_cell(from_col, from_row)
            .and_then(|c| c.style.clone());

        if let Some(style) = source_style {
            Self::set_styles(sheet, to_range, &style)
        } else {
            0
        }
    }

    /// Clear styles from cells in a range.
    pub fn clear_styles(
        sheet: &mut Sheet,
        range: &CellRange,
        _formats_only: bool,
    ) -> usize {
        let mut count = 0;
        for (col, row) in range.iter_cells() {
            sheet.set_cell(col, row, Cell::empty());
            count += 1;
        }
        count
    }

    /// Set row height.
    pub fn set_row_height(sheet: &mut Sheet, row: u32, height: f64) {
        if let Some(row_data) = sheet.rows.get_mut(row as usize) {
            row_data.height = Some(height);
        }
    }

    /// Set column width.
    pub fn set_column_width(sheet: &mut Sheet, col: u32, width: f64) {
        while sheet.columns.len() <= col as usize {
            sheet.columns.push(excli_core::sheet::Column::new(
                sheet.columns.len() as u32,
                width,
            ));
        }
        sheet.columns[col as usize].width = Some(width);
    }

    /// Merge cells in a range.
    pub fn merge_cells(sheet: &mut Sheet, range: &CellRange) {
        let merged = excli_core::sheet::MergedRegion {
            start_col: range.start_col,
            start_row: range.start_row,
            end_col: range.end_col,
            end_row: range.end_row,
        };
        sheet.merged_cells.push(merged);
    }

    /// Unmerge cells in a range.
    pub fn unmerge_cells(sheet: &mut Sheet, range: &CellRange) {
        sheet.merged_cells.retain(|m| {
            !(m.start_col == range.start_col
                && m.start_row == range.start_row
                && m.end_col == range.end_col
                && m.end_row == range.end_row)
        });
    }
}

/// Style information for a cell.
#[derive(Debug, Clone)]
pub struct StyleInfo {
    pub cell_ref: String,
    pub style: CellStyle,
}

impl std::fmt::Display for StyleInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: ", self.cell_ref)?;
        if let Some(ref font) = self.style.font {
            if let Some(ref name) = font.name {
                write!(f, "font={}, ", name)?;
            }
            if let Some(size) = font.size {
                write!(f, "size={}, ", size)?;
            }
            if font.bold == Some(true) {
                write!(f, "bold, ")?;
            }
            if font.italic == Some(true) {
                write!(f, "italic, ")?;
            }
        }
        if let Some(ref fill) = self.style.fill {
            if let Some(ref color) = fill.fg_color {
                write!(f, "bg={}, ", color)?;
            }
        }
        if let Some(ref nf) = self.style.number_format {
            write!(f, "fmt={}, ", nf)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_and_read_style() {
        let mut sheet = Sheet::new("Test");
        let range = CellRange::new(0, 0, 2, 3);

        let style = CellStyle::bold();
        let count = StyleManager::set_styles(&mut sheet, &range, &style);
        assert_eq!(count, 12); // 3x4 = 12 cells

        let styles = StyleManager::read_styles(&sheet, Some(&range));
        assert_eq!(styles.len(), 12);
    }

    #[test]
    fn test_clear_styles() {
        let mut sheet = Sheet::new("Test");
        let range = CellRange::new(0, 0, 0, 0);
        StyleManager::set_styles(&mut sheet, &range, &CellStyle::bold());
        StyleManager::clear_styles(&mut sheet, &range, true);
        assert!(sheet.get_cell(0, 0).unwrap().style.is_none());
    }

    #[test]
    fn test_merge_cells() {
        let mut sheet = Sheet::new("Test");
        let range = CellRange::new(0, 0, 2, 0);
        StyleManager::merge_cells(&mut sheet, &range);
        assert_eq!(sheet.merged_cells.len(), 1);
    }

    #[test]
    fn test_unmerge_cells() {
        let mut sheet = Sheet::new("Test");
        let range = CellRange::new(0, 0, 2, 0);
        StyleManager::merge_cells(&mut sheet, &range);
        StyleManager::unmerge_cells(&mut sheet, &range);
        assert_eq!(sheet.merged_cells.len(), 0);
    }

    #[test]
    fn test_set_row_height() {
        use excli_core::cell::{Cell, CellValue};
        let mut sheet = Sheet::new("Test");
        // Populate the row first so it exists
        sheet.set_cell(0, 0, Cell::new(CellValue::Empty));
        StyleManager::set_row_height(&mut sheet, 0, 25.0);
        assert_eq!(sheet.rows[0].height, Some(25.0));
    }

    #[test]
    fn test_set_column_width() {
        let mut sheet = Sheet::new("Test");
        StyleManager::set_column_width(&mut sheet, 0, 20.0);
        assert_eq!(sheet.columns[0].width, Some(20.0));
    }

    #[test]
    fn test_copy_style() {
        let mut sheet = Sheet::new("Test");
        let range = CellRange::new(0, 0, 0, 0);
        StyleManager::set_styles(&mut sheet, &range, &CellStyle::bold());
        let to_range = CellRange::new(1, 0, 2, 0);
        let count = StyleManager::copy_style(&mut sheet, ("A1", 0, 0), &to_range);
        assert_eq!(count, 2);
    }
}
