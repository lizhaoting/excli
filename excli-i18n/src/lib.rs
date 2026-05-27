//! excli-i18n: Multi-language content support, encoding detection, function name mapping.

pub mod encoding;
pub mod function_names;
pub mod collation;
pub mod normalization;
pub mod language_detect;

pub use encoding::{detect_encoding, EncodingDetector, EncodingInfo};
pub use function_names::FunctionNameMapper;
pub use collation::{CollationSorter, CollationLocale};
pub use normalization::{normalize_text, NormalizationForm, width_convert, WidthMode};
pub use language_detect::{detect_language, LanguageInfo};
