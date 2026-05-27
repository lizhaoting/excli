use crate::token::Token;

/// Lexer/tokenizer for Excel formulas.
pub struct Lexer {
    chars: Vec<char>,
    pos: usize,
}

impl Lexer {
    pub fn new(input: &str) -> Self {
        // Strip leading '=' if present
        let input = input.strip_prefix('=').unwrap_or(input);
        Lexer {
            chars: input.chars().collect(),
            pos: 0,
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn peek_ahead(&self, offset: usize) -> Option<char> {
        self.chars.get(self.pos + offset).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let ch = self.chars.get(self.pos).copied();
        self.pos += 1;
        ch
    }

    fn skip_whitespace(&mut self) {
        while let Some(ch) = self.peek() {
            if ch.is_whitespace() {
                self.advance();
            } else {
                break;
            }
        }
    }

    fn read_number(&mut self, first: char) -> Token {
        let mut num_str = String::from(first);
        while let Some(ch) = self.peek() {
            if ch.is_ascii_digit() || ch == '.' || ch == 'e' || ch == 'E' || ch == '+' || ch == '-' {
                if (ch == '+' || ch == '-') && !num_str.ends_with(|c: char| c == 'e' || c == 'E') {
                    break;
                }
                num_str.push(ch);
                self.advance();
            } else {
                break;
            }
        }
        if let Ok(n) = num_str.parse::<f64>() {
            Token::Number(n)
        } else {
            // Fallback: treat as string
            Token::String(num_str)
        }
    }

    fn read_string(&mut self) -> Token {
        let mut s = String::new();
        self.advance(); // skip opening quote
        while let Some(ch) = self.advance() {
            if ch == '"' {
                if self.peek() == Some('"') {
                    // Escaped quote
                    s.push('"');
                    self.advance();
                } else {
                    break; // closing quote
                }
            } else {
                s.push(ch);
            }
        }
        Token::String(s)
    }

    fn read_identifier(&mut self, first: char) -> Token {
        let mut s = String::from(first);
        while let Some(ch) = self.peek() {
            // Include '$' to handle absolute references like $A$1
            if ch.is_alphanumeric() || ch == '_' || ch == '.' || ch == '$' {
                s.push(ch);
                self.advance();
            } else {
                break;
            }
        }

        // Check for sheet-qualified reference: SheetName!CellRef
        if self.peek() == Some('!') {
            let save_pos = self.pos;
            self.advance(); // consume '!'
            let mut cell_part = String::new();
            while let Some(c) = self.peek() {
                if c.is_alphanumeric() || c == '$' {
                    cell_part.push(c);
                    self.advance();
                } else {
                    break;
                }
            }
            if !cell_part.is_empty() && self.is_cell_ref(&cell_part) {
                let full = format!("{}!{}", s, cell_part);
                return Token::CellRef(full);
            }
            // Not a sheet-qualified ref — restore position and continue
            self.pos = save_pos;
        }

        // Check if followed by '(' => function
        self.skip_whitespace();
        if self.peek() == Some('(') {
            Token::FunctionName(s)
        } else {
            match s.to_uppercase().as_str() {
                "TRUE" => Token::Boolean(true),
                "FALSE" => Token::Boolean(false),
                _ => {
                    // Check if it's a cell reference (letter(s) followed by digits)
                    if self.is_cell_ref(&s) {
                        Token::CellRef(s)
                    } else {
                        Token::Identifier(s)
                    }
                }
            }
        }
    }

    fn is_cell_ref(&self, s: &str) -> bool {
        let s = s.to_uppercase();
        let mut chars = s.chars().peekable();

        // Skip leading $
        let _col_abs = chars.peek() == Some(&'$');
        if _col_abs {
            chars.next();
        }

        // Must have at least one letter
        let mut has_letter = false;
        while chars.peek().map_or(false, |c| c.is_ascii_uppercase()) {
            chars.next();
            has_letter = true;
        }
        if !has_letter {
            return false;
        }

        // Skip row $
        let _row_abs = chars.peek() == Some(&'$');
        if _row_abs {
            chars.next();
        }

        // Must have at least one digit
        let mut has_digit = false;
        while chars.peek().map_or(false, |c| c.is_ascii_digit()) {
            chars.next();
            has_digit = true;
        }

        has_digit && chars.peek().is_none()
    }

    /// Tokenize the entire input.
    pub fn tokenize(&mut self) -> Vec<Token> {
        let mut tokens = Vec::new();

        while self.pos < self.chars.len() {
            self.skip_whitespace();
            if self.pos >= self.chars.len() {
                break;
            }

            let ch = self.peek().unwrap();

            let token = match ch {
                '"' => self.read_string(),
                '0'..='9' => {
                    self.advance();
                    self.read_number(ch)
                }
                '.' => {
                    if self.peek_ahead(1).map_or(false, |c| c.is_ascii_digit()) {
                        self.advance();
                        self.read_number('.')
                    } else {
                        self.advance();
                        Token::Unknown('.')
                    }
                }
                '+' => { self.advance(); Token::Plus }
                '-' => { self.advance(); Token::Minus }
                '*' => { self.advance(); Token::Star }
                '/' => { self.advance(); Token::Slash }
                '^' => { self.advance(); Token::Caret }
                '%' => { self.advance(); Token::Percent }
                '&' => { self.advance(); Token::Ampersand }
                '(' => { self.advance(); Token::LeftParen }
                ')' => { self.advance(); Token::RightParen }
                '{' => { self.advance(); Token::LeftBrace }
                '}' => { self.advance(); Token::RightBrace }
                ',' => { self.advance(); Token::Comma }
                ';' => { self.advance(); Token::Semicolon }
                ':' => { self.advance(); Token::Colon }
                '=' => { self.advance(); Token::Equal }
                '<' => {
                    self.advance();
                    if self.peek() == Some('=') { self.advance(); Token::LessEqual }
                    else if self.peek() == Some('>') { self.advance(); Token::NotEqual }
                    else { Token::LessThan }
                }
                '>' => {
                    self.advance();
                    if self.peek() == Some('=') { self.advance(); Token::GreaterEqual }
                    else { Token::GreaterThan }
                }
                '$' | 'A'..='Z' | 'a'..='z' | '_' | '\\' => {
                    self.advance();
                    self.read_identifier(ch)
                }
                '#' => {
                    // Error literal like #REF!, #N/A, etc.
                    self.advance();
                    let mut err = String::from('#');
                    while let Some(c) = self.peek() {
                        if c.is_alphanumeric() || c == '/' || c == '?' || c == '!' {
                            err.push(c);
                            self.advance();
                        } else {
                            break;
                        }
                    }
                    Token::Error(err)
                }
                _ => {
                    self.advance();
                    Token::Unknown(ch)
                }
            };

            tokens.push(token);
        }

        tokens.push(Token::Eof);
        tokens
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lex_numbers() {
        let mut lexer = Lexer::new("123");
        let tokens = lexer.tokenize();
        assert_eq!(tokens[0], Token::Number(123.0));
    }

    #[test]
    fn test_lex_simple_formula() {
        let mut lexer = Lexer::new("1+2*3");
        let tokens = lexer.tokenize();
        assert_eq!(tokens[0], Token::Number(1.0));
        assert_eq!(tokens[1], Token::Plus);
        assert_eq!(tokens[2], Token::Number(2.0));
        assert_eq!(tokens[3], Token::Star);
        assert_eq!(tokens[4], Token::Number(3.0));
    }

    #[test]
    fn test_lex_function_call() {
        let mut lexer = Lexer::new("SUM(A1,B1)");
        let tokens = lexer.tokenize();
        assert_eq!(tokens[0], Token::FunctionName("SUM".to_string()));
        assert_eq!(tokens[1], Token::LeftParen);
        assert_eq!(tokens[2], Token::CellRef("A1".to_string()));
        assert_eq!(tokens[3], Token::Comma);
        assert_eq!(tokens[4], Token::CellRef("B1".to_string()));
        assert_eq!(tokens[5], Token::RightParen);
    }

    #[test]
    fn test_lex_string() {
        let mut lexer = Lexer::new("\"Hello, World!\"");
        let tokens = lexer.tokenize();
        assert_eq!(tokens[0], Token::String("Hello, World!".to_string()));
    }

    #[test]
    fn test_lex_boolean() {
        let mut lexer = Lexer::new("TRUE");
        let tokens = lexer.tokenize();
        assert_eq!(tokens[0], Token::Boolean(true));
    }

    #[test]
    fn test_lex_comparison() {
        let mut lexer = Lexer::new("A1>=10");
        let tokens = lexer.tokenize();
        assert_eq!(tokens[0], Token::CellRef("A1".to_string()));
        assert_eq!(tokens[1], Token::GreaterEqual);
        assert_eq!(tokens[2], Token::Number(10.0));
    }

    #[test]
    fn test_lex_range() {
        let mut lexer = Lexer::new("SUM(A1:B10)");
        let tokens = lexer.tokenize();
        assert_eq!(tokens[0], Token::FunctionName("SUM".to_string()));
        assert_eq!(tokens[1], Token::LeftParen);
        assert_eq!(tokens[2], Token::CellRef("A1".to_string()));
        assert_eq!(tokens[3], Token::Colon);
        assert_eq!(tokens[4], Token::CellRef("B10".to_string()));
    }

    #[test]
    fn test_lex_error_literal() {
        let mut lexer = Lexer::new("#REF!");
        let tokens = lexer.tokenize();
        assert_eq!(tokens[0], Token::Error("#REF!".to_string()));
    }

    #[test]
    fn test_lex_concatenation() {
        let mut lexer = Lexer::new("\"Hello\" & \"World\"");
        let tokens = lexer.tokenize();
        assert_eq!(tokens[0], Token::String("Hello".to_string()));
        assert_eq!(tokens[1], Token::Ampersand);
        assert_eq!(tokens[2], Token::String("World".to_string()));
    }

    #[test]
    fn test_lex_percent() {
        let mut lexer = Lexer::new("50%");
        let tokens = lexer.tokenize();
        assert_eq!(tokens[0], Token::Number(50.0));
        assert_eq!(tokens[1], Token::Percent);
    }

    #[test]
    fn test_lex_leading_equals_stripped() {
        // "=1+2" should tokenize as if it were "1+2"
        let mut lexer = Lexer::new("=1+2");
        let tokens = lexer.tokenize();
        assert_eq!(tokens[0], Token::Number(1.0));
        assert_eq!(tokens[1], Token::Plus);
        assert_eq!(tokens[2], Token::Number(2.0));
    }

    #[test]
    fn test_lex_not_equal_operator() {
        let mut lexer = Lexer::new("A1<>B1");
        let tokens = lexer.tokenize();
        assert_eq!(tokens[0], Token::CellRef("A1".to_string()));
        assert_eq!(tokens[1], Token::NotEqual);
        assert_eq!(tokens[2], Token::CellRef("B1".to_string()));
    }

    #[test]
    fn test_lex_absolute_cell_ref() {
        let mut lexer = Lexer::new("$A$1");
        let tokens = lexer.tokenize();
        // $A$1 should tokenize as a single CellRef token
        assert_eq!(tokens[0], Token::CellRef("$A$1".to_string()));
    }

    #[test]
    fn test_lex_decimal_number() {
        let mut lexer = Lexer::new(".5");
        let tokens = lexer.tokenize();
        assert_eq!(tokens[0], Token::Number(0.5));
    }

    #[test]
    fn test_lex_scientific_notation() {
        let mut lexer = Lexer::new("1.5e3");
        let tokens = lexer.tokenize();
        assert_eq!(tokens[0], Token::Number(1500.0));
    }

    #[test]
    fn test_lex_negative_number_tokens() {
        // -5 lexes as Minus then Number, not as a single negative number
        let mut lexer = Lexer::new("-5");
        let tokens = lexer.tokenize();
        assert_eq!(tokens[0], Token::Minus);
        assert_eq!(tokens[1], Token::Number(5.0));
    }

    #[test]
    fn test_lex_string_escaped_quote() {
        let mut lexer = Lexer::new("\"He said \"\"hello\"\"\"");
        let tokens = lexer.tokenize();
        assert_eq!(tokens[0], Token::String("He said \"hello\"".to_string()));
    }

    #[test]
    fn test_lex_all_comparison_operators() {
        // Note: bare "=" is stripped as a formula prefix by the lexer → Eof.
        // Test the remaining comparison operators in an expression context.
        let ops = [
            ("A1<B1", Token::LessThan),
            ("A1<=B1", Token::LessEqual),
            ("A1>B1", Token::GreaterThan),
            ("A1>=B1", Token::GreaterEqual),
            ("A1<>B1", Token::NotEqual),
        ];
        for (input, expected) in &ops {
            let mut lexer = Lexer::new(input);
            let tokens = lexer.tokenize();
            assert_eq!(&tokens[1], expected, "Failed for input: {}", input);
        }
    }

    #[test]
    fn test_lex_na_error() {
        let mut lexer = Lexer::new("#N/A");
        let tokens = lexer.tokenize();
        assert_eq!(tokens[0], Token::Error("#N/A".to_string()));
    }
}

