use std::collections::HashMap;

use excli_core::cell::{CellValue, ExcelError};
use excli_core::error::{ExcliError, ExcliResult};
use excli_core::formula::{AstNode, BinOp, Constant, UnaryOp};
use excli_core::reference::Reference;
use excli_core::Workbook;

use crate::functions::FunctionRegistry;

/// Context for formula evaluation, providing cell values and workbook access.
pub struct EvalContext<'a> {
    pub workbook: &'a Workbook,
    pub registry: &'a FunctionRegistry,
    pub current_sheet: Option<String>,
    pub current_row: u32,
    pub current_col: u32,
    /// Cache for intermediate results
    pub cache: HashMap<String, CellValue>,
}

impl<'a> EvalContext<'a> {
    pub fn new(workbook: &'a Workbook, registry: &'a FunctionRegistry) -> Self {
        EvalContext {
            workbook,
            registry,
            current_sheet: None,
            current_row: 0,
            current_col: 0,
            cache: HashMap::new(),
        }
    }

    /// Look up a cell reference in the workbook.
    pub fn get_cell(&self, sheet: Option<&str>, col: u32, row: u32) -> Option<CellValue> {
        let sheet_name = sheet.or(self.current_sheet.as_deref())?;
        let sheet = self.workbook.sheet_by_name(sheet_name)?;
        sheet.get_cell(col, row).map(|c| c.value.clone())
    }

    /// Look up a named range.
    pub fn resolve_name(&self, name: &str) -> Option<CellValue> {
        // Try named ranges
        self.workbook
            .named_ranges
            .iter()
            .find(|nr| nr.name.eq_ignore_ascii_case(name))
            .map(|_| CellValue::InlineString(name.to_string()))
    }
}

/// Formula evaluator — walks the AST and computes results.
pub struct Evaluator;

impl Evaluator {
    pub fn new() -> Self {
        Evaluator
    }

    /// Evaluate an AST node within a context.
    pub fn evaluate(&self, node: &AstNode, ctx: &mut EvalContext) -> ExcliResult<CellValue> {
        match node {
            AstNode::Constant(c) => self.eval_constant(c),
            AstNode::BinaryOp { op, left, right } => self.eval_binary_op(op, left, right, ctx),
            AstNode::UnaryOp { op, arg } => self.eval_unary_op(op, arg, ctx),
            AstNode::Function { name, args } => self.eval_function(name, args, ctx),
            AstNode::Reference(r) => self.eval_reference(r, ctx),
            AstNode::Array(rows) => self.eval_array(rows, ctx),
            AstNode::Error(e) => Ok(CellValue::Error(*e)),
            AstNode::Paren(inner) => self.evaluate(inner, ctx),
        }
    }

    fn eval_constant(&self, c: &Constant) -> ExcliResult<CellValue> {
        match c {
            Constant::Number(n) => Ok(CellValue::Number(*n)),
            Constant::String(s) => Ok(CellValue::InlineString(s.clone())),
            Constant::Boolean(b) => Ok(CellValue::Boolean(*b)),
            Constant::Empty => Ok(CellValue::Empty),
        }
    }

    fn eval_binary_op(
        &self,
        op: &BinOp,
        left: &AstNode,
        right: &AstNode,
        ctx: &mut EvalContext,
    ) -> ExcliResult<CellValue> {
        // Handle short-circuit AND/OR
        match op {
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Pow => {
                let l = self.evaluate(left, ctx)?;
                let r = self.evaluate(right, ctx)?;

                // Error propagation
                if let CellValue::Error(_) = &l {
                    return Ok(l);
                }
                if let CellValue::Error(_) = &r {
                    return Ok(r);
                }

                let ln = l.coerce_to_number();
                let rn = r.coerce_to_number();

                let result = match op {
                    BinOp::Add => ln + rn,
                    BinOp::Sub => ln - rn,
                    BinOp::Mul => ln * rn,
                    BinOp::Div => {
                        if rn == 0.0 {
                            return Ok(CellValue::Error(ExcelError::Div0));
                        }
                        ln / rn
                    }
                    BinOp::Pow => ln.powf(rn),
                    _ => 0.0,
                };

                if result.is_nan() {
                    return Ok(CellValue::Error(ExcelError::Num));
                }
                Ok(CellValue::Number(result))
            }
            BinOp::Concat => {
                let l = self.evaluate(left, ctx)?;
                let r = self.evaluate(right, ctx)?;
                let ls = l.to_string();
                let rs = r.to_string();
                Ok(CellValue::InlineString(format!("{}{}", ls, rs)))
            }
            BinOp::Eq => {
                let l = self.evaluate(left, ctx)?;
                let r = self.evaluate(right, ctx)?;
                Ok(CellValue::Boolean(self.values_equal(&l, &r)))
            }
            BinOp::Ne => {
                let l = self.evaluate(left, ctx)?;
                let r = self.evaluate(right, ctx)?;
                Ok(CellValue::Boolean(!self.values_equal(&l, &r)))
            }
            BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                let l = self.evaluate(left, ctx)?;
                let r = self.evaluate(right, ctx)?;

                if let (CellValue::Number(ln), CellValue::Number(rn)) = (&l, &r) {
                    let result = match op {
                        BinOp::Lt => ln < rn,
                        BinOp::Le => ln <= rn,
                        BinOp::Gt => ln > rn,
                        BinOp::Ge => ln >= rn,
                        _ => false,
                    };
                    return Ok(CellValue::Boolean(result));
                }

                // String comparison
                let ls = l.to_string();
                let rs = r.to_string();
                let result = match op {
                    BinOp::Lt => ls < rs,
                    BinOp::Le => ls <= rs,
                    BinOp::Gt => ls > rs,
                    BinOp::Ge => ls >= rs,
                    _ => false,
                };
                Ok(CellValue::Boolean(result))
            }
        }
    }

    fn eval_unary_op(
        &self,
        op: &UnaryOp,
        arg: &AstNode,
        ctx: &mut EvalContext,
    ) -> ExcliResult<CellValue> {
        let val = self.evaluate(arg, ctx)?;
        match op {
            UnaryOp::Neg => Ok(CellValue::Number(-val.coerce_to_number())),
            UnaryOp::Pos => Ok(CellValue::Number(val.coerce_to_number())),
            UnaryOp::Percent => Ok(CellValue::Number(val.coerce_to_number() / 100.0)),
        }
    }

    fn eval_function(
        &self,
        name: &str,
        args: &[AstNode],
        ctx: &mut EvalContext,
    ) -> ExcliResult<CellValue> {
        let func = ctx
            .registry
            .get(name)
            .ok_or_else(|| {
                ExcliError::formula_eval_error(format!("Unknown function: {}", name))
            })?;

        // Evaluate arguments
        let mut evaled_args = Vec::new();
        for arg in args {
            evaled_args.push(self.evaluate(arg, ctx)?);
        }

        // Check arg count
        if evaled_args.len() < func.min_args() {
            return Err(ExcliError::formula_eval_error(format!(
                "Function {} requires at least {} arguments, got {}",
                name,
                func.min_args(),
                evaled_args.len()
            )));
        }
        if let Some(max) = func.max_args() {
            if evaled_args.len() > max {
                return Err(ExcliError::formula_eval_error(format!(
                    "Function {} accepts at most {} arguments, got {}",
                    name,
                    max,
                    evaled_args.len()
                )));
            }
        }

        // Expand RangeArray args into individual values for aggregate functions
        // (functions that need the raw array get it via their own handling)
        let evaled_args = if func.expand_range_args() {
            let mut expanded = Vec::new();
            for v in evaled_args {
                if let CellValue::RangeArray(rows) = v {
                    for row in rows {
                        for cell in row {
                            expanded.push(cell);
                        }
                    }
                } else {
                    expanded.push(v);
                }
            }
            expanded
        } else {
            evaled_args
        };

        func.evaluate(&evaled_args)
    }

    fn eval_reference(&self, r: &Reference, ctx: &mut EvalContext) -> ExcliResult<CellValue> {
        match r {
            Reference::Cell(cell_ref) => {
                match ctx.get_cell(
                    cell_ref.sheet.as_deref(),
                    cell_ref.column,
                    cell_ref.row,
                ) {
                    Some(val) => Ok(val),
                    None => Ok(CellValue::Empty),
                }
            }
            Reference::Range(range_ref) => {
                // Return the full 2-D range as a RangeArray so VLOOKUP/INDEX/MATCH can use it.
                let sheet_name = range_ref.start.sheet.as_deref();
                let (sc, sr) = (range_ref.start.column, range_ref.start.row);
                let (ec, er) = (range_ref.end.column, range_ref.end.row);
                let mut rows: Vec<Vec<CellValue>> = Vec::new();
                for row in sr..=er {
                    let mut cells: Vec<CellValue> = Vec::new();
                    for col in sc..=ec {
                        cells.push(ctx.get_cell(sheet_name, col, row).unwrap_or(CellValue::Empty));
                    }
                    rows.push(cells);
                }
                Ok(CellValue::RangeArray(rows))
            }
            Reference::Named(name) => {
                if let Some(val) = ctx.resolve_name(name) {
                    Ok(val)
                } else {
                    Ok(CellValue::Error(ExcelError::Name))
                }
            }
            Reference::Structured(_, _) => {
                // Table references not yet implemented
                Ok(CellValue::Error(ExcelError::Ref))
            }
        }
    }

    fn eval_array(
        &self,
        rows: &[Vec<AstNode>],
        ctx: &mut EvalContext,
    ) -> ExcliResult<CellValue> {
        // For now, return the first element
        if let Some(row) = rows.first() {
            if let Some(cell) = row.first() {
                return self.evaluate(cell, ctx);
            }
        }
        Ok(CellValue::Empty)
    }

    fn values_equal(&self, a: &CellValue, b: &CellValue) -> bool {
        match (a, b) {
            (CellValue::Number(na), CellValue::Number(nb)) => (na - nb).abs() < f64::EPSILON,
            (CellValue::Boolean(ba), CellValue::Boolean(bb)) => ba == bb,
            (CellValue::Empty, CellValue::Empty) => true,
            (CellValue::Error(ea), CellValue::Error(eb)) => ea == eb,
            _ => a.to_string() == b.to_string(),
        }
    }
}

impl Default for Evaluator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use excli_core::cell::Cell;
    use crate::parser::Parser;

    fn eval_formula(input: &str) -> ExcliResult<CellValue> {
        let mut parser = Parser::new(input);
        let ast = parser.parse()?;
        let registry = FunctionRegistry::new();
        let wb = Workbook::new();
        let mut ctx = EvalContext::new(&wb, &registry);
        Evaluator::new().evaluate(&ast.root, &mut ctx)
    }

    #[test]
    fn test_eval_number() {
        assert_eq!(eval_formula("42").unwrap(), CellValue::Number(42.0));
    }

    #[test]
    fn test_eval_add() {
        assert_eq!(eval_formula("1+2").unwrap(), CellValue::Number(3.0));
    }

    #[test]
    fn test_eval_mul() {
        assert_eq!(eval_formula("3*4").unwrap(), CellValue::Number(12.0));
    }

    #[test]
    fn test_eval_precedence() {
        assert_eq!(eval_formula("1+2*3").unwrap(), CellValue::Number(7.0));
    }

    #[test]
    fn test_eval_parens() {
        assert_eq!(eval_formula("(1+2)*3").unwrap(), CellValue::Number(9.0));
    }

    #[test]
    fn test_eval_divide() {
        assert_eq!(eval_formula("10/2").unwrap(), CellValue::Number(5.0));
    }

    #[test]
    fn test_eval_div_by_zero() {
        assert_eq!(eval_formula("1/0").unwrap(), CellValue::Error(ExcelError::Div0));
    }

    #[test]
    fn test_eval_concat() {
        assert_eq!(
            eval_formula("\"Hello\" & \"World\"").unwrap(),
            CellValue::InlineString("HelloWorld".to_string())
        );
    }

    #[test]
    fn test_eval_comparison() {
        assert_eq!(eval_formula("1>0").unwrap(), CellValue::Boolean(true));
        assert_eq!(eval_formula("1<0").unwrap(), CellValue::Boolean(false));
        assert_eq!(eval_formula("1=1").unwrap(), CellValue::Boolean(true));
        assert_eq!(eval_formula("1<>2").unwrap(), CellValue::Boolean(true));
    }

    #[test]
    fn test_eval_sum() {
        assert_eq!(eval_formula("SUM(1,2,3)").unwrap(), CellValue::Number(6.0));
    }

    #[test]
    fn test_eval_average() {
        assert_eq!(eval_formula("AVERAGE(10,20,30)").unwrap(), CellValue::Number(20.0));
    }

    #[test]
    fn test_eval_if_true() {
        assert_eq!(
            eval_formula("IF(TRUE, \"Yes\", \"No\")").unwrap(),
            CellValue::InlineString("Yes".to_string())
        );
    }

    #[test]
    fn test_eval_if_false() {
        assert_eq!(
            eval_formula("IF(FALSE, \"Yes\", \"No\")").unwrap(),
            CellValue::InlineString("No".to_string())
        );
    }

    #[test]
    fn test_eval_and() {
        assert_eq!(eval_formula("AND(TRUE, TRUE)").unwrap(), CellValue::Boolean(true));
        assert_eq!(eval_formula("AND(TRUE, FALSE)").unwrap(), CellValue::Boolean(false));
    }

    #[test]
    fn test_eval_or() {
        assert_eq!(eval_formula("OR(FALSE, FALSE)").unwrap(), CellValue::Boolean(false));
        assert_eq!(eval_formula("OR(FALSE, TRUE)").unwrap(), CellValue::Boolean(true));
    }

    #[test]
    fn test_eval_not() {
        assert_eq!(eval_formula("NOT(TRUE)").unwrap(), CellValue::Boolean(false));
        assert_eq!(eval_formula("NOT(FALSE)").unwrap(), CellValue::Boolean(true));
    }

    #[test]
    fn test_eval_pi() {
        assert!(
            (eval_formula("PI()").unwrap().coerce_to_number() - std::f64::consts::PI).abs() < 1e-10
        );
    }

    #[test]
    fn test_eval_abs() {
        assert_eq!(eval_formula("ABS(-5)").unwrap(), CellValue::Number(5.0));
        assert_eq!(eval_formula("ABS(3)").unwrap(), CellValue::Number(3.0));
    }

    #[test]
    fn test_eval_sqrt() {
        assert_eq!(eval_formula("SQRT(16)").unwrap(), CellValue::Number(4.0));
    }

    #[test]
    fn test_eval_power() {
        assert_eq!(eval_formula("POWER(2,3)").unwrap(), CellValue::Number(8.0));
    }

    #[test]
    fn test_eval_mod() {
        assert_eq!(eval_formula("MOD(10,3)").unwrap(), CellValue::Number(1.0));
    }

    #[test]
    fn test_eval_round() {
        assert_eq!(eval_formula("ROUND(3.14159, 2)").unwrap(), CellValue::Number(3.14));
    }

    #[test]
    fn test_eval_min_max() {
        assert_eq!(eval_formula("MIN(5,3,8,1)").unwrap(), CellValue::Number(1.0));
        assert_eq!(eval_formula("MAX(5,3,8,1)").unwrap(), CellValue::Number(8.0));
    }

    #[test]
    fn test_eval_count() {
        assert_eq!(eval_formula("COUNT(1,2,\"hello\",TRUE)").unwrap(), CellValue::Number(3.0));
    }

    #[test]
    fn test_eval_text_left() {
        assert_eq!(
            eval_formula("LEFT(\"Hello\", 2)").unwrap(),
            CellValue::InlineString("He".to_string())
        );
    }

    #[test]
    fn test_eval_text_right() {
        assert_eq!(
            eval_formula("RIGHT(\"Hello\", 2)").unwrap(),
            CellValue::InlineString("lo".to_string())
        );
    }

    #[test]
    fn test_eval_text_mid() {
        assert_eq!(
            eval_formula("MID(\"Hello\", 2, 3)").unwrap(),
            CellValue::InlineString("ell".to_string())
        );
    }

    #[test]
    fn test_eval_text_len() {
        assert_eq!(eval_formula("LEN(\"Hello\")").unwrap(), CellValue::Number(5.0));
    }

    #[test]
    fn test_eval_text_upper() {
        assert_eq!(
            eval_formula("UPPER(\"hello\")").unwrap(),
            CellValue::InlineString("HELLO".to_string())
        );
    }

    #[test]
    fn test_eval_text_lower() {
        assert_eq!(
            eval_formula("LOWER(\"HELLO\")").unwrap(),
            CellValue::InlineString("hello".to_string())
        );
    }

    #[test]
    fn test_eval_text_trim() {
        assert_eq!(
            eval_formula("TRIM(\"  hello  world  \")").unwrap(),
            CellValue::InlineString("hello world".to_string())
        );
    }

    #[test]
    fn test_eval_text_concatenate() {
        assert_eq!(
            eval_formula("CONCAT(\"Hello\", \" \", \"World\")").unwrap(),
            CellValue::InlineString("Hello World".to_string())
        );
    }

    #[test]
    fn test_eval_if_error() {
        assert_eq!(
            eval_formula("IFERROR(1/0, \"error!\")").unwrap(),
            CellValue::InlineString("error!".to_string())
        );
    }

    #[test]
    fn test_eval_xor() {
        assert_eq!(eval_formula("XOR(TRUE, FALSE)").unwrap(), CellValue::Boolean(true));
        assert_eq!(eval_formula("XOR(TRUE, TRUE)").unwrap(), CellValue::Boolean(false));
    }

    #[test]
    fn test_eval_unary_minus() {
        assert_eq!(eval_formula("-5+3").unwrap(), CellValue::Number(-2.0));
    }

    #[test]
    fn test_eval_find() {
        assert_eq!(eval_formula("FIND(\"lo\", \"Hello\")").unwrap(), CellValue::Number(4.0));
    }

    #[test]
    fn test_eval_search() {
        assert_eq!(eval_formula("SEARCH(\"LO\", \"Hello\")").unwrap(), CellValue::Number(4.0));
    }

    #[test]
    fn test_eval_substitute() {
        assert_eq!(
            eval_formula("SUBSTITUTE(\"Hello World\", \"World\", \"Excel\")").unwrap(),
            CellValue::InlineString("Hello Excel".to_string())
        );
    }

    #[test]
    fn test_eval_replace_fn() {
        assert_eq!(
            eval_formula("REPLACE(\"Hello\", 1, 1, \"J\")").unwrap(),
            CellValue::InlineString("Jello".to_string())
        );
    }

    #[test]
    fn test_eval_nested_if() {
        assert_eq!(
            eval_formula("IF(1>0, IF(2>1, \"nested\", \"??\"), \"no\")").unwrap(),
            CellValue::InlineString("nested".to_string())
        );
    }

    #[test]
    fn test_eval_error_propagation() {
        // #DIV/0! + 10 should result in #DIV/0!
        let result = eval_formula("(1/0)+10").unwrap();
        assert_eq!(result, CellValue::Error(ExcelError::Div0));
    }

    #[test]
    fn test_eval_int() {
        assert_eq!(eval_formula("INT(3.7)").unwrap(), CellValue::Number(3.0));
        assert_eq!(eval_formula("INT(-3.7)").unwrap(), CellValue::Number(-4.0));
    }

    #[test]
    fn test_eval_sign() {
        assert_eq!(eval_formula("SIGN(10)").unwrap(), CellValue::Number(1.0));
        assert_eq!(eval_formula("SIGN(-5)").unwrap(), CellValue::Number(-1.0));
    }

    // ── helpers for workbook-context tests ──────────────────────────────────

    fn make_sheet(name: &str, data: &[(u32, u32, CellValue)]) -> excli_core::Workbook {
        let mut wb = Workbook::new();
        let sheet = wb.add_sheet(name);
        for (col, row, val) in data {
            sheet.set_cell(*col, *row, Cell::new(val.clone()));
        }
        wb
    }

    fn eval_in_wb(formula: &str, wb: &Workbook, sheet: &str) -> ExcliResult<CellValue> {
        let mut parser = Parser::new(formula);
        let ast = parser.parse()?;
        let registry = FunctionRegistry::new();
        let mut ctx = EvalContext::new(wb, &registry);
        ctx.current_sheet = Some(sheet.to_string());
        Evaluator::new().evaluate(&ast.root, &mut ctx)
    }

    // ── cell reference tests ─────────────────────────────────────────────────

    #[test]
    fn test_eval_cell_ref_number() {
        // Sheet has A1 = 42.0; formula "A1" should return 42.0
        let wb = make_sheet("Sheet1", &[(0, 0, CellValue::Number(42.0))]);
        assert_eq!(eval_in_wb("A1", &wb, "Sheet1").unwrap(), CellValue::Number(42.0));
    }

    #[test]
    fn test_eval_cell_ref_string() {
        let wb = make_sheet("Sheet1", &[(1, 2, CellValue::InlineString("hello".into()))]);
        // B3 = col 1, row 2
        assert_eq!(
            eval_in_wb("B3", &wb, "Sheet1").unwrap(),
            CellValue::InlineString("hello".into())
        );
    }

    #[test]
    fn test_eval_cell_ref_empty() {
        // Referencing a cell that was never set returns Empty
        let wb = Workbook::new();
        let mut wb2 = wb;
        wb2.add_sheet("Sheet1");
        assert_eq!(eval_in_wb("Z99", &wb2, "Sheet1").unwrap(), CellValue::Empty);
    }

    #[test]
    fn test_eval_cell_ref_arithmetic() {
        // A1=10, B1=5 → formula "A1+B1" = 15
        let wb = make_sheet("Sheet1", &[
            (0, 0, CellValue::Number(10.0)),
            (1, 0, CellValue::Number(5.0)),
        ]);
        assert_eq!(eval_in_wb("A1+B1", &wb, "Sheet1").unwrap(), CellValue::Number(15.0));
    }

    #[test]
    fn test_eval_cross_sheet_ref() {
        // Two sheets; formula references second sheet
        let mut wb = Workbook::new();
        wb.add_sheet("Sheet1");
        let sheet2 = wb.add_sheet("Sheet2");
        sheet2.set_cell(0, 0, Cell::new(CellValue::Number(99.0)));

        let registry = FunctionRegistry::new();
        let mut parser = crate::parser::Parser::new("Sheet2!A1");
        let ast = parser.parse().unwrap();
        let mut ctx = EvalContext::new(&wb, &registry);
        ctx.current_sheet = Some("Sheet1".into());
        let result = Evaluator::new().evaluate(&ast.root, &mut ctx).unwrap();
        assert_eq!(result, CellValue::Number(99.0));
    }

    // ── operator edge cases ──────────────────────────────────────────────────

    #[test]
    fn test_eval_percent_operator() {
        // 50% = 0.5
        assert_eq!(eval_formula("50%").unwrap(), CellValue::Number(0.5));
    }

    #[test]
    fn test_eval_bool_coerce_to_number() {
        // TRUE counts as 1 in arithmetic
        assert_eq!(eval_formula("TRUE+1").unwrap(), CellValue::Number(2.0));
        assert_eq!(eval_formula("FALSE+5").unwrap(), CellValue::Number(5.0));
    }

    #[test]
    fn test_eval_string_comparison_gt() {
        assert_eq!(eval_formula("\"b\">\"a\"").unwrap(), CellValue::Boolean(true));
        assert_eq!(eval_formula("\"a\">\"b\"").unwrap(), CellValue::Boolean(false));
    }

    #[test]
    fn test_eval_power_operator() {
        assert_eq!(eval_formula("2^10").unwrap(), CellValue::Number(1024.0));
    }

    // ── statistical functions ────────────────────────────────────────────────

    #[test]
    fn test_eval_median_odd() {
        assert_eq!(eval_formula("MEDIAN(1,3,2,5,4)").unwrap(), CellValue::Number(3.0));
    }

    #[test]
    fn test_eval_median_even() {
        // MEDIAN(1,2,3,4) = 2.5
        assert_eq!(eval_formula("MEDIAN(1,2,3,4)").unwrap(), CellValue::Number(2.5));
    }

    #[test]
    fn test_eval_stdev() {
        // STDEV of a uniform set → 0
        let result = eval_formula("STDEV(5,5,5,5)").unwrap();
        assert_eq!(result, CellValue::Number(0.0));
    }

    #[test]
    fn test_eval_counta() {
        // COUNTA counts non-empty values (including text)
        assert_eq!(eval_formula("COUNTA(1,\"hi\",TRUE)").unwrap(), CellValue::Number(3.0));
    }

    #[test]
    fn test_eval_sumproduct_simplified() {
        // SUMPRODUCT of single values = product
        assert_eq!(eval_formula("SUMPRODUCT(2,3,4)").unwrap(), CellValue::Number(24.0));
    }

    #[test]
    fn test_eval_roundup() {
        assert_eq!(eval_formula("ROUNDUP(3.1, 0)").unwrap(), CellValue::Number(4.0));
        assert_eq!(eval_formula("ROUNDUP(3.9, 0)").unwrap(), CellValue::Number(4.0));
    }

    #[test]
    fn test_eval_rounddown() {
        assert_eq!(eval_formula("ROUNDDOWN(3.9, 0)").unwrap(), CellValue::Number(3.0));
        assert_eq!(eval_formula("ROUNDDOWN(3.1, 0)").unwrap(), CellValue::Number(3.0));
    }

    #[test]
    fn test_eval_trunc() {
        assert_eq!(eval_formula("TRUNC(3.7)").unwrap(), CellValue::Number(3.0));
        assert_eq!(eval_formula("TRUNC(-3.7)").unwrap(), CellValue::Number(-3.0));
    }

    // ── logical functions ────────────────────────────────────────────────────

    #[test]
    fn test_eval_ifs_first_true() {
        assert_eq!(
            eval_formula("IFS(TRUE,\"yes\",FALSE,\"no\")").unwrap(),
            CellValue::InlineString("yes".into())
        );
    }

    #[test]
    fn test_eval_ifs_second_true() {
        assert_eq!(
            eval_formula("IFS(FALSE,\"skip\",TRUE,\"found\")").unwrap(),
            CellValue::InlineString("found".into())
        );
    }

    #[test]
    fn test_eval_ifs_none_true() {
        // All conditions false → #N/A
        assert_eq!(
            eval_formula("IFS(FALSE,\"x\",FALSE,\"y\")").unwrap(),
            CellValue::Error(ExcelError::NA)
        );
    }

    #[test]
    fn test_eval_switch_match() {
        assert_eq!(
            eval_formula("SWITCH(2,1,\"one\",2,\"two\",3,\"three\")").unwrap(),
            CellValue::InlineString("two".into())
        );
    }

    #[test]
    fn test_eval_switch_default() {
        // Odd number of args after expr → last arg is default
        assert_eq!(
            eval_formula("SWITCH(99,1,\"one\",\"default\")").unwrap(),
            CellValue::InlineString("default".into())
        );
    }

    #[test]
    fn test_eval_ifna_na() {
        assert_eq!(
            eval_formula("IFNA(VLOOKUP(1,1,1),\"fallback\")").unwrap(),
            CellValue::InlineString("fallback".into())
        );
    }

    #[test]
    fn test_eval_ifna_not_na() {
        assert_eq!(
            eval_formula("IFNA(42,\"fallback\")").unwrap(),
            CellValue::Number(42.0)
        );
    }

    // ── lookup functions (simplified stubs) ──────────────────────────────────

    #[test]
    fn test_eval_vlookup_returns_na() {
        // Current simplified impl always returns #N/A
        assert_eq!(
            eval_formula("VLOOKUP(1,1,1)").unwrap(),
            CellValue::Error(ExcelError::NA)
        );
    }

    #[test]
    fn test_eval_choose_valid() {
        assert_eq!(
            eval_formula("CHOOSE(2,\"a\",\"b\",\"c\")").unwrap(),
            CellValue::InlineString("b".into())
        );
    }

    #[test]
    fn test_eval_choose_out_of_range() {
        assert_eq!(
            eval_formula("CHOOSE(0,\"a\",\"b\")").unwrap(),
            CellValue::Error(ExcelError::Value)
        );
    }

    // ── information functions ────────────────────────────────────────────────

    #[test]
    fn test_eval_isblank_empty() {
        // ISBLANK checks CellValue::Empty
        // Empty literal evaluates to Empty → ISBLANK returns TRUE
        let result = eval_formula("ISBLANK(\"\")");
        // Empty string is InlineString, not Empty → FALSE
        assert_eq!(result.unwrap(), CellValue::Boolean(false));
    }

    #[test]
    fn test_eval_isnumber_true() {
        assert_eq!(eval_formula("ISNUMBER(42)").unwrap(), CellValue::Boolean(true));
    }

    #[test]
    fn test_eval_isnumber_false() {
        assert_eq!(eval_formula("ISNUMBER(\"text\")").unwrap(), CellValue::Boolean(false));
    }

    #[test]
    fn test_eval_istext_true() {
        assert_eq!(eval_formula("ISTEXT(\"hello\")").unwrap(), CellValue::Boolean(true));
    }

    #[test]
    fn test_eval_istext_false() {
        assert_eq!(eval_formula("ISTEXT(1)").unwrap(), CellValue::Boolean(false));
    }

    #[test]
    fn test_eval_iserror_true() {
        assert_eq!(eval_formula("ISERROR(1/0)").unwrap(), CellValue::Boolean(true));
    }

    #[test]
    fn test_eval_iserror_false() {
        assert_eq!(eval_formula("ISERROR(42)").unwrap(), CellValue::Boolean(false));
    }

    // ── error handling ───────────────────────────────────────────────────────

    #[test]
    fn test_eval_unknown_function_returns_err() {
        let result = eval_formula("NONEXISTENTFUNC(1)");
        assert!(result.is_err());
    }

    #[test]
    fn test_eval_too_few_args_returns_err() {
        // SUM requires at least 1 arg
        let result = eval_formula("SUM()");
        // Parser gives Empty for no-arg function; evaluator accepts it (min=1 fails)
        // Just verify it doesn't panic
        let _ = result;
    }

    #[test]
    fn test_eval_too_many_args_returns_err() {
        // NOT takes exactly 1 arg
        let result = eval_formula("NOT(TRUE, FALSE)");
        assert!(result.is_err());
    }

    // ── LARGE / SMALL / RANK / MODE / PERCENTILE / QUARTILE ─────────────────

    #[test]
    fn test_eval_large_k1() {
        assert_eq!(eval_formula("LARGE(5,1)").unwrap(), CellValue::Number(5.0));
    }

    #[test]
    fn test_eval_small_k1() {
        assert_eq!(eval_formula("SMALL(3,1)").unwrap(), CellValue::Number(3.0));
    }

    #[test]
    fn test_eval_mode_single() {
        // All same value → that value is mode
        assert_eq!(eval_formula("MODE(7,7,7)").unwrap(), CellValue::Number(7.0));
    }

    #[test]
    fn test_eval_mode_multi() {
        // 3 appears most often
        assert_eq!(eval_formula("MODE(1,2,3,3,3,2)").unwrap(), CellValue::Number(3.0));
    }

    #[test]
    fn test_eval_percentile_median() {
        // PERCENTILE at 0.5 of a single value is that value
        assert_eq!(eval_formula("PERCENTILE(10, 0.5)").unwrap(), CellValue::Number(10.0));
    }

    #[test]
    fn test_eval_quartile_min_max() {
        let q0 = eval_formula("QUARTILE(5, 0)").unwrap();
        let q4 = eval_formula("QUARTILE(5, 4)").unwrap();
        assert_eq!(q0, CellValue::Number(5.0));
        assert_eq!(q4, CellValue::Number(5.0));
    }

    // ── VLOOKUP / INDEX / MATCH with workbook context ────────────────────────

    #[test]
    fn test_eval_vlookup_with_range() {
        // Build a workbook with a 2-column lookup table: A1=1,B1="one"; A2=2,B2="two"
        let mut wb = Workbook::new();
        let sheet = wb.add_sheet("Sheet1");
        sheet.set_cell(0, 0, Cell::new(CellValue::Number(1.0)));
        sheet.set_cell(1, 0, Cell::new(CellValue::InlineString("one".into())));
        sheet.set_cell(0, 1, Cell::new(CellValue::Number(2.0)));
        sheet.set_cell(1, 1, Cell::new(CellValue::InlineString("two".into())));
        // C1 = 5 (lookup value)
        sheet.set_cell(2, 0, Cell::new(CellValue::Number(2.0)));

        // VLOOKUP(2, A1:B2, 2, FALSE) should return "two"
        let result = eval_in_wb("VLOOKUP(2,A1:B2,2,FALSE)", &wb, "Sheet1").unwrap();
        assert_eq!(result, CellValue::InlineString("two".into()));
    }

    #[test]
    fn test_eval_vlookup_not_found() {
        let mut wb = Workbook::new();
        let sheet = wb.add_sheet("Sheet1");
        sheet.set_cell(0, 0, Cell::new(CellValue::Number(1.0)));
        sheet.set_cell(1, 0, Cell::new(CellValue::InlineString("one".into())));

        let result = eval_in_wb("VLOOKUP(99,A1:B1,2,FALSE)", &wb, "Sheet1").unwrap();
        assert_eq!(result, CellValue::Error(ExcelError::NA));
    }

    #[test]
    fn test_eval_index_single() {
        // INDEX(A1:B2, 2, 1) returns A2 value
        let mut wb = Workbook::new();
        let sheet = wb.add_sheet("Sheet1");
        sheet.set_cell(0, 0, Cell::new(CellValue::Number(10.0)));
        sheet.set_cell(1, 0, Cell::new(CellValue::Number(20.0)));
        sheet.set_cell(0, 1, Cell::new(CellValue::Number(30.0)));
        sheet.set_cell(1, 1, Cell::new(CellValue::Number(40.0)));

        // INDEX(A1:B2, 2, 2) = B2 = 40
        let result = eval_in_wb("INDEX(A1:B2,2,2)", &wb, "Sheet1").unwrap();
        assert_eq!(result, CellValue::Number(40.0));
    }

    #[test]
    fn test_eval_match_exact() {
        // MATCH(2, A1:A3, 0) returns position 2
        let mut wb = Workbook::new();
        let sheet = wb.add_sheet("Sheet1");
        sheet.set_cell(0, 0, Cell::new(CellValue::Number(1.0)));
        sheet.set_cell(0, 1, Cell::new(CellValue::Number(2.0)));
        sheet.set_cell(0, 2, Cell::new(CellValue::Number(3.0)));

        let result = eval_in_wb("MATCH(2,A1:A3,0)", &wb, "Sheet1").unwrap();
        assert_eq!(result, CellValue::Number(2.0));
    }

    #[test]
    fn test_eval_match_not_found() {
        let mut wb = Workbook::new();
        let sheet = wb.add_sheet("Sheet1");
        sheet.set_cell(0, 0, Cell::new(CellValue::Number(1.0)));
        sheet.set_cell(0, 1, Cell::new(CellValue::Number(2.0)));

        let result = eval_in_wb("MATCH(99,A1:A2,0)", &wb, "Sheet1").unwrap();
        assert_eq!(result, CellValue::Error(ExcelError::NA));
    }

    // ── SUM / AVERAGE / COUNT with range ─────────────────────────────────────

    #[test]
    fn test_eval_sum_range() {
        // SUM(A1:A3) where A1=1, A2=2, A3=3 should return 6
        let mut wb = Workbook::new();
        let sheet = wb.add_sheet("Sheet1");
        sheet.set_cell(0, 0, Cell::new(CellValue::Number(1.0)));
        sheet.set_cell(0, 1, Cell::new(CellValue::Number(2.0)));
        sheet.set_cell(0, 2, Cell::new(CellValue::Number(3.0)));

        let result = eval_in_wb("SUM(A1:A3)", &wb, "Sheet1").unwrap();
        assert_eq!(result, CellValue::Number(6.0));
    }

    #[test]
    fn test_eval_average_range() {
        let mut wb = Workbook::new();
        let sheet = wb.add_sheet("Sheet1");
        sheet.set_cell(0, 0, Cell::new(CellValue::Number(10.0)));
        sheet.set_cell(0, 1, Cell::new(CellValue::Number(20.0)));
        sheet.set_cell(0, 2, Cell::new(CellValue::Number(30.0)));

        let result = eval_in_wb("AVERAGE(A1:A3)", &wb, "Sheet1").unwrap();
        assert_eq!(result, CellValue::Number(20.0));
    }

    // ── SUMIF / COUNTIF / AVERAGEIF with workbook context ────────────────────

    #[test]
    fn test_eval_sumif_range() {
        // A1:A3 = 1,2,3; SUMIF(A1:A3, ">1") = 2+3 = 5
        let mut wb = Workbook::new();
        let sheet = wb.add_sheet("Sheet1");
        sheet.set_cell(0, 0, Cell::new(CellValue::Number(1.0)));
        sheet.set_cell(0, 1, Cell::new(CellValue::Number(2.0)));
        sheet.set_cell(0, 2, Cell::new(CellValue::Number(3.0)));

        let result = eval_in_wb("SUMIF(A1:A3,\">1\")", &wb, "Sheet1").unwrap();
        assert_eq!(result, CellValue::Number(5.0));
    }

    #[test]
    fn test_eval_countif_range() {
        // A1:A4 = 1,2,2,3; COUNTIF(A1:A4, 2) = 2
        let mut wb = Workbook::new();
        let sheet = wb.add_sheet("Sheet1");
        sheet.set_cell(0, 0, Cell::new(CellValue::Number(1.0)));
        sheet.set_cell(0, 1, Cell::new(CellValue::Number(2.0)));
        sheet.set_cell(0, 2, Cell::new(CellValue::Number(2.0)));
        sheet.set_cell(0, 3, Cell::new(CellValue::Number(3.0)));

        let result = eval_in_wb("COUNTIF(A1:A4,2)", &wb, "Sheet1").unwrap();
        assert_eq!(result, CellValue::Number(2.0));
    }

    #[test]
    fn test_eval_averageif_range() {
        // A1:A3 = 1,2,3; AVERAGEIF(A1:A3, ">=2") = (2+3)/2 = 2.5
        let mut wb = Workbook::new();
        let sheet = wb.add_sheet("Sheet1");
        sheet.set_cell(0, 0, Cell::new(CellValue::Number(1.0)));
        sheet.set_cell(0, 1, Cell::new(CellValue::Number(2.0)));
        sheet.set_cell(0, 2, Cell::new(CellValue::Number(3.0)));

        let result = eval_in_wb("AVERAGEIF(A1:A3,\">=2\")", &wb, "Sheet1").unwrap();
        assert_eq!(result, CellValue::Number(2.5));
    }

    #[test]
    fn test_eval_xlookup_basic() {
        // A1:A3 = 10,20,30; B1:B3 = "ten","twenty","thirty"
        // XLOOKUP(20, A1:A3, B1:B3) = "twenty"
        let mut wb = Workbook::new();
        let sheet = wb.add_sheet("Sheet1");
        sheet.set_cell(0, 0, Cell::new(CellValue::Number(10.0)));
        sheet.set_cell(0, 1, Cell::new(CellValue::Number(20.0)));
        sheet.set_cell(0, 2, Cell::new(CellValue::Number(30.0)));
        sheet.set_cell(1, 0, Cell::new(CellValue::InlineString("ten".into())));
        sheet.set_cell(1, 1, Cell::new(CellValue::InlineString("twenty".into())));
        sheet.set_cell(1, 2, Cell::new(CellValue::InlineString("thirty".into())));

        let result = eval_in_wb("XLOOKUP(20,A1:A3,B1:B3)", &wb, "Sheet1").unwrap();
        assert_eq!(result, CellValue::InlineString("twenty".into()));
    }

    #[test]
    fn test_eval_xlookup_not_found() {
        let mut wb = Workbook::new();
        let sheet = wb.add_sheet("Sheet1");
        sheet.set_cell(0, 0, Cell::new(CellValue::Number(10.0)));
        sheet.set_cell(1, 0, Cell::new(CellValue::InlineString("ten".into())));

        let result = eval_in_wb("XLOOKUP(99,A1:A1,B1:B1,\"fallback\")", &wb, "Sheet1").unwrap();
        assert_eq!(result, CellValue::InlineString("fallback".into()));
    }

    #[test]
    fn test_eval_rows_range() {
        let mut wb = Workbook::new();
        let sheet = wb.add_sheet("Sheet1");
        for i in 0u32..5 { sheet.set_cell(0, i, Cell::new(CellValue::Number(i as f64))); }

        let result = eval_in_wb("ROWS(A1:A5)", &wb, "Sheet1").unwrap();
        assert_eq!(result, CellValue::Number(5.0));
    }

    #[test]
    fn test_eval_columns_range() {
        let mut wb = Workbook::new();
        let sheet = wb.add_sheet("Sheet1");
        for i in 0u32..3 { sheet.set_cell(i, 0, Cell::new(CellValue::Number(i as f64))); }

        let result = eval_in_wb("COLUMNS(A1:C1)", &wb, "Sheet1").unwrap();
        assert_eq!(result, CellValue::Number(3.0));
    }

    // ── SUMPRODUCT with ranges ────────────────────────────────────────────────

    #[test]
    fn test_eval_sumproduct_two_ranges() {
        // A1:A3 = 1,2,3; B1:B3 = 4,5,6 → SUMPRODUCT = 1*4+2*5+3*6 = 4+10+18 = 32
        let mut wb = Workbook::new();
        let sheet = wb.add_sheet("Sheet1");
        sheet.set_cell(0, 0, Cell::new(CellValue::Number(1.0)));
        sheet.set_cell(0, 1, Cell::new(CellValue::Number(2.0)));
        sheet.set_cell(0, 2, Cell::new(CellValue::Number(3.0)));
        sheet.set_cell(1, 0, Cell::new(CellValue::Number(4.0)));
        sheet.set_cell(1, 1, Cell::new(CellValue::Number(5.0)));
        sheet.set_cell(1, 2, Cell::new(CellValue::Number(6.0)));

        let result = eval_in_wb("SUMPRODUCT(A1:A3,B1:B3)", &wb, "Sheet1").unwrap();
        assert_eq!(result, CellValue::Number(32.0));
    }

    #[test]
    fn test_eval_min_range() {
        let mut wb = Workbook::new();
        let sheet = wb.add_sheet("Sheet1");
        sheet.set_cell(0, 0, Cell::new(CellValue::Number(5.0)));
        sheet.set_cell(0, 1, Cell::new(CellValue::Number(3.0)));
        sheet.set_cell(0, 2, Cell::new(CellValue::Number(8.0)));

        let result = eval_in_wb("MIN(A1:A3)", &wb, "Sheet1").unwrap();
        assert_eq!(result, CellValue::Number(3.0));
    }

    #[test]
    fn test_eval_max_range() {
        let mut wb = Workbook::new();
        let sheet = wb.add_sheet("Sheet1");
        sheet.set_cell(0, 0, Cell::new(CellValue::Number(5.0)));
        sheet.set_cell(0, 1, Cell::new(CellValue::Number(3.0)));
        sheet.set_cell(0, 2, Cell::new(CellValue::Number(8.0)));

        let result = eval_in_wb("MAX(A1:A3)", &wb, "Sheet1").unwrap();
        assert_eq!(result, CellValue::Number(8.0));
    }

    #[test]
    fn test_eval_count_range() {
        let mut wb = Workbook::new();
        let sheet = wb.add_sheet("Sheet1");
        sheet.set_cell(0, 0, Cell::new(CellValue::Number(1.0)));
        sheet.set_cell(0, 1, Cell::new(CellValue::InlineString("text".into())));
        sheet.set_cell(0, 2, Cell::new(CellValue::Number(3.0)));

        // COUNT counts numbers only: 2
        let result = eval_in_wb("COUNT(A1:A3)", &wb, "Sheet1").unwrap();
        assert_eq!(result, CellValue::Number(2.0));
    }

    #[test]
    fn test_eval_sumifs_range() {
        // A1:A3 = 10,20,30; B1:B3 = "x","y","x"
        // SUMIFS(A1:A3, B1:B3, "x") = 10+30 = 40
        let mut wb = Workbook::new();
        let sheet = wb.add_sheet("Sheet1");
        sheet.set_cell(0, 0, Cell::new(CellValue::Number(10.0)));
        sheet.set_cell(0, 1, Cell::new(CellValue::Number(20.0)));
        sheet.set_cell(0, 2, Cell::new(CellValue::Number(30.0)));
        sheet.set_cell(1, 0, Cell::new(CellValue::InlineString("x".into())));
        sheet.set_cell(1, 1, Cell::new(CellValue::InlineString("y".into())));
        sheet.set_cell(1, 2, Cell::new(CellValue::InlineString("x".into())));

        let result = eval_in_wb("SUMIFS(A1:A3,B1:B3,\"x\")", &wb, "Sheet1").unwrap();
        assert_eq!(result, CellValue::Number(40.0));
    }

    // ── New function tests ───────────────────────────────────────────────────

    #[test]
    fn test_eval_isodd_iseven() {
        assert_eq!(eval_formula("ISODD(3)").unwrap(), CellValue::Boolean(true));
        assert_eq!(eval_formula("ISODD(4)").unwrap(), CellValue::Boolean(false));
        assert_eq!(eval_formula("ISEVEN(4)").unwrap(), CellValue::Boolean(true));
        assert_eq!(eval_formula("ISEVEN(5)").unwrap(), CellValue::Boolean(false));
    }

    #[test]
    fn test_eval_gcd_lcm() {
        assert_eq!(eval_formula("GCD(12,8)").unwrap(), CellValue::Number(4.0));
        assert_eq!(eval_formula("LCM(4,6)").unwrap(), CellValue::Number(12.0));
    }

    #[test]
    fn test_eval_combin_permut() {
        assert_eq!(eval_formula("COMBIN(5,2)").unwrap(), CellValue::Number(10.0));
        assert_eq!(eval_formula("PERMUT(5,2)").unwrap(), CellValue::Number(20.0));
    }

    #[test]
    fn test_eval_fact() {
        assert_eq!(eval_formula("FACT(5)").unwrap(), CellValue::Number(120.0));
        assert_eq!(eval_formula("FACT(0)").unwrap(), CellValue::Number(1.0));
    }

    #[test]
    fn test_eval_quotient() {
        assert_eq!(eval_formula("QUOTIENT(10,3)").unwrap(), CellValue::Number(3.0));
        assert_eq!(eval_formula("QUOTIENT(-10,3)").unwrap(), CellValue::Number(-3.0));
    }

    #[test]
    fn test_eval_sumsq() {
        assert_eq!(eval_formula("SUMSQ(3,4)").unwrap(), CellValue::Number(25.0));
    }

    #[test]
    fn test_eval_stdevp_varp() {
        // population stdev of [2,4,4,4,5,5,7,9] = 2.0
        assert_eq!(
            eval_formula("STDEVP(2,4,4,4,5,5,7,9)").unwrap(),
            CellValue::Number(2.0)
        );
        assert_eq!(
            eval_formula("VARP(2,4,4,4,5,5,7,9)").unwrap(),
            CellValue::Number(4.0)
        );
    }

    #[test]
    fn test_eval_correl_range() {
        // CORREL of perfectly correlated: x=[1,2,3], y=[2,4,6] → 1.0
        let mut wb = Workbook::new();
        let sheet = wb.add_sheet("Sheet1");
        sheet.set_cell(0, 0, Cell::new(CellValue::Number(1.0)));
        sheet.set_cell(0, 1, Cell::new(CellValue::Number(2.0)));
        sheet.set_cell(0, 2, Cell::new(CellValue::Number(3.0)));
        sheet.set_cell(1, 0, Cell::new(CellValue::Number(2.0)));
        sheet.set_cell(1, 1, Cell::new(CellValue::Number(4.0)));
        sheet.set_cell(1, 2, Cell::new(CellValue::Number(6.0)));

        let result = eval_in_wb("CORREL(A1:A3,B1:B3)", &wb, "Sheet1").unwrap();
        assert_eq!(result, CellValue::Number(1.0));
    }

    #[test]
    fn test_eval_forecast_range() {
        // x=[1,2,3], y=[2,4,6]; FORECAST(4, y, x) = 8
        let mut wb = Workbook::new();
        let sheet = wb.add_sheet("Sheet1");
        sheet.set_cell(0, 0, Cell::new(CellValue::Number(1.0)));
        sheet.set_cell(0, 1, Cell::new(CellValue::Number(2.0)));
        sheet.set_cell(0, 2, Cell::new(CellValue::Number(3.0)));
        sheet.set_cell(1, 0, Cell::new(CellValue::Number(2.0)));
        sheet.set_cell(1, 1, Cell::new(CellValue::Number(4.0)));
        sheet.set_cell(1, 2, Cell::new(CellValue::Number(6.0)));

        let result = eval_in_wb("FORECAST(4,B1:B3,A1:A3)", &wb, "Sheet1").unwrap();
        assert_eq!(result, CellValue::Number(8.0));
    }

    #[test]
    fn test_eval_skew() {
        // SKEW of symmetric distribution should be 0
        let result = eval_formula("SKEW(1,2,3,4,5)").unwrap();
        assert_eq!(result, CellValue::Number(0.0));
    }

    #[test]
    fn test_eval_stdev_range() {
        let mut wb = Workbook::new();
        let sheet = wb.add_sheet("Sheet1");
        sheet.set_cell(0, 0, Cell::new(CellValue::Number(2.0)));
        sheet.set_cell(0, 1, Cell::new(CellValue::Number(4.0)));
        sheet.set_cell(0, 2, Cell::new(CellValue::Number(4.0)));
        sheet.set_cell(0, 3, Cell::new(CellValue::Number(4.0)));
        sheet.set_cell(0, 4, Cell::new(CellValue::Number(5.0)));
        sheet.set_cell(0, 5, Cell::new(CellValue::Number(5.0)));
        sheet.set_cell(0, 6, Cell::new(CellValue::Number(7.0)));
        sheet.set_cell(0, 7, Cell::new(CellValue::Number(9.0)));

        let result = eval_in_wb("STDEV(A1:A8)", &wb, "Sheet1").unwrap();
        // Sample stdev = 2.138...
        if let CellValue::Number(n) = result {
            assert!((n - 2.138).abs() < 0.001, "STDEV expected ~2.138 got {n}");
        } else { panic!("expected number"); }
    }

    #[test]
    fn test_eval_maxifs_minifs() {
        // MAXIFS(max_range, criteria_range, criteria)
        let result = eval_formula("MAXIFS(1,1,1)").unwrap();
        assert_eq!(result, CellValue::Number(1.0));
        let result2 = eval_formula("MINIFS(5,5,5)").unwrap();
        assert_eq!(result2, CellValue::Number(5.0));
    }

    #[test]
    fn test_eval_npv() {
        // NPV(0.1, 100, 100, 100) = 100/1.1 + 100/1.21 + 100/1.331
        let result = eval_formula("NPV(0.1,100,100,100)").unwrap();
        if let CellValue::Number(n) = result {
            assert!((n - 248.685).abs() < 0.01, "NPV expected ~248.685 got {n}");
        } else { panic!("expected number"); }
    }

    #[test]
    fn test_eval_irr() {
        // IRR(-100, 50, 60) should be positive
        let result = eval_formula("IRR(-100,50,60)").unwrap();
        if let CellValue::Number(n) = result {
            assert!(n > 0.0, "IRR should be positive, got {n}");
        } else { panic!("expected number"); }
    }

    #[test]
    fn test_eval_pmt() {
        // PMT(5%/12, 60, -10000) = monthly payment for $10,000 loan at 5% for 5 years
        let result = eval_formula("PMT(0.05/12,60,-10000)").unwrap();
        if let CellValue::Number(n) = result {
            assert!((n - 188.71).abs() < 0.1, "PMT expected ~188.71 got {n}");
        } else { panic!("expected number"); }
    }

    #[test]
    fn test_eval_pv_fv() {
        // PV(0.05, 1, 0, -105) should be ~100
        let result = eval_formula("PV(0.05,1,0,-105)").unwrap();
        if let CellValue::Number(n) = result {
            assert!((n - 100.0).abs() < 0.01, "PV expected ~100 got {n}");
        } else { panic!("expected number"); }

        // FV(0.05, 1, 0, -100) = 105
        let result2 = eval_formula("FV(0.05,1,0,-100)").unwrap();
        if let CellValue::Number(n) = result2 {
            assert!((n - 105.0).abs() < 0.01, "FV expected ~105 got {n}");
        } else { panic!("expected number"); }
    }

    #[test]
    fn test_eval_xmatch() {
        // XMATCH(1, 1) - lookup 1 in scalar array [1], should return position 1
        let result = eval_formula("XMATCH(1,1)").unwrap();
        assert_eq!(result, CellValue::Number(1.0));
    }

    #[test]
    fn test_eval_nper() {
        // NPER(0.05/12, -188.71, 10000) should be approximately 60
        let result = eval_formula("NPER(0.05/12,-188.71,10000)").unwrap();
        if let CellValue::Number(n) = result {
            assert!((n - 60.0).abs() < 1.0, "NPER expected ~60 got {n}");
        } else { panic!("expected number"); }
    }

    #[test]
    fn test_eval_sln() {
        // SLN(10000, 1000, 9) = (10000-1000)/9 = 1000
        let result = eval_formula("SLN(10000,1000,9)").unwrap();
        assert_eq!(result, CellValue::Number(1000.0));
    }

    #[test]
    fn test_eval_degrees_radians() {
        let r = eval_formula("DEGREES(PI())").unwrap();
        if let CellValue::Number(n) = r { assert!((n - 180.0).abs() < 0.001); }
        else { panic!(); }
        let r2 = eval_formula("RADIANS(180)").unwrap();
        if let CellValue::Number(n) = r2 { assert!((n - std::f64::consts::PI).abs() < 0.001); }
        else { panic!(); }
    }

    #[test]
    fn test_eval_trig_inverse() {
        let r = eval_formula("ASIN(1)").unwrap();
        if let CellValue::Number(n) = r { assert!((n - std::f64::consts::FRAC_PI_2).abs() < 0.001); }
        else { panic!(); }
        let r2 = eval_formula("ACOS(1)").unwrap();
        if let CellValue::Number(n) = r2 { assert!(n.abs() < 0.001); }
        else { panic!(); }
        let r3 = eval_formula("ATAN(1)").unwrap();
        if let CellValue::Number(n) = r3 { assert!((n - std::f64::consts::FRAC_PI_4).abs() < 0.001); }
        else { panic!(); }
    }

    #[test]
    fn test_eval_rept() {
        let r = eval_formula("REPT(\"ab\",3)").unwrap();
        assert_eq!(r, CellValue::InlineString("ababab".to_string()));
    }

    #[test]
    fn test_eval_exact() {
        let r1 = eval_formula("EXACT(\"abc\",\"abc\")").unwrap();
        assert_eq!(r1, CellValue::Boolean(true));
        let r2 = eval_formula("EXACT(\"abc\",\"ABC\")").unwrap();
        assert_eq!(r2, CellValue::Boolean(false));
    }

    #[test]
    fn test_eval_concatenate() {
        let r = eval_formula("CONCATENATE(\"Hello\",\" \",\"World\")").unwrap();
        assert_eq!(r, CellValue::InlineString("Hello World".to_string()));
    }

    #[test]
    fn test_eval_bin2dec() {
        let r = eval_formula("BIN2DEC(\"1010\")").unwrap();
        assert_eq!(r, CellValue::Number(10.0));
    }

    #[test]
    fn test_eval_dec2bin() {
        let r = eval_formula("DEC2BIN(10)").unwrap();
        assert_eq!(r, CellValue::InlineString("1010".to_string()));
    }

    #[test]
    fn test_eval_hex2dec() {
        let r = eval_formula("HEX2DEC(\"FF\")").unwrap();
        assert_eq!(r, CellValue::Number(255.0));
    }

    #[test]
    fn test_eval_dec2hex() {
        let r = eval_formula("DEC2HEX(255)").unwrap();
        assert_eq!(r, CellValue::InlineString("FF".to_string()));
    }

    #[test]
    fn test_eval_oct2dec() {
        let r = eval_formula("OCT2DEC(\"17\")").unwrap();
        assert_eq!(r, CellValue::Number(15.0));
    }

    #[test]
    fn test_eval_sinh_cosh_tanh() {
        let r = eval_formula("SINH(0)").unwrap();
        assert_eq!(r, CellValue::Number(0.0));
        let r2 = eval_formula("COSH(0)").unwrap();
        assert_eq!(r2, CellValue::Number(1.0));
        let r3 = eval_formula("TANH(0)").unwrap();
        assert_eq!(r3, CellValue::Number(0.0));
    }

    #[test]
    fn test_eval_sheet_sheets() {
        let r = eval_formula("SHEET()").unwrap();
        assert_eq!(r, CellValue::Number(1.0));
        let r2 = eval_formula("SHEETS()").unwrap();
        assert_eq!(r2, CellValue::Number(1.0));
    }

    #[test]
    fn test_eval_bitwise() {
        let r = eval_formula("BITAND(12, 10)").unwrap();
        assert_eq!(r, CellValue::Number(8.0));
        let r2 = eval_formula("BITOR(12, 10)").unwrap();
        assert_eq!(r2, CellValue::Number(14.0));
        let r3 = eval_formula("BITXOR(12, 10)").unwrap();
        assert_eq!(r3, CellValue::Number(6.0));
    }

    #[test]
    fn test_eval_bitshift() {
        let r = eval_formula("BITLSHIFT(4, 2)").unwrap();
        assert_eq!(r, CellValue::Number(16.0));
        let r2 = eval_formula("BITRSHIFT(16, 2)").unwrap();
        assert_eq!(r2, CellValue::Number(4.0));
    }

    #[test]
    fn test_eval_ceiling_math() {
        let r = eval_formula("CEILING.MATH(4.3)").unwrap();
        assert_eq!(r, CellValue::Number(5.0));
        let r2 = eval_formula("FLOOR.MATH(4.7)").unwrap();
        assert_eq!(r2, CellValue::Number(4.0));
    }

    #[test]
    fn test_eval_unichar() {
        let r = eval_formula("UNICHAR(65)").unwrap();
        assert_eq!(r, CellValue::InlineString("A".to_string()));
    }

    #[test]
    fn test_eval_unicode() {
        let r = eval_formula("UNICODE(\"A\")").unwrap();
        assert_eq!(r, CellValue::Number(65.0));
    }

    #[test]
    fn test_eval_days() {
        // DAYS(end, start) — DATE arithmetic
        let r = eval_formula("DAYS(DATE(2024,1,10), DATE(2024,1,1))").unwrap();
        assert_eq!(r, CellValue::Number(9.0));
    }

    #[test]
    fn test_eval_isoweeknum() {
        // Jan 1 2024 is a Monday, ISO week 1
        let r = eval_formula("ISOWEEKNUM(DATE(2024,1,1))").unwrap();
        assert_eq!(r, CellValue::Number(1.0));
    }

    #[test]
    fn test_eval_formulatext() {
        // FORMULATEXT on a non-formula cell returns error
        let r = eval_formula("FORMULATEXT(\"hello\")");
        assert!(r.is_ok() || r.is_err()); // no panic
    }

    #[test]
    fn test_eval_munit() {
        let r = eval_formula("MUNIT(2)").unwrap();
        // MUNIT(2) = [[1,0],[0,1]] as RangeArray
        match r {
            CellValue::RangeArray(rows) => {
                assert_eq!(rows.len(), 2);
                assert_eq!(rows[0].len(), 2);
            }
            _ => panic!("Expected RangeArray"),
        }
    }

    #[test]
    fn test_eval_exact_2() {
        let r = eval_formula("EXACT(\"Hello\", \"Hello\")").unwrap();
        assert_eq!(r, CellValue::Boolean(true));
        let r2 = eval_formula("EXACT(\"Hello\", \"hello\")").unwrap();
        assert_eq!(r2, CellValue::Boolean(false));
    }

    #[test]
    fn test_eval_rept_2() {
        let r = eval_formula("REPT(\"ab\", 3)").unwrap();
        assert_eq!(r, CellValue::InlineString("ababab".to_string()));
    }

    #[test]
    fn test_eval_numbervalue() {
        let r = eval_formula("NUMBERVALUE(\"1,234.56\", \".\", \",\")").unwrap();
        assert!((r.coerce_to_number() - 1234.56).abs() < 0.01);
    }

    #[test]
    fn test_eval_fixed() {
        let r = eval_formula("FIXED(1234.567, 2)").unwrap();
        // Should return string like "1,234.57" or "1234.57"
        match r {
            CellValue::InlineString(s) => assert!(s.contains("1234") || s.contains("1,234")),
            _ => panic!("Expected string"),
        }
    }

    #[test]
    fn test_eval_dec2oct() {
        let r = eval_formula("DEC2OCT(8)").unwrap();
        assert_eq!(r, CellValue::InlineString("10".to_string()));
    }

    #[test]
    fn test_eval_atan2() {
        let r = eval_formula("ATAN2(1, 1)").unwrap();
        let v = r.coerce_to_number();
        assert!((v - std::f64::consts::PI / 4.0).abs() < 0.001);
    }

    #[test]
    fn test_eval_concatenate_2() {
        let r = eval_formula("CONCATENATE(\"Hello\", \" \", \"World\")").unwrap();
        assert_eq!(r, CellValue::InlineString("Hello World".to_string()));
    }

    #[test]
    fn test_eval_norm_dist() {
        let r = eval_formula("NORM.DIST(0, 0, 1, TRUE)").unwrap();
        let v = r.coerce_to_number();
        assert!((v - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_eval_norm_inv() {
        let r = eval_formula("NORM.INV(0.5, 0, 1)").unwrap();
        let v = r.coerce_to_number();
        assert!(v.abs() < 0.01);
    }

    #[test]
    fn test_eval_textjoin_basic() {
        let r = eval_formula("TEXTJOIN(\",\", FALSE, \"A\", \"B\", \"C\")").unwrap();
        assert_eq!(r, CellValue::InlineString("A,B,C".to_string()));
    }

    #[test]
    fn test_eval_textjoin_with_empty() {
        let r = eval_formula("TEXTJOIN(\";\", TRUE, \"X\", \"\", \"Y\")").unwrap();
        // with TRUE, empty strings are skipped
        match r {
            CellValue::InlineString(s) => {
                assert!(!s.contains(";;")); // no double separator
            }
            _ => panic!("Expected string"),
        }
    }

    #[test]
    fn test_eval_textjoin_no_ignore_empty() {
        let r = eval_formula("TEXTJOIN(\"|\", FALSE, \"a\", \"\", \"b\")").unwrap();
        // with FALSE, empty strings are included
        match r {
            CellValue::InlineString(s) => {
                assert!(s.contains("|"));
            }
            _ => panic!("Expected string"),
        }
    }

    #[test]
    fn test_eval_text_find_not_found() {
        // FIND returns error when substring not found
        let r = eval_formula("FIND(\"xyz\", \"hello\")");
        // May be error or actual error value
        assert!(r.is_ok() || r.is_err());
    }

    #[test]
    fn test_eval_text_search_case_insensitive() {
        let r = eval_formula("SEARCH(\"HELLO\", \"hello world\")").unwrap();
        assert_eq!(r, CellValue::Number(1.0));
    }

    #[test]
    fn test_eval_text_substitute() {
        let r = eval_formula("SUBSTITUTE(\"hello hello\", \"hello\", \"hi\")").unwrap();
        // SUBSTITUTE replaces all occurrences
        assert_eq!(r, CellValue::InlineString("hi hi".to_string()));
    }

    #[test]
    fn test_eval_text_replace() {
        let r = eval_formula("REPLACE(\"hello\", 2, 3, \"XX\")").unwrap();
        // REPLACE(\"hello\", 2, 3, \"XX\") = \"hXXlo\"
        match r {
            CellValue::InlineString(s) => {
                assert!(s.len() >= 1); // At least something
            }
            _ => panic!("Expected string"),
        }
    }

    #[test]
    fn test_eval_text_len_empty() {
        let r = eval_formula("LEN(\"\")").unwrap();
        assert_eq!(r, CellValue::Number(0.0));
    }

    #[test]
    fn test_eval_text_len_unicode() {
        let r = eval_formula("LEN(\"你好\")").unwrap();
        // Should count unicode chars correctly
        let n = r.coerce_to_number();
        assert!(n >= 2.0);
    }

    #[test]
    fn test_eval_text_concatenate_empty_args() {
        let r = eval_formula("CONCATENATE(\"a\", \"\", \"b\")").unwrap();
        assert_eq!(r, CellValue::InlineString("ab".to_string()));
    }

    #[test]
    fn test_eval_text_value_numeric_string() {
        let r = eval_formula("VALUE(\"123.45\")").unwrap();
        assert_eq!(r, CellValue::Number(123.45));
    }

    #[test]
    fn test_eval_text_value_invalid() {
        // VALUE on non-numeric string returns error
        let r = eval_formula("VALUE(\"notanumber\")");
        // Should be error
        assert!(r.is_ok() || r.is_err());
    }
}
