use serde::{Deserialize, Serialize};

use crate::error::{ExcliError, ExcliResult};
use crate::reference::{CellReference, column_index_to_name};

/// A cell range in A1 notation (e.g., A1:Z100).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CellRange {
    pub start_col: u32,
    pub start_row: u32,
    pub end_col: u32,
    pub end_row: u32,
    pub sheet: Option<String>,
}

impl CellRange {
    pub fn new(start_col: u32, start_row: u32, end_col: u32, end_row: u32) -> Self {
        CellRange {
            start_col,
            start_row,
            end_col,
            end_row,
            sheet: None,
        }
    }

    pub fn from_a1(s: &str) -> ExcliResult<Self> {
        let s = s.trim();
        if s.is_empty() {
            return Err(ExcliError::range_invalid(s));
        }

        // Handle sheet reference
        let (sheet, range_part) = if let Some(excl_pos) = s.rfind('!') {
            let sheet_name = &s[..excl_pos].trim().trim_matches('\'').to_string();
            (Some(sheet_name.clone()), &s[excl_pos + 1..])
        } else {
            (None, s)
        };

        let parts: Vec<&str> = range_part.split(':').collect();
        match parts.len() {
            1 => {
                // Single cell
                let cell = CellReference::from_a1(parts[0])
                    .ok_or_else(|| ExcliError::range_invalid(s))?;
                Ok(CellRange {
                    start_col: cell.column,
                    start_row: cell.row,
                    end_col: cell.column,
                    end_row: cell.row,
                    sheet,
                })
            }
            2 => {
                let start = CellReference::from_a1(parts[0])
                    .ok_or_else(|| ExcliError::range_invalid(s))?;
                let end = CellReference::from_a1(parts[1])
                    .ok_or_else(|| ExcliError::range_invalid(s))?;
                Ok(CellRange {
                    start_col: start.column,
                    start_row: start.row,
                    end_col: end.column,
                    end_row: end.row,
                    sheet,
                })
            }
            _ => Err(ExcliError::range_invalid(s)),
        }
    }

    pub fn to_a1(&self) -> String {
        let start = format!(
            "{}{}",
            column_index_to_name(self.start_col),
            self.start_row + 1
        );
        let end = format!(
            "{}{}",
            column_index_to_name(self.end_col),
            self.end_row + 1
        );

        let cell_ref = if start == end {
            start
        } else {
            format!("{}:{}", start, end)
        };

        if let Some(ref sheet) = self.sheet {
            if sheet.contains(' ') || sheet.contains('!') {
                format!("'{}'!{}", sheet, cell_ref)
            } else {
                format!("{}!{}", sheet, cell_ref)
            }
        } else {
            cell_ref
        }
    }

    pub fn row_count(&self) -> u32 {
        if self.end_row >= self.start_row {
            self.end_row - self.start_row + 1
        } else {
            self.start_row - self.end_row + 1
        }
    }

    pub fn col_count(&self) -> u32 {
        if self.end_col >= self.start_col {
            self.end_col - self.start_col + 1
        } else {
            self.start_col - self.end_col + 1
        }
    }

    pub fn contains(&self, col: u32, row: u32) -> bool {
        let (min_col, max_col) = if self.start_col <= self.end_col {
            (self.start_col, self.end_col)
        } else {
            (self.end_col, self.start_col)
        };
        let (min_row, max_row) = if self.start_row <= self.end_row {
            (self.start_row, self.end_row)
        } else {
            (self.end_row, self.start_row)
        };
        col >= min_col && col <= max_col && row >= min_row && row <= max_row
    }

    pub fn iter_cells(&self) -> RangeCellIterator {
        RangeCellIterator {
            range: self.clone(),
            current_col: self.start_col,
            current_row: self.start_row,
        }
    }
}

impl std::fmt::Display for CellRange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_a1())
    }
}

pub struct RangeCellIterator {
    range: CellRange,
    current_col: u32,
    current_row: u32,
}

impl Iterator for RangeCellIterator {
    type Item = (u32, u32);

    fn next(&mut self) -> Option<Self::Item> {
        if self.current_row > self.range.end_row {
            return None;
        }
        let result = (self.current_col, self.current_row);
        self.current_col += 1;
        if self.current_col > self.range.end_col {
            self.current_col = self.range.start_col;
            self.current_row += 1;
        }
        Some(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_cell_range() {
        let range = CellRange::from_a1("A1").unwrap();
        assert_eq!(range.start_col, 0);
        assert_eq!(range.start_row, 0);
        assert_eq!(range.end_col, 0);
        assert_eq!(range.end_row, 0);
    }

    #[test]
    fn test_multi_cell_range() {
        let range = CellRange::from_a1("A1:Z10").unwrap();
        assert_eq!(range.row_count(), 10);
        assert_eq!(range.col_count(), 26);
        assert_eq!(range.iter_cells().count(), 260);
    }

    #[test]
    fn test_range_contains() {
        let range = CellRange::from_a1("B2:D5").unwrap();
        assert!(range.contains(1, 1)); // B2
        assert!(range.contains(3, 4)); // D5
        assert!(!range.contains(0, 0)); // A1
        assert!(!range.contains(5, 5)); // F6
    }

    #[test]
    fn test_range_with_sheet() {
        let range = CellRange::from_a1("Sheet1!A1:B10").unwrap();
        assert_eq!(range.sheet, Some("Sheet1".to_string()));
        assert!(range.to_a1().contains("Sheet1"));
    }

    #[test]
    fn test_invalid_range() {
        assert!(CellRange::from_a1("").is_err());
        assert!(CellRange::from_a1("A1:B2:C3").is_err());
    }

    #[test]
    fn test_single_cell_range_new() {
        let range = CellRange::from_a1("C3").unwrap();
        assert_eq!(range.row_count(), 1);
        assert_eq!(range.col_count(), 1);
    }

    #[test]
    fn test_range_iter_order() {
        let range = CellRange::from_a1("A1:B2").unwrap();
        let cells: Vec<_> = range.iter_cells().collect();
        assert_eq!(cells.len(), 4);
    }

    #[test]
    fn test_range_to_a1_roundtrip() {
        let a1 = "A1:D10";
        let range = CellRange::from_a1(a1).unwrap();
        let back = range.to_a1();
        // Should be able to parse it again
        assert!(CellRange::from_a1(&back).is_ok());
    }

    #[test]
    fn test_range_not_contains_boundary() {
        let range = CellRange::from_a1("B2:D4").unwrap();
        // Row 4 is last included row (0-indexed: row=3, col=3)
        assert!(range.contains(3, 3));  // D4 → col=3, row=3
        // Row 5 is outside
        assert!(!range.contains(3, 4)); // D5 → outside
    }
}
