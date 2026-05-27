use std::path::Path;
use std::time::Instant;

use excli_core::cell::{Cell, CellValue};
use excli_core::error::{ErrorCode, ExcliError, ExcliResult};
use excli_core::sheet::SheetState;
use excli_core::Workbook;
use excli_ops::aggregate::{aggregate, AggregateOp, Aggregation};
use excli_ops::clean::{clean_blanks, clean_case, clean_trim, remove_duplicates, BlankFillStrategy, CaseType};
use excli_ops::filter::{filter_rows, filter_unique, FilterCondition, FilterOperator};
use excli_ops::merge::{merge_workbooks, JoinType};
use excli_ops::search::{replace_cells, search_cells, SearchOptions};
use excli_ops::sort::{sort_rows, SortOrder, SortSpec};
use excli_ops::transform::{concat_workbooks, split_by_column, split_by_rows, transpose, ConcatAxis};

use crate::cli::{Cli, Commands, FormulaCmd, SheetCmd};

// ─── Helpers ──────────────────────────────────────────────────────────────────

/// Print a sheet as CSV to stdout.
fn print_sheet_csv(sheet: &excli_core::Sheet) {
    for row in &sheet.rows {
        let cells: Vec<String> = row
            .cells
            .iter()
            .map(|c| {
                let s = c.value.to_string();
                if s.contains(',') || s.contains('"') || s.contains('\n') {
                    format!("\"{}\"", s.replace('"', "\"\""))
                } else {
                    s
                }
            })
            .collect();
        println!("{}", cells.join(","));
    }
}

/// Print a sheet as JSON array-of-arrays to stdout.
fn print_sheet_json(sheet: &excli_core::Sheet) {
    let rows: Vec<Vec<serde_json::Value>> = sheet
        .rows
        .iter()
        .map(|row| {
            row.cells
                .iter()
                .map(|c| match &c.value {
                    CellValue::Number(n) => serde_json::Value::Number(
                        serde_json::Number::from_f64(*n)
                            .unwrap_or(serde_json::Number::from(0)),
                    ),
                    CellValue::Boolean(b) => serde_json::Value::Bool(*b),
                    CellValue::Empty => serde_json::Value::Null,
                    other => serde_json::Value::String(other.to_string()),
                })
                .collect()
        })
        .collect();
    println!("{}", serde_json::to_string_pretty(&rows).unwrap_or_default());
}

/// Simple pseudo-random number generator (LCG) for sampling.
fn pseudo_random_u64(state: &mut u64) -> u64 {
    *state = state
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    *state
}

/// Sample `n` row indices from `total` rows, returning sorted indices.
fn sample_row_indices(total: usize, n: usize) -> Vec<usize> {
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(42);
    let mut rng = seed;
    let mut indices: Vec<usize> = (0..total).collect();
    for i in (1..indices.len()).rev() {
        let j = (pseudo_random_u64(&mut rng) as usize) % (i + 1);
        indices.swap(i, j);
    }
    indices.truncate(n);
    indices.sort_unstable();
    indices
}

/// Parse a cell reference like A1 into (col, row) 0-based.
fn parse_cell_ref(s: &str) -> Option<(u32, u32)> {
    excli_core::reference::CellReference::from_a1(s).map(|r| (r.column, r.row))
}

/// Parse A1:C10 range into ((start_col, start_row), (end_col, end_row)).
fn parse_range(s: &str) -> Option<((u32, u32), (u32, u32))> {
    let parts: Vec<&str> = s.splitn(2, ':').collect();
    if parts.len() == 2 {
        let start = parse_cell_ref(parts[0])?;
        let end = parse_cell_ref(parts[1])?;
        Some((start, end))
    } else {
        None
    }
}

/// Glob-style pattern matching: ? matches any single char, * matches any sequence.
fn glob_match(pattern: &str, text: &str) -> bool {
    let pat: Vec<char> = pattern.chars().collect();
    let txt: Vec<char> = text.chars().collect();
    let mut dp = vec![vec![false; txt.len() + 1]; pat.len() + 1];
    dp[0][0] = true;
    for i in 1..=pat.len() {
        if pat[i - 1] == '*' {
            dp[i][0] = dp[i - 1][0];
        }
    }
    for i in 1..=pat.len() {
        for j in 1..=txt.len() {
            dp[i][j] = if pat[i - 1] == '*' {
                dp[i - 1][j] || dp[i][j - 1]
            } else if pat[i - 1] == '?' || pat[i - 1] == txt[j - 1] {
                dp[i - 1][j - 1]
            } else {
                false
            };
        }
    }
    dp[pat.len()][txt.len()]
}

// ─── Dispatch ─────────────────────────────────────────────────────────────────

pub fn dispatch(cli: Cli) -> ExcliResult<()> {
    let _start = Instant::now();
    let json = cli.json;
    let quiet = cli.quiet;

    match cli.command {
        Commands::Read {
            file,
            sheet,
            range,
            skip_rows,
            nrows,
            header,
            stats,
            ..
        } => cmd_read(
            &file,
            sheet.as_deref(),
            range.as_deref(),
            skip_rows,
            nrows,
            header,
            stats,
            json,
        ),

        Commands::Write {
            output,
            data,
            sheet,
            start_cell,
            append,
            ..
        } => cmd_write(
            &output,
            data.as_deref(),
            sheet.as_deref(),
            start_cell.as_deref(),
            append,
            quiet,
        ),

        Commands::Sheet { action } => cmd_sheet(action, json),

        Commands::Formula { action } => cmd_formula(action, json),

        Commands::Calc {
            evaluate,
            context,
            file,
            ..
        } => cmd_calc(evaluate.as_deref(), context.as_deref(), file.as_deref(), json),

        Commands::Filter {
            file,
            column,
            gt,
            lt,
            contains,
            between,
            r#in,
            is_null,
            regex,
            unique,
            ..
        } => cmd_filter(
            &file,
            column.as_deref(),
            gt.as_deref(),
            lt.as_deref(),
            contains.as_deref(),
            between.as_deref(),
            r#in.as_deref(),
            is_null,
            regex.as_deref(),
            unique,
            json,
            quiet,
        ),

        Commands::Sort {
            file,
            column,
            columns,
            asc,
            desc,
            custom,
        } => cmd_sort(
            &file,
            column.as_deref(),
            columns.as_deref(),
            asc,
            desc,
            custom.as_deref(),
            json,
            quiet,
        ),

        Commands::Agg {
            file,
            group_by,
            sum,
            count,
            avg,
            ..
        } => cmd_agg(
            &file,
            group_by.as_deref(),
            sum.as_deref(),
            count,
            avg.as_deref(),
            json,
            quiet,
        ),

        Commands::Merge {
            file1,
            file2,
            on,
            r#type,
            left_on,
            right_on,
            cross,
        } => cmd_merge(
            &file1,
            &file2,
            on.as_deref(),
            r#type.as_deref().unwrap_or("inner"),
            left_on.as_deref(),
            right_on.as_deref(),
            cross,
            json,
            quiet,
        ),

        Commands::Concat { files, axis } => cmd_concat(&files, &axis, json, quiet),

        Commands::Transpose { file, output } => {
            cmd_transpose(&file, output.as_deref(), json, quiet)
        }

        Commands::Split {
            file,
            by,
            rows,
            output_dir,
        } => cmd_split(&file, by.as_deref(), rows, output_dir.as_deref(), quiet),

        Commands::Sample {
            file,
            n,
            frac,
            stratify,
        } => cmd_sample(&file, n, frac, stratify.as_deref(), json, quiet),

        Commands::Search {
            file,
            pattern,
            regex,
            case_sensitive,
            whole_word,
            ..
        } => cmd_search(&file, &pattern, regex, case_sensitive, whole_word, json, quiet),

        Commands::Replace {
            file,
            find,
            replace,
            dry_run,
            ..
        } => cmd_replace(&file, &find, &replace, dry_run, quiet),

        Commands::Info {
            file,
            detailed,
            schema,
        } => cmd_info(&file, detailed, schema, json),

        Commands::Convert {
            input,
            output,
            sheet,
            delimiter,
            ..
        } => cmd_convert(
            &input,
            &output,
            sheet.as_deref(),
            delimiter.as_deref(),
            quiet,
        ),

        Commands::Clean {
            file,
            command,
            range,
            columns,
            fill,
            ..
        } => cmd_clean(
            &file,
            &command,
            range.as_deref(),
            columns.as_deref(),
            fill.as_deref(),
            json,
            quiet,
        ),

        Commands::Diff {
            file1,
            file2,
            sheet,
            values_only,
            tolerance,
        } => cmd_diff(
            &file1,
            &file2,
            sheet.as_deref(),
            values_only,
            tolerance,
            json,
            quiet,
        ),

        Commands::Batch { glob, command } => cmd_batch(&glob, &command, quiet),

        Commands::DetectEncoding { file, all } => cmd_detect_encoding(&file, all),

        Commands::DetectLang { file, .. } => cmd_detect_lang(&file, json),

        Commands::Validate { file, reference } => cmd_validate(&file, &reference, json, quiet),

        Commands::Describe { all, command } => cmd_describe(command.as_deref(), all),
    }
}

// ─── Read ──────────────────────────────────────────────────────────────────────

fn cmd_read(
    file: &str,
    sheet: Option<&str>,
    range: Option<&str>,
    skip: usize,
    nrows: Option<usize>,
    _header: bool,
    stats: bool,
    json: bool,
) -> ExcliResult<()> {
    let wb = excli_io::reader::read_workbook(Path::new(file))?;

    if stats {
        let st = wb.stats();
        if json {
            println!("{}", serde_json::to_string_pretty(&st).unwrap_or_default());
        } else {
            println!("Sheets:   {}", st.sheet_count);
            println!("Rows:     {}", st.row_count);
            println!("Cols:     {}", st.col_count);
            println!("Formulas: {}", st.formula_count);
            println!("Bytes:    {}", st.file_size_bytes);
        }
        return Ok(());
    }

    let sheets_to_print: Vec<&excli_core::Sheet> = if let Some(name_or_idx) = sheet {
        if let Ok(idx) = name_or_idx.parse::<usize>() {
            wb.sheet_by_index(idx.saturating_sub(1))
                .map(|s| vec![s])
                .unwrap_or_default()
        } else {
            wb.sheet_by_name(name_or_idx)
                .map(|s| vec![s])
                .unwrap_or_default()
        }
    } else {
        wb.sheets.iter().collect()
    };

    for sheet_ref in sheets_to_print {
        if wb.sheets.len() > 1 {
            eprintln!("=== {} ===", sheet_ref.name);
        }

        let range_filter = range.and_then(parse_range);
        let rows_iter = sheet_ref
            .rows
            .iter()
            .skip(skip)
            .take(nrows.unwrap_or(usize::MAX));

        if json {
            let data: Vec<Vec<serde_json::Value>> = rows_iter
                .enumerate()
                .map(|(r_off, row)| {
                    let actual_row = (skip + r_off) as u32;
                    row.cells
                        .iter()
                        .enumerate()
                        .filter(|(col, _)| {
                            if let Some(((sc, sr), (ec, er))) = range_filter {
                                let c = *col as u32;
                                c >= sc && c <= ec && actual_row >= sr && actual_row <= er
                            } else {
                                true
                            }
                        })
                        .map(|(_, cell)| match &cell.value {
                            CellValue::Number(n) => serde_json::Value::Number(
                                serde_json::Number::from_f64(*n)
                                    .unwrap_or(serde_json::Number::from(0)),
                            ),
                            CellValue::Boolean(b) => serde_json::Value::Bool(*b),
                            CellValue::Empty => serde_json::Value::Null,
                            other => serde_json::Value::String(other.to_string()),
                        })
                        .collect()
                })
                .collect();
            println!("{}", serde_json::to_string_pretty(&data).unwrap_or_default());
        } else {
            for (r_off, row) in rows_iter.enumerate() {
                let actual_row = (skip + r_off) as u32;
                let cells: Vec<String> = row
                    .cells
                    .iter()
                    .enumerate()
                    .filter(|(col, _)| {
                        if let Some(((sc, sr), (ec, er))) = range_filter {
                            let c = *col as u32;
                            c >= sc && c <= ec && actual_row >= sr && actual_row <= er
                        } else {
                            true
                        }
                    })
                    .map(|(_, c)| c.value.to_string())
                    .collect();
                println!("{}", cells.join(","));
            }
        }
    }
    Ok(())
}

// ─── Write ─────────────────────────────────────────────────────────────────────

fn cmd_write(
    output: &str,
    data: Option<&str>,
    sheet_name: Option<&str>,
    start_cell: Option<&str>,
    append: bool,
    quiet: bool,
) -> ExcliResult<()> {
    let (col_offset, row_offset) = start_cell
        .and_then(parse_cell_ref)
        .unwrap_or((0, 0));

    let mut wb = if append && Path::new(output).exists() {
        excli_io::reader::read_workbook(Path::new(output))?
    } else {
        Workbook::new()
    };

    let data_str = data.ok_or_else(|| {
        ExcliError::new(ErrorCode::GeneralError, "No data provided (use --data)")
    })?;

    let target_name = sheet_name.unwrap_or("Sheet1");
    if wb.sheet_by_name(target_name).is_none() {
        wb.add_sheet(target_name);
    }

    let base_row = if append {
        let s = wb.sheet_by_name(target_name).unwrap();
        s.row_count() as u32
    } else {
        row_offset
    };

    let sheet = wb.sheet_by_name_mut(target_name).unwrap();

    if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(data_str) {
        if let Some(arr) = json_val.as_array() {
            for (row_idx, row_val) in arr.iter().enumerate() {
                let row_pos = base_row + row_idx as u32;
                if let Some(cells) = row_val.as_array() {
                    for (col_idx, val) in cells.iter().enumerate() {
                        sheet.set_cell(col_offset + col_idx as u32, row_pos, json_value_to_cell(val));
                    }
                } else if let Some(obj) = row_val.as_object() {
                    for (col_idx, (_, val)) in obj.iter().enumerate() {
                        sheet.set_cell(col_offset + col_idx as u32, row_pos, json_value_to_cell(val));
                    }
                }
            }
        }
    } else {
        for (row_idx, line) in data_str.lines().enumerate() {
            let row_pos = base_row + row_idx as u32;
            for (col_idx, val) in line.split(',').enumerate() {
                let trimmed = val.trim();
                let cell = if let Ok(n) = trimmed.parse::<f64>() {
                    Cell::number(n)
                } else {
                    Cell::string(trimmed)
                };
                sheet.set_cell(col_offset + col_idx as u32, row_pos, cell);
            }
        }
    }

    excli_io::writer::write_workbook(Path::new(output), &wb)?;
    if !quiet {
        println!("Written to {}", output);
    }
    Ok(())
}

fn json_value_to_cell(val: &serde_json::Value) -> Cell {
    match val {
        serde_json::Value::Number(n) => Cell::number(n.as_f64().unwrap_or(0.0)),
        serde_json::Value::Bool(b) => Cell::boolean(*b),
        serde_json::Value::Null => Cell::empty(),
        serde_json::Value::String(s) => Cell::string(s),
        other => Cell::string(other.to_string().trim_matches('"')),
    }
}

// ─── Sheet ─────────────────────────────────────────────────────────────────────

fn cmd_sheet(action: SheetCmd, json: bool) -> ExcliResult<()> {
    match action {
        SheetCmd::List { file } => {
            let wb = excli_io::reader::read_workbook(Path::new(&file))?;
            if json {
                let info: Vec<serde_json::Value> = wb
                    .sheets
                    .iter()
                    .enumerate()
                    .map(|(i, s)| {
                        serde_json::json!({
                            "index": i,
                            "name": s.name,
                            "rows": s.row_count(),
                            "cols": s.col_count(),
                            "hidden": s.state != SheetState::Visible,
                        })
                    })
                    .collect();
                println!("{}", serde_json::to_string_pretty(&info).unwrap_or_default());
            } else {
                for (i, s) in wb.sheets.iter().enumerate() {
                    let hidden = if s.state != SheetState::Visible { " [hidden]" } else { "" };
                    println!("  {}. {}{}  ({} rows x {} cols)", i + 1, s.name, hidden, s.row_count(), s.col_count());
                }
            }
        }

        SheetCmd::Info { file, sheet } => {
            let wb = excli_io::reader::read_workbook(Path::new(&file))?;
            let s = wb.sheet_by_name(&sheet).ok_or_else(|| ExcliError::sheet_not_found(&sheet))?;
            let st = s.stats();
            if json {
                println!("{}", serde_json::to_string_pretty(&st).unwrap_or_default());
            } else {
                println!("Sheet:    {}", s.name);
                println!("Rows:     {}", st.row_count);
                println!("Cols:     {}", st.col_count);
                println!("Formulas: {}", st.formula_count);
                println!("Hidden:   {}", s.state != SheetState::Visible);
            }
        }

        SheetCmd::Rename { file, old, new } => {
            let mut wb = excli_io::reader::read_workbook(Path::new(&file))?;
            wb.rename_sheet(&old, &new)?;
            excli_io::writer::write_workbook(Path::new(&file), &wb)?;
            println!("Renamed '{}' -> '{}'", old, new);
        }

        SheetCmd::Add { file, name } => {
            let mut wb = excli_io::reader::read_workbook(Path::new(&file))?;
            wb.add_sheet(&name);
            excli_io::writer::write_workbook(Path::new(&file), &wb)?;
            println!("Added sheet '{}'", name);
        }

        SheetCmd::Delete { file, sheet } => {
            let mut wb = excli_io::reader::read_workbook(Path::new(&file))?;
            wb.remove_sheet(&sheet)?;
            excli_io::writer::write_workbook(Path::new(&file), &wb)?;
            println!("Deleted sheet '{}'", sheet);
        }

        SheetCmd::Copy { file, source, dest } => {
            let mut wb = excli_io::reader::read_workbook(Path::new(&file))?;
            let mut new_sheet = wb
                .sheet_by_name(&source)
                .ok_or_else(|| ExcliError::sheet_not_found(&source))?
                .clone();
            new_sheet.name = dest.clone();
            wb.sheets.push(new_sheet);
            excli_io::writer::write_workbook(Path::new(&file), &wb)?;
            println!("Copied '{}' -> '{}'", source, dest);
        }

        SheetCmd::Move { file, sheet, index } => {
            let mut wb = excli_io::reader::read_workbook(Path::new(&file))?;
            let pos = wb.sheets.iter().position(|s| s.name == sheet)
                .ok_or_else(|| ExcliError::sheet_not_found(&sheet))?;
            let s = wb.sheets.remove(pos);
            let insert_at = index.min(wb.sheets.len());
            wb.sheets.insert(insert_at, s);
            excli_io::writer::write_workbook(Path::new(&file), &wb)?;
            println!("Moved '{}' to index {}", sheet, index);
        }

        SheetCmd::Hide { file, sheet } => {
            let mut wb = excli_io::reader::read_workbook(Path::new(&file))?;
            wb.sheet_by_name_mut(&sheet)
                .ok_or_else(|| ExcliError::sheet_not_found(&sheet))?
                .state = SheetState::Hidden;
            excli_io::writer::write_workbook(Path::new(&file), &wb)?;
            println!("Hidden sheet '{}'", sheet);
        }

        SheetCmd::Unhide { file, sheet } => {
            let mut wb = excli_io::reader::read_workbook(Path::new(&file))?;
            wb.sheet_by_name_mut(&sheet)
                .ok_or_else(|| ExcliError::sheet_not_found(&sheet))?
                .state = SheetState::Visible;
            excli_io::writer::write_workbook(Path::new(&file), &wb)?;
            println!("Unhidden sheet '{}'", sheet);
        }
    }
    Ok(())
}

// ─── Formula ───────────────────────────────────────────────────────────────────

fn cmd_formula(action: FormulaCmd, json: bool) -> ExcliResult<()> {
    match action {
        FormulaCmd::Read { file, sheet, evaluate, both, .. } => {
            let wb = excli_io::reader::read_workbook(Path::new(&file))?;
            let registry = excli_formula::functions::FunctionRegistry::new();
            let mut results: Vec<serde_json::Value> = Vec::new();

            for s in &wb.sheets {
                if let Some(ref name) = sheet {
                    if &s.name != name { continue; }
                }
                for (row_idx, row) in s.rows.iter().enumerate() {
                    for (col_idx, cell) in row.cells.iter().enumerate() {
                        if let Some(ref f) = cell.formula {
                            let cell_ref = format!(
                                "{}{}",
                                excli_core::reference::column_index_to_name(col_idx as u32),
                                row_idx + 1
                            );
                            let evaluated_val = if evaluate || both {
                                excli_formula::parser::Parser::new(&f.raw)
                                    .parse()
                                    .ok()
                                    .and_then(|ast| {
                                        let mut ctx = excli_formula::evaluator::EvalContext::new(&wb, &registry);
                                        ctx.current_sheet = Some(s.name.clone());
                                        excli_formula::evaluator::Evaluator::new()
                                            .evaluate(&ast.root, &mut ctx)
                                            .ok()
                                            .map(|v| v.to_string())
                                    })
                            } else {
                                None
                            };

                            if json {
                                let mut obj = serde_json::json!({ "sheet": s.name, "cell": cell_ref, "formula": f.raw });
                                if let Some(ref v) = evaluated_val {
                                    obj["value"] = serde_json::Value::String(v.clone());
                                }
                                results.push(obj);
                            } else if both {
                                println!("{}!{}: {} = {}", s.name, cell_ref, f.raw, evaluated_val.as_deref().unwrap_or("?"));
                            } else if evaluate {
                                println!("{}!{}: {}", s.name, cell_ref, evaluated_val.as_deref().unwrap_or("#ERROR"));
                            } else {
                                println!("{}!{}: {}", s.name, cell_ref, f.raw);
                            }
                        }
                    }
                }
            }
            if json {
                println!("{}", serde_json::to_string_pretty(&results).unwrap_or_default());
            }
        }

        FormulaCmd::Set { file, cell, range, formula } => {
            let mut wb = excli_io::reader::read_workbook(Path::new(&file))?;
            let sheet = wb.sheets.first_mut()
                .ok_or_else(|| ExcliError::new(ErrorCode::GeneralError, "No sheets"))?;

            if let Some(ref cell_str) = cell {
                if let Some((col, row)) = parse_cell_ref(cell_str) {
                    let mut c = sheet.get_cell(col, row).cloned().unwrap_or_else(Cell::empty);
                    c.formula = Some(excli_core::formula::Formula::new(formula.clone()));
                    sheet.set_cell(col, row, c);
                } else {
                    return Err(ExcliError::new(ErrorCode::RangeInvalid, format!("Invalid cell reference: {}", cell_str)));
                }
            } else if let Some(ref range_str) = range {
                if let Some(((sc, sr), (ec, er))) = parse_range(range_str) {
                    for row in sr..=er {
                        for col in sc..=ec {
                            let mut c = sheet.get_cell(col, row).cloned().unwrap_or_else(Cell::empty);
                            c.formula = Some(excli_core::formula::Formula::new(formula.clone()));
                            sheet.set_cell(col, row, c);
                        }
                    }
                }
            } else {
                return Err(ExcliError::new(ErrorCode::GeneralError, "Specify --cell or --range"));
            }
            excli_io::writer::write_workbook(Path::new(&file), &wb)?;
            println!("Formula set: {}", formula);
        }

        FormulaCmd::Deps { file, cell } => {
            let wb = excli_io::reader::read_workbook(Path::new(&file))?;
            let graph = build_dependency_graph(&wb);
            let prec = graph.precedents_of(&cell);
            println!("{} depends on: {:?}", cell, prec);
        }

        FormulaCmd::Dependents { file, cell } => {
            let wb = excli_io::reader::read_workbook(Path::new(&file))?;
            let graph = build_dependency_graph(&wb);
            let deps = graph.dependents_of(&cell);
            println!("{} is used by: {:?}", cell, deps);
        }

        FormulaCmd::Errors { file } => {
            let wb = excli_io::reader::read_workbook(Path::new(&file))?;
            let registry = excli_formula::functions::FunctionRegistry::new();
            let mut found = false;
            for sheet in &wb.sheets {
                for (row_idx, row) in sheet.rows.iter().enumerate() {
                    for (col_idx, cell) in row.cells.iter().enumerate() {
                        let has_error = match &cell.value {
                            CellValue::Error(_) => true,
                            _ => false,
                        };
                        let evaluated_error = if let Some(ref f) = cell.formula {
                            excli_formula::parser::Parser::new(&f.raw).parse().ok().and_then(|ast| {
                                let mut ctx = excli_formula::evaluator::EvalContext::new(&wb, &registry);
                                ctx.current_sheet = Some(sheet.name.clone());
                                excli_formula::evaluator::Evaluator::new().evaluate(&ast.root, &mut ctx).ok()
                            }).map_or(false, |v| matches!(v, CellValue::Error(_)))
                        } else {
                            false
                        };

                        if has_error || evaluated_error {
                            found = true;
                            let cref = format!("{}{}",
                                excli_core::reference::column_index_to_name(col_idx as u32),
                                row_idx + 1);
                            println!("{}!{}: {}", sheet.name, cref, cell.value);
                        }
                    }
                }
            }
            if !found { println!("No formula errors found."); }
        }

        FormulaCmd::Circular { file } => {
            let wb = excli_io::reader::read_workbook(Path::new(&file))?;
            let graph = build_dependency_graph(&wb);
            let cycles = graph.detect_circular();
            if cycles.is_empty() {
                println!("No circular references detected.");
            } else {
                println!("Circular references:");
                for c in &cycles { println!("  {}", c.join(" -> ")); }
            }
        }
    }
    Ok(())
}

fn build_dependency_graph(wb: &Workbook) -> excli_formula::dependency::DependencyGraph {
    let mut graph = excli_formula::dependency::DependencyGraph::new();
    for sheet in &wb.sheets {
        for (row_idx, row) in sheet.rows.iter().enumerate() {
            for (col_idx, cell) in row.cells.iter().enumerate() {
                let ref_str = format!(
                    "{}{}",
                    excli_core::reference::column_index_to_name(col_idx as u32),
                    row_idx + 1
                );
                if let Some(ref f) = cell.formula {
                    if let Ok(ast) = excli_formula::parser::Parser::new(&f.raw).parse() {
                        graph.register(&ref_str, &ast);
                        continue;
                    }
                }
                graph.register_value(&ref_str);
            }
        }
    }
    graph
}

// ─── Calc ──────────────────────────────────────────────────────────────────────

fn cmd_calc(
    evaluate: Option<&str>,
    context: Option<&str>,
    file: Option<&str>,
    json: bool,
) -> ExcliResult<()> {
    let registry = excli_formula::functions::FunctionRegistry::new();

    if let Some(expr) = evaluate {
        let mut tmp_wb = Workbook::new();
        if let Some(ctx_str) = context {
            if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(ctx_str) {
                if let Some(obj) = json_val.as_object() {
                    let sheet = tmp_wb.add_sheet("Sheet1");
                    for (key, val) in obj {
                        if let Some((col, row)) = parse_cell_ref(key) {
                            sheet.set_cell(col, row, json_value_to_cell(val));
                        }
                    }
                }
            }
        } else {
            tmp_wb.add_sheet("Sheet1");
        }

        let wb_ref: &Workbook = if let Some(f) = file {
            // We need to handle this differently — can't borrow tmp_wb and also have the file wb
            // For simplicity, if file is specified with evaluate, use the file wb
            drop(tmp_wb); // drop the temp
            let owned = excli_io::reader::read_workbook(Path::new(f))?;
            let result = eval_formula_in_wb(expr, &owned, &registry, json)?;
            return Ok(result);
        } else {
            &tmp_wb
        };

        let result = eval_formula_in_wb(expr, wb_ref, &registry, json)?;
        return Ok(result);
    }

    if let Some(f) = file {
        let mut wb = excli_io::reader::read_workbook(Path::new(f))?;
        let mut calc_count = 0usize;
        for sheet_idx in 0..wb.sheets.len() {
            let sheet_name = wb.sheets[sheet_idx].name.clone();
            let mut updates: Vec<(u32, u32, CellValue)> = Vec::new();
            {
                let sheet = &wb.sheets[sheet_idx];
                for (row_idx, row) in sheet.rows.iter().enumerate() {
                    for (col_idx, cell) in row.cells.iter().enumerate() {
                        if let Some(ref formula) = cell.formula {
                            if let Ok(ast) = excli_formula::parser::Parser::new(&formula.raw).parse() {
                                let mut ctx = excli_formula::evaluator::EvalContext::new(&wb, &registry);
                                ctx.current_sheet = Some(sheet_name.clone());
                                if let Ok(val) = excli_formula::evaluator::Evaluator::new().evaluate(&ast.root, &mut ctx) {
                                    updates.push((col_idx as u32, row_idx as u32, val));
                                    calc_count += 1;
                                }
                            }
                        }
                    }
                }
            }
            let sheet_mut = &mut wb.sheets[sheet_idx];
            for (col, row, val) in updates {
                if let Some(cell) = sheet_mut.get_cell_mut(col, row) {
                    cell.value = val;
                }
            }
        }
        excli_io::writer::write_workbook(Path::new(f), &wb)?;
        println!("Recalculated {} formulas in {}", calc_count, f);
        return Ok(());
    }

    Err(ExcliError::new(ErrorCode::GeneralError, "Specify --evaluate <expr> or --file <path>"))
}

fn eval_formula_in_wb(
    expr: &str,
    wb: &Workbook,
    registry: &excli_formula::functions::FunctionRegistry,
    json: bool,
) -> ExcliResult<()> {
    let mut parser = excli_formula::parser::Parser::new(expr);
    let ast = parser.parse().map_err(|e| ExcliError::new(ErrorCode::FormulaParseError, e.to_string()))?;
    let mut ctx = excli_formula::evaluator::EvalContext::new(wb, registry);
    ctx.current_sheet = wb.sheets.first().map(|s| s.name.clone());
    let result = excli_formula::evaluator::Evaluator::new()
        .evaluate(&ast.root, &mut ctx)
        .map_err(|e| ExcliError::new(ErrorCode::FormulaEvalError, e.to_string()))?;

    if json {
        let val = match &result {
            CellValue::Number(n) => serde_json::json!({ "result": n }),
            CellValue::Boolean(b) => serde_json::json!({ "result": b }),
            CellValue::Empty => serde_json::json!({ "result": null }),
            other => serde_json::json!({ "result": other.to_string() }),
        };
        println!("{}", serde_json::to_string_pretty(&val).unwrap_or_default());
    } else {
        println!("{}", result);
    }
    Ok(())
}

// ─── Filter ────────────────────────────────────────────────────────────────────

fn cmd_filter(
    file: &str,
    column: Option<&str>,
    gt: Option<&str>,
    lt: Option<&str>,
    contains: Option<&str>,
    between: Option<&str>,
    in_values: Option<&str>,
    is_null: bool,
    regex: Option<&str>,
    unique: bool,
    json: bool,
    _quiet: bool,
) -> ExcliResult<()> {
    let wb = excli_io::reader::read_workbook(Path::new(file))?;
    let sheet = wb.sheet_by_index(0)
        .ok_or_else(|| ExcliError::new(ErrorCode::GeneralError, "No sheets in workbook"))?;

    let result = if unique {
        filter_unique(sheet)
    } else {
        let col = column.unwrap_or("A");
        let mut conditions: Vec<FilterCondition> = Vec::new();

        if let Some(v) = gt {
            conditions.push(FilterCondition::new(col, FilterOperator::Gt, Some(v.to_string())));
        }
        if let Some(v) = lt {
            conditions.push(FilterCondition::new(col, FilterOperator::Lt, Some(v.to_string())));
        }
        if let Some(v) = contains {
            conditions.push(FilterCondition::new(col, FilterOperator::Contains, Some(v.to_string())));
        }
        if let Some(v) = between {
            let parts: Vec<&str> = v.splitn(2, ',').collect();
            if parts.len() == 2 {
                let mut cond = FilterCondition::new(col, FilterOperator::Between, Some(parts[0].to_string()));
                cond.value2 = Some(parts[1].to_string());
                conditions.push(cond);
            }
        }
        if let Some(v) = in_values {
            let vals: Vec<String> = v.split(',').map(|s| s.trim().to_string()).collect();
            let mut cond = FilterCondition::new(col, FilterOperator::In, None);
            cond.values = Some(vals);
            conditions.push(cond);
        }
        if is_null {
            conditions.push(FilterCondition::new(col, FilterOperator::IsNull, None));
        }
        if let Some(v) = regex {
            conditions.push(FilterCondition::new(col, FilterOperator::Regex, Some(v.to_string())));
        }

        if conditions.is_empty() {
            sheet.clone()
        } else {
            filter_rows(sheet, &conditions)?
        }
    };

    if json { print_sheet_json(&result); } else { print_sheet_csv(&result); }
    Ok(())
}

// ─── Sort ──────────────────────────────────────────────────────────────────────

fn cmd_sort(
    file: &str,
    column: Option<&str>,
    columns: Option<&str>,
    asc: bool,
    desc: bool,
    custom: Option<&str>,
    json: bool,
    _quiet: bool,
) -> ExcliResult<()> {
    let wb = excli_io::reader::read_workbook(Path::new(file))?;
    let sheet = wb.sheet_by_index(0)
        .ok_or_else(|| ExcliError::new(ErrorCode::GeneralError, "No sheets in workbook"))?;

    let _ = asc;
    let order = if desc { SortOrder::Desc } else { SortOrder::Asc };

    let specs: Vec<SortSpec> = if let Some(cols) = columns {
        cols.split(',').map(|c| SortSpec::new(c.trim(), order)).collect()
    } else {
        let col = column.unwrap_or("A");
        let mut spec = SortSpec::new(col, order);
        if let Some(co) = custom {
            spec.custom_order = Some(co.split(',').map(|s| s.trim().to_string()).collect());
        }
        vec![spec]
    };

    let result = sort_rows(sheet, &specs);
    if json { print_sheet_json(&result); } else { print_sheet_csv(&result); }
    Ok(())
}

// ─── Aggregate ─────────────────────────────────────────────────────────────────

fn cmd_agg(
    file: &str,
    group_by: Option<&str>,
    sum: Option<&str>,
    count: bool,
    avg: Option<&str>,
    json: bool,
    _quiet: bool,
) -> ExcliResult<()> {
    let wb = excli_io::reader::read_workbook(Path::new(file))?;
    let sheet = wb.sheet_by_index(0)
        .ok_or_else(|| ExcliError::new(ErrorCode::GeneralError, "No sheets in workbook"))?;

    let group_cols: Vec<String> = group_by
        .map(|s| s.split(',').map(|c| c.trim().to_string()).collect())
        .unwrap_or_default();

    let mut aggs: Vec<Aggregation> = Vec::new();
    if let Some(col) = sum {
        for c in col.split(',') { aggs.push(Aggregation::new(c.trim(), AggregateOp::Sum)); }
    }
    if let Some(col) = avg {
        for c in col.split(',') { aggs.push(Aggregation::new(c.trim(), AggregateOp::Avg)); }
    }
    if count {
        let fc = group_cols.first().map(|s| s.as_str()).unwrap_or("A");
        aggs.push(Aggregation::new(fc, AggregateOp::Count));
    }
    if aggs.is_empty() {
        aggs.push(Aggregation::new("A", AggregateOp::Count));
    }

    let result = aggregate(sheet, &group_cols, &aggs);
    if json { print_sheet_json(&result); } else { print_sheet_csv(&result); }
    Ok(())
}

// ─── Merge ─────────────────────────────────────────────────────────────────────

fn cmd_merge(
    file1: &str,
    file2: &str,
    on: Option<&str>,
    join_type_str: &str,
    left_on: Option<&str>,
    right_on: Option<&str>,
    cross: bool,
    json: bool,
    _quiet: bool,
) -> ExcliResult<()> {
    let wb1 = excli_io::reader::read_workbook(Path::new(file1))?;
    let wb2 = excli_io::reader::read_workbook(Path::new(file2))?;

    let join_type = if cross {
        JoinType::Cross
    } else {
        match join_type_str.to_lowercase().as_str() {
            "left" => JoinType::Left,
            "right" => JoinType::Right,
            "outer" | "full" => JoinType::Outer,
            _ => JoinType::Inner,
        }
    };

    let key_l = left_on.or(on).unwrap_or("A");
    let key_r = right_on.or(on).unwrap_or("A");

    let result = merge_workbooks(&wb1, &wb2, key_l, key_r, join_type);
    let sheet = result.sheet_by_index(0)
        .ok_or_else(|| ExcliError::new(ErrorCode::GeneralError, "No result sheet"))?;
    if json { print_sheet_json(sheet); } else { print_sheet_csv(sheet); }
    Ok(())
}

// ─── Concat ────────────────────────────────────────────────────────────────────

fn cmd_concat(files: &[String], axis: &str, json: bool, _quiet: bool) -> ExcliResult<()> {
    if files.is_empty() {
        return Err(ExcliError::new(ErrorCode::GeneralError, "No files specified"));
    }
    let workbooks: Vec<Workbook> = files
        .iter()
        .map(|f| excli_io::reader::read_workbook(Path::new(f)))
        .collect::<ExcliResult<_>>()?;
    let refs: Vec<&Workbook> = workbooks.iter().collect();
    let concat_axis = match axis.to_lowercase().as_str() {
        "cols" | "col" | "columns" | "1" => ConcatAxis::Cols,
        _ => ConcatAxis::Rows,
    };
    let result = concat_workbooks(&refs, concat_axis);
    let sheet = result.sheet_by_index(0)
        .ok_or_else(|| ExcliError::new(ErrorCode::GeneralError, "No result sheet"))?;
    if json { print_sheet_json(sheet); } else { print_sheet_csv(sheet); }
    Ok(())
}

// ─── Transpose ─────────────────────────────────────────────────────────────────

fn cmd_transpose(file: &str, output: Option<&str>, json: bool, quiet: bool) -> ExcliResult<()> {
    let wb = excli_io::reader::read_workbook(Path::new(file))?;
    let sheet = wb.sheet_by_index(0)
        .ok_or_else(|| ExcliError::new(ErrorCode::GeneralError, "No sheets in workbook"))?;
    let transposed = transpose(sheet);

    if let Some(out_path) = output {
        let mut out_wb = Workbook::new();
        out_wb.sheets.push(transposed);
        excli_io::writer::write_workbook(Path::new(out_path), &out_wb)?;
        if !quiet { println!("Transposed to {}", out_path); }
    } else {
        if json { print_sheet_json(&transposed); } else { print_sheet_csv(&transposed); }
    }
    Ok(())
}

// ─── Split ─────────────────────────────────────────────────────────────────────

fn cmd_split(
    file: &str,
    by: Option<&str>,
    rows: Option<usize>,
    output_dir: Option<&str>,
    quiet: bool,
) -> ExcliResult<()> {
    let wb = excli_io::reader::read_workbook(Path::new(file))?;
    let sheet = wb.sheet_by_index(0)
        .ok_or_else(|| ExcliError::new(ErrorCode::GeneralError, "No sheets in workbook"))?;

    let out_dir = output_dir.unwrap_or(".");
    std::fs::create_dir_all(out_dir)?;
    let out_path = Path::new(out_dir);
    let stem = Path::new(file).file_stem().and_then(|s| s.to_str()).unwrap_or("split");

    if let Some(col) = by {
        let groups = split_by_column(sheet, col, out_path);
        let count = groups.len();
        for (key, group_sheet) in groups {
            let safe_key: String = key.chars()
                .map(|c| if c.is_alphanumeric() || c == '-' { c } else { '_' })
                .collect();
            let out_file = out_path.join(format!("{}_{}.csv", stem, safe_key));
            let mut group_wb = Workbook::new();
            group_wb.sheets.push(group_sheet);
            excli_io::writer::write_workbook(&out_file, &group_wb)?;
        }
        if !quiet { println!("Split {} groups into {}", count, out_dir); }
    } else if let Some(chunk_size) = rows {
        let chunks = split_by_rows(sheet, chunk_size);
        for (i, chunk) in chunks.iter().enumerate() {
            let out_file = out_path.join(format!("{}_part{:03}.csv", stem, i + 1));
            let mut chunk_wb = Workbook::new();
            chunk_wb.sheets.push(chunk.clone());
            excli_io::writer::write_workbook(&out_file, &chunk_wb)?;
        }
        if !quiet { println!("Split into {} parts in {}", chunks.len(), out_dir); }
    } else {
        return Err(ExcliError::new(ErrorCode::GeneralError, "Specify --by <column> or --rows <n>"));
    }
    Ok(())
}

// ─── Sample ────────────────────────────────────────────────────────────────────

fn cmd_sample(
    file: &str,
    n: Option<usize>,
    frac: Option<f64>,
    _stratify: Option<&str>,
    json: bool,
    _quiet: bool,
) -> ExcliResult<()> {
    let wb = excli_io::reader::read_workbook(Path::new(file))?;
    let sheet = wb.sheet_by_index(0)
        .ok_or_else(|| ExcliError::new(ErrorCode::GeneralError, "No sheets in workbook"))?;

    let total = sheet.row_count();
    let sample_n = if let Some(count) = n {
        count.min(total)
    } else if let Some(f) = frac {
        ((total as f64) * f.clamp(0.0, 1.0)) as usize
    } else {
        return Err(ExcliError::new(ErrorCode::GeneralError, "Specify --n <count> or --frac <fraction>"));
    };

    let indices = sample_row_indices(total, sample_n);
    let mut result = excli_core::Sheet::new(format!("{}_sample", sheet.name));
    for (new_idx, &src_idx) in indices.iter().enumerate() {
        if let Some(row) = sheet.rows.get(src_idx) {
            for (col_idx, cell) in row.cells.iter().enumerate() {
                result.set_cell(col_idx as u32, new_idx as u32, cell.clone());
            }
        }
    }

    if json { print_sheet_json(&result); } else { print_sheet_csv(&result); }
    Ok(())
}

// ─── Search ────────────────────────────────────────────────────────────────────

fn cmd_search(
    file: &str,
    pattern: &str,
    regex: bool,
    case_sensitive: bool,
    whole_word: bool,
    json: bool,
    _quiet: bool,
) -> ExcliResult<()> {
    let wb = excli_io::reader::read_workbook(Path::new(file))?;
    let mut options = SearchOptions::new(pattern);
    options.regex = regex;
    options.case_sensitive = case_sensitive;
    options.whole_word = whole_word;

    let mut all_results = Vec::new();
    for sheet in &wb.sheets {
        all_results.extend(search_cells(sheet, &options));
    }

    if json {
        let out: Vec<serde_json::Value> = all_results.iter().map(|r| {
            serde_json::json!({ "location": r.display_location(), "value": r.value, "matched": r.matched_text })
        }).collect();
        println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
    } else {
        if all_results.is_empty() {
            println!("No matches found for '{}'", pattern);
        } else {
            println!("{} match(es) for '{}':", all_results.len(), pattern);
            for r in &all_results {
                println!("  {} = {:?}", r.display_location(), r.value);
            }
        }
    }
    Ok(())
}

// ─── Replace ───────────────────────────────────────────────────────────────────

fn cmd_replace(file: &str, find: &str, replace: &str, dry_run: bool, quiet: bool) -> ExcliResult<()> {
    let mut wb = excli_io::reader::read_workbook(Path::new(file))?;

    let total: usize = wb.sheets.iter_mut().map(|s| {
        let (new_sheet, count) = replace_cells(s, find, replace, false, dry_run);
        *s = new_sheet;
        count
    }).sum();

    if dry_run {
        println!("DRY RUN: would replace {} occurrence(s) of '{}' with '{}'", total, find, replace);
    } else {
        excli_io::writer::write_workbook(Path::new(file), &wb)?;
        if !quiet {
            println!("Replaced {} occurrence(s) of '{}' with '{}'", total, find, replace);
        }
    }
    Ok(())
}

// ─── Info ──────────────────────────────────────────────────────────────────────

fn cmd_info(file: &str, detailed: bool, schema: bool, json: bool) -> ExcliResult<()> {
    let wb = excli_io::reader::read_workbook(Path::new(file))?;
    let stats = wb.stats();

    if schema {
        if let Some(sheet) = wb.sheet_by_index(0) {
            let schema_info: Vec<serde_json::Value> = sheet.rows.first()
                .map(|header| {
                    header.cells.iter().enumerate().map(|(i, cell)| {
                        let inferred = sheet.rows.get(1)
                            .and_then(|r| r.cells.get(i))
                            .map(|c| infer_type(&c.value))
                            .unwrap_or("string");
                        serde_json::json!({ "column": cell.value.to_string(), "index": i, "type": inferred })
                    }).collect()
                })
                .unwrap_or_default();
            println!("{}", serde_json::to_string_pretty(&schema_info).unwrap_or_default());
        }
        return Ok(());
    }

    if json {
        let mut info = serde_json::json!({
            "file": file,
            "sheets": stats.sheet_count,
            "total_rows": stats.row_count,
            "total_cols": stats.col_count,
            "formulas": stats.formula_count,
            "file_size_bytes": stats.file_size_bytes,
        });
        if detailed {
            let sheets: Vec<serde_json::Value> = wb.sheets.iter().map(|s| {
                serde_json::json!({
                    "name": s.name,
                    "rows": s.row_count(),
                    "cols": s.col_count(),
                    "formulas": s.stats().formula_count,
                    "hidden": s.state != SheetState::Visible,
                })
            }).collect();
            info["sheet_details"] = serde_json::Value::Array(sheets);
        }
        println!("{}", serde_json::to_string_pretty(&info).unwrap_or_default());
    } else {
        println!("File:     {}", file);
        println!("Sheets:   {}", stats.sheet_count);
        println!("Rows:     {}", stats.row_count);
        println!("Cols:     {}", stats.col_count);
        println!("Formulas: {}", stats.formula_count);
        println!("Size:     {} bytes", stats.file_size_bytes);
        if detailed {
            println!("\nSheet details:");
            for s in &wb.sheets {
                println!("  {:<30} {} rows x {} cols", s.name, s.row_count(), s.col_count());
            }
        }
    }
    Ok(())
}

fn infer_type(val: &CellValue) -> &'static str {
    match val {
        CellValue::Number(_) => "number",
        CellValue::Boolean(_) => "boolean",
        CellValue::Empty => "null",
        _ => "string",
    }
}

// ─── Convert ───────────────────────────────────────────────────────────────────

fn cmd_convert(input: &str, output: &str, sheet: Option<&str>, _delimiter: Option<&str>, quiet: bool) -> ExcliResult<()> {
    let mut wb = excli_io::reader::read_workbook(Path::new(input))?;
    if let Some(name) = sheet {
        wb.sheets.retain(|s| s.name == name);
        if wb.sheets.is_empty() {
            return Err(ExcliError::sheet_not_found(name));
        }
    }
    excli_io::writer::write_workbook(Path::new(output), &wb)?;
    if !quiet { println!("Converted {} -> {}", input, output); }
    Ok(())
}

// ─── Clean ─────────────────────────────────────────────────────────────────────

fn cmd_clean(
    file: &str,
    command: &str,
    _range: Option<&str>,
    columns: Option<&str>,
    fill: Option<&str>,
    json: bool,
    quiet: bool,
) -> ExcliResult<()> {
    let wb = excli_io::reader::read_workbook(Path::new(file))?;
    let sheet = wb.sheet_by_index(0)
        .ok_or_else(|| ExcliError::new(ErrorCode::GeneralError, "No sheets in workbook"))?;

    let result = match command.to_lowercase().as_str() {
        "trim" => clean_trim(sheet, None),
        "upper" => clean_case(sheet, None, CaseType::Upper),
        "lower" => clean_case(sheet, None, CaseType::Lower),
        "proper" | "title" => clean_case(sheet, None, CaseType::Proper),
        "drop-blanks" | "drop_blanks" => clean_blanks(sheet, None, BlankFillStrategy::DropRows),
        "fill-forward" | "ffill" => clean_blanks(sheet, None, BlankFillStrategy::FillForward),
        "fill-interpolate" | "interpolate" => clean_blanks(sheet, None, BlankFillStrategy::FillInterpolate),
        "fill" => {
            let val = fill.unwrap_or("");
            clean_blanks(sheet, None, BlankFillStrategy::FillValue(CellValue::InlineString(val.to_string())))
        }
        "dedup" | "remove-duplicates" | "drop-duplicates" => {
            let cols: Vec<String> = columns
                .map(|c| c.split(',').map(|s| s.trim().to_string()).collect())
                .unwrap_or_default();
            remove_duplicates(sheet, &cols)
        }
        other => return Err(ExcliError::new(ErrorCode::GeneralError,
            format!("Unknown clean command '{}'. Try: trim, upper, lower, proper, drop-blanks, fill-forward, fill, dedup", other))),
    };

    if json { print_sheet_json(&result); } else { print_sheet_csv(&result); }
    if !quiet { eprintln!("Clean '{}' applied to {}", command, file); }
    Ok(())
}

// ─── Diff ──────────────────────────────────────────────────────────────────────

#[derive(serde::Serialize)]
struct DiffEntry {
    location: String,
    file1: String,
    file2: String,
    kind: String,
}

fn cmd_diff(
    file1: &str,
    file2: &str,
    sheet: Option<&str>,
    values_only: bool,
    tolerance: Option<f64>,
    json: bool,
    _quiet: bool,
) -> ExcliResult<()> {
    let wb1 = excli_io::reader::read_workbook(Path::new(file1))?;
    let wb2 = excli_io::reader::read_workbook(Path::new(file2))?;
    let mut diffs: Vec<DiffEntry> = Vec::new();

    for sheet1 in &wb1.sheets {
        let sname = &sheet1.name;
        if let Some(ref filter) = sheet { if sname != filter { continue; } }

        match wb2.sheet_by_name(sname) {
            None => diffs.push(DiffEntry {
                location: format!("Sheet:{}", sname),
                file1: format!("exists ({} rows)", sheet1.row_count()),
                file2: "missing".to_string(),
                kind: "sheet_missing".to_string(),
            }),
            Some(sheet2) => {
                let max_rows = sheet1.row_count().max(sheet2.row_count());
                let max_cols = sheet1.col_count().max(sheet2.col_count());

                for row in 0..max_rows {
                    for col in 0..max_cols {
                        let get_val = |s: &excli_core::Sheet| -> String {
                            s.get_cell(col as u32, row as u32).map(|c| {
                                if values_only { c.value.to_string() }
                                else { c.formula.as_ref().map(|f| f.raw.clone()).unwrap_or_else(|| c.value.to_string()) }
                            }).unwrap_or_default()
                        };
                        let v1 = get_val(sheet1);
                        let v2 = get_val(sheet2);
                        if v1 != v2 {
                            let within_tol = tolerance.map_or(false, |tol| {
                                v1.parse::<f64>().ok().zip(v2.parse::<f64>().ok())
                                    .map_or(false, |(n1, n2)| (n1 - n2).abs() <= tol)
                            });
                            if !within_tol {
                                diffs.push(DiffEntry {
                                    location: format!("{}!{}{}", sname,
                                        excli_core::reference::column_index_to_name(col as u32), row + 1),
                                    file1: v1, file2: v2, kind: "changed".to_string(),
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    for sheet2 in &wb2.sheets {
        if wb1.sheet_by_name(&sheet2.name).is_none() {
            diffs.push(DiffEntry {
                location: format!("Sheet:{}", sheet2.name),
                file1: "missing".to_string(),
                file2: format!("exists ({} rows)", sheet2.row_count()),
                kind: "sheet_added".to_string(),
            });
        }
    }

    if json {
        println!("{}", serde_json::to_string_pretty(&diffs).unwrap_or_default());
    } else if diffs.is_empty() {
        println!("No differences found.");
    } else {
        println!("{} difference(s):", diffs.len());
        for d in &diffs { println!("  {} | {:?} -> {:?}", d.location, d.file1, d.file2); }
    }
    Ok(())
}

// ─── Batch ─────────────────────────────────────────────────────────────────────

fn cmd_batch(glob_pattern: &str, command: &[String], quiet: bool) -> ExcliResult<()> {
    if command.is_empty() {
        return Err(ExcliError::new(ErrorCode::GeneralError, "No command specified for batch"));
    }

    let parent = Path::new(glob_pattern).parent().unwrap_or(Path::new("."));
    let pattern = Path::new(glob_pattern).file_name().and_then(|s| s.to_str()).unwrap_or(glob_pattern);

    let mut matched: Vec<std::path::PathBuf> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(parent) {
        for entry in entries.flatten() {
            if glob_match(pattern, &entry.file_name().to_string_lossy()) {
                matched.push(entry.path());
            }
        }
    }
    matched.sort();

    if matched.is_empty() {
        if !quiet { println!("No files matched: {}", glob_pattern); }
        return Ok(());
    }

    let (mut ok, mut err) = (0usize, 0usize);
    for path in &matched {
        if !quiet { println!("Processing: {}", path.display()); }
        match excli_io::reader::read_workbook(path) {
            Ok(_) => { ok += 1; if !quiet { println!("  OK"); } }
            Err(e) => { err += 1; eprintln!("  ERROR: {}", e); }
        }
    }
    if !quiet { println!("Batch complete: {}/{} ok, {} errors", ok, matched.len(), err); }
    Ok(())
}

// ─── Detect encoding / language ────────────────────────────────────────────────

fn cmd_detect_encoding(file: &str, _all: bool) -> ExcliResult<()> {
    let data = std::fs::read(file)?;
    let info = excli_io::format_detect::detect_encoding(&data);
    println!("{} (confidence: {:.1}%)", info.encoding, info.confidence * 100.0);
    Ok(())
}

fn cmd_detect_lang(file: &str, json: bool) -> ExcliResult<()> {
    let wb = excli_io::reader::read_workbook(Path::new(file))?;
    let (mut cjk, mut latin, mut arabic, mut cyrillic) = (0u32, 0u32, 0u32, 0u32);

    for sheet in &wb.sheets {
        for row in &sheet.rows {
            for cell in &row.cells {
                for ch in cell.value.to_string().chars() {
                    if ('\u{4E00}'..='\u{9FFF}').contains(&ch) || ('\u{3040}'..='\u{30FF}').contains(&ch) { cjk += 1; }
                    else if ('\u{0600}'..='\u{06FF}').contains(&ch) { arabic += 1; }
                    else if ('\u{0400}'..='\u{04FF}').contains(&ch) { cyrillic += 1; }
                    else if ch.is_ascii_alphabetic() { latin += 1; }
                }
            }
        }
    }

    let max = cjk.max(latin).max(arabic).max(cyrillic);
    let dominant = if max == 0 { "unknown" }
    else if cjk == max { "zh/ja/ko" }
    else if arabic == max { "ar" }
    else if cyrillic == max { "ru/uk" }
    else { "en/latin" };

    if json {
        println!("{}", serde_json::json!({ "dominant": dominant, "cjk": cjk, "latin": latin, "arabic": arabic, "cyrillic": cyrillic }));
    } else {
        println!("Dominant: {} (CJK={}, Latin={}, Arabic={}, Cyrillic={})", dominant, cjk, latin, arabic, cyrillic);
    }
    Ok(())
}

// ─── Validate ──────────────────────────────────────────────────────────────────

fn cmd_validate(file: &str, reference: &str, json: bool, quiet: bool) -> ExcliResult<()> {
    let wb = excli_io::reader::read_workbook(Path::new(file))?;
    let ref_wb = excli_io::reader::read_workbook(Path::new(reference))?;
    let mut issues: Vec<String> = Vec::new();

    for ref_sheet in &ref_wb.sheets {
        match wb.sheet_by_name(&ref_sheet.name) {
            None => issues.push(format!("Missing sheet: {}", ref_sheet.name)),
            Some(data_sheet) => {
                if let (Some(rh), Some(dh)) = (ref_sheet.rows.first(), data_sheet.rows.first()) {
                    for (i, rc) in rh.cells.iter().enumerate() {
                        let rv = rc.value.to_string();
                        let dv = dh.cells.get(i).map(|c| c.value.to_string()).unwrap_or_default();
                        if rv != dv {
                            issues.push(format!("Column mismatch in '{}': expected '{}', found '{}'", ref_sheet.name, rv, dv));
                        }
                    }
                }
            }
        }
    }

    if json {
        println!("{}", serde_json::json!({ "valid": issues.is_empty(), "issues": issues }));
    } else if issues.is_empty() {
        if !quiet { println!("Validation passed: {} matches schema of {}", file, reference); }
    } else {
        println!("{} issue(s):", issues.len());
        for issue in &issues { println!("  - {}", issue); }
    }
    Ok(())
}

// ─── Describe ──────────────────────────────────────────────────────────────────

fn cmd_describe(command: Option<&str>, all: bool) -> ExcliResult<()> {
    let commands: &[(&str, &str, &str)] = &[
        ("read",    "Read data from Excel/CSV file",          "excli read <file> [--sheet <name>] [--range A1:Z100] [--stats] [--json]"),
        ("write",   "Write data to Excel/CSV file",           "excli write <output> --data <json|csv> [--sheet <name>] [--append] [--start-cell B2]"),
        ("sheet",   "Sheet management",                       "excli sheet <list|info|rename|add|delete|copy|move|hide|unhide> <file>"),
        ("formula", "Formula operations",                     "excli formula <read|set|deps|dependents|errors|circular>"),
        ("calc",    "Evaluate formulas",                      "excli calc --evaluate \"=SUM(1,2,3)\" | --file <path>"),
        ("filter",  "Filter rows",                            "excli filter <file> --column <col> [--gt|--lt|--contains|--between|--in|--regex <val>] [--unique]"),
        ("sort",    "Sort rows",                              "excli sort <file> [--column <col>] [--asc|--desc] [--custom a,b,c]"),
        ("agg",     "Aggregate data",                         "excli agg <file> [--group-by <cols>] [--sum <col>] [--avg <col>] [--count]"),
        ("merge",   "Join two files",                         "excli merge <file1> <file2> --on <col> [--type inner|left|right|outer]"),
        ("concat",  "Concatenate files",                      "excli concat <files...> [--axis rows|cols]"),
        ("transpose","Transpose a sheet",                     "excli transpose <file> [--output <path>]"),
        ("split",   "Split file",                             "excli split <file> [--by <col>] [--rows <n>] [--output-dir <dir>]"),
        ("sample",  "Random sample",                          "excli sample <file> [--n <count>] [--frac <0-1>]"),
        ("search",  "Search for pattern",                     "excli search <file> --pattern <text> [--regex] [--case-sensitive] [--whole-word]"),
        ("replace", "Find and replace",                       "excli replace <file> --find <old> --replace <new> [--dry-run]"),
        ("info",    "File information",                       "excli info <file> [--detailed] [--schema] [--json]"),
        ("convert", "Convert between formats",                "excli convert <input> <output> [--sheet <name>]"),
        ("clean",   "Data cleaning",                          "excli clean <file> --command trim|upper|lower|dedup|drop-blanks|fill-forward|fill"),
        ("diff",    "Compare two files",                      "excli diff <file1> <file2> [--values-only] [--tolerance <n>] [--json]"),
        ("batch",   "Batch process by glob",                  "excli batch '*.xlsx' info"),
        ("detect-encoding", "Detect file encoding",           "excli detect-encoding <file>"),
        ("detect-lang",     "Detect content language",        "excli detect-lang <file> [--json]"),
        ("validate",        "Validate against reference",     "excli validate <file> --reference <schema> [--json]"),
    ];

    if all {
        let schema: Vec<serde_json::Value> = commands.iter().map(|(name, desc, usage)| {
            serde_json::json!({ "command": name, "description": desc, "usage": usage })
        }).collect();
        println!("{}", serde_json::to_string_pretty(&schema).unwrap_or_default());
    } else if let Some(cmd) = command {
        if let Some((_, desc, usage)) = commands.iter().find(|(n, _, _)| *n == cmd) {
            println!("{}", serde_json::json!({ "command": cmd, "description": desc, "usage": usage }));
        } else {
            println!("Unknown command '{}'. Use --all to list all.", cmd);
        }
    } else {
        println!("Available commands (use --all for full list):");
        for (name, desc, _) in commands {
            println!("  {:<20} {}", name, desc);
        }
    }
    Ok(())
}

// ─── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_csv(path: &std::path::Path, content: &str) {
        let mut f = std::fs::File::create(path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
    }

    // ── cmd_read ─────────────────────────────────────────────────────────────

    #[test]
    fn test_cmd_read_csv_plain() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("data.csv");
        write_csv(&p, "Name,Age\nAlice,30\nBob,25\n");
        assert!(cmd_read(p.to_str().unwrap(), None, None, 0, None, false, false, false).is_ok());
    }

    #[test]
    fn test_cmd_read_csv_json_mode() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("data.csv");
        write_csv(&p, "A,B\n1,2\n");
        assert!(cmd_read(p.to_str().unwrap(), None, None, 0, None, false, false, true).is_ok());
    }

    #[test]
    fn test_cmd_read_skip_rows() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("data.csv");
        write_csv(&p, "H1,H2\nX,Y\n1,2\n3,4\n");
        assert!(cmd_read(p.to_str().unwrap(), None, None, 1, Some(2), false, false, false).is_ok());
    }

    #[test]
    fn test_cmd_read_stats() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("data.csv");
        write_csv(&p, "A,B\n1,2\n3,4\n");
        assert!(cmd_read(p.to_str().unwrap(), None, None, 0, None, false, true, false).is_ok());
    }

    #[test]
    fn test_cmd_read_nonexistent_file_errors() {
        let result = cmd_read("/tmp/no_such_file_excli.csv", None, None, 0, None, false, false, false);
        assert!(result.is_err());
    }

    // ── cmd_write ────────────────────────────────────────────────────────────

    #[test]
    fn test_cmd_write_json_array() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("out.csv");
        assert!(cmd_write(out.to_str().unwrap(), Some("[[1,2,3],[4,5,6]]"), None, None, false, true).is_ok());
        assert!(out.exists());
    }

    #[test]
    fn test_cmd_write_csv_text() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("out.csv");
        assert!(cmd_write(out.to_str().unwrap(), Some("a,b,c\n1,2,3"), None, None, false, true).is_ok());
        let content = std::fs::read_to_string(&out).unwrap();
        assert!(content.contains("a") || content.contains("1"));
    }

    #[test]
    fn test_cmd_write_no_data_errors() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("out.csv");
        assert!(cmd_write(out.to_str().unwrap(), None, None, None, false, true).is_err());
    }

    #[test]
    fn test_cmd_write_with_start_cell() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("out.csv");
        assert!(cmd_write(out.to_str().unwrap(), Some("X,Y"), None, Some("B2"), false, true).is_ok());
    }

    // ── cmd_info ─────────────────────────────────────────────────────────────

    #[test]
    fn test_cmd_info_plain() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("data.csv");
        write_csv(&p, "col1,col2\n1,2\n3,4\n");
        assert!(cmd_info(p.to_str().unwrap(), false, false, false).is_ok());
    }

    #[test]
    fn test_cmd_info_json() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("data.csv");
        write_csv(&p, "x,y\n10,20\n");
        assert!(cmd_info(p.to_str().unwrap(), false, false, true).is_ok());
    }

    #[test]
    fn test_cmd_info_detailed() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("data.csv");
        write_csv(&p, "x,y\n10,20\n");
        assert!(cmd_info(p.to_str().unwrap(), true, false, false).is_ok());
    }

    #[test]
    fn test_cmd_info_schema() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("data.csv");
        write_csv(&p, "Name,Age,Score\nAlice,30,95.5\n");
        assert!(cmd_info(p.to_str().unwrap(), false, true, false).is_ok());
    }

    // ── cmd_convert ──────────────────────────────────────────────────────────

    #[test]
    fn test_cmd_convert_csv_to_csv() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src.csv");
        let dst = dir.path().join("dst.csv");
        write_csv(&src, "A,B\n1,2\n");
        assert!(cmd_convert(src.to_str().unwrap(), dst.to_str().unwrap(), None, None, true).is_ok());
        assert!(dst.exists());
    }

    // ── cmd_detect_encoding ──────────────────────────────────────────────────

    #[test]
    fn test_cmd_detect_encoding_utf8() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("utf8.txt");
        write_csv(&p, "hello world\n");
        assert!(cmd_detect_encoding(p.to_str().unwrap(), false).is_ok());
    }

    // ── cmd_filter ───────────────────────────────────────────────────────────

    #[test]
    fn test_cmd_filter_gt() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("data.csv");
        write_csv(&p, "Name,Score\nAlice,95\nBob,70\n");
        assert!(cmd_filter(p.to_str().unwrap(), Some("B"), Some("80"), None, None, None, None, false, None, false, false, false).is_ok());
    }

    #[test]
    fn test_cmd_filter_unique() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("data.csv");
        write_csv(&p, "Dept\nEng\nEng\nHR\n");
        assert!(cmd_filter(p.to_str().unwrap(), None, None, None, None, None, None, false, None, true, false, false).is_ok());
    }

    #[test]
    fn test_cmd_filter_stub_ok() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("data.csv");
        write_csv(&p, "A,B\n1,2\n");
        assert!(cmd_filter(p.to_str().unwrap(), None, None, None, None, None, None, false, None, false, false, false).is_ok());
    }

    // ── cmd_sort ─────────────────────────────────────────────────────────────

    #[test]
    fn test_cmd_sort_asc() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("data.csv");
        write_csv(&p, "Name,Score\nCarol,88\nAlice,95\nBob,70\n");
        assert!(cmd_sort(p.to_str().unwrap(), Some("B"), None, true, false, None, false, false).is_ok());
    }

    #[test]
    fn test_cmd_sort_stub_ok() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("data.csv");
        write_csv(&p, "A,B\n3,1\n1,2\n");
        assert!(cmd_sort(p.to_str().unwrap(), Some("A"), None, true, false, None, false, false).is_ok());
    }

    // ── cmd_agg ──────────────────────────────────────────────────────────────

    #[test]
    fn test_cmd_agg_sum() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("data.csv");
        write_csv(&p, "Dept,Salary\nEng,100\nEng,200\nHR,150\n");
        assert!(cmd_agg(p.to_str().unwrap(), Some("A"), Some("B"), false, None, false, false).is_ok());
    }

    #[test]
    fn test_cmd_agg_stub_ok() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("data.csv");
        write_csv(&p, "Dept,Salary\nEng,100\nEng,200\n");
        assert!(cmd_agg(p.to_str().unwrap(), None, None, false, None, false, false).is_ok());
    }

    // ── cmd_merge ────────────────────────────────────────────────────────────

    #[test]
    fn test_cmd_merge_inner() {
        let dir = tempfile::tempdir().unwrap();
        let p1 = dir.path().join("a.csv");
        let p2 = dir.path().join("b.csv");
        write_csv(&p1, "ID,Name\n1,Alice\n2,Bob\n");
        write_csv(&p2, "ID,Score\n1,95\n3,80\n");
        assert!(cmd_merge(p1.to_str().unwrap(), p2.to_str().unwrap(), Some("A"), "inner", None, None, false, false, false).is_ok());
    }

    // ── cmd_concat ───────────────────────────────────────────────────────────

    #[test]
    fn test_cmd_concat_rows() {
        let dir = tempfile::tempdir().unwrap();
        let p1 = dir.path().join("a.csv");
        let p2 = dir.path().join("b.csv");
        write_csv(&p1, "A\n1\n");
        write_csv(&p2, "A\n2\n");
        assert!(cmd_concat(&[p1.to_str().unwrap().to_string(), p2.to_str().unwrap().to_string()], "rows", false, false).is_ok());
    }

    // ── cmd_transpose ────────────────────────────────────────────────────────

    #[test]
    fn test_cmd_transpose_to_stdout() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("data.csv");
        write_csv(&p, "A,B\n1,2\n3,4\n");
        assert!(cmd_transpose(p.to_str().unwrap(), None, false, false).is_ok());
    }

    #[test]
    fn test_cmd_transpose_to_file() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("data.csv");
        let out = dir.path().join("out.csv");
        write_csv(&p, "A,B\n1,2\n3,4\n");
        assert!(cmd_transpose(p.to_str().unwrap(), Some(out.to_str().unwrap()), false, true).is_ok());
        assert!(out.exists());
    }

    // ── cmd_split ────────────────────────────────────────────────────────────

    #[test]
    fn test_cmd_split_by_rows() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("data.csv");
        let out = dir.path().join("out");
        write_csv(&p, "Name\nAlice\nBob\nCarol\nDave\n");
        assert!(cmd_split(p.to_str().unwrap(), None, Some(2), Some(out.to_str().unwrap()), true).is_ok());
    }

    // ── cmd_sample ───────────────────────────────────────────────────────────

    #[test]
    fn test_cmd_sample_n() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("data.csv");
        write_csv(&p, "Name\nA\nB\nC\nD\nE\n");
        assert!(cmd_sample(p.to_str().unwrap(), Some(3), None, None, false, false).is_ok());
    }

    // ── cmd_search ───────────────────────────────────────────────────────────

    #[test]
    fn test_cmd_search_plain() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("data.csv");
        write_csv(&p, "Name,City\nAlice,New York\nBob,Boston\n");
        assert!(cmd_search(p.to_str().unwrap(), "Alice", false, false, false, false, false).is_ok());
    }

    // ── cmd_replace ──────────────────────────────────────────────────────────

    #[test]
    fn test_cmd_replace_dry_run() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("data.csv");
        write_csv(&p, "Name\nAlice\nBob\n");
        assert!(cmd_replace(p.to_str().unwrap(), "Alice", "Carol", true, true).is_ok());
    }

    #[test]
    fn test_cmd_replace_live() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("data.csv");
        write_csv(&p, "Name\nAlice\nBob\n");
        assert!(cmd_replace(p.to_str().unwrap(), "Alice", "Carol", false, true).is_ok());
    }

    // ── cmd_clean ────────────────────────────────────────────────────────────

    #[test]
    fn test_cmd_clean_trim() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("data.csv");
        write_csv(&p, "Name\n  Alice  \n  Bob  \n");
        assert!(cmd_clean(p.to_str().unwrap(), "trim", None, None, None, false, false).is_ok());
    }

    #[test]
    fn test_cmd_clean_upper() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("data.csv");
        write_csv(&p, "Name\nalice\nbob\n");
        assert!(cmd_clean(p.to_str().unwrap(), "upper", None, None, None, false, false).is_ok());
    }

    #[test]
    fn test_cmd_clean_dedup() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("data.csv");
        write_csv(&p, "Dept\nEng\nEng\nHR\n");
        assert!(cmd_clean(p.to_str().unwrap(), "dedup", None, None, None, false, false).is_ok());
    }

    #[test]
    fn test_cmd_clean_unknown_errors() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("data.csv");
        write_csv(&p, "A\n1\n");
        assert!(cmd_clean(p.to_str().unwrap(), "bogus", None, None, None, false, false).is_err());
    }

    // ── cmd_diff ─────────────────────────────────────────────────────────────

    #[test]
    fn test_cmd_diff_identical() {
        let dir = tempfile::tempdir().unwrap();
        let p1 = dir.path().join("a.csv");
        let p2 = dir.path().join("b.csv");
        write_csv(&p1, "A\n1\n");
        write_csv(&p2, "A\n1\n");
        assert!(cmd_diff(p1.to_str().unwrap(), p2.to_str().unwrap(), None, false, None, false, false).is_ok());
    }

    #[test]
    fn test_cmd_diff_stub_ok() {
        let dir = tempfile::tempdir().unwrap();
        let p1 = dir.path().join("a.csv");
        let p2 = dir.path().join("b.csv");
        write_csv(&p1, "A\n1\n");
        write_csv(&p2, "A\n2\n");
        assert!(cmd_diff(p1.to_str().unwrap(), p2.to_str().unwrap(), None, false, None, false, false).is_ok());
    }

    // ── cmd_describe ─────────────────────────────────────────────────────────

    #[test]
    fn test_cmd_describe_all() { assert!(cmd_describe(None, true).is_ok()); }

    #[test]
    fn test_cmd_describe_command() { assert!(cmd_describe(Some("read"), false).is_ok()); }

    #[test]
    fn test_cmd_describe_no_args() { assert!(cmd_describe(None, false).is_ok()); }

    // ── cmd_detect_lang ──────────────────────────────────────────────────────

    #[test]
    fn test_cmd_detect_lang() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("data.csv");
        write_csv(&p, "Name\nAlice\n");
        assert!(cmd_detect_lang(p.to_str().unwrap(), false).is_ok());
    }

    // ── helpers ──────────────────────────────────────────────────────────────

    #[test]
    fn test_glob_match_star() {
        assert!(glob_match("*.csv", "data.csv"));
        assert!(!glob_match("*.csv", "data.xlsx"));
    }

    #[test]
    fn test_glob_match_question() {
        assert!(glob_match("data?.csv", "data1.csv"));
        assert!(!glob_match("data?.csv", "data10.csv"));
    }

    #[test]
    fn test_parse_cell_ref_valid() {
        assert_eq!(parse_cell_ref("A1"), Some((0, 0)));
        assert_eq!(parse_cell_ref("B2"), Some((1, 1)));
    }

    #[test]
    fn test_sample_indices_count() {
        let idx = sample_row_indices(100, 10);
        assert_eq!(idx.len(), 10);
        assert!(idx.iter().all(|&i| i < 100));
        assert!(idx.windows(2).all(|w| w[0] <= w[1]));
    }

    // ── Additional coverage ──────────────────────────────────────────────────

    #[test]
    fn test_cmd_read_json_mode_2() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("data.csv");
        write_csv(&p, "Name,Score\nAlice,95\nBob,87\n");
        assert!(cmd_read(p.to_str().unwrap(), None, None, 0, None, false, false, true).is_ok());
    }

    #[test]
    fn test_cmd_read_stats_mode_2() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("data.csv");
        write_csv(&p, "A,B\n1,2\n3,4\n");
        assert!(cmd_read(p.to_str().unwrap(), None, None, 0, None, false, true, false).is_ok());
    }

    #[test]
    fn test_cmd_filter_lt() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("data.csv");
        write_csv(&p, "A,B\n1,2\n3,4\n5,6\n");
        assert!(cmd_filter(p.to_str().unwrap(), Some("A"), None, Some("4"), None, None, None, false, None, false, false, false).is_ok());
    }

    #[test]
    fn test_cmd_filter_notnull() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("data.csv");
        write_csv(&p, "A\n1\n\n3\n");
        // notnull filter via is_null=false, no other conditions = returns all
        assert!(cmd_filter(p.to_str().unwrap(), None, None, None, None, None, None, false, None, false, false, false).is_ok());
    }

    #[test]
    fn test_cmd_sort_desc() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("data.csv");
        write_csv(&p, "A\n1\n3\n2\n");
        assert!(cmd_sort(p.to_str().unwrap(), Some("A"), None, false, true, None, false, false).is_ok());
    }

    #[test]
    fn test_cmd_agg_count() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("data.csv");
        write_csv(&p, "Name,Score\nAlice,90\nBob,80\n");
        assert!(cmd_agg(p.to_str().unwrap(), None, None, true, None, false, false).is_ok());
    }

    #[test]
    fn test_cmd_agg_avg() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("data.csv");
        write_csv(&p, "A\n10\n20\n30\n");
        assert!(cmd_agg(p.to_str().unwrap(), None, None, false, Some("A"), false, false).is_ok());
    }

    #[test]
    fn test_cmd_search_json() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("data.csv");
        write_csv(&p, "A\nfoo\nbar\nbaz\n");
        assert!(cmd_search(p.to_str().unwrap(), "foo", false, false, false, true, false).is_ok());
    }

    #[test]
    fn test_cmd_info_nonexistent() {
        assert!(cmd_info("no_such_file.xlsx", false, false, false).is_err());
    }

    #[test]
    fn test_cmd_validate_basic() {
        let dir = tempfile::tempdir().unwrap();
        let p1 = dir.path().join("data.csv");
        let p2 = dir.path().join("ref.csv");
        write_csv(&p1, "A,B\n1,2\n");
        write_csv(&p2, "A,B\n1,2\n");
        assert!(cmd_validate(p1.to_str().unwrap(), p2.to_str().unwrap(), false, false).is_ok());
    }

    #[test]
    fn test_cmd_clean_fill() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("data.csv");
        write_csv(&p, "A\n1\n\n3\n");
        assert!(cmd_clean(p.to_str().unwrap(), "fill", None, None, None, false, false).is_ok());
    }

    #[test]
    fn test_cmd_clean_lower() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("data.csv");
        write_csv(&p, "A\nHELLO\nWORLD\n");
        assert!(cmd_clean(p.to_str().unwrap(), "lower", None, None, None, false, false).is_ok());
    }

    #[test]
    fn test_parse_cell_ref_invalid() {
        assert_eq!(parse_cell_ref("123"), None);
        assert_eq!(parse_cell_ref(""), None);
    }

    #[test]
    fn test_sample_indices_clamps_to_total() {
        // If n >= total, return all indices
        let idx = sample_row_indices(5, 100);
        assert_eq!(idx.len(), 5);
    }

    #[test]
    fn test_cmd_replace() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("data.csv");
        write_csv(&p, "Name\nAlice\nBob\n");
        assert!(cmd_replace(p.to_str().unwrap(), "Alice", "Carol", false, false).is_ok());
    }

    #[test]
    fn test_cmd_replace_dry_run_new() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("data.csv");
        write_csv(&p, "Name\nAlice\nBob\n");
        assert!(cmd_replace(p.to_str().unwrap(), "Alice", "Carol", true, false).is_ok());
    }

    #[test]
    fn test_cmd_transpose() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("data.csv");
        write_csv(&p, "A,B\n1,2\n3,4\n");
        assert!(cmd_transpose(p.to_str().unwrap(), None, false, false).is_ok());
    }

    #[test]
    fn test_cmd_sample() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("data.csv");
        write_csv(&p, "Name\nAlice\nBob\nCarol\nDave\n");
        assert!(cmd_sample(p.to_str().unwrap(), Some(2), None, None, false, false).is_ok());
    }

    #[test]
    fn test_cmd_convert() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("data.csv");
        let output = dir.path().join("out.csv");
        write_csv(&input, "A,B\n1,2\n");
        assert!(cmd_convert(input.to_str().unwrap(), output.to_str().unwrap(), None, None, false).is_ok());
    }

    #[test]
    fn test_cmd_diff_same_file() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("data.csv");
        write_csv(&p, "A\n1\n2\n");
        let path = p.to_str().unwrap();
        assert!(cmd_diff(path, path, None, false, None, false, false).is_ok());
    }

    #[test]
    fn test_cmd_concat() {
        let dir = tempfile::tempdir().unwrap();
        let p1 = dir.path().join("d1.csv");
        let p2 = dir.path().join("d2.csv");
        write_csv(&p1, "A\n1\n");
        write_csv(&p2, "A\n2\n");
        let files = vec![p1.to_str().unwrap().to_string(), p2.to_str().unwrap().to_string()];
        assert!(cmd_concat(&files, "rows", false, false).is_ok());
    }

    #[test]
    fn test_cmd_info_existing() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("data.csv");
        write_csv(&p, "A,B\n1,2\n3,4\n");
        assert!(cmd_info(p.to_str().unwrap(), false, false, false).is_ok());
    }

    #[test]
    fn test_cmd_detect_encoding_existing() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("data.csv");
        write_csv(&p, "Name\nAlice\n");
        assert!(cmd_detect_encoding(p.to_str().unwrap(), false).is_ok());
    }

    #[test]
    fn test_cmd_describe_some() {
        assert!(cmd_describe(Some("sum"), false).is_ok());
    }

    #[test]
    fn test_cmd_clean_trim_new() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("data.csv");
        write_csv(&p, "Name\n  Alice  \n Bob \n");
        assert!(cmd_clean(p.to_str().unwrap(), "trim", None, None, None, false, false).is_ok());
    }

    #[test]
    fn test_cmd_filter_basic() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("data.csv");
        write_csv(&p, "Age\n25\n30\n35\n");
        assert!(cmd_filter(p.to_str().unwrap(), Some("Age"), Some("28"), None, None, None, None, false, None, false, false, false).is_ok());
    }

    #[test]
    fn test_cmd_sort_asc_new() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("data.csv");
        write_csv(&p, "Name\nZoe\nAlice\nBob\n");
        assert!(cmd_sort(p.to_str().unwrap(), Some("Name"), None, true, false, None, false, false).is_ok());
    }

    #[test]
    fn test_cmd_agg_sum_new() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("data.csv");
        write_csv(&p, "Amount\n10\n20\n30\n");
        assert!(cmd_agg(p.to_str().unwrap(), None, Some("Amount"), false, None, false, false).is_ok());
    }

    #[test]
    fn test_cmd_search_new() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("data.csv");
        write_csv(&p, "Name\nAlice\nBob\nCarol\n");
        assert!(cmd_search(p.to_str().unwrap(), "Alice", false, false, false, false, false).is_ok());
    }

    #[test]
    fn test_cmd_batch_pattern() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("file.csv");
        write_csv(&p, "A\n1\n");
        let pat = format!("{}/*.csv", dir.path().display());
        assert!(cmd_batch(&pat, &["info".to_string()], false).is_ok());
    }

    #[test]
    fn test_cmd_detect_lang_new() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("data.csv");
        write_csv(&p, "Name\nHello\nWorld\n");
        assert!(cmd_detect_lang(p.to_str().unwrap(), false).is_ok());
    }

    #[test]
    fn test_cmd_sheet_list() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("data.csv");
        write_csv(&p, "A\n1\n");
        assert!(cmd_sheet(SheetCmd::List { file: p.to_str().unwrap().to_string() }, false).is_ok());
    }

    #[test]
    fn test_cmd_sheet_info() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("data.csv");
        write_csv(&p, "A,B\n1,2\n");
        assert!(cmd_sheet(SheetCmd::Info { file: p.to_str().unwrap().to_string(), sheet: "Sheet1".to_string() }, false).is_ok());
    }

    #[test]
    fn test_cmd_calc_evaluate() {
        assert!(cmd_calc(Some("SUM(1,2,3)"), None, None, false).is_ok());
    }

    #[test]
    fn test_cmd_read_basic() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("data.csv");
        write_csv(&p, "A,B\n1,2\n3,4\n");
        assert!(cmd_read(p.to_str().unwrap(), None, None, 0, None, false, false, false).is_ok());
    }

    #[test]
    fn test_cmd_split_by_rows_new() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("data.csv");
        write_csv(&p, "Name\nAlice\nBob\nCarol\nDave\nEve\n");
        let out = dir.path().join("out");
        std::fs::create_dir_all(&out).unwrap();
        assert!(cmd_split(p.to_str().unwrap(), None, Some(2), Some(out.to_str().unwrap()), false).is_ok());
    }

    #[test]
    fn test_cmd_sample_frac() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("data.csv");
        write_csv(&p, "Name\nAlice\nBob\nCarol\nDave\n");
        assert!(cmd_sample(p.to_str().unwrap(), None, Some(0.5), None, false, false).is_ok());
    }

    #[test]
    fn test_cmd_formula_read() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("data.csv");
        write_csv(&p, "A\n1\n2\n");
        assert!(cmd_formula(FormulaCmd::Read {
            file: p.to_str().unwrap().to_string(),
            sheet: None, range: None, evaluate: false, both: false,
        }, false).is_ok());
    }

    #[test]
    fn test_cmd_info_basic() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("data.csv");
        write_csv(&p, "Name,Age\nAlice,30\nBob,25\n");
        assert!(cmd_info(p.to_str().unwrap(), false, false, false).is_ok());
    }

    #[test]
    fn test_cmd_info_json_new() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("data_info.csv");
        write_csv(&p, "Name,Score\nAlice,90\n");
        assert!(cmd_info(p.to_str().unwrap(), false, false, true).is_ok());
    }

    #[test]
    fn test_cmd_info_detailed_new() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("data_detailed.csv");
        write_csv(&p, "Name,Score\nAlice,90\nBob,80\n");
        assert!(cmd_info(p.to_str().unwrap(), true, false, false).is_ok());
    }

    #[test]
    fn test_cmd_read_with_json() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("data.csv");
        write_csv(&p, "A,B\n1,2\n3,4\n");
        assert!(cmd_read(p.to_str().unwrap(), None, None, 0, None, false, false, true).is_ok());
    }

    #[test]
    fn test_cmd_read_with_stats() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("data.csv");
        write_csv(&p, "A,B\n1,2\n3,4\n");
        assert!(cmd_read(p.to_str().unwrap(), None, None, 0, None, false, true, false).is_ok());
    }

    #[test]
    fn test_cmd_read_with_skip() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("data.csv");
        write_csv(&p, "A,B\n1,2\n3,4\n5,6\n");
        assert!(cmd_read(p.to_str().unwrap(), None, None, 1, None, false, false, false).is_ok());
    }

    #[test]
    fn test_cmd_read_with_nrows() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("data.csv");
        write_csv(&p, "A,B\n1,2\n3,4\n5,6\n7,8\n");
        assert!(cmd_read(p.to_str().unwrap(), None, None, 0, Some(2), false, false, false).is_ok());
    }

    #[test]
    fn test_cmd_clean_fill_down() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("data.csv");
        write_csv(&p, "Name\nAlice\n\nBob\n");
        assert!(cmd_clean(p.to_str().unwrap(), "fill-forward", None, None, None, false, false).is_ok());
    }

    #[test]
    fn test_cmd_convert_csv_to_csv_2() {
        let dir = tempfile::tempdir().unwrap();
        let p_in = dir.path().join("in2.csv");
        let p_out = dir.path().join("out2.csv");
        write_csv(&p_in, "A,B\n1,2\n");
        assert!(cmd_convert(p_in.to_str().unwrap(), p_out.to_str().unwrap(), None, None, false).is_ok());
    }

    #[test]
    fn test_cmd_transpose_basic() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("data.csv");
        write_csv(&p, "A,B\n1,2\n3,4\n");
        assert!(cmd_transpose(p.to_str().unwrap(), None, false, false).is_ok());
    }

    #[test]
    fn test_cmd_diff_same_file_2() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("data3.csv");
        write_csv(&p, "A,B\n1,2\n3,4\n");
        let r = cmd_diff(p.to_str().unwrap(), p.to_str().unwrap(), None, false, None, false, false);
        assert!(r.is_ok());
    }

    #[test]
    fn test_cmd_replace_basic() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("data.csv");
        write_csv(&p, "Name\nAlice\nBob\n");
        assert!(cmd_replace(p.to_str().unwrap(), "Alice", "Carol", false, false).is_ok());
    }

    #[test]
    fn test_cmd_describe_all_new() {
        assert!(cmd_describe(None, true).is_ok());
    }

    #[test]
    fn test_cmd_calc_context() {
        assert!(cmd_calc(Some("A1+B1"), Some("{\"A1\":1,\"B1\":2}"), None, false).is_ok());
    }

    #[test]
    fn test_cmd_calc_json_output() {
        assert!(cmd_calc(Some("SUM(1,2)"), None, None, true).is_ok());
    }

    #[test]
    fn test_cmd_search_regex_mode() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("data.csv");
        write_csv(&p, "Name\nAlice\nBob\nCharlie\n");
        assert!(cmd_search(p.to_str().unwrap(), "^A", true, false, false, false, false).is_ok());
    }

    #[test]
    fn test_cmd_filter_null_check() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("data.csv");
        write_csv(&p, "Name,Val\nAlice,1\n,2\nBob,\n");
        assert!(cmd_filter(p.to_str().unwrap(), Some("A"), None, None, None, None, None, true, None, false, false, false).is_ok());
    }

    #[test]
    fn test_cmd_sort_multi_col() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("data.csv");
        write_csv(&p, "Name,Score\nAlice,90\nBob,85\nCarol,90\n");
        assert!(cmd_sort(p.to_str().unwrap(), None, Some("A,B"), true, false, None, false, false).is_ok());
    }

    #[test]
    fn test_cmd_agg_count_json() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("data.csv");
        write_csv(&p, "Cat,Val\nA,10\nA,20\nB,30\n");
        assert!(cmd_agg(p.to_str().unwrap(), Some("A"), None, true, None, true, false).is_ok());
    }

    #[test]
    fn test_cmd_sample_n_new() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("data.csv");
        write_csv(&p, "Name\nAlice\nBob\nCarol\nDave\nEve\n");
        assert!(cmd_sample(p.to_str().unwrap(), Some(3), None, None, false, false).is_ok());
    }

    #[test]
    fn test_cmd_split_by_column() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("data.csv");
        let out = dir.path().join("out2");
        std::fs::create_dir_all(&out).unwrap();
        write_csv(&p, "Cat,Val\nA,1\nB,2\nA,3\n");
        assert!(cmd_split(p.to_str().unwrap(), Some("A"), None, Some(out.to_str().unwrap()), false).is_ok());
    }

    #[test]
    fn test_cmd_detect_encoding_basic() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("data.csv");
        write_csv(&p, "Hello,World\n");
        assert!(cmd_detect_encoding(p.to_str().unwrap(), false).is_ok());
    }
}
