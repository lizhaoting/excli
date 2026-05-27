use serde::{Deserialize, Serialize};


/// Represents the value stored in a cell.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CellValue {
    Empty,
    Boolean(bool),
    Number(f64),
    SharedString(u32),
    InlineString(String),
    Error(ExcelError),
    RichText(Vec<RichTextRun>),
    /// A 2-D array of values (used internally by the formula engine for range args).
    /// Not persisted to spreadsheet files.
    #[serde(skip)]
    RangeArray(Vec<Vec<CellValue>>),
}

impl CellValue {
    pub fn is_empty(&self) -> bool {
        matches!(self, CellValue::Empty)
    }

    pub fn is_number(&self) -> bool {
        matches!(self, CellValue::Number(_))
    }

    pub fn is_error(&self) -> bool {
        matches!(self, CellValue::Error(_))
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            CellValue::Number(n) => Some(*n),
            CellValue::Boolean(true) => Some(1.0),
            CellValue::Boolean(false) => Some(0.0),
            CellValue::Empty => Some(0.0),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            CellValue::Boolean(b) => Some(*b),
            CellValue::Number(n) => Some(*n != 0.0),
            _ => None,
        }
    }

    pub fn as_string(&self) -> Option<String> {
        match self {
            CellValue::InlineString(s) => Some(s.clone()),
            CellValue::RichText(runs) => {
                Some(runs.iter().map(|r| r.text.clone()).collect::<Vec<_>>().join(""))
            }
            CellValue::Number(n) => Some(n.to_string()),
            CellValue::Boolean(b) => Some(b.to_string()),
            CellValue::Empty => Some(String::new()),
            CellValue::Error(e) => Some(e.to_string()),
            CellValue::SharedString(_) => None, // Needs shared string table
            CellValue::RangeArray(_) => None,
        }
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            CellValue::Empty => "empty",
            CellValue::Boolean(_) => "boolean",
            CellValue::Number(_) => "number",
            CellValue::SharedString(_) | CellValue::InlineString(_) => "string",
            CellValue::Error(_) => "error",
            CellValue::RichText(_) => "richtext",
            CellValue::RangeArray(_) => "range",
        }
    }

    /// Coerce to a numeric value, Excel-style.
    pub fn coerce_to_number(&self) -> f64 {
        match self {
            CellValue::Number(n) => *n,
            CellValue::Boolean(true) => 1.0,
            CellValue::Boolean(false) => 0.0,
            CellValue::InlineString(s) => s.parse::<f64>().unwrap_or(0.0),
            CellValue::Empty => 0.0,
            _ => 0.0,
        }
    }

    /// Coerce to a boolean value, Excel-style.
    pub fn coerce_to_bool(&self) -> bool {
        match self {
            CellValue::Boolean(b) => *b,
            CellValue::Number(n) => *n != 0.0,
            CellValue::InlineString(s) => {
                matches!(s.to_uppercase().as_str(), "TRUE" | "1" | "YES")
            }
            _ => false,
        }
    }
}

impl std::fmt::Display for CellValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CellValue::Empty => write!(f, ""),
            CellValue::Boolean(b) => write!(f, "{}", if *b { "TRUE" } else { "FALSE" }),
            CellValue::Number(n) => write!(f, "{}", n),
            CellValue::InlineString(s) => write!(f, "{}", s),
            CellValue::SharedString(i) => write!(f, "<shared_string[{}]>", i),
            CellValue::Error(e) => write!(f, "{}", e),
            CellValue::RichText(runs) => {
                let s: String = runs.iter().map(|r| r.text.as_str()).collect();
                write!(f, "{}", s)
            }
            CellValue::RangeArray(rows) => {
                let first = rows.first().and_then(|r| r.first()).map(|v| v.to_string()).unwrap_or_default();
                write!(f, "{}", first)
            }
        }
    }
}

impl From<f64> for CellValue {
    fn from(n: f64) -> Self {
        CellValue::Number(n)
    }
}

impl From<bool> for CellValue {
    fn from(b: bool) -> Self {
        CellValue::Boolean(b)
    }
}

impl From<&str> for CellValue {
    fn from(s: &str) -> Self {
        CellValue::InlineString(s.to_string())
    }
}

impl From<String> for CellValue {
    fn from(s: String) -> Self {
        CellValue::InlineString(s)
    }
}

impl From<ExcelError> for CellValue {
    fn from(e: ExcelError) -> Self {
        CellValue::Error(e)
    }
}

/// Excel error values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ExcelError {
    /// #NULL! — Intersection of two areas that don't intersect
    Null,
    /// #DIV/0! — Division by zero
    Div0,
    /// #VALUE! — Wrong type of argument
    Value,
    /// #REF! — Invalid cell reference
    Ref,
    /// #NAME? — Unrecognized formula name
    Name,
    /// #NUM! — Invalid numeric value
    Num,
    /// #N/A — Value not available
    NA,
    /// #GETTING_DATA — Data is being retrieved
    GettingData,
    /// #SPILL! — Dynamic array spill error
    Spill,
    /// #CALC! — Calculation error
    Calc,
}

impl ExcelError {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.trim() {
            "#NULL!" => Some(ExcelError::Null),
            "#DIV/0!" => Some(ExcelError::Div0),
            "#VALUE!" => Some(ExcelError::Value),
            "#REF!" => Some(ExcelError::Ref),
            "#NAME?" => Some(ExcelError::Name),
            "#NUM!" => Some(ExcelError::Num),
            "#N/A" => Some(ExcelError::NA),
            "#GETTING_DATA" => Some(ExcelError::GettingData),
            "#SPILL!" => Some(ExcelError::Spill),
            "#CALC!" => Some(ExcelError::Calc),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            ExcelError::Null => "#NULL!",
            ExcelError::Div0 => "#DIV/0!",
            ExcelError::Value => "#VALUE!",
            ExcelError::Ref => "#REF!",
            ExcelError::Name => "#NAME?",
            ExcelError::Num => "#NUM!",
            ExcelError::NA => "#N/A",
            ExcelError::GettingData => "#GETTING_DATA",
            ExcelError::Spill => "#SPILL!",
            ExcelError::Calc => "#CALC!",
        }
    }
}

impl std::fmt::Display for ExcelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// A run of rich text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RichTextRun {
    pub text: String,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub underline: Option<bool>,
    pub strikethrough: Option<bool>,
    pub font_name: Option<String>,
    pub font_size: Option<f64>,
    pub color: Option<String>,
}

/// A fully specified cell in a worksheet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cell {
    pub value: CellValue,
    pub formula: Option<crate::formula::Formula>,
    pub style: Option<crate::style::CellStyle>,
    pub comment: Option<Comment>,
}

impl Cell {
    pub fn empty() -> Self {
        Cell {
            value: CellValue::Empty,
            formula: None,
            style: None,
            comment: None,
        }
    }

    pub fn new(value: CellValue) -> Self {
        Cell {
            value,
            formula: None,
            style: None,
            comment: None,
        }
    }

    pub fn number(n: f64) -> Self {
        Cell::new(CellValue::Number(n))
    }

    pub fn string(s: impl Into<String>) -> Self {
        Cell::new(CellValue::InlineString(s.into()))
    }

    pub fn boolean(b: bool) -> Self {
        Cell::new(CellValue::Boolean(b))
    }

    pub fn error(e: ExcelError) -> Self {
        Cell::new(CellValue::Error(e))
    }

    pub fn with_formula(mut self, formula: crate::formula::Formula) -> Self {
        self.formula = Some(formula);
        self
    }

    pub fn with_style(mut self, style: crate::style::CellStyle) -> Self {
        self.style = Some(style);
        self
    }

    pub fn is_empty(&self) -> bool {
        self.value.is_empty() && self.formula.is_none()
    }
}

impl Default for Cell {
    fn default() -> Self {
        Cell::empty()
    }
}

/// A cell comment / note.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Comment {
    pub text: String,
    pub author: Option<String>,
    pub visible: bool,
}

impl Comment {
    pub fn new(text: impl Into<String>, author: Option<String>) -> Self {
        Comment {
            text: text.into(),
            author,
            visible: false,
        }
    }
}

/// Compute a simple hash for NaN-safety in comparisons.
#[allow(clippy::non_canonical_partial_ord_impl)]
impl PartialOrd for CellValue {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        match (self, other) {
            (CellValue::Number(a), CellValue::Number(b)) => a.partial_cmp(b),
            (CellValue::InlineString(a), CellValue::InlineString(b)) => Some(a.cmp(b)),
            (CellValue::Boolean(a), CellValue::Boolean(b)) => Some(a.cmp(b)),
            (CellValue::Empty, CellValue::Empty) => Some(std::cmp::Ordering::Equal),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cell_value_numeric_and_bool_coercions() {
        assert_eq!(CellValue::Number(7.5).as_f64(), Some(7.5));
        assert_eq!(CellValue::Boolean(true).as_f64(), Some(1.0));
        assert_eq!(CellValue::Boolean(false).as_f64(), Some(0.0));
        assert_eq!(CellValue::InlineString("x".to_string()).as_f64(), None);
        assert_eq!(CellValue::Number(0.0).as_bool(), Some(false));
        assert_eq!(CellValue::Number(1.0).as_bool(), Some(true));
    }

    #[test]
    fn test_cell_value_string_and_type_name() {
        let rich = CellValue::RichText(vec![
            RichTextRun {
                text: "Hel".to_string(),
                bold: None,
                italic: None,
                underline: None,
                strikethrough: None,
                font_name: None,
                font_size: None,
                color: None,
            },
            RichTextRun {
                text: "lo".to_string(),
                bold: None,
                italic: None,
                underline: None,
                strikethrough: None,
                font_name: None,
                font_size: None,
                color: None,
            },
        ]);
        assert_eq!(rich.as_string(), Some("Hello".to_string()));
        assert_eq!(CellValue::Empty.type_name(), "empty");
        assert_eq!(CellValue::InlineString("a".to_string()).type_name(), "string");
        assert_eq!(CellValue::RangeArray(vec![vec![CellValue::Number(1.0)]]).type_name(), "range");
    }

    #[test]
    fn test_cell_value_excel_style_coercions() {
        assert_eq!(CellValue::InlineString("12.5".to_string()).coerce_to_number(), 12.5);
        assert_eq!(CellValue::InlineString("bad".to_string()).coerce_to_number(), 0.0);
        assert!(CellValue::InlineString("TRUE".to_string()).coerce_to_bool());
        assert!(CellValue::InlineString("yes".to_string()).coerce_to_bool());
        assert!(!CellValue::InlineString("no".to_string()).coerce_to_bool());
    }

    #[test]
    fn test_excel_error_roundtrip() {
        let all = [
            ExcelError::Null,
            ExcelError::Div0,
            ExcelError::Value,
            ExcelError::Ref,
            ExcelError::Name,
            ExcelError::Num,
            ExcelError::NA,
            ExcelError::GettingData,
            ExcelError::Spill,
            ExcelError::Calc,
        ];
        for err in all {
            let s = err.as_str();
            assert_eq!(ExcelError::from_str(s), Some(err));
        }
        assert_eq!(ExcelError::from_str("#NOPE"), None);
    }

    #[test]
    fn test_cell_builders_and_empty_state() {
        let empty = Cell::empty();
        assert!(empty.is_empty());

        let n = Cell::number(42.0);
        assert!(!n.is_empty());
        assert!(n.value.is_number());

        let with_formula = Cell::string("x").with_formula(crate::formula::Formula::new("=1+1"));
        assert!(!with_formula.is_empty());
        assert!(with_formula.formula.is_some());

        let mut c = Cell::default();
        assert!(c.is_empty());
        c.comment = Some(Comment::new("note", Some("user".to_string())));
        assert!(c.comment.is_some());
    }

    #[test]
    fn test_comment_defaults() {
        let c = Comment::new("hello", Some("alice".to_string()));
        assert_eq!(c.text, "hello");
        assert_eq!(c.author.as_deref(), Some("alice"));
        assert!(!c.visible);
    }
}
