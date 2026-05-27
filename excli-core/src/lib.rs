//! excli-core: Core data structures and traits for the excli Excel CLI tool.
//!
//! This crate defines the fundamental types used across all other excli crates:
//! - Workbook, Sheet, Cell, CellValue
//! - Formula AST nodes
//! - Cell references and ranges
//! - Error types and error codes

pub mod error;
pub mod types;
pub mod cell;
pub mod formula;
pub mod reference;
pub mod range;
pub mod style;
pub mod sheet;
pub mod workbook;
pub mod utils;

pub use error::{ExcliError, ExcliResult, ErrorCode};
pub use types::*;
pub use cell::{Cell, CellValue};
pub use formula::{Formula, FormulaAst, AstNode, BinOp, UnaryOp, FormulaType, Constant};
pub use reference::{CellReference, RangeReference, Reference};
pub use range::CellRange;
pub use sheet::{Sheet, SheetState, Column, Row};
pub use workbook::Workbook;
