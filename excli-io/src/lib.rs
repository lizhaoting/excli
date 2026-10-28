//! excli-io: Multi-format I/O adapters for reading and writing Excel files.
//!
//! Supports: xlsx, xlsm, xlsb, xls, csv, tsv, ods, and more.

pub mod csv_io;
pub mod excel_io;
pub mod format_detect;
pub mod ods_io;
mod ooxml_comment_read;
mod ooxml_conditional_read;
mod ooxml_connection_read;
mod ooxml_drawing_read;
pub mod ooxml_edit;
mod ooxml_external_link_read;
mod ooxml_filter_read;
pub mod ooxml_formula_edit;
pub mod ooxml_package;
mod ooxml_pivot_read;
pub mod ooxml_read;
mod ooxml_rich_read;
pub mod ooxml_sheet_add;
pub mod ooxml_sheet_delete;
pub mod ooxml_sheet_rename;
mod ooxml_style_read;
mod ooxml_table_read;
mod ooxml_threaded_comment_read;
mod ooxml_validation_read;
pub mod ooxml_value_edit;
pub mod reader;
pub mod writer;

pub use format_detect::{detect_encoding, detect_format};
pub use reader::read_workbook;
pub use writer::write_workbook;
