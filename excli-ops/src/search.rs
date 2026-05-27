#[allow(unused_imports)]
use excli_core::cell::{Cell, CellValue};
use excli_core::Sheet;

/// Search options.
#[derive(Debug, Clone)]
pub struct SearchOptions {
    pub pattern: String,
    pub case_sensitive: bool,
    pub whole_word: bool,
    pub regex: bool,
    pub search_formulas: bool,
    pub search_values: bool,
}

impl SearchOptions {
    pub fn new(pattern: impl Into<String>) -> Self {
        SearchOptions {
            pattern: pattern.into(),
            case_sensitive: false,
            whole_word: false,
            regex: false,
            search_formulas: false,
            search_values: true,
        }
    }
}

/// A search result.
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub sheet: String,
    pub column: u32,
    pub row: u32,
    pub cell_ref: String,
    pub value: String,
    pub matched_text: String,
}

impl SearchResult {
    pub fn display_location(&self) -> String {
        format!("{}!{}", self.sheet, self.cell_ref)
    }
}

/// Search for a pattern in a sheet.
pub fn search_cells(sheet: &Sheet, options: &SearchOptions) -> Vec<SearchResult> {
    let mut results = Vec::new();
    let re = if options.regex {
        regex::Regex::new(&options.pattern).ok()
    } else {
        None
    };

    for (row_idx, row) in sheet.rows.iter().enumerate() {
        for (col_idx, cell) in row.cells.iter().enumerate() {
            let text = if options.search_formulas {
                cell.formula
                    .as_ref()
                    .map(|f| f.raw.clone())
                    .unwrap_or_else(|| cell.value.to_string())
            } else {
                cell.value.to_string()
            };

            let matched = if let Some(ref re) = re {
                re.find(&text).map(|m| m.as_str().to_string())
            } else if options.whole_word {
                if contains_whole_word(&text, &options.pattern, options.case_sensitive) {
                    Some(options.pattern.clone())
                } else {
                    None
                }
            } else {
                let text_cmp = if options.case_sensitive {
                    text.clone()
                } else {
                    text.to_lowercase()
                };
                let pat_cmp = if options.case_sensitive {
                    options.pattern.clone()
                } else {
                    options.pattern.to_lowercase()
                };
                if text_cmp.contains(&pat_cmp) {
                    Some(options.pattern.clone())
                } else {
                    None
                }
            };

            if let Some(matched_text) = matched {
                results.push(SearchResult {
                    sheet: sheet.name.clone(),
                    column: col_idx as u32,
                    row: row_idx as u32,
                    cell_ref: excli_core::reference::CellReference::new(col_idx as u32, row_idx as u32).to_a1(),
                    value: cell.value.to_string(),
                    matched_text,
                });
            }
        }
    }

    results
}

fn contains_whole_word(text: &str, pattern: &str, case_sensitive: bool) -> bool {
    let text = if case_sensitive { text.to_string() } else { text.to_lowercase() };
    let pattern = if case_sensitive { pattern.to_string() } else { pattern.to_lowercase() };

    // Simple whole word matching
    text.split(|c: char| !c.is_alphanumeric())
        .any(|word| word == pattern)
}

/// Replace all occurrences of a pattern in a sheet.
pub fn replace_cells(
    sheet: &Sheet,
    find: &str,
    replace: &str,
    search_formulas: bool,
    dry_run: bool,
) -> (Sheet, usize) {
    let mut result = if dry_run { sheet.clone() } else { sheet.clone() };
    let mut count = 0;

    if dry_run {
        // Just count matches
        let search_opts = SearchOptions::new(find);
        let results = search_cells(sheet, &search_opts);
        return (sheet.clone(), results.len());
    }

    for row in &mut result.rows {
        for cell in &mut row.cells {
            if search_formulas {
                if let Some(ref formula) = cell.formula {
                    let new_raw = formula.raw.replace(find, replace);
                    if new_raw != formula.raw {
                        count += 1;
                    }
                }
            } else if let CellValue::InlineString(s) = &cell.value {
                let new_s = s.replace(find, replace);
                if &new_s != s {
                    cell.value = CellValue::InlineString(new_s);
                    count += 1;
                }
            }
        }
    }

    (result, count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_basic() {
        let mut sheet = Sheet::new("Test");
        sheet.set_cell(0, 0, Cell::string("Hello World"));
        sheet.set_cell(1, 0, Cell::string("Foo Bar"));
        sheet.set_cell(0, 1, Cell::string("hello again"));

        let opts = SearchOptions {
            pattern: "Hello".to_string(),
            case_sensitive: true,
            ..SearchOptions::new("Hello")
        };
        let results = search_cells(&sheet, &opts);
        assert_eq!(results.len(), 1); // Only exact case match
    }

    #[test]
    fn test_search_case_insensitive() {
        let mut sheet = Sheet::new("Test");
        sheet.set_cell(0, 0, Cell::string("Hello"));
        sheet.set_cell(0, 1, Cell::string("hello"));

        let opts = SearchOptions::new("hello");
        let results = search_cells(&sheet, &opts);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_search_regex() {
        let mut sheet = Sheet::new("Test");
        sheet.set_cell(0, 0, Cell::string("abc123xyz"));
        sheet.set_cell(0, 1, Cell::string("abc"));

        let opts = SearchOptions {
            pattern: r"\d+".to_string(),
            regex: true,
            ..SearchOptions::new("")
        };
        let results = search_cells(&sheet, &opts);
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_replace() {
        let mut sheet = Sheet::new("Test");
        sheet.set_cell(0, 0, Cell::string("Hello World"));
        sheet.set_cell(0, 1, Cell::string("World"));

        let (result, count) = replace_cells(&sheet, "World", "Earth", false, false);
        assert_eq!(count, 2);
        assert_eq!(
            result.get_cell(0, 0).unwrap().value.as_string().unwrap(),
            "Hello Earth"
        );
    }

    #[test]
    fn test_replace_dry_run() {
        let mut sheet = Sheet::new("Test");
        sheet.set_cell(0, 0, Cell::string("Hello"));

        let (result, count) = replace_cells(&sheet, "Hello", "Hi", false, true);
        assert_eq!(count, 1);
        // Original should be unchanged
        assert_eq!(
            result.get_cell(0, 0).unwrap().value.as_string().unwrap(),
            "Hello"
        );
    }

    #[test]
    fn test_whole_word_search() {
        let mut sheet = Sheet::new("Test");
        sheet.set_cell(0, 0, Cell::string("cat"));
        sheet.set_cell(1, 0, Cell::string("caterpillar"));
        sheet.set_cell(2, 0, Cell::string("concatenate"));

        let opts = SearchOptions {
            pattern: "cat".to_string(),
            whole_word: true,
            ..SearchOptions::new("cat")
        };
        let results = search_cells(&sheet, &opts);
        // Only exact word "cat" matches, not "caterpillar" or "concatenate"
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_search_case_sensitive() {
        let mut sheet = Sheet::new("Test");
        sheet.set_cell(0, 0, Cell::string("Hello"));
        sheet.set_cell(1, 0, Cell::string("hello"));

        let opts = SearchOptions {
            pattern: "Hello".to_string(),
            case_sensitive: true,
            ..SearchOptions::new("Hello")
        };
        let results = search_cells(&sheet, &opts);
        // Only uppercase Hello
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_replace_multiple() {
        let mut sheet = Sheet::new("Test");
        sheet.set_cell(0, 0, Cell::string("foo"));
        sheet.set_cell(0, 1, Cell::string("foo"));
        sheet.set_cell(0, 2, Cell::string("bar"));

        let (result, count) = replace_cells(&sheet, "foo", "baz", false, false);
        assert_eq!(count, 2);
        assert_eq!(result.get_cell(0, 0).unwrap().value.as_string().unwrap(), "baz");
        assert_eq!(result.get_cell(0, 2).unwrap().value.as_string().unwrap(), "bar");
    }

    #[test]
    fn test_search_numeric_cell() {
        let mut sheet = Sheet::new("Test");
        sheet.set_cell(0, 0, Cell::number(42.0));

        let opts = SearchOptions::new("42");
        let results = search_cells(&sheet, &opts);
        assert_eq!(results.len(), 1);
    }
}
