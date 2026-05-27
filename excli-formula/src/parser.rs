use excli_core::cell::ExcelError;
use excli_core::error::{ExcliError, ExcliResult};
use excli_core::formula::{AstNode, BinOp, Constant, FormulaAst, UnaryOp};
use excli_core::reference::{CellReference, RangeReference, Reference};

use crate::lexer::Lexer;
use crate::token::Token;

/// Pratt parser for Excel formulas.
pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    pub fn new(input: &str) -> Self {
        let mut lexer = Lexer::new(input);
        let tokens = lexer.tokenize();
        Parser { tokens, pos: 0 }
    }

    pub fn parse(&mut self) -> ExcliResult<FormulaAst> {
        let node = self.parse_expression(0)?;
        Ok(FormulaAst::new(node))
    }

    fn peek(&self) -> &Token {
        self.tokens.get(self.pos).unwrap_or(&Token::Eof)
    }

    fn advance(&mut self) -> &Token {
        let _token = self.peek();
        self.pos += 1;
        self.tokens.get(self.pos - 1).unwrap_or(&Token::Eof)
    }

    fn expect(&mut self, expected: Token) -> ExcliResult<&Token> {
        let token = self.peek();
        if std::mem::discriminant(token) != std::mem::discriminant(&expected) {
            return Err(ExcliError::formula_parse_error(format!(
                "Expected {:?}, got {:?}",
                expected, token
            )));
        }
        Ok(self.advance())
    }

    fn parse_expression(&mut self, min_precedence: u8) -> ExcliResult<AstNode> {
        let mut left = self.parse_prefix()?;

        while self.peek().is_binary_op() && self.peek().precedence() >= min_precedence {
            let op_token = self.advance().clone();
            let op_prec = op_token.precedence();
            let next_min = if op_token.is_left_associative() {
                op_prec + 1
            } else {
                op_prec
            };

            // Handle postfix percent
            if matches!(&op_token, Token::Percent) {
                left = AstNode::UnaryOp {
                    op: UnaryOp::Percent,
                    arg: Box::new(left),
                };
                continue;
            }

            let right = self.parse_expression(next_min)?;
            left = AstNode::BinaryOp {
                op: token_to_binop(&op_token)?,
                left: Box::new(left),
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    fn parse_prefix(&mut self) -> ExcliResult<AstNode> {
        match self.advance().clone() {
            Token::Number(n) => Ok(AstNode::Constant(Constant::Number(n))),
            Token::String(s) => Ok(AstNode::Constant(Constant::String(s))),
            Token::Boolean(b) => Ok(AstNode::Constant(Constant::Boolean(b))),
            Token::Error(e) => {
                if let Some(err) = ExcelError::from_str(&e) {
                    Ok(AstNode::Error(err))
                } else {
                    Ok(AstNode::Error(ExcelError::Value))
                }
            }
            Token::Minus => {
                let arg = self.parse_expression(6)?; // Unary minus high precedence
                Ok(AstNode::UnaryOp {
                    op: UnaryOp::Neg,
                    arg: Box::new(arg),
                })
            }
            Token::Plus => {
                let arg = self.parse_expression(6)?;
                Ok(AstNode::UnaryOp {
                    op: UnaryOp::Pos,
                    arg: Box::new(arg),
                })
            }
            Token::LeftParen => {
                let expr = self.parse_expression(0)?;
                self.expect(Token::RightParen)?;
                Ok(AstNode::Paren(Box::new(expr)))
            }
            Token::FunctionName(name) => {
                self.expect(Token::LeftParen)?;
                let mut args = Vec::new();

                if !matches!(self.peek(), Token::RightParen) {
                    args.push(self.parse_expression(0)?);
                    while matches!(self.peek(), Token::Comma | Token::Semicolon) {
                        self.advance();
                        if matches!(self.peek(), Token::RightParen) {
                            // Trailing comma — empty arg
                            args.push(AstNode::Constant(Constant::Empty));
                        } else {
                            args.push(self.parse_expression(0)?);
                        }
                    }
                }

                self.expect(Token::RightParen)?;
                Ok(AstNode::Function { name, args })
            }
            Token::CellRef(start_cell) => {
                // Check if this is the start of a range: A1:B10
                if matches!(self.peek(), Token::Colon) {
                    self.advance(); // skip colon
                    if let Token::CellRef(end_cell) = self.advance().clone() {
                        if let (Some(start), Some(end)) = (
                            CellReference::from_a1(&start_cell),
                            CellReference::from_a1(&end_cell),
                        ) {
                            return Ok(AstNode::Reference(Reference::Range(
                                RangeReference::new(start, end),
                            )));
                        }
                    }
                    return Err(ExcliError::formula_parse_error("Invalid range reference"));
                }

                if let Some(cell_ref) = CellReference::from_a1(&start_cell) {
                    Ok(AstNode::Reference(Reference::Cell(cell_ref)))
                } else {
                    Err(ExcliError::formula_parse_error(format!(
                        "Invalid cell reference: {}",
                        start_cell
                    )))
                }
            }
            Token::Identifier(name) => {
                // Could be a named range or table reference
                Ok(AstNode::Reference(Reference::Named(name)))
            }
            Token::LeftBrace => {
                // Array constant
                let mut rows = Vec::new();
                let mut current_row = Vec::new();
                current_row.push(self.parse_expression(0)?);

                loop {
                    match self.peek() {
                        Token::Comma => {
                            self.advance();
                            current_row.push(self.parse_expression(0)?);
                        }
                        Token::Semicolon => {
                            self.advance();
                            rows.push(std::mem::take(&mut current_row));
                            current_row.push(self.parse_expression(0)?);
                        }
                        Token::RightBrace => {
                            self.advance();
                            rows.push(current_row);
                            break;
                        }
                        _ => {
                            return Err(ExcliError::formula_parse_error(
                                "Expected comma, semicolon, or closing brace in array constant",
                            ));
                        }
                    }
                }
                Ok(AstNode::Array(rows))
            }
            Token::Eof => Ok(AstNode::Constant(Constant::Empty)),
            other => Err(ExcliError::formula_parse_error(format!(
                "Unexpected token: {:?}",
                other
            ))),
        }
    }
}

fn token_to_binop(token: &Token) -> ExcliResult<BinOp> {
    match token {
        Token::Plus => Ok(BinOp::Add),
        Token::Minus => Ok(BinOp::Sub),
        Token::Star => Ok(BinOp::Mul),
        Token::Slash => Ok(BinOp::Div),
        Token::Caret => Ok(BinOp::Pow),
        Token::Ampersand => Ok(BinOp::Concat),
        Token::Equal => Ok(BinOp::Eq),
        Token::NotEqual => Ok(BinOp::Ne),
        Token::LessThan => Ok(BinOp::Lt),
        Token::LessEqual => Ok(BinOp::Le),
        Token::GreaterThan => Ok(BinOp::Gt),
        Token::GreaterEqual => Ok(BinOp::Ge),
        _ => Err(ExcliError::formula_parse_error(format!(
            "Not a binary operator: {:?}",
            token
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_number() {
        let result = Parser::new("42").parse().unwrap();
        assert_eq!(result.root, Box::new(AstNode::Constant(Constant::Number(42.0))));
    }

    #[test]
    fn test_parse_simple_add() {
        let result = Parser::new("1+2").parse().unwrap();
        if let AstNode::BinaryOp { op, left, right } = result.root.as_ref() {
            assert_eq!(*op, BinOp::Add);
            assert_eq!(*left, Box::new(AstNode::Constant(Constant::Number(1.0))));
            assert_eq!(*right, Box::new(AstNode::Constant(Constant::Number(2.0))));
        } else {
            panic!("Expected BinaryOp");
        }
    }

    #[test]
    fn test_parse_operator_precedence() {
        // 1+2*3 should parse as 1+(2*3), not (1+2)*3
        let result = Parser::new("1+2*3").parse().unwrap();
        if let AstNode::BinaryOp { op, left, right } = result.root.as_ref() {
            assert_eq!(*op, BinOp::Add); // Top-level is +
            assert_eq!(*left, Box::new(AstNode::Constant(Constant::Number(1.0))));
            if let AstNode::BinaryOp { op, left, right } = right.as_ref() {
                assert_eq!(*op, BinOp::Mul); // Right subtree is *
                assert_eq!(*left, Box::new(AstNode::Constant(Constant::Number(2.0))));
                assert_eq!(*right, Box::new(AstNode::Constant(Constant::Number(3.0))));
            } else {
                panic!("Expected multiplication in right subtree");
            }
        } else {
            panic!("Expected BinaryOp at root");
        }
    }

    #[test]
    fn test_parse_parens() {
        let result = Parser::new("(1+2)*3").parse().unwrap();
        if let AstNode::BinaryOp { op, left, right } = result.root.as_ref() {
            assert_eq!(*op, BinOp::Mul);
            assert_eq!(*right, Box::new(AstNode::Constant(Constant::Number(3.0))));
            if let AstNode::Paren(inner) = left.as_ref() {
                if let AstNode::BinaryOp { op, .. } = inner.as_ref() {
                    assert_eq!(*op, BinOp::Add);
                } else {
                    panic!("Expected add inside parens");
                }
            } else {
                panic!("Expected Paren");
            }
        } else {
            panic!("Expected BinaryOp");
        }
    }

    #[test]
    fn test_parse_function_call() {
        let result = Parser::new("SUM(1,2,3)").parse().unwrap();
        if let AstNode::Function { name, args } = result.root.as_ref() {
            assert_eq!(name, "SUM");
            assert_eq!(args.len(), 3);
        } else {
            panic!("Expected Function call");
        }
    }

    #[test]
    fn test_parse_cell_reference() {
        let result = Parser::new("A1").parse().unwrap();
        if let AstNode::Reference(Reference::Cell(cell)) = result.root.as_ref() {
            assert_eq!(cell.column, 0);
            assert_eq!(cell.row, 0);
        } else {
            panic!("Expected cell reference");
        }
    }

    #[test]
    fn test_parse_comparison() {
        let result = Parser::new("A1>10").parse().unwrap();
        if let AstNode::BinaryOp { op, .. } = result.root.as_ref() {
            assert_eq!(*op, BinOp::Gt);
        } else {
            panic!("Expected comparison");
        }
    }

    #[test]
    fn test_parse_string_concat() {
        let result = Parser::new("\"Hello\" & \"World\"").parse().unwrap();
        if let AstNode::BinaryOp { op, .. } = result.root.as_ref() {
            assert_eq!(*op, BinOp::Concat);
        } else {
            panic!("Expected concatenation");
        }
    }

    #[test]
    fn test_parse_vlookup() {
        let result = Parser::new("VLOOKUP(A1,B1:C10,2,FALSE)").parse().unwrap();
        if let AstNode::Function { name, args } = result.root.as_ref() {
            assert_eq!(name, "VLOOKUP");
            assert_eq!(args.len(), 4);
        } else {
            panic!("Expected VLOOKUP call");
        }
    }

    #[test]
    fn test_parse_if_function() {
        let result = Parser::new("IF(A1>10,\"Yes\",\"No\")").parse().unwrap();
        assert_eq!(result.functions, vec!["IF".to_string()]);
        assert_eq!(result.references.len(), 1);
    }

    #[test]
    fn test_parse_unary_minus() {
        let result = Parser::new("-5").parse().unwrap();
        if let AstNode::UnaryOp { op, arg } = result.root.as_ref() {
            assert_eq!(*op, UnaryOp::Neg);
            assert_eq!(*arg, Box::new(AstNode::Constant(Constant::Number(5.0))));
        } else {
            panic!("Expected unary minus");
        }
    }

    #[test]
    fn test_parse_exponentiation_right_assoc() {
        // 2^3^2 should parse as 2^(3^2) = 2^9 = 512, not (2^3)^2 = 64
        let result = Parser::new("2^3^2").parse().unwrap();
        if let AstNode::BinaryOp { op, left, right } = result.root.as_ref() {
            assert_eq!(*op, BinOp::Pow);
            assert_eq!(*left, Box::new(AstNode::Constant(Constant::Number(2.0))));
            // Right should be 3^2
            if let AstNode::BinaryOp { op, left: inner_left, right: inner_right } = right.as_ref() {
                assert_eq!(*op, BinOp::Pow);
                assert_eq!(*inner_left, Box::new(AstNode::Constant(Constant::Number(3.0))));
                assert_eq!(*inner_right, Box::new(AstNode::Constant(Constant::Number(2.0))));
            } else {
                panic!("Right-associative: right should be 3^2");
            }
        } else {
            panic!("Expected exponentiation");
        }
    }

    #[test]
    fn test_parse_absolute_cell_reference() {
        let result = Parser::new("$A$1").parse().unwrap();
        if let AstNode::Reference(Reference::Cell(cell)) = result.root.as_ref() {
            assert_eq!(cell.column, 0);
            assert_eq!(cell.row, 0);
            assert!(cell.col_absolute);
            assert!(cell.row_absolute);
        } else {
            panic!("Expected absolute cell reference");
        }
    }

    #[test]
    fn test_parse_mixed_absolute_reference() {
        let result = Parser::new("$B2").parse().unwrap();
        if let AstNode::Reference(Reference::Cell(cell)) = result.root.as_ref() {
            assert_eq!(cell.column, 1); // B
            assert_eq!(cell.row, 1);    // row 2 → 0-based = 1
            assert!(cell.col_absolute);
            assert!(!cell.row_absolute);
        } else {
            panic!("Expected mixed absolute reference");
        }
    }

    #[test]
    fn test_parse_range_reference() {
        let result = Parser::new("A1:C3").parse().unwrap();
        if let AstNode::Reference(Reference::Range(range)) = result.root.as_ref() {
            assert_eq!(range.start.column, 0);
            assert_eq!(range.start.row, 0);
            assert_eq!(range.end.column, 2); // C
            assert_eq!(range.end.row, 2);    // row 3 → 0-based = 2
        } else {
            panic!("Expected range reference, got: {:?}", result.root);
        }
    }

    #[test]
    fn test_parse_sheet_qualified_reference() {
        let result = Parser::new("Sheet1!B2").parse().unwrap();
        if let AstNode::Reference(Reference::Cell(cell)) = result.root.as_ref() {
            assert_eq!(cell.sheet.as_deref(), Some("Sheet1"));
            assert_eq!(cell.column, 1); // B
            assert_eq!(cell.row, 1);    // row 2 → 0-based = 1
        } else {
            panic!("Expected sheet-qualified cell reference");
        }
    }

    #[test]
    fn test_parse_array_constant() {
        let result = Parser::new("{1,2,3}").parse().unwrap();
        if let AstNode::Array(rows) = result.root.as_ref() {
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].len(), 3);
        } else {
            panic!("Expected array constant");
        }
    }

    #[test]
    fn test_parse_2d_array_constant() {
        let result = Parser::new("{1,2;3,4}").parse().unwrap();
        if let AstNode::Array(rows) = result.root.as_ref() {
            assert_eq!(rows.len(), 2);
            assert_eq!(rows[0].len(), 2);
            assert_eq!(rows[1].len(), 2);
        } else {
            panic!("Expected 2D array constant");
        }
    }

    #[test]
    fn test_parse_error_literal_ref() {
        let result = Parser::new("#REF!").parse().unwrap();
        assert!(matches!(
            result.root.as_ref(),
            AstNode::Error(excli_core::cell::ExcelError::Ref)
        ));
    }

    #[test]
    fn test_parse_error_na_literal() {
        let result = Parser::new("#N/A").parse().unwrap();
        assert!(matches!(
            result.root.as_ref(),
            AstNode::Error(excli_core::cell::ExcelError::NA)
        ));
    }

    #[test]
    fn test_parse_nested_functions() {
        let result = Parser::new("MAX(MIN(1,2),3)").parse().unwrap();
        if let AstNode::Function { name, args } = result.root.as_ref() {
            assert_eq!(name, "MAX");
            assert_eq!(args.len(), 2);
            assert!(matches!(&args[0], AstNode::Function { name, .. } if name == "MIN"));
        } else {
            panic!("Expected nested function calls");
        }
    }

    #[test]
    fn test_parse_references_collected_in_ast() {
        let result = Parser::new("SUM(A1,B2:C3)").parse().unwrap();
        // A1 + range B2:C3 should both be collected in references
        assert!(!result.references.is_empty());
        assert!(result.functions.contains(&"SUM".to_string()));
    }

    #[test]
    fn test_parse_not_equal_operator() {
        let result = Parser::new("A1<>B1").parse().unwrap();
        if let AstNode::BinaryOp { op, .. } = result.root.as_ref() {
            assert_eq!(*op, BinOp::Ne);
        } else {
            panic!("Expected not-equal operator");
        }
    }

    #[test]
    fn test_parse_named_range() {
        let result = Parser::new("TaxRate").parse().unwrap();
        if let AstNode::Reference(Reference::Named(name)) = result.root.as_ref() {
            assert_eq!(name, "TaxRate");
        } else {
            panic!("Expected named range reference");
        }
    }
}
