use thiserror::Error;

/// Standard error codes for excli operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ErrorCode {
    Success = 0,
    GeneralError = 1,
    FileNotFound = 10,
    FormatNotSupported = 11,
    FileCorrupted = 12,
    FileLocked = 13,
    PasswordRequired = 14,
    PasswordIncorrect = 15,
    SheetNotFound = 20,
    RangeInvalid = 21,
    CellNotFound = 22,
    ColumnNotFound = 23,
    FormulaParseError = 30,
    FormulaCircular = 31,
    FormulaEvalError = 32,
    FunctionNotFound = 33,
    DivByZero = 34,
    ValidationError = 40,
    TypeMismatch = 41,
    WriteError = 50,
    DiskFull = 51,
    LocaleNotSupported = 60,
    EncodingNotSupported = 61,
    NotImplemented = 99,
}

impl ErrorCode {
    pub fn code(self) -> i32 {
        self as i32
    }

    pub fn name(self) -> &'static str {
        match self {
            ErrorCode::Success => "SUCCESS",
            ErrorCode::GeneralError => "GENERAL_ERROR",
            ErrorCode::FileNotFound => "FILE_NOT_FOUND",
            ErrorCode::FormatNotSupported => "FORMAT_NOT_SUPPORTED",
            ErrorCode::FileCorrupted => "FILE_CORRUPTED",
            ErrorCode::FileLocked => "FILE_LOCKED",
            ErrorCode::PasswordRequired => "PASSWORD_REQUIRED",
            ErrorCode::PasswordIncorrect => "PASSWORD_INCORRECT",
            ErrorCode::SheetNotFound => "SHEET_NOT_FOUND",
            ErrorCode::RangeInvalid => "RANGE_INVALID",
            ErrorCode::CellNotFound => "CELL_NOT_FOUND",
            ErrorCode::ColumnNotFound => "COLUMN_NOT_FOUND",
            ErrorCode::FormulaParseError => "FORMULA_PARSE_ERROR",
            ErrorCode::FormulaCircular => "FORMULA_CIRCULAR",
            ErrorCode::FormulaEvalError => "FORMULA_EVAL_ERROR",
            ErrorCode::FunctionNotFound => "FUNCTION_NOT_FOUND",
            ErrorCode::DivByZero => "DIV_BY_ZERO",
            ErrorCode::ValidationError => "VALIDATION_ERROR",
            ErrorCode::TypeMismatch => "TYPE_MISMATCH",
            ErrorCode::WriteError => "WRITE_ERROR",
            ErrorCode::DiskFull => "DISK_FULL",
            ErrorCode::LocaleNotSupported => "LOCALE_NOT_SUPPORTED",
            ErrorCode::EncodingNotSupported => "ENCODING_NOT_SUPPORTED",
            ErrorCode::NotImplemented => "NOT_IMPLEMENTED",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            ErrorCode::Success => "Success",
            ErrorCode::GeneralError => "Unknown error",
            ErrorCode::FileNotFound => "File not found",
            ErrorCode::FormatNotSupported => "Format not supported",
            ErrorCode::FileCorrupted => "File corrupted",
            ErrorCode::FileLocked => "File is locked",
            ErrorCode::PasswordRequired => "Password required",
            ErrorCode::PasswordIncorrect => "Password incorrect",
            ErrorCode::SheetNotFound => "Sheet not found",
            ErrorCode::RangeInvalid => "Cell range invalid",
            ErrorCode::CellNotFound => "Cell not found",
            ErrorCode::ColumnNotFound => "Column name not found",
            ErrorCode::FormulaParseError => "Formula parse error",
            ErrorCode::FormulaCircular => "Circular reference detected",
            ErrorCode::FormulaEvalError => "Formula evaluation error",
            ErrorCode::FunctionNotFound => "Function not found",
            ErrorCode::DivByZero => "Division by zero",
            ErrorCode::ValidationError => "Data validation failed",
            ErrorCode::TypeMismatch => "Type mismatch",
            ErrorCode::WriteError => "Write failed",
            ErrorCode::DiskFull => "Disk full",
            ErrorCode::LocaleNotSupported => "Unsupported locale",
            ErrorCode::EncodingNotSupported => "Unsupported encoding",
            ErrorCode::NotImplemented => "Feature not yet implemented",
        }
    }
}

impl std::fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code(), self.name())
    }
}

/// Main error type for excli.
#[derive(Debug, Error)]
pub enum ExcliError {
    #[error("{code}: {message}")]
    General {
        code: ErrorCode,
        message: String,
        suggestions: Vec<String>,
    },

    #[error("{code}: {message}")]
    Simple {
        code: ErrorCode,
        message: String,
    },

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error("Zip error: {0}")]
    Zip(String),

    #[error("Parse error: {0}")]
    Parse(String),

    #[error("Serialization error: {0}")]
    Serialize(String),
}

impl ExcliError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        ExcliError::Simple {
            code,
            message: message.into(),
        }
    }

    pub fn with_suggestions(
        code: ErrorCode,
        message: impl Into<String>,
        suggestions: Vec<String>,
    ) -> Self {
        ExcliError::General {
            code,
            message: message.into(),
            suggestions,
        }
    }

    pub fn code(&self) -> ErrorCode {
        match self {
            ExcliError::General { code, .. } | ExcliError::Simple { code, .. } => *code,
            ExcliError::Io(_) => ErrorCode::GeneralError,
            ExcliError::Zip(_) => ErrorCode::GeneralError,
            ExcliError::Parse(_) => ErrorCode::FormulaParseError,
            ExcliError::Serialize(_) => ErrorCode::GeneralError,
        }
    }

    pub fn to_json(&self) -> serde_json::Value {
        match self {
            ExcliError::General {
                code,
                message,
                suggestions,
            } => {
                serde_json::json!({
                    "status": "error",
                    "error_code": code.name(),
                    "message": message,
                    "suggestions": suggestions
                })
            }
            ExcliError::Simple { code, message } => {
                serde_json::json!({
                    "status": "error",
                    "error_code": code.name(),
                    "message": message
                })
            }
            other => {
                serde_json::json!({
                    "status": "error",
                    "error_code": other.code().name(),
                    "message": other.to_string()
                })
            }
        }
    }

    pub fn file_not_found(path: impl Into<String>) -> Self {
        ExcliError::with_suggestions(
            ErrorCode::FileNotFound,
            format!("File not found: {}", path.into()),
            vec![
                "Check if the file path is correct".to_string(),
                "Use excli info to see supported file formats".to_string(),
            ],
        )
    }

    pub fn sheet_not_found(name: impl Into<String>) -> Self {
        ExcliError::new(ErrorCode::SheetNotFound, format!("Sheet not found: {}", name.into()))
    }

    pub fn range_invalid(range: impl Into<String>) -> Self {
        ExcliError::new(ErrorCode::RangeInvalid, format!("Invalid range: {}", range.into()))
    }

    pub fn not_implemented(feature: impl Into<String>) -> Self {
        ExcliError::new(
            ErrorCode::NotImplemented,
            format!("Feature not yet implemented: {}", feature.into()),
        )
    }

    pub fn div_by_zero() -> Self {
        ExcliError::new(
            ErrorCode::DivByZero,
            "Division by zero",
        )
    }

    pub fn formula_parse_error(msg: impl Into<String>) -> Self {
        ExcliError::new(ErrorCode::FormulaParseError, msg)
    }

    pub fn formula_eval_error(msg: impl Into<String>) -> Self {
        ExcliError::new(ErrorCode::FormulaEvalError, msg)
    }

    pub fn circular_reference() -> Self {
        ExcliError::new(
            ErrorCode::FormulaCircular,
            "Circular reference detected",
        )
    }

    pub fn format_not_supported(format: impl Into<String>) -> Self {
        ExcliError::new(
            ErrorCode::FormatNotSupported,
            format!("Format not supported: {}", format.into()),
        )
    }
}

pub type ExcliResult<T> = Result<T, ExcliError>;
