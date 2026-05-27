//! excli-formula: Formula parsing, AST construction, and evaluation engine.
//!
//! Implements a Pratt parser for Excel formula syntax and a tree-walk
//! interpreter for evaluating formulas with full Excel semantics.

pub mod lexer;
pub mod parser;
pub mod evaluator;
pub mod functions;
pub mod dependency;
pub mod token;

pub use lexer::Lexer;
pub use parser::Parser;
pub use evaluator::{Evaluator, EvalContext};
pub use dependency::DependencyGraph;
