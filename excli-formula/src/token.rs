use std::fmt;

/// Tokens produced by the formula lexer.
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    // Literals
    Number(f64),
    String(String),
    Boolean(bool),
    Error(String),

    // References
    CellRef(String),     // A1, $B$2
    RangeRef(String),    // A1:B10

    // Operators
    Plus,
    Minus,
    Star,
    Slash,
    Caret,        // ^
    Percent,      // %
    Ampersand,    // &
    Equal,
    LessThan,
    GreaterThan,
    LessEqual,
    GreaterEqual,
    NotEqual,

    // Punctuation
    LeftParen,
    RightParen,
    LeftBrace,
    RightBrace,
    Comma,
    Semicolon,
    Colon,

    // Identifiers
    Identifier(String),
    FunctionName(String),

    // Special
    Whitespace,
    Unknown(char),
    Eof,
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Token::Number(n) => write!(f, "{}", n),
            Token::String(s) => write!(f, "\"{}\"", s),
            Token::Boolean(b) => write!(f, "{}", b),
            Token::Error(e) => write!(f, "{}", e),
            Token::CellRef(s) => write!(f, "{}", s),
            Token::RangeRef(s) => write!(f, "{}", s),
            Token::Plus => write!(f, "+"),
            Token::Minus => write!(f, "-"),
            Token::Star => write!(f, "*"),
            Token::Slash => write!(f, "/"),
            Token::Caret => write!(f, "^"),
            Token::Percent => write!(f, "%"),
            Token::Ampersand => write!(f, "&"),
            Token::Equal => write!(f, "="),
            Token::LessThan => write!(f, "<"),
            Token::GreaterThan => write!(f, ">"),
            Token::LessEqual => write!(f, "<="),
            Token::GreaterEqual => write!(f, ">="),
            Token::NotEqual => write!(f, "<>"),
            Token::LeftParen => write!(f, "("),
            Token::RightParen => write!(f, ")"),
            Token::LeftBrace => write!(f, "{{"),
            Token::RightBrace => write!(f, "}}"),
            Token::Comma => write!(f, ","),
            Token::Semicolon => write!(f, ";"),
            Token::Colon => write!(f, ":"),
            Token::Identifier(s) => write!(f, "{}", s),
            Token::FunctionName(s) => write!(f, "{}(", s),
            Token::Whitespace => write!(f, " "),
            Token::Unknown(c) => write!(f, "{}", c),
            Token::Eof => write!(f, "<EOF>"),
        }
    }
}

impl Token {
    /// Check if this token is a binary operator.
    pub fn is_binary_op(&self) -> bool {
        matches!(
            self,
            Token::Plus
                | Token::Minus
                | Token::Star
                | Token::Slash
                | Token::Caret
                | Token::Percent
                | Token::Ampersand
                | Token::Equal
                | Token::LessThan
                | Token::GreaterThan
                | Token::LessEqual
                | Token::GreaterEqual
                | Token::NotEqual
        )
    }

    /// Get operator precedence (higher = tighter binding).
    pub fn precedence(&self) -> u8 {
        match self {
            Token::Caret => 7,
            Token::Percent => 6,
            Token::Star | Token::Slash => 5,
            Token::Plus | Token::Minus => 4,
            Token::Ampersand => 3,
            Token::Equal
            | Token::LessThan
            | Token::GreaterThan
            | Token::LessEqual
            | Token::GreaterEqual
            | Token::NotEqual => 2,
            _ => 0,
        }
    }

    /// Check if this operator is left-associative.
    pub fn is_left_associative(&self) -> bool {
        // Caret (exponentiation) is right-associative in Excel
        !matches!(self, Token::Caret)
    }
}
