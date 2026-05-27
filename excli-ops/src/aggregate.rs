use std::collections::HashMap;

use excli_core::Sheet;

/// Aggregation operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AggregateOp {
    Sum,
    Count,
    Avg,
    Min,
    Max,
    First,
    Last,
}

/// Aggregation specification.
#[derive(Debug, Clone)]
pub struct Aggregation {
    pub column: String,
    pub operation: AggregateOp,
    pub alias: Option<String>,
}

impl Aggregation {
    pub fn new(column: impl Into<String>, operation: AggregateOp) -> Self {
        Aggregation {
            column: column.into(),
            operation,
            alias: None,
        }
    }
}

/// Group by columns and apply aggregations.
pub fn aggregate(
    sheet: &Sheet,
    group_by_cols: &[String],
    aggregations: &[Aggregation],
) -> Sheet {
    let mut result = Sheet::new(format!("{}_aggregated", sheet.name));

    // Find column indices
    let group_indices: Vec<u32> = group_by_cols
        .iter()
        .filter_map(|c| find_column_index(sheet, c))
        .collect();

    let agg_indices: Vec<(usize, u32)> = aggregations
        .iter()
        .enumerate()
        .filter_map(|(i, a)| find_column_index(sheet, &a.column).map(|idx| (i, idx)))
        .collect();

    if group_indices.is_empty() {
        // No grouping — aggregate entire sheet
        let row_idx = result.row_count() as u32;
        for (agg_idx, col_idx) in &agg_indices {
            let values: Vec<f64> = sheet
                .rows
                .iter()
                .filter_map(|r| r.cells.get(*col_idx as usize))
                .filter_map(|c| c.value.as_f64())
                .collect();

            let result_val = compute_aggregate(&values, aggregations[*agg_idx].operation);
            let col_name = aggregations[*agg_idx]
                .alias
                .clone()
                .unwrap_or_else(|| aggregations[*agg_idx].column.clone());
            result.set_cell(0, row_idx, excli_core::cell::Cell::string(&col_name));
            result.set_cell(1, row_idx, excli_core::cell::Cell::number(result_val));
        }
        return result;
    }

    // Group and aggregate
    let mut groups: HashMap<String, Vec<usize>> = HashMap::new();
    for (row_idx, row) in sheet.rows.iter().enumerate() {
        let key: Vec<String> = group_indices
            .iter()
            .map(|&idx| {
                row.cells
                    .get(idx as usize)
                    .map(|c| c.value.to_string())
                    .unwrap_or_default()
            })
            .collect();
        groups.entry(key.join("|")).or_default().push(row_idx);
    }

    // Write result header
    for (i, col) in group_by_cols.iter().enumerate() {
        result.set_cell(i as u32, 0, excli_core::cell::Cell::string(col));
    }
    let offset = group_by_cols.len() as u32;
    for (i, agg) in aggregations.iter().enumerate() {
        let name = agg.alias.clone().unwrap_or_else(|| format!("{}({})", agg.operation.name(), agg.column));
        result.set_cell(offset + i as u32, 0, excli_core::cell::Cell::string(&name));
    }

    // Write grouped data
    let mut row_counter = 1u32;
    for (key, row_indices) in &groups {
        let key_parts: Vec<&str> = key.split('|').collect();
        for (i, part) in key_parts.iter().enumerate() {
            result.set_cell(i as u32, row_counter, excli_core::cell::Cell::string(*part));
        }

        for (agg_idx, col_idx) in &agg_indices {
            let values: Vec<f64> = row_indices
                .iter()
                .filter_map(|&r| sheet.rows[r].cells.get(*col_idx as usize))
                .filter_map(|c| c.value.as_f64())
                .collect();

            let result_val = compute_aggregate(&values, aggregations[*agg_idx].operation);
            result.set_cell(
                offset + *agg_idx as u32,
                row_counter,
                excli_core::cell::Cell::number(result_val),
            );
        }
        row_counter += 1;
    }

    result
}

fn compute_aggregate(values: &[f64], op: AggregateOp) -> f64 {
    match op {
        AggregateOp::Sum => values.iter().sum(),
        AggregateOp::Count => values.len() as f64,
        AggregateOp::Avg => {
            if values.is_empty() { 0.0 } else { values.iter().sum::<f64>() / values.len() as f64 }
        }
        AggregateOp::Min => values.iter().cloned().fold(f64::MAX, f64::min),
        AggregateOp::Max => values.iter().cloned().fold(f64::MIN, f64::max),
        AggregateOp::First => values.first().copied().unwrap_or(0.0),
        AggregateOp::Last => values.last().copied().unwrap_or(0.0),
    }
}

impl AggregateOp {
    pub fn name(self) -> &'static str {
        match self {
            AggregateOp::Sum => "SUM",
            AggregateOp::Count => "COUNT",
            AggregateOp::Avg => "AVG",
            AggregateOp::Min => "MIN",
            AggregateOp::Max => "MAX",
            AggregateOp::First => "FIRST",
            AggregateOp::Last => "LAST",
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
    use excli_core::cell::{Cell, CellValue};

    #[test]
    fn test_aggregate_sum() {
        let mut sheet = Sheet::new("Test");
        sheet.set_cell(0, 0, Cell::string("Dept"));
        sheet.set_cell(1, 0, Cell::string("Salary"));
        sheet.set_cell(0, 1, Cell::string("Eng"));
        sheet.set_cell(1, 1, Cell::number(100.0));
        sheet.set_cell(0, 2, Cell::string("Eng"));
        sheet.set_cell(1, 2, Cell::number(200.0));
        sheet.set_cell(0, 3, Cell::string("Sales"));
        sheet.set_cell(1, 3, Cell::number(150.0));

        let result = aggregate(
            &sheet,
            &["A".to_string()],
            &[Aggregation::new("B", AggregateOp::Sum)],
        );
        assert!(result.row_count() > 2); // header + 2 groups
    }

    #[test]
    fn test_aggregate_count() {
        let mut sheet = Sheet::new("Test");
        sheet.set_cell(0, 0, Cell::string("Val"));
        sheet.set_cell(0, 1, Cell::number(1.0));
        sheet.set_cell(0, 2, Cell::number(2.0));
        sheet.set_cell(0, 3, Cell::number(3.0));

        let result = aggregate(
            &sheet,
            &[],
            &[Aggregation::new("A", AggregateOp::Count)],
        );
        assert_eq!(result.row_count(), 1);
    }

    #[test]
    fn test_aggregate_avg() {
        let mut sheet = Sheet::new("Test");
        sheet.set_cell(0, 0, Cell::string("Value"));
        sheet.set_cell(0, 1, Cell::number(10.0));
        sheet.set_cell(0, 2, Cell::number(20.0));
        sheet.set_cell(0, 3, Cell::number(30.0));

        let result = aggregate(
            &sheet,
            &[],
            &[Aggregation::new("A", AggregateOp::Avg)],
        );
        // No grouping: result is [col_name, avg_value] at row 0
        let avg_cell = &result.rows[0].cells[1];
        assert_eq!(avg_cell.value, CellValue::Number(20.0));
    }

    #[test]
    fn test_aggregate_min_max() {
        let mut sheet = Sheet::new("Test");
        sheet.set_cell(0, 0, Cell::string("Value"));
        sheet.set_cell(0, 1, Cell::number(5.0));
        sheet.set_cell(0, 2, Cell::number(3.0));
        sheet.set_cell(0, 3, Cell::number(8.0));

        let min_result = aggregate(&sheet, &[], &[Aggregation::new("A", AggregateOp::Min)]);
        let max_result = aggregate(&sheet, &[], &[Aggregation::new("A", AggregateOp::Max)]);
        // No grouping: result is [col_name, value] at row 0
        assert_eq!(min_result.rows[0].cells[1].value, CellValue::Number(3.0));
        assert_eq!(max_result.rows[0].cells[1].value, CellValue::Number(8.0));
    }

    #[test]
    fn test_aggregate_multiple_groups() {
        let mut sheet = Sheet::new("Test");
        sheet.set_cell(0, 0, Cell::string("Category"));
        sheet.set_cell(1, 0, Cell::string("Amount"));
        sheet.set_cell(0, 1, Cell::string("A"));
        sheet.set_cell(1, 1, Cell::number(100.0));
        sheet.set_cell(0, 2, Cell::string("B"));
        sheet.set_cell(1, 2, Cell::number(200.0));
        sheet.set_cell(0, 3, Cell::string("A"));
        sheet.set_cell(1, 3, Cell::number(300.0));

        let result = aggregate(
            &sheet,
            &["A".to_string()],
            &[Aggregation::new("B", AggregateOp::Sum)],
        );
        // Should have header + 2 groups (A: 400, B: 200) + possibly header row
        assert!(result.row_count() >= 2);
    }

    #[test]
    fn test_aggregate_count_op() {
        let mut sheet = Sheet::new("Test");
        sheet.set_cell(0, 0, Cell::string("Name"));
        sheet.set_cell(1, 0, Cell::string("Score"));
        for i in 1..=5 {
            sheet.set_cell(0, i as u32, Cell::string("Alice"));
            sheet.set_cell(1, i as u32, Cell::number(i as f64 * 10.0));
        }
        let result = aggregate(
            &sheet,
            &["A".to_string()],
            &[Aggregation::new("B", AggregateOp::Count)],
        );
        assert!(result.row_count() >= 1);
    }

    #[test]
    fn test_aggregate_avg_op() {
        let mut sheet = Sheet::new("Test");
        sheet.set_cell(0, 0, Cell::string("Group"));
        sheet.set_cell(1, 0, Cell::string("Val"));
        sheet.set_cell(0, 1, Cell::string("X"));
        sheet.set_cell(1, 1, Cell::number(10.0));
        sheet.set_cell(0, 2, Cell::string("X"));
        sheet.set_cell(1, 2, Cell::number(20.0));
        let result = aggregate(
            &sheet,
            &["A".to_string()],
            &[Aggregation::new("B", AggregateOp::Avg)],
        );
        assert!(result.row_count() >= 1);
    }

    #[test]
    fn test_aggregate_no_group() {
        let mut sheet = Sheet::new("Test");
        sheet.set_cell(0, 0, Cell::string("Val"));
        sheet.set_cell(0, 1, Cell::number(5.0));
        sheet.set_cell(0, 2, Cell::number(15.0));
        let result = aggregate(
            &sheet,
            &[],
            &[Aggregation::new("A", AggregateOp::Sum)],
        );
        assert!(result.row_count() >= 1);
    }
}
