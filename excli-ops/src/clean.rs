use excli_core::cell::{Cell, CellValue};
use excli_core::Sheet;
use excli_core::range::CellRange;

/// Case conversion type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaseType {
    Upper,
    Lower,
    Proper,
}

/// Trim whitespace from cells in a range.
pub fn clean_trim(sheet: &Sheet, range: Option<&CellRange>) -> Sheet {
    apply_transform(sheet, range, |val| {
        if let CellValue::InlineString(s) = val {
            CellValue::InlineString(s.trim().to_string())
        } else {
            val.clone()
        }
    })
}

/// Change case of text in cells.
pub fn clean_case(sheet: &Sheet, range: Option<&CellRange>, case_type: CaseType) -> Sheet {
    apply_transform(sheet, range, |val| {
        if let CellValue::InlineString(s) = val {
            let new_str = match case_type {
                CaseType::Upper => s.to_uppercase(),
                CaseType::Lower => s.to_lowercase(),
                CaseType::Proper => {
                    let mut result = String::new();
                    let mut capitalize = true;
                    for ch in s.chars() {
                        if ch.is_whitespace() {
                            capitalize = true;
                            result.push(ch);
                        } else if capitalize {
                            result.extend(ch.to_uppercase());
                            capitalize = false;
                        } else {
                            result.extend(ch.to_lowercase());
                        }
                    }
                    result
                }
            };
            CellValue::InlineString(new_str)
        } else {
            val.clone()
        }
    })
}

/// Fill or remove blanks.
pub fn clean_blanks(
    sheet: &Sheet,
    range: Option<&CellRange>,
    fill_strategy: BlankFillStrategy,
) -> Sheet {
    let mut result = sheet.clone();

    match fill_strategy {
        BlankFillStrategy::DropRows => {
            let mut new_sheet = Sheet::new(format!("{}_cleaned", sheet.name));
            for row in &sheet.rows {
                let all_empty = row.cells.iter().all(|c| c.value.is_empty());
                if !all_empty {
                    let row_idx = new_sheet.row_count() as u32;
                    for (col_idx, cell) in row.cells.iter().enumerate() {
                        new_sheet.set_cell(col_idx as u32, row_idx, cell.clone());
                    }
                }
            }
            return new_sheet;
        }
        BlankFillStrategy::FillValue(val) => {
            if let Some(ref range) = range {
                for (col, row) in range.iter_cells() {
                    if let Some(cell) = result.get_cell_mut(col, row) {
                        if cell.value.is_empty() {
                            cell.value = val.clone();
                        }
                    }
                }
            }
        }
        BlankFillStrategy::FillForward => {
            // Forward fill within each column
            let max_col = sheet.col_count();
            for col in 0..max_col {
                let mut last_value = CellValue::Empty;
                for row_idx in 0..sheet.row_count() {
                    if let Some(cell) = result.get_cell_mut(col as u32, row_idx as u32) {
                        if cell.value.is_empty() {
                            cell.value = last_value.clone();
                        } else {
                            last_value = cell.value.clone();
                        }
                    }
                }
            }
        }
        BlankFillStrategy::FillInterpolate => {
            // Linear interpolation for numeric columns
            let max_col = sheet.col_count();
            for col in 0..max_col {
                let values: Vec<Option<f64>> = (0..sheet.row_count())
                    .map(|r| {
                        sheet
                            .get_cell(col as u32, r as u32)
                            .and_then(|c| c.value.as_f64())
                    })
                    .collect();

                let interpolated = linear_interpolate(&values);
                for (row_idx, val) in interpolated.iter().enumerate() {
                    if let Some(v) = val {
                        result.set_cell(col as u32, row_idx as u32, Cell::number(*v));
                    }
                }
            }
        }
    }

    result
}

fn linear_interpolate(values: &[Option<f64>]) -> Vec<Option<f64>> {
    let mut result = values.to_vec();
    let n = values.len();
    let mut i = 0;
    while i < n {
        if result[i].is_none() {
            // Find previous non-null
            let prev_idx = (0..i).rev().find(|&j| result[j].is_some());
            // Find next non-null
            let next_idx = ((i + 1)..n).find(|&j| result[j].is_some());

            match (prev_idx, next_idx) {
                (Some(p), Some(nxt)) => {
                    let prev_val = result[p].unwrap();
                    let next_val = result[nxt].unwrap();
                    let steps = (nxt - p) as f64;
                    for j in (p + 1)..nxt {
                        let frac = (j - p) as f64 / steps;
                        result[j] = Some(prev_val + (next_val - prev_val) * frac);
                    }
                    i = nxt + 1;
                }
                _ => {
                    i += 1;
                }
            }
        } else {
            i += 1;
        }
    }
    result
}

/// Strategy for handling blank cells.
#[derive(Debug, Clone)]
pub enum BlankFillStrategy {
    DropRows,
    FillValue(CellValue),
    FillForward,
    FillInterpolate,
}

/// Remove duplicate rows based on specified columns.
pub fn remove_duplicates(sheet: &Sheet, columns: &[String]) -> Sheet {
    let mut result = Sheet::new(format!("{}_deduped", sheet.name));
    let mut seen = std::collections::HashSet::new();

    let col_indices: Vec<u32> = columns
        .iter()
        .filter_map(|c| {
            if let Some(idx) = excli_core::reference::column_name_to_index(c) {
                Some(idx)
            } else if let Some(header_row) = sheet.rows.first() {
                header_row
                    .cells
                    .iter()
                    .position(|cell| cell.value.to_string().eq_ignore_ascii_case(c))
                    .map(|i| i as u32)
            } else {
                None
            }
        })
        .collect();

    for row in &sheet.rows {
        let key: Vec<String> = if col_indices.is_empty() {
            row.cells.iter().map(|c| c.value.to_string()).collect()
        } else {
            col_indices
                .iter()
                .map(|&idx| {
                    row.cells
                        .get(idx as usize)
                        .map(|c| c.value.to_string())
                        .unwrap_or_default()
                })
                .collect()
        };

        if seen.insert(key) {
            let row_idx = result.row_count() as u32;
            for (col_idx, cell) in row.cells.iter().enumerate() {
                result.set_cell(col_idx as u32, row_idx, cell.clone());
            }
        }
    }

    result
}

fn apply_transform(
    sheet: &Sheet,
    range: Option<&CellRange>,
    transform: impl Fn(&CellValue) -> CellValue,
) -> Sheet {
    let mut result = sheet.clone();

    if let Some(range) = range {
        for (col, row) in range.iter_cells() {
            if let Some(cell) = result.get_cell_mut(col, row) {
                cell.value = transform(&cell.value);
            }
        }
    } else {
        for row in &mut result.rows {
            for cell in &mut row.cells {
                cell.value = transform(&cell.value);
            }
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trim() {
        let mut sheet = Sheet::new("Test");
        sheet.set_cell(0, 0, Cell::string("  hello  "));
        sheet.set_cell(0, 1, Cell::string("world"));

        let result = clean_trim(&sheet, None);
        assert_eq!(result.get_cell(0, 0).unwrap().value.as_string().unwrap(), "hello");
    }

    #[test]
    fn test_upper_case() {
        let mut sheet = Sheet::new("Test");
        sheet.set_cell(0, 0, Cell::string("hello"));

        let result = clean_case(&sheet, None, CaseType::Upper);
        assert_eq!(result.get_cell(0, 0).unwrap().value.as_string().unwrap(), "HELLO");
    }

    #[test]
    fn test_lower_case() {
        let mut sheet = Sheet::new("Test");
        sheet.set_cell(0, 0, Cell::string("HELLO"));

        let result = clean_case(&sheet, None, CaseType::Lower);
        assert_eq!(result.get_cell(0, 0).unwrap().value.as_string().unwrap(), "hello");
    }

    #[test]
    fn test_remove_duplicates() {
        let mut sheet = Sheet::new("Test");
        sheet.set_cell(0, 0, Cell::string("A"));
        sheet.set_cell(0, 1, Cell::string("A"));
        sheet.set_cell(0, 2, Cell::string("B"));

        let result = remove_duplicates(&sheet, &[]);
        assert_eq!(result.row_count(), 2); // A, B
    }

    #[test]
    fn test_fill_forward() {
        let mut sheet = Sheet::new("Test");
        sheet.set_cell(0, 0, Cell::string("Hello"));
        sheet.set_cell(0, 1, Cell::empty());
        sheet.set_cell(0, 2, Cell::empty());

        let result = clean_blanks(&sheet, None, BlankFillStrategy::FillForward);
        assert_eq!(result.get_cell(0, 1).unwrap().value.as_string().unwrap(), "Hello");
        assert_eq!(result.get_cell(0, 2).unwrap().value.as_string().unwrap(), "Hello");
    }

    #[test]
    fn test_linear_interpolate() {
        let values = vec![Some(0.0), None, None, Some(3.0)];
        let result = linear_interpolate(&values);
        assert_eq!(result[1], Some(1.0));
        assert_eq!(result[2], Some(2.0));
    }

    #[test]
    fn test_clean_trim_multiple_spaces() {
        let mut sheet = Sheet::new("Test");
        sheet.set_cell(0, 0, Cell::string("  hello   world  "));
        let result = clean_trim(&sheet, None);
        assert_eq!(result.get_cell(0, 0).unwrap().value.as_string().unwrap(), "hello   world");
    }

    #[test]
    fn test_upper_case_new() {
        let mut sheet = Sheet::new("Test");
        sheet.set_cell(0, 0, Cell::string("hello world"));
        let result = clean_case(&sheet, None, CaseType::Upper);
        assert_eq!(result.get_cell(0, 0).unwrap().value.as_string().unwrap(), "HELLO WORLD");
    }

    #[test]
    fn test_proper_case() {
        let mut sheet = Sheet::new("Test");
        sheet.set_cell(0, 0, Cell::string("hello world"));
        let result = clean_case(&sheet, None, CaseType::Proper);
        let val = result.get_cell(0, 0).unwrap().value.as_string().unwrap();
        // Proper case capitalizes first letter of each word
        assert!(val.starts_with('H'));
    }

    #[test]
    fn test_remove_duplicates_multi_row() {
        let mut sheet = Sheet::new("Test");
        sheet.set_cell(0, 0, Cell::string("A"));
        sheet.set_cell(1, 0, Cell::string("X"));
        sheet.set_cell(0, 1, Cell::string("A"));
        sheet.set_cell(1, 1, Cell::string("X"));
        sheet.set_cell(0, 2, Cell::string("B"));
        sheet.set_cell(1, 2, Cell::string("Y"));

        let result = remove_duplicates(&sheet, &[]);
        assert_eq!(result.row_count(), 2); // row(A,X) + row(B,Y)
    }

    #[test]
    fn test_fill_drop_rows() {
        let mut sheet = Sheet::new("Test");
        sheet.set_cell(0, 0, Cell::string("Hello"));
        sheet.set_cell(0, 1, Cell::empty());
        sheet.set_cell(0, 2, Cell::string("World"));

        let result = clean_blanks(&sheet, None, BlankFillStrategy::DropRows);
        assert_eq!(result.row_count(), 2); // Hello + World
    }
}
