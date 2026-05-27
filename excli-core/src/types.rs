/// Status emoji definitions for CLI feedback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusEmoji {
    Start,
    Reading,
    Writing,
    Calculating,
    Parsing,
    Processing,
    Converting,
    Searching,
    Cleaning,
    Comparing,
    Validating,
    Merging,
    Splitting,
    Compressing,
    Success,
    Failure,
    Warning,
    Skip,
    Interrupt,
    Waiting,
    Complete,
    Tip,
    Info,
}

impl StatusEmoji {
    pub fn as_str(self) -> &'static str {
        match self {
            StatusEmoji::Start => "🚀",
            StatusEmoji::Reading => "📖",
            StatusEmoji::Writing => "✍️",
            StatusEmoji::Calculating => "🧮",
            StatusEmoji::Parsing => "🔍",
            StatusEmoji::Processing => "⚙️",
            StatusEmoji::Converting => "🔄",
            StatusEmoji::Searching => "🔎",
            StatusEmoji::Cleaning => "🧹",
            StatusEmoji::Comparing => "⚖️",
            StatusEmoji::Validating => "✅",
            StatusEmoji::Merging => "🔗",
            StatusEmoji::Splitting => "✂️",
            StatusEmoji::Compressing => "📦",
            StatusEmoji::Success => "✅",
            StatusEmoji::Failure => "❌",
            StatusEmoji::Warning => "⚠️",
            StatusEmoji::Skip => "⏭️",
            StatusEmoji::Interrupt => "🛑",
            StatusEmoji::Waiting => "⏳",
            StatusEmoji::Complete => "🎉",
            StatusEmoji::Tip => "💡",
            StatusEmoji::Info => "ℹ️",
        }
    }
}

impl std::fmt::Display for StatusEmoji {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Output mode for CLI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputMode {
    /// Human-readable output (default)
    Human,
    /// JSON structured output
    Json,
    /// Quiet mode — data only
    Quiet,
}

/// Supported date system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DateSystem {
    /// 1900 date system (Windows default, with Lotus 1-2-3 bug)
    Date1900,
    /// 1904 date system (Mac default)
    Date1904,
}

impl Default for DateSystem {
    fn default() -> Self {
        DateSystem::Date1900
    }
}

/// Document metadata / properties.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct DocumentProperties {
    pub title: Option<String>,
    pub subject: Option<String>,
    pub author: Option<String>,
    pub creator: Option<String>,
    pub keywords: Option<String>,
    pub description: Option<String>,
    pub category: Option<String>,
    pub created: Option<chrono::NaiveDateTime>,
    pub modified: Option<chrono::NaiveDateTime>,
    pub last_printed: Option<chrono::NaiveDateTime>,
    pub revision: Option<String>,
    pub application: Option<String>,
    pub company: Option<String>,
    pub manager: Option<String>,
    pub custom: std::collections::HashMap<String, String>,
}

/// Statistics about a workbook or sheet.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Stats {
    pub row_count: usize,
    pub col_count: usize,
    pub formula_count: usize,
    pub style_count: usize,
    pub sheet_count: usize,
    pub file_size_bytes: u64,
    pub null_counts: std::collections::HashMap<String, usize>,
    pub column_types: std::collections::HashMap<String, String>,
}

/// Schema information for AI Agent consumption.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Schema {
    pub sheet_name: String,
    pub columns: Vec<ColumnSchema>,
    pub row_count: usize,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ColumnSchema {
    pub name: String,
    pub dtype: String,
    pub nullable: bool,
    pub sample_values: Vec<String>,
    pub stats: ColumnStats,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ColumnStats {
    pub min: Option<String>,
    pub max: Option<String>,
    pub mean: Option<f64>,
    pub null_count: usize,
    pub distinct_count: usize,
}

/// JSON response wrapper for AI Agent consumption.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct JsonResponse<T: serde::Serialize> {
    pub status: String,
    pub command: String,
    pub elapsed_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonError>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct JsonError {
    pub error_code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub suggestions: Vec<String>,
}

impl<T: serde::Serialize> JsonResponse<T> {
    pub fn ok(command: &str, elapsed_ms: u64, data: T) -> Self {
        JsonResponse {
            status: "ok".to_string(),
            command: command.to_string(),
            elapsed_ms,
            data: Some(data),
            error: None,
        }
    }

    pub fn err(command: &str, elapsed_ms: u64, error: JsonError) -> Self {
        JsonResponse {
            status: "error".to_string(),
            command: command.to_string(),
            elapsed_ms,
            data: None,
            error: Some(error),
        }
    }
}

/// Supported file formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum FileFormat {
    Xlsx,
    Xlsm,
    Xlsb,
    Xls,
    Xltx,
    Xltm,
    Xlam,
    Csv,
    Tsv,
    Ods,
    Xml,
    Slk,
    Dif,
    Prn,
    Json,
    Parquet,
    Arrow,
    Html,
    Pdf,
}

impl FileFormat {
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_lowercase().as_str() {
            "xlsx" => Some(FileFormat::Xlsx),
            "xlsm" => Some(FileFormat::Xlsm),
            "xlsb" => Some(FileFormat::Xlsb),
            "xls" => Some(FileFormat::Xls),
            "xltx" => Some(FileFormat::Xltx),
            "xltm" => Some(FileFormat::Xltm),
            "xlam" => Some(FileFormat::Xlam),
            "csv" => Some(FileFormat::Csv),
            "tsv" | "tab" => Some(FileFormat::Tsv),
            "ods" => Some(FileFormat::Ods),
            "xml" => Some(FileFormat::Xml),
            "slk" => Some(FileFormat::Slk),
            "dif" => Some(FileFormat::Dif),
            "prn" | "txt" => Some(FileFormat::Prn),
            "json" => Some(FileFormat::Json),
            "parquet" => Some(FileFormat::Parquet),
            "arrow" | "feather" => Some(FileFormat::Arrow),
            "html" => Some(FileFormat::Html),
            "pdf" => Some(FileFormat::Pdf),
            _ => None,
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            FileFormat::Xlsx => "xlsx",
            FileFormat::Xlsm => "xlsm",
            FileFormat::Xlsb => "xlsb",
            FileFormat::Xls => "xls",
            FileFormat::Xltx => "xltx",
            FileFormat::Xltm => "xltm",
            FileFormat::Xlam => "xlam",
            FileFormat::Csv => "csv",
            FileFormat::Tsv => "tsv",
            FileFormat::Ods => "ods",
            FileFormat::Xml => "xml",
            FileFormat::Slk => "slk",
            FileFormat::Dif => "dif",
            FileFormat::Prn => "prn",
            FileFormat::Json => "json",
            FileFormat::Parquet => "parquet",
            FileFormat::Arrow => "arrow",
            FileFormat::Html => "html",
            FileFormat::Pdf => "pdf",
        }
    }

    pub fn is_xml_based(self) -> bool {
        matches!(
            self,
            FileFormat::Xlsx
                | FileFormat::Xlsm
                | FileFormat::Xltx
                | FileFormat::Xltm
                | FileFormat::Xlam
        )
    }

    pub fn supports_formulas(self) -> bool {
        matches!(
            self,
            FileFormat::Xlsx | FileFormat::Xlsm | FileFormat::Xlsb | FileFormat::Xls | FileFormat::Ods
        )
    }

    pub fn supports_styles(self) -> bool {
        matches!(
            self,
            FileFormat::Xlsx | FileFormat::Xlsm | FileFormat::Xlsb | FileFormat::Xls | FileFormat::Ods
        )
    }
}

impl std::fmt::Display for FileFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.extension().to_uppercase())
    }
}
