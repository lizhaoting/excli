use comfy_table::Table;
use excli_core::Workbook;

/// Output format for displaying data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Table,
    Csv,
    Json,
    Tsv,
}

impl OutputFormat {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "table" => Some(OutputFormat::Table),
            "csv" => Some(OutputFormat::Csv),
            "json" => Some(OutputFormat::Json),
            "tsv" => Some(OutputFormat::Tsv),
            _ => None,
        }
    }
}

/// Format a workbook for output.
pub fn format_output(wb: &Workbook, format: OutputFormat, sheet_name: Option<&str>) -> String {
    match format {
        OutputFormat::Json => {
            serde_json::to_string_pretty(wb).unwrap_or_else(|_| "{}".to_string())
        }
        OutputFormat::Table => format_as_table(wb, sheet_name),
        OutputFormat::Csv => format_as_csv(wb, sheet_name, ','),
        OutputFormat::Tsv => format_as_csv(wb, sheet_name, '\t'),
    }
}

fn format_as_table(wb: &Workbook, sheet_name: Option<&str>) -> String {
    let mut output = String::new();

    for sheet in &wb.sheets {
        if let Some(ref name) = sheet_name {
            if &sheet.name != name {
                continue;
            }
        }

        if wb.sheets.len() > 1 {
            output.push_str(&format!("=== {} ===\n", sheet.name));
        }

        if sheet.rows.is_empty() {
            output.push_str("(empty)\n");
            continue;
        }

        let mut table = Table::new();

        let max_cols = sheet.col_count();
        let mut headers: Vec<String> = vec!["#".to_string()];
        for col in 0..max_cols {
            let col_name = excli_core::reference::column_index_to_name(col as u32);
            headers.push(col_name);
        }
        table.set_header(headers);

        for (row_idx, row) in sheet.rows.iter().enumerate() {
            let mut row_data: Vec<String> = vec![(row_idx + 1).to_string()];
            for col in 0..max_cols {
                let cell_val = row
                    .cells
                    .get(col)
                    .map(|c| c.value.to_string())
                    .unwrap_or_default();
                row_data.push(cell_val);
            }
            let _ = table.add_row(row_data);
        }

        output.push_str(&table.to_string());
        output.push('\n');
    }

    output
}

fn format_as_csv(wb: &Workbook, sheet_name: Option<&str>, delimiter: char) -> String {
    let mut output = String::new();

    for sheet in &wb.sheets {
        if let Some(ref name) = sheet_name {
            if &sheet.name != name {
                continue;
            }
        }

        if wb.sheets.len() > 1 {
            output.push_str(&format!("# Sheet: {}\n", sheet.name));
        }

        for row in &sheet.rows {
            let cells: Vec<String> = row
                .cells
                .iter()
                .map(|c| {
                    let s = c.value.to_string();
                    if s.contains(delimiter) || s.contains('"') || s.contains('\n') {
                        format!("\"{}\"", s.replace('"', "\"\""))
                    } else {
                        s
                    }
                })
                .collect();
            output.push_str(&cells.join(&delimiter.to_string()));
            output.push('\n');
        }
    }

    output
}

/// Format an error for JSON output.
pub fn format_error_json(error: &excli_core::error::ExcliError) -> String {
    serde_json::to_string_pretty(&error.to_json()).unwrap_or_else(|_| {
        format!(r#"{{"status":"error","message":"{}"}}"#, error)
    })
}

/// Print a success message with optional emoji.
pub fn print_success(msg: &str, use_emoji: bool) {
    if use_emoji {
        println!("✅ {}", msg);
    } else {
        println!("[SUCCESS] {}", msg);
    }
}

/// Print an error message.
pub fn print_error(msg: &str, use_emoji: bool) {
    if use_emoji {
        eprintln!("❌ {}", msg);
    } else {
        eprintln!("[ERROR] {}", msg);
    }
}

/// Print a warning message.
pub fn print_warning(msg: &str, use_emoji: bool) {
    if use_emoji {
        println!("⚠️ {}", msg);
    } else {
        println!("[WARNING] {}", msg);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use excli_core::cell::Cell;

    #[test]
    fn test_format_as_table() {
        let mut wb = Workbook::new();
        let sheet = wb.add_sheet("Test");
        sheet.set_cell(0, 0, Cell::string("Name"));
        sheet.set_cell(1, 0, Cell::string("Age"));
        sheet.set_cell(0, 1, Cell::string("Alice"));
        sheet.set_cell(1, 1, Cell::number(30.0));

        let output = format_output(&wb, OutputFormat::Table, None);
        assert!(output.contains("Name"));
        assert!(output.contains("Alice"));
    }

    #[test]
    fn test_format_as_csv() {
        let mut wb = Workbook::new();
        let sheet = wb.add_sheet("Test");
        sheet.set_cell(0, 0, Cell::string("A"));
        sheet.set_cell(1, 0, Cell::string("B"));
        sheet.set_cell(0, 1, Cell::string("1"));
        sheet.set_cell(1, 1, Cell::string("2"));

        let output = format_output(&wb, OutputFormat::Csv, None);
        assert!(output.contains("A,B"));
        assert!(output.contains("1,2"));
    }

    #[test]
    fn test_format_as_json() {
        let mut wb = Workbook::new();
        wb.add_sheet("Sheet1");

        let output = format_output(&wb, OutputFormat::Json, None);
        assert!(output.contains("Sheet1"));
    }

    #[test]
    fn test_print_success() {
        // Just ensure no panic
        print_success("Done", true);
        print_success("Done", false);
    }

    #[test]
    fn test_print_error() {
        print_error("Failed", true);
        print_error("Failed", false);
    }
}
