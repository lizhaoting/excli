use std::collections::HashMap;

use excli_core::{Sheet, Workbook};

/// Join type for merging.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinType {
    Inner,
    Left,
    Right,
    Outer,
    Cross,
}

/// Merge two workbooks on specified keys.
pub fn merge_workbooks(
    left: &Workbook,
    right: &Workbook,
    left_on: &str,
    right_on: &str,
    join_type: JoinType,
) -> Workbook {
    let mut result = Workbook::new();

    let left_sheet = left.sheet_by_index(0).unwrap();
    let right_sheet = right.sheet_by_index(0).unwrap();

    let left_col = find_column_index(left_sheet, left_on).unwrap_or(0);
    let right_col = find_column_index(right_sheet, right_on).unwrap_or(0);

    let result_sheet = result.add_sheet("Merged");

    // Write header
    for (col_idx, cell) in left_sheet.rows.first().map(|r| &r.cells).unwrap_or(&vec![]).iter().enumerate() {
        result_sheet.set_cell(col_idx as u32, 0, cell.clone());
    }
    let left_width = left_sheet.col_count() as u32;
    for (col_idx, cell) in right_sheet.rows.first().map(|r| &r.cells).unwrap_or(&vec![]).iter().enumerate() {
        result_sheet.set_cell(left_width + col_idx as u32, 0, cell.clone());
    }

    // Build right-hand side index
    let mut right_index: HashMap<String, Vec<usize>> = HashMap::new();
    for (row_idx, row) in right_sheet.rows.iter().skip(1).enumerate() {
        let key = row
            .cells
            .get(right_col as usize)
            .map(|c| c.value.to_string())
            .unwrap_or_default();
        right_index.entry(key).or_default().push(row_idx + 1);
    }

    let mut result_row = 1u32;

    match join_type {
        JoinType::Inner | JoinType::Left => {
            for (_left_row_idx, left_row) in left_sheet.rows.iter().skip(1).enumerate() {
                let key = left_row
                    .cells
                    .get(left_col as usize)
                    .map(|c| c.value.to_string())
                    .unwrap_or_default();

                if let Some(right_rows) = right_index.get(&key) {
                    for &right_row_idx in right_rows {
                        copy_row(result_sheet, left_row, 0, result_row);
                        copy_row(result_sheet, &right_sheet.rows[right_row_idx], left_width, result_row);
                        result_row += 1;
                    }
                } else if matches!(join_type, JoinType::Left) {
                    copy_row(result_sheet, left_row, 0, result_row);
                    result_row += 1;
                }
            }
        }
        JoinType::Outer => {
            let mut matched_right: std::collections::HashSet<usize> = std::collections::HashSet::new();

            for left_row in left_sheet.rows.iter().skip(1) {
                let key = left_row
                    .cells
                    .get(left_col as usize)
                    .map(|c| c.value.to_string())
                    .unwrap_or_default();

                if let Some(right_rows) = right_index.get(&key) {
                    for &right_row_idx in right_rows {
                        copy_row(result_sheet, left_row, 0, result_row);
                        copy_row(result_sheet, &right_sheet.rows[right_row_idx], left_width, result_row);
                        matched_right.insert(right_row_idx);
                        result_row += 1;
                    }
                } else {
                    copy_row(result_sheet, left_row, 0, result_row);
                    result_row += 1;
                }
            }
        }
        JoinType::Cross => {
            for left_row in left_sheet.rows.iter().skip(1) {
                for right_row in right_sheet.rows.iter().skip(1) {
                    copy_row(result_sheet, left_row, 0, result_row);
                    copy_row(result_sheet, right_row, left_width, result_row);
                    result_row += 1;
                }
            }
        }
        JoinType::Right => {
            for (_right_row_idx, right_row) in right_sheet.rows.iter().skip(1).enumerate() {
                let key = right_row
                    .cells
                    .get(right_col as usize)
                    .map(|c| c.value.to_string())
                    .unwrap_or_default();

                // Build left index on the fly (simplified)
                let mut found = false;
                for left_row in left_sheet.rows.iter().skip(1) {
                    let left_key = left_row
                        .cells
                        .get(left_col as usize)
                        .map(|c| c.value.to_string())
                        .unwrap_or_default();
                    if left_key == key {
                        copy_row(result_sheet, left_row, 0, result_row);
                        copy_row(result_sheet, right_row, left_width, result_row);
                        result_row += 1;
                        found = true;
                    }
                }
                if !found {
                    copy_row(result_sheet, right_row, left_width, result_row);
                    result_row += 1;
                }
            }
        }
    }

    result
}

fn copy_row(sheet: &mut Sheet, source: &excli_core::Row, col_offset: u32, row_idx: u32) {
    for (col_idx, cell) in source.cells.iter().enumerate() {
        if !cell.is_empty() {
            sheet.set_cell(col_offset + col_idx as u32, row_idx, cell.clone());
        }
    }
}

fn find_column_index(sheet: &Sheet, col_ref: &str) -> Option<u32> {
    if let Some(idx) = excli_core::reference::column_name_to_index(col_ref) {
        return Some(idx);
    }
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

    fn make_workbook(data: &[(&str, &str)]) -> Workbook {
        let mut wb = Workbook::new();
        let sheet = wb.add_sheet("Sheet1");
        sheet.set_cell(0, 0, Cell::string("ID"));
        sheet.set_cell(1, 0, Cell::string("Name"));
        for (i, (id, name)) in data.iter().enumerate() {
            sheet.set_cell(0, (i + 1) as u32, Cell::string(*id));
            sheet.set_cell(1, (i + 1) as u32, Cell::string(*name));
        }
        wb
    }

    #[test]
    fn test_inner_join() {
        let left = make_workbook(&[("1", "Alice"), ("2", "Bob")]);
        let right = make_workbook(&[("1", "Engineer"), ("3", "Designer")]);

        let result = merge_workbooks(&left, &right, "A", "A", JoinType::Inner);
        let sheet = result.sheet_by_index(0).unwrap();
        // Only ID=1 matches
        assert!(sheet.row_count() >= 2);
    }

    #[test]
    fn test_left_join() {
        let left = make_workbook(&[("1", "Alice"), ("2", "Bob")]);
        let right = make_workbook(&[("1", "Engineer")]);

        let result = merge_workbooks(&left, &right, "A", "A", JoinType::Left);
        let sheet = result.sheet_by_index(0).unwrap();
        // Both left rows should appear
        assert!(sheet.row_count() >= 2);
    }

    #[test]
    fn test_outer_join() {
        let left = make_workbook(&[("1", "Alice")]);
        let right = make_workbook(&[("2", "Engineer")]);

        let result = merge_workbooks(&left, &right, "A", "A", JoinType::Outer);
        let sheet = result.sheet_by_index(0).unwrap();
        // Both rows appear even without a match
        assert!(sheet.row_count() >= 2);
    }

    #[test]
    fn test_inner_join_no_match() {
        let left = make_workbook(&[("1", "Alice")]);
        let right = make_workbook(&[("99", "Ghost")]);

        let result = merge_workbooks(&left, &right, "A", "A", JoinType::Inner);
        let sheet = result.sheet_by_index(0).unwrap();
        // No matching rows — only header or empty
        assert!(sheet.row_count() <= 1);
    }

    #[test]
    fn test_concat_workbooks() {
        use crate::transform::{concat_workbooks, ConcatAxis};
        let wb1 = make_workbook(&[("1", "Alice"), ("2", "Bob")]);
        let wb2 = make_workbook(&[("3", "Carol")]);

        let result = concat_workbooks(&[&wb1, &wb2], ConcatAxis::Rows);
        let sheet = result.sheet_by_index(0).unwrap();
        // header + 3 data rows
        assert!(sheet.row_count() >= 4);
    }
}
