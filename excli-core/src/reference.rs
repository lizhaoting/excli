use serde::{Deserialize, Serialize};

/// A single cell reference like A1, $B$2, Sheet1!C3.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CellReference {
    pub column: u32,    // 0-based
    pub row: u32,       // 0-based
    pub col_absolute: bool,
    pub row_absolute: bool,
    pub sheet: Option<String>,
}

impl CellReference {
    pub fn new(column: u32, row: u32) -> Self {
        CellReference {
            column,
            row,
            col_absolute: false,
            row_absolute: false,
            sheet: None,
        }
    }

    pub fn from_a1(s: &str) -> Option<Self> {
        let s = s.trim();

        // Handle sheet reference: Sheet1!A1
        let (sheet, cell_part) = if let Some(excl_pos) = s.rfind('!') {
            let sheet_name = &s[..excl_pos];
            let sheet_name = sheet_name.trim().trim_matches('\'').to_string();
            (Some(sheet_name), &s[excl_pos + 1..])
        } else {
            (None, s)
        };

        let cell_part = cell_part.trim();
        let mut col_absolute = false;
        let mut row_absolute = false;
        let mut remaining = cell_part;

        if remaining.starts_with('$') {
            col_absolute = true;
            remaining = &remaining[1..];
        }

        // Parse column letters
        let col_end = remaining
            .find(|c: char| !c.is_ascii_alphabetic())
            .unwrap_or(remaining.len());
        let col_str = &remaining[..col_end];
        remaining = &remaining[col_end..];

        if col_str.is_empty() {
            return None;
        }

        if remaining.starts_with('$') {
            row_absolute = true;
            remaining = &remaining[1..];
        }

        let row_str = remaining;
        if row_str.is_empty() {
            return None;
        }

        let column = column_name_to_index(col_str)?;
        let row: u32 = row_str.parse().ok()?;
        if row < 1 {
            return None;
        }

        Some(CellReference {
            column,
            row: row - 1,
            col_absolute,
            row_absolute,
            sheet,
        })
    }

    pub fn to_a1(&self) -> String {
        let col = column_index_to_name(self.column);
        let row = self.row + 1;
        let abs_col = if self.col_absolute { "$" } else { "" };
        let abs_row = if self.row_absolute { "$" } else { "" };
        if let Some(ref sheet) = self.sheet {
            if sheet.contains(' ') || sheet.contains('!') {
                format!("'{}'!{}{}{}{}", sheet, abs_col, col, abs_row, row)
            } else {
                format!("{}!{}{}{}{}", sheet, abs_col, col, abs_row, row)
            }
        } else {
            format!("{}{}{}{}", abs_col, col, abs_row, row)
        }
    }

    pub fn offset(&self, row_delta: i32, col_delta: i32) -> Self {
        CellReference {
            column: (self.column as i32 + col_delta) as u32,
            row: (self.row as i32 + row_delta) as u32,
            col_absolute: self.col_absolute,
            row_absolute: self.row_absolute,
            sheet: self.sheet.clone(),
        }
    }
}

impl std::fmt::Display for CellReference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_a1())
    }
}

/// Convert column name to 0-based index. A→0, Z→25, AA→26, ZZ→701, AAA→702, XFD→16383.
pub fn column_name_to_index(name: &str) -> Option<u32> {
    let name = name.to_uppercase();
    let mut result: u32 = 0;
    for ch in name.chars() {
        if !ch.is_ascii_uppercase() {
            return None;
        }
        result = result.checked_mul(26)?;
        result = result.checked_add((ch as u32) - ('A' as u32) + 1)?;
    }
    // 1-based → 0-based
    result = result.checked_sub(1)?;
    Some(result)
}

/// Convert 0-based column index to name. 0→A, 25→Z, 26→AA, 701→ZZ.
pub fn column_index_to_name(mut index: u32) -> String {
    let mut result = String::new();
    loop {
        let remainder = index % 26;
        result.push((b'A' + remainder as u8) as char);
        if index < 26 {
            break;
        }
        index = (index / 26) - 1;
    }
    result.chars().rev().collect()
}

/// A 2D cell range like A1:B10.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RangeReference {
    pub start: CellReference,
    pub end: CellReference,
}

impl RangeReference {
    pub fn new(start: CellReference, end: CellReference) -> Self {
        RangeReference { start, end }
    }

    pub fn from_a1(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s.split(':').collect();
        if parts.len() != 2 {
            return None;
        }
        let start = CellReference::from_a1(parts[0])?;
        let end = CellReference::from_a1(parts[1])?;
        Some(RangeReference { start, end })
    }

    pub fn to_a1(&self) -> String {
        format!("{}:{}", self.start.to_a1(), self.end.to_a1())
    }

    pub fn row_count(&self) -> u32 {
        if self.end.row >= self.start.row {
            self.end.row - self.start.row + 1
        } else {
            self.start.row - self.end.row + 1
        }
    }

    pub fn col_count(&self) -> u32 {
        if self.end.column >= self.start.column {
            self.end.column - self.start.column + 1
        } else {
            self.start.column - self.end.column + 1
        }
    }

    pub fn iter_cells(&self) -> RangeCellIterator {
        RangeCellIterator {
            range: self.clone(),
            current_col: self.start.column,
            current_row: self.start.row,
        }
    }
}

impl std::fmt::Display for RangeReference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_a1())
    }
}

pub struct RangeCellIterator {
    range: RangeReference,
    current_col: u32,
    current_row: u32,
}

impl Iterator for RangeCellIterator {
    type Item = CellReference;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current_row > self.range.end.row {
            return None;
        }
        let cell = CellReference::new(self.current_col, self.current_row);
        self.current_col += 1;
        if self.current_col > self.range.end.column {
            self.current_col = self.range.start.column;
            self.current_row += 1;
        }
        Some(cell)
    }
}

/// A reference in a formula: either a single cell, a range, or a named range.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Reference {
    Cell(CellReference),
    Range(RangeReference),
    Named(String),
    Structured(String, String), // TableName[ColumnName]
}

impl std::fmt::Display for Reference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Reference::Cell(c) => write!(f, "{}", c),
            Reference::Range(r) => write!(f, "{}", r),
            Reference::Named(n) => write!(f, "{}", n),
            Reference::Structured(t, c) => write!(f, "{}[{}]", t, c),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case("A1", Some((0, 0)))]
    #[case("Z1", Some((25, 0)))]
    #[case("AA1", Some((26, 0)))]
    #[case("ZZ1", Some((701, 0)))]
    #[case("AAA1", Some((702, 0)))]
    #[case("XFD1", Some((16383, 0)))]
    #[case("A1048576", Some((0, 1048575)))]
    fn test_cell_reference_parse(#[case] input: &str, #[case] expected: Option<(u32, u32)>) {
        let result = CellReference::from_a1(input);
        assert_eq!(result.is_some(), expected.is_some());
        if let (Some(cell), Some((col, row))) = (result, expected) {
            assert_eq!(cell.column, col);
            assert_eq!(cell.row, row);
        }
    }

    #[rstest]
    #[case("A1", "A1")]
    #[case("Z1", "Z1")]
    #[case("AA1", "AA1")]
    #[case("ZZ1", "ZZ1")]
    #[case("$B$2", "$B$2")]
    #[case("Sheet1!A1", "Sheet1!A1")]
    fn test_cell_reference_roundtrip(#[case] input: &str, #[case] expected: &str) {
        let cell = CellReference::from_a1(input).unwrap();
        assert_eq!(cell.to_a1(), expected);
    }

    #[rstest]
    #[case(0, "A")]
    #[case(25, "Z")]
    #[case(26, "AA")]
    #[case(701, "ZZ")]
    #[case(702, "AAA")]
    #[case(16383, "XFD")]
    fn test_column_index_to_name(#[case] input: u32, #[case] expected: &str) {
        assert_eq!(column_index_to_name(input), expected);
    }

    #[rstest]
    #[case("A", Some(0))]
    #[case("Z", Some(25))]
    #[case("AA", Some(26))]
    #[case("ZZ", Some(701))]
    #[case("AAA", Some(702))]
    #[case("XFD", Some(16383))]
    fn test_column_name_to_index(#[case] input: &str, #[case] expected: Option<u32>) {
        assert_eq!(column_name_to_index(input), expected);
    }

    #[test]
    fn test_range_reference() {
        let range = RangeReference::from_a1("A1:B10").unwrap();
        assert_eq!(range.start, CellReference::new(0, 0));
        assert_eq!(range.end, CellReference::new(1, 9));
        assert_eq!(range.row_count(), 10);
        assert_eq!(range.col_count(), 2);
        assert_eq!(range.iter_cells().count(), 20);
    }

    #[test]
    fn test_cell_offset() {
        let cell = CellReference::new(0, 0);
        let offset = cell.offset(1, 1);
        assert_eq!(offset.column, 1);
        assert_eq!(offset.row, 1);
    }

    proptest::proptest! {
        #[test]
        fn column_name_roundtrip(col in 0u32..16384) {
            let name = column_index_to_name(col);
            let idx = column_name_to_index(&name);
            proptest::prop_assert_eq!(col, idx.unwrap());
        }
    }

    #[test]
    fn test_column_index_to_name_basics() {
        assert_eq!(column_index_to_name(0), "A");
        assert_eq!(column_index_to_name(25), "Z");
        assert_eq!(column_index_to_name(26), "AA");
        assert_eq!(column_index_to_name(51), "AZ");
        assert_eq!(column_index_to_name(52), "BA");
    }

    #[test]
    fn test_column_name_to_index_basics() {
        assert_eq!(column_name_to_index("A"), Some(0));
        assert_eq!(column_name_to_index("Z"), Some(25));
        assert_eq!(column_name_to_index("AA"), Some(26));
    }

    #[test]
    fn test_cell_reference_to_a1() {
        let cell = CellReference::new(0, 0);
        assert_eq!(cell.to_a1(), "A1");
        let cell2 = CellReference::new(25, 9);
        assert_eq!(cell2.to_a1(), "Z10");
    }

    #[test]
    fn test_range_reference_with_sheet() {
        let range = RangeReference::from_a1("Sheet2!C3:D4").unwrap();
        assert_eq!(range.start.sheet.as_deref(), Some("Sheet2"));
        assert_eq!(range.row_count(), 2);
        assert_eq!(range.col_count(), 2);
    }

    #[test]
    fn test_cell_reference_with_absolute() {
        let cell = CellReference::from_a1("$B$2").unwrap();
        assert_eq!(cell.column, 1);
        assert_eq!(cell.row, 1);
    }
}
