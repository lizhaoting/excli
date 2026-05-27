//! excli-io: Multi-format I/O adapters for reading and writing Excel files.
//!
//! Supports: xlsx, xlsm, xlsb, xls, csv, tsv, ods, and more.

pub mod csv_io;
pub mod excel_io;
pub mod format_detect;
pub mod ods_io;
pub mod reader;
pub mod writer;

pub use reader::read_workbook;
pub use writer::write_workbook;
pub use format_detect::{detect_format, detect_encoding};
