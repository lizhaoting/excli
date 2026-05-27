use serde::{Deserialize, Serialize};

use crate::cell::CellValue;
use crate::reference::Reference;

/// Formula stored in a cell.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Formula {
    pub raw: String,
    pub ast: Option<FormulaAst>,
    pub cached_value: Option<CellValue>,
    pub formula_type: FormulaType,
}

impl Formula {
    pub fn new(raw: impl Into<String>) -> Self {
        Formula {
            raw: raw.into(),
            ast: None,
            cached_value: None,
            formula_type: FormulaType::Normal,
        }
    }

    pub fn with_ast(mut self, ast: FormulaAst) -> Self {
        self.ast = Some(ast);
        self
    }

    pub fn with_cached(mut self, value: CellValue) -> Self {
        self.cached_value = Some(value);
        self
    }
}

/// Type of formula.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FormulaType {
    Normal,
    Array,
    Shared,
    DataTable,
}

/// A parsed formula AST.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FormulaAst {
    pub root: Box<AstNode>,
    pub references: Vec<Reference>,
    pub functions: Vec<String>,
}

impl FormulaAst {
    pub fn new(root: AstNode) -> Self {
        let mut references = Vec::new();
        let mut functions = Vec::new();
        root.collect_refs_and_funcs(&mut references, &mut functions);
        FormulaAst {
            root: Box::new(root),
            references,
            functions,
        }
    }
}

/// AST node types for formula expressions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AstNode {
    BinaryOp {
        op: BinOp,
        left: Box<AstNode>,
        right: Box<AstNode>,
    },
    UnaryOp {
        op: UnaryOp,
        arg: Box<AstNode>,
    },
    Function {
        name: String,
        args: Vec<AstNode>,
    },
    Reference(Reference),
    Constant(Constant),
    Array(Vec<Vec<AstNode>>),
    Error(crate::cell::ExcelError),
    Paren(Box<AstNode>),
}

impl AstNode {
    fn collect_refs_and_funcs(&self, refs: &mut Vec<Reference>, funcs: &mut Vec<String>) {
        match self {
            AstNode::Reference(r) => refs.push(r.clone()),
            AstNode::Function { name, args } => {
                funcs.push(name.clone());
                for arg in args {
                    arg.collect_refs_and_funcs(refs, funcs);
                }
            }
            AstNode::BinaryOp { left, right, .. } => {
                left.collect_refs_and_funcs(refs, funcs);
                right.collect_refs_and_funcs(refs, funcs);
            }
            AstNode::UnaryOp { arg, .. } => {
                arg.collect_refs_and_funcs(refs, funcs);
            }
            AstNode::Array(rows) => {
                for row in rows {
                    for cell in row {
                        cell.collect_refs_and_funcs(refs, funcs);
                    }
                }
            }
            AstNode::Paren(inner) => {
                inner.collect_refs_and_funcs(refs, funcs);
            }
            _ => {}
        }
    }
}

impl std::fmt::Display for AstNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AstNode::BinaryOp { op, left, right } => {
                write!(f, "({} {} {})", left, op, right)
            }
            AstNode::UnaryOp { op, arg } => {
                write!(f, "{}{}", op, arg)
            }
            AstNode::Function { name, args } => {
                let args_str: Vec<String> = args.iter().map(|a| a.to_string()).collect();
                write!(f, "{}({})", name, args_str.join(", "))
            }
            AstNode::Reference(r) => write!(f, "{}", r),
            AstNode::Constant(c) => write!(f, "{}", c),
            AstNode::Array(rows) => {
                let row_strs: Vec<String> = rows
                    .iter()
                    .map(|row| {
                        let cells: Vec<String> = row.iter().map(|c| c.to_string()).collect();
                        cells.join(", ")
                    })
                    .collect();
                write!(f, "{{{}}}", row_strs.join("; "))
            }
            AstNode::Error(e) => write!(f, "{}", e),
            AstNode::Paren(inner) => write!(f, "({})", inner),
        }
    }
}

/// Binary operators in Excel formulas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
    Concat,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

impl BinOp {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "+" => Some(BinOp::Add),
            "-" => Some(BinOp::Sub),
            "*" => Some(BinOp::Mul),
            "/" => Some(BinOp::Div),
            "^" => Some(BinOp::Pow),
            "&" => Some(BinOp::Concat),
            "=" => Some(BinOp::Eq),
            "<>" => Some(BinOp::Ne),
            "<" => Some(BinOp::Lt),
            "<=" => Some(BinOp::Le),
            ">" => Some(BinOp::Gt),
            ">=" => Some(BinOp::Ge),
            _ => None,
        }
    }
}

impl std::fmt::Display for BinOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BinOp::Add => write!(f, "+"),
            BinOp::Sub => write!(f, "-"),
            BinOp::Mul => write!(f, "*"),
            BinOp::Div => write!(f, "/"),
            BinOp::Pow => write!(f, "^"),
            BinOp::Concat => write!(f, "&"),
            BinOp::Eq => write!(f, "="),
            BinOp::Ne => write!(f, "<>"),
            BinOp::Lt => write!(f, "<"),
            BinOp::Le => write!(f, "<="),
            BinOp::Gt => write!(f, ">"),
            BinOp::Ge => write!(f, ">="),
        }
    }
}

/// Unary operators in Excel formulas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum UnaryOp {
    Neg,    // -
    Pos,    // +
    Percent, // %
}

impl std::fmt::Display for UnaryOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UnaryOp::Neg => write!(f, "-"),
            UnaryOp::Pos => write!(f, "+"),
            UnaryOp::Percent => write!(f, "%"),
        }
    }
}

/// Constant values in formula expressions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Constant {
    Number(f64),
    String(String),
    Boolean(bool),
    Empty,
}

impl std::fmt::Display for Constant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Constant::Number(n) => write!(f, "{}", n),
            Constant::String(s) => write!(f, "\"{}\"", s),
            Constant::Boolean(b) => write!(f, "{}", if *b { "TRUE" } else { "FALSE" }),
            Constant::Empty => write!(f, ""),
        }
    }
}

impl From<Constant> for CellValue {
    fn from(c: Constant) -> Self {
        match c {
            Constant::Number(n) => CellValue::Number(n),
            Constant::String(s) => CellValue::InlineString(s),
            Constant::Boolean(b) => CellValue::Boolean(b),
            Constant::Empty => CellValue::Empty,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_formula_creation() {
        let f = Formula::new("=SUM(A1:A10)");
        assert_eq!(f.raw, "=SUM(A1:A10)");
        assert!(f.ast.is_none());
        assert_eq!(f.formula_type, FormulaType::Normal);
    }

    #[test]
    fn test_binary_op_display() {
        assert_eq!(BinOp::Add.to_string(), "+");
        assert_eq!(BinOp::Mul.to_string(), "*");
        assert_eq!(BinOp::Eq.to_string(), "=");
        assert_eq!(BinOp::Ne.to_string(), "<>");
    }

    #[test]
    fn test_binary_op_from_str() {
        assert_eq!(BinOp::from_str("+"), Some(BinOp::Add));
        assert_eq!(BinOp::from_str("^"), Some(BinOp::Pow));
        assert_eq!(BinOp::from_str("&"), Some(BinOp::Concat));
        assert_eq!(BinOp::from_str("invalid"), None);
    }

    #[test]
    fn test_ast_collect_refs() {
        let node = AstNode::BinaryOp {
            op: BinOp::Add,
            left: Box::new(AstNode::Reference(Reference::Cell(
                crate::reference::CellReference::new(0, 0),
            ))),
            right: Box::new(AstNode::Function {
                name: "SUM".to_string(),
                args: vec![AstNode::Reference(Reference::Range(
                    crate::reference::RangeReference::new(
                        crate::reference::CellReference::new(1, 0),
                        crate::reference::CellReference::new(1, 9),
                    ),
                ))],
            }),
        };

        let ast = FormulaAst::new(node);
        assert_eq!(ast.references.len(), 2);
        assert_eq!(ast.functions, vec!["SUM"]);
    }

    #[test]
    fn test_ast_display() {
        let node = AstNode::BinaryOp {
            op: BinOp::Add,
            left: Box::new(AstNode::Constant(Constant::Number(1.0))),
            right: Box::new(AstNode::Constant(Constant::Number(2.0))),
        };
        assert_eq!(node.to_string(), "(1 + 2)");
    }

    #[test]
    fn test_formula_builder_methods() {
        let ast = FormulaAst::new(AstNode::Constant(Constant::Number(5.0)));
        let f = Formula::new("=5")
            .with_ast(ast)
            .with_cached(CellValue::Number(5.0));
        assert!(f.ast.is_some());
        assert_eq!(f.cached_value, Some(CellValue::Number(5.0)));
    }

    #[test]
    fn test_binop_from_str_relations() {
        assert_eq!(BinOp::from_str("<"), Some(BinOp::Lt));
        assert_eq!(BinOp::from_str("<="), Some(BinOp::Le));
        assert_eq!(BinOp::from_str(">"), Some(BinOp::Gt));
        assert_eq!(BinOp::from_str(">="), Some(BinOp::Ge));
    }

    #[test]
    fn test_unary_and_constant_display() {
        assert_eq!(UnaryOp::Neg.to_string(), "-");
        assert_eq!(UnaryOp::Pos.to_string(), "+");
        assert_eq!(UnaryOp::Percent.to_string(), "%");
        assert_eq!(Constant::String("abc".to_string()).to_string(), "\"abc\"");
        assert_eq!(Constant::Boolean(true).to_string(), "TRUE");
    }

    #[test]
    fn test_ast_array_and_paren_collect() {
        let node = AstNode::Paren(Box::new(AstNode::Array(vec![vec![
            AstNode::Function {
                name: "LEN".to_string(),
                args: vec![AstNode::Reference(Reference::Cell(
                    crate::reference::CellReference::new(2, 3),
                ))],
            },
        ]])));

        let ast = FormulaAst::new(node);
        assert_eq!(ast.functions, vec!["LEN"]);
        assert_eq!(ast.references.len(), 1);
    }

    #[test]
    fn test_constant_into_cell_value() {
        let n: CellValue = Constant::Number(3.0).into();
        let s: CellValue = Constant::String("x".to_string()).into();
        let b: CellValue = Constant::Boolean(false).into();
        let e: CellValue = Constant::Empty.into();
        assert_eq!(n, CellValue::Number(3.0));
        assert_eq!(s, CellValue::InlineString("x".to_string()));
        assert_eq!(b, CellValue::Boolean(false));
        assert_eq!(e, CellValue::Empty);
    }
}
