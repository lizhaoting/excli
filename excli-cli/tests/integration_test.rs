//! Integration tests — end-to-end data pipelines using the library APIs directly.

use std::io::Write;

use excli_core::cell::Cell;
use excli_core::Workbook;
use excli_io::{reader, writer};
use excli_ops::{
    aggregate::{aggregate, AggregateOp, Aggregation},
    filter::{filter_rows, filter_unique, FilterCondition, FilterOperator},
    merge::{merge_workbooks, JoinType},
    sort::{sort_rows, SortOrder, SortSpec},
    transform::{concat_workbooks, transpose, ConcatAxis},
};

// ── helpers ──────────────────────────────────────────────────────────────────

fn temp_csv(content: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.csv");
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(content.as_bytes()).unwrap();
    (dir, path)
}

fn write_temp_csv(dir: &tempfile::TempDir, name: &str, content: &str) -> std::path::PathBuf {
    let path = dir.path().join(name);
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(content.as_bytes()).unwrap();
    path
}

#[test]
fn cli_replace_edits_xlsx_without_rebuilding_other_cells() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("report.xlsx");
    let mut workbook = Workbook::new();
    let sheet = workbook.add_sheet("Data");
    sheet.set_cell(0, 0, Cell::string("Hello"));
    sheet.set_cell(1, 0, Cell::number(42.0));
    writer::write_xlsx(&path, &workbook).unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_excli-cli"))
        .args([
            "replace",
            path.to_str().unwrap(),
            "--find",
            "Hello",
            "--replace",
            "World",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let changed = reader::read_workbook(&path).unwrap();
    let sheet = changed.sheet_by_name("Data").unwrap();
    assert_eq!(sheet.get_cell(0, 0).unwrap().value.to_string(), "World");
    assert_eq!(sheet.get_cell(1, 0).unwrap().value.to_string(), "42");
}

#[test]
fn cli_add_sheet_preserves_existing_xlsx_cells() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("report.xlsx");
    let mut workbook = Workbook::new();
    workbook
        .add_sheet("Data")
        .set_cell(0, 0, Cell::string("keep"));
    writer::write_xlsx(&path, &workbook).unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_excli-cli"))
        .args(["sheet", "add", path.to_str().unwrap(), "--name", "Archive"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let loaded = reader::read_workbook(&path).unwrap();
    assert_eq!(loaded.sheets.len(), 2);
    assert_eq!(loaded.sheets[1].name, "Archive");
    assert_eq!(
        loaded.sheets[0].get_cell(0, 0).unwrap().value.to_string(),
        "keep"
    );
}

#[test]
fn cli_rename_sheet_preserves_existing_xlsx_cells() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("report.xlsx");
    let mut workbook = Workbook::new();
    workbook
        .add_sheet("Data")
        .set_cell(0, 0, Cell::string("keep"));
    writer::write_xlsx(&path, &workbook).unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_excli-cli"))
        .args(["sheet", "rename", path.to_str().unwrap(), "Data", "Sales"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let loaded = reader::read_workbook(&path).unwrap();
    assert_eq!(loaded.sheets[0].name, "Sales");
    assert_eq!(
        loaded.sheets[0].get_cell(0, 0).unwrap().value.to_string(),
        "keep"
    );
}

#[test]
fn cli_hide_and_unhide_preserve_existing_xlsx() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("report.xlsx");
    let mut workbook = Workbook::new();
    workbook
        .add_sheet("Visible")
        .set_cell(0, 0, Cell::string("keep"));
    workbook
        .add_sheet("Archive")
        .set_cell(0, 0, Cell::number(42.0));
    writer::write_xlsx(&path, &workbook).unwrap();

    let binary = env!("CARGO_BIN_EXE_excli-cli");
    let hide = std::process::Command::new(binary)
        .args([
            "sheet",
            "hide",
            path.to_str().unwrap(),
            "--sheet",
            "Archive",
        ])
        .output()
        .unwrap();
    assert!(
        hide.status.success(),
        "{}",
        String::from_utf8_lossy(&hide.stderr)
    );
    let hidden = reader::read_workbook(&path).unwrap();
    assert_eq!(
        hidden.sheet_by_name("Archive").unwrap().state,
        excli_core::sheet::SheetState::Hidden
    );
    assert_eq!(
        hidden
            .sheet_by_name("Visible")
            .unwrap()
            .get_cell(0, 0)
            .unwrap()
            .value
            .to_string(),
        "keep"
    );

    let unhide = std::process::Command::new(binary)
        .args([
            "sheet",
            "unhide",
            path.to_str().unwrap(),
            "--sheet",
            "Archive",
        ])
        .output()
        .unwrap();
    assert!(
        unhide.status.success(),
        "{}",
        String::from_utf8_lossy(&unhide.stderr)
    );
    assert_eq!(
        reader::read_workbook(&path)
            .unwrap()
            .sheet_by_name("Archive")
            .unwrap()
            .state,
        excli_core::sheet::SheetState::Visible
    );
}

#[test]
fn cli_move_sheet_keeps_values_and_formulas() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("report.xlsx");
    let mut workbook = Workbook::new();
    workbook.add_sheet("A").set_cell(0, 0, Cell::string("keep"));
    workbook.add_sheet("B").set_cell(
        0,
        0,
        Cell::number(3.0).with_formula(excli_core::formula::Formula::new("=1+2")),
    );
    writer::write_xlsx(&path, &workbook).unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_excli-cli"))
        .args([
            "sheet",
            "move",
            path.to_str().unwrap(),
            "--sheet",
            "B",
            "--index",
            "0",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let loaded = reader::read_workbook(&path).unwrap();
    assert_eq!(
        loaded
            .sheets
            .iter()
            .map(|sheet| sheet.name.as_str())
            .collect::<Vec<_>>(),
        vec!["B", "A"]
    );
    assert_eq!(
        loaded
            .sheet_by_name("B")
            .unwrap()
            .get_cell(0, 0)
            .unwrap()
            .formula
            .as_ref()
            .unwrap()
            .raw,
        "=1+2"
    );
    assert_eq!(
        loaded
            .sheet_by_name("A")
            .unwrap()
            .get_cell(0, 0)
            .unwrap()
            .value
            .to_string(),
        "keep"
    );
}

#[test]
fn cli_formula_set_preserves_other_sheets() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("report.xlsx");
    let mut workbook = Workbook::new();
    workbook
        .add_sheet("Data")
        .set_cell(0, 0, Cell::number(42.0));
    workbook
        .add_sheet("Other")
        .set_cell(0, 0, Cell::number(7.0));
    writer::write_xlsx(&path, &workbook).unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_excli-cli"))
        .args([
            "formula",
            "set",
            path.to_str().unwrap(),
            "--sheet",
            "Other",
            "--cell",
            "B2",
            "--formula",
            "=A1+1",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let loaded = reader::read_workbook(&path).unwrap();
    assert_eq!(
        loaded
            .sheet_by_name("Other")
            .unwrap()
            .get_cell(1, 1)
            .unwrap()
            .formula
            .as_ref()
            .unwrap()
            .raw,
        "=A1+1"
    );
    assert_eq!(
        loaded
            .sheet_by_name("Data")
            .unwrap()
            .get_cell(0, 0)
            .unwrap()
            .value
            .to_string(),
        "42"
    );
}

#[test]
fn cli_formula_set_rejects_csv_without_changing_it() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("data.csv");
    std::fs::write(&path, b"Amount\n7\n").unwrap();
    let original = std::fs::read(&path).unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_excli-cli"))
        .args([
            "formula",
            "set",
            path.to_str().unwrap(),
            "--cell",
            "B2",
            "--formula",
            "=A2+1",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_eq!(std::fs::read(&path).unwrap(), original);
}

#[test]
fn cli_convert_same_excel_format_keeps_identical_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("source.xlsx");
    let output_path = dir.path().join("copy.xlsx");
    let mut workbook = Workbook::new();
    workbook
        .add_sheet("Data")
        .set_cell(0, 0, Cell::string("keep"));
    writer::write_xlsx(&input, &workbook).unwrap();
    let before = std::fs::read(&input).unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_excli-cli"))
        .args([
            "convert",
            input.to_str().unwrap(),
            output_path.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(std::fs::read(&output_path).unwrap(), before);
}

#[test]
fn cli_convert_xlsx_selected_sheet_preserves_selected_workbook_part() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("source.xlsx");
    let output_path = dir.path().join("selected.xlsx");
    let mut workbook = Workbook::new();
    workbook
        .add_sheet("Data")
        .set_cell(0, 0, Cell::string("keep"));
    workbook
        .add_sheet("Other")
        .set_cell(0, 0, Cell::string("omit"));
    writer::write_xlsx(&input, &workbook).unwrap();
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_excli-cli"))
        .args([
            "convert",
            input.to_str().unwrap(),
            output_path.to_str().unwrap(),
            "--sheet",
            "Data",
        ])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let loaded = reader::read_workbook(&output_path).unwrap();
    assert_eq!(loaded.sheets.len(), 1);
    assert_eq!(loaded.sheets[0].name, "Data");
    assert_eq!(
        loaded.sheets[0].get_cell(0, 0).unwrap().value.to_string(),
        "keep"
    );
}

#[test]
fn cli_write_append_preserves_existing_excel_formulas() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("report.xlsx");
    let mut workbook = Workbook::new();
    workbook.add_sheet("Data").set_cell(
        0,
        0,
        Cell::number(3.0).with_formula(excli_core::formula::Formula::new("=1+2")),
    );
    workbook
        .add_sheet("Other")
        .set_cell(0, 0, Cell::string("keep"));
    writer::write_xlsx(&path, &workbook).unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_excli-cli"))
        .args([
            "write",
            path.to_str().unwrap(),
            "--append",
            "--sheet",
            "Data",
            "--data",
            "[[12.5,true,\"a,b\"]]",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let loaded = reader::read_workbook(&path).unwrap();
    let data = loaded.sheet_by_name("Data").unwrap();
    assert_eq!(
        data.get_cell(0, 0).unwrap().formula.as_ref().unwrap().raw,
        "=1+2"
    );
    assert_eq!(data.get_cell(0, 1).unwrap().value.to_string(), "12.5");
    assert_eq!(data.get_cell(1, 1).unwrap().value.to_string(), "TRUE");
    assert_eq!(data.get_cell(2, 1).unwrap().value.to_string(), "a,b");
    assert_eq!(
        loaded
            .sheet_by_name("Other")
            .unwrap()
            .get_cell(0, 0)
            .unwrap()
            .value
            .to_string(),
        "keep"
    );

    let csv_output = std::process::Command::new(env!("CARGO_BIN_EXE_excli-cli"))
        .args([
            "write",
            path.to_str().unwrap(),
            "--append",
            "--sheet",
            "Data",
            "--data",
            "\"item, one\",\"line\nbreak\"",
        ])
        .output()
        .unwrap();
    assert!(
        csv_output.status.success(),
        "{}",
        String::from_utf8_lossy(&csv_output.stderr)
    );
    let after_csv = reader::read_workbook(&path).unwrap();
    let data = after_csv.sheet_by_name("Data").unwrap();
    assert_eq!(data.get_cell(0, 2).unwrap().value.to_string(), "item, one");
    assert_eq!(
        data.get_cell(1, 2).unwrap().value.to_string(),
        "line\nbreak"
    );
}

#[test]
fn cli_write_requires_append_for_existing_excel() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("report.xlsx");
    let mut workbook = Workbook::new();
    workbook
        .add_sheet("Data")
        .set_cell(0, 0, Cell::string("keep"));
    writer::write_xlsx(&path, &workbook).unwrap();
    let before = std::fs::read(&path).unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_excli-cli"))
        .args(["write", path.to_str().unwrap(), "--data", "replacement"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

// ── CSV roundtrip ─────────────────────────────────────────────────────────────

#[test]
fn test_csv_roundtrip() {
    let (_dir, path) = temp_csv("Name,Score\nAlice,95\nBob,87\nCarol,91\n");

    let wb = reader::read_workbook(&path).unwrap();
    assert_eq!(wb.sheet_count(), 1);

    let sheet = wb.sheet_by_index(0).unwrap();
    assert_eq!(sheet.row_count(), 4); // header + 3 data rows
    assert_eq!(sheet.col_count(), 2);

    // Verify cell content
    let name_cell = sheet.get_cell(0, 1).unwrap(); // A2 = "Alice"
    assert_eq!(name_cell.value.to_string(), "Alice");
}

#[test]
fn test_csv_write_then_read() {
    let dir = tempfile::tempdir().unwrap();
    let out_path = dir.path().join("output.csv");

    // Build a workbook in memory
    let mut wb = Workbook::new();
    let sheet = wb.add_sheet("Result");
    sheet.set_cell(0, 0, Cell::string("X"));
    sheet.set_cell(1, 0, Cell::string("Y"));
    sheet.set_cell(0, 1, Cell::number(10.0));
    sheet.set_cell(1, 1, Cell::number(20.0));
    sheet.set_cell(0, 2, Cell::number(30.0));
    sheet.set_cell(1, 2, Cell::number(40.0));

    writer::write_workbook(&out_path, &wb).unwrap();
    assert!(out_path.exists());

    // Read it back
    let wb2 = reader::read_workbook(&out_path).unwrap();
    let s = wb2.sheet_by_index(0).unwrap();
    assert_eq!(s.row_count(), 3);
    assert_eq!(s.col_count(), 2);

    let cell = s.get_cell(0, 1).unwrap();
    assert_eq!(cell.value.to_string(), "10");
}

// ── filter pipeline ───────────────────────────────────────────────────────────

#[test]
fn test_filter_gt_pipeline() {
    let (_dir, path) = temp_csv("Name,Score\nAlice,95\nBob,70\nCarol,88\nDave,60\n");
    let wb = reader::read_workbook(&path).unwrap();
    let sheet = wb.sheet_by_index(0).unwrap();

    // Filter: Score > 80
    let cond = FilterCondition::new("B", FilterOperator::Gt, Some("80".to_string()));
    let result = filter_rows(sheet, &[cond]).unwrap();

    // Alice (95) and Carol (88) pass; header row included
    assert_eq!(result.row_count(), 3); // header + Alice + Carol
}

#[test]
fn test_filter_contains_pipeline() {
    let (_dir, path) = temp_csv("Name,City\nAlice,New York\nBob,Los Angeles\nCarol,New Jersey\n");
    let wb = reader::read_workbook(&path).unwrap();
    let sheet = wb.sheet_by_index(0).unwrap();

    let cond = FilterCondition::new("B", FilterOperator::Contains, Some("New".to_string()));
    let result = filter_rows(sheet, &[cond]).unwrap();

    // header row "City" does not contain "New", so only data rows match
    assert_eq!(result.row_count(), 2); // New York + New Jersey
}

#[test]
fn test_filter_unique_pipeline() {
    let (_dir, path) = temp_csv("Dept\nEng\nEng\nHR\nHR\nEng\n");
    let wb = reader::read_workbook(&path).unwrap();
    let sheet = wb.sheet_by_index(0).unwrap();

    let result = filter_unique(sheet);
    assert_eq!(result.row_count(), 3); // Eng, HR, plus empty dedup
}

// ── sort pipeline ─────────────────────────────────────────────────────────────

#[test]
fn test_sort_ascending_pipeline() {
    let (_dir, path) = temp_csv("Name,Score\nCarol,88\nAlice,95\nBob,70\n");
    let wb = reader::read_workbook(&path).unwrap();
    let sheet = wb.sheet_by_index(0).unwrap();

    let spec = SortSpec::new("B", SortOrder::Asc);
    let result = sort_rows(sheet, &[spec]);

    // Rows should be sorted by Score ascending: 70, 88, 95
    let first_score = result
        .get_cell(1, 0)
        .map(|c| c.value.to_string())
        .unwrap_or_default();
    let last_score = result
        .get_cell(1, 2)
        .map(|c| c.value.to_string())
        .unwrap_or_default();
    assert_eq!(first_score, "70");
    assert_eq!(last_score, "95");
}

#[test]
fn test_sort_descending_pipeline() {
    let (_dir, path) = temp_csv("Val\n3\n1\n4\n1\n5\n");
    let wb = reader::read_workbook(&path).unwrap();
    let sheet = wb.sheet_by_index(0).unwrap();

    let spec = SortSpec::new("A", SortOrder::Desc);
    let result = sort_rows(sheet, &[spec]);

    // sort_rows sorts all rows including the header; "Val" > "5" as string so it ends up first.
    // The largest numeric value (5) is in row 1 after the header row.
    let first_data = result
        .get_cell(0, 1)
        .map(|c| c.value.to_string())
        .unwrap_or_default();
    assert_eq!(first_data, "5");
}

// ── aggregate pipeline ────────────────────────────────────────────────────────

#[test]
fn test_aggregate_group_by_sum() {
    let (_dir, path) = temp_csv("Dept,Salary\nEng,100\nEng,200\nHR,150\n");
    let wb = reader::read_workbook(&path).unwrap();
    let sheet = wb.sheet_by_index(0).unwrap();

    let aggs = vec![Aggregation::new("B", AggregateOp::Sum)];
    let result = aggregate(sheet, &["A".to_string()], &aggs);

    // aggregate includes the original header row as its own group key "Dept"
    // so result has: header row + Dept group + Eng group + HR group = 4 rows
    assert_eq!(result.row_count(), 4); // header + 3 groups (incl. original header row)
}

#[test]
fn test_aggregate_no_groupby_count() {
    let (_dir, path) = temp_csv("Score\n10\n20\n30\n40\n");
    let wb = reader::read_workbook(&path).unwrap();
    let sheet = wb.sheet_by_index(0).unwrap();

    let aggs = vec![Aggregation::new("A", AggregateOp::Count)];
    let result = aggregate(sheet, &[], &aggs);

    // One row result with count
    assert_eq!(result.row_count(), 1);
    let val = result
        .get_cell(1, 0)
        .map(|c| c.value.coerce_to_number())
        .unwrap_or(0.0);
    assert_eq!(val, 4.0);
}

// ── merge (join) pipeline ─────────────────────────────────────────────────────

#[test]
fn test_inner_join_pipeline() {
    let dir = tempfile::tempdir().unwrap();
    let left_path = write_temp_csv(&dir, "left.csv", "ID,Name\n1,Alice\n2,Bob\n3,Carol\n");
    let right_path = write_temp_csv(&dir, "right.csv", "ID,Score\n1,95\n3,88\n4,70\n");

    let left = reader::read_workbook(&left_path).unwrap();
    let right = reader::read_workbook(&right_path).unwrap();

    let result = merge_workbooks(&left, &right, "A", "A", JoinType::Inner);
    let sheet = result.sheet_by_index(0).unwrap();

    // Inner join on ID: only IDs 1 and 3 match → 2 data rows + header
    assert_eq!(sheet.row_count(), 3);
}

#[test]
fn test_left_join_pipeline() {
    let dir = tempfile::tempdir().unwrap();
    let left_path = write_temp_csv(&dir, "left.csv", "ID,Name\n1,Alice\n2,Bob\n");
    let right_path = write_temp_csv(&dir, "right.csv", "ID,Score\n1,95\n");

    let left = reader::read_workbook(&left_path).unwrap();
    let right = reader::read_workbook(&right_path).unwrap();

    let result = merge_workbooks(&left, &right, "A", "A", JoinType::Left);
    let sheet = result.sheet_by_index(0).unwrap();

    // Left join: both Alice (match) and Bob (no match) included → 2 data rows + header
    assert_eq!(sheet.row_count(), 3);
}

// ── concat pipeline ───────────────────────────────────────────────────────────

#[test]
fn test_concat_rows_pipeline() {
    let dir = tempfile::tempdir().unwrap();
    let p1 = write_temp_csv(&dir, "a.csv", "Name\nAlice\nBob\n");
    let p2 = write_temp_csv(&dir, "b.csv", "Name\nCarol\nDave\n");

    let wb1 = reader::read_workbook(&p1).unwrap();
    let wb2 = reader::read_workbook(&p2).unwrap();

    let result = concat_workbooks(&[&wb1, &wb2], ConcatAxis::Rows);
    let sheet = result.sheet_by_index(0).unwrap();

    assert_eq!(sheet.row_count(), 6); // 3 from each
}

#[test]
fn test_concat_cols_pipeline() {
    let dir = tempfile::tempdir().unwrap();
    let p1 = write_temp_csv(&dir, "a.csv", "A\n1\n2\n");
    let p2 = write_temp_csv(&dir, "b.csv", "B\n3\n4\n");

    let wb1 = reader::read_workbook(&p1).unwrap();
    let wb2 = reader::read_workbook(&p2).unwrap();

    let result = concat_workbooks(&[&wb1, &wb2], ConcatAxis::Cols);
    let sheet = result.sheet_by_index(0).unwrap();

    assert_eq!(sheet.col_count(), 2);
}

// ── transpose pipeline ────────────────────────────────────────────────────────

#[test]
fn test_transpose_pipeline() {
    let (_dir, path) = temp_csv("A,B,C\n1,2,3\n");
    let wb = reader::read_workbook(&path).unwrap();
    let sheet = wb.sheet_by_index(0).unwrap();

    let orig_rows = sheet.row_count();
    let orig_cols = sheet.col_count();
    let result = transpose(sheet);

    assert_eq!(result.row_count(), orig_cols);
    assert_eq!(result.col_count(), orig_rows);
}

// ── filter + sort composition ─────────────────────────────────────────────────

#[test]
fn test_filter_then_sort() {
    let (_dir, path) = temp_csv("Name,Score\nAlice,95\nBob,70\nCarol,88\nDave,60\n");
    let wb = reader::read_workbook(&path).unwrap();
    let sheet = wb.sheet_by_index(0).unwrap();

    // Step 1: filter Score > 75
    let cond = FilterCondition::new("B", FilterOperator::Gt, Some("75".to_string()));
    let filtered = filter_rows(sheet, &[cond]).unwrap();

    // Step 2: sort by Score descending
    let spec = SortSpec::new("B", SortOrder::Desc);
    let sorted = sort_rows(&filtered, &[spec]);

    // filter_rows includes the header row ("Score" > "75" lexicographically = true)
    // so filtered has 3 rows: [header, Alice(95), Carol(88)]
    // after desc sort by col B: "Score" (string) > 95, so header stays first
    assert_eq!(sorted.row_count(), 3);
    // row 0 = header, row 1 = Alice (95), row 2 = Carol (88)
    let first = sorted
        .get_cell(0, 1)
        .map(|c| c.value.to_string())
        .unwrap_or_default();
    assert_eq!(first, "Alice");
}

// ── write XLSX ────────────────────────────────────────────────────────────────

#[test]
fn test_write_xlsx_and_stat() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.xlsx");

    let mut wb = Workbook::new();
    let sheet = wb.add_sheet("Data");
    for i in 0..5u32 {
        sheet.set_cell(0, i, Cell::number(i as f64 * 10.0));
        sheet.set_cell(1, i, Cell::string(format!("row{}", i)));
    }

    writer::write_workbook(&path, &wb).unwrap();
    assert!(path.exists());
    assert!(path.metadata().unwrap().len() > 0);
}

// ── multi-sheet workbook ──────────────────────────────────────────────────────

#[test]
fn test_multi_sheet_workbook() {
    let mut wb = Workbook::new();

    let s1 = wb.add_sheet("Sheet1");
    s1.set_cell(0, 0, Cell::string("Sheet1Data"));

    let s2 = wb.add_sheet("Sheet2");
    s2.set_cell(0, 0, Cell::string("Sheet2Data"));

    assert_eq!(wb.sheet_count(), 2);

    let found = wb.sheet_by_name("Sheet2").unwrap();
    assert_eq!(
        found.get_cell(0, 0).unwrap().value.to_string(),
        "Sheet2Data"
    );
}

// ── workbook stats ────────────────────────────────────────────────────────────

#[test]
fn test_workbook_stats() {
    let (_dir, path) = temp_csv("A,B\n1,2\n3,4\n5,6\n");
    let wb = reader::read_workbook(&path).unwrap();
    let stats = wb.stats();

    assert_eq!(stats.sheet_count, 1);
    assert_eq!(stats.row_count, 4); // header + 3 rows
    assert_eq!(stats.col_count, 2);
}
