use std::error::Error as StdError;
use std::fmt::{Display, Formatter};

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    Parse(ParseError),
    InvalidEncoding(std::str::Utf8Error),
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(err) => write!(f, "I/O error: {err}"),
            Self::Parse(err) => write!(f, "{err}"),
            Self::InvalidEncoding(err) => write!(f, "invalid UTF-8 input: {err}"),
        }
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            Self::Parse(err) => Some(err),
            Self::InvalidEncoding(err) => Some(err),
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<ParseError> for Error {
    fn from(value: ParseError) -> Self {
        Self::Parse(value)
    }
}

impl From<std::str::Utf8Error> for Error {
    fn from(value: std::str::Utf8Error) -> Self {
        Self::InvalidEncoding(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub line: u32,
    pub column: u32,
    pub kind: ParseErrorKind,
}

impl ParseError {
    pub(crate) fn new(line: u32, column: u32, kind: ParseErrorKind) -> Self {
        Self { line, column, kind }
    }
}

impl Display for ParseError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "parse error at line {}, column {}: {}",
            self.line, self.column, self.kind
        )
    }
}

impl StdError for ParseError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseErrorKind {
    InvalidSyntax,
    MissingKey,
    InvalidKey,
    UnterminatedQuote,
}

impl Display for ParseErrorKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidSyntax => write!(f, "invalid syntax"),
            Self::MissingKey => write!(f, "missing key"),
            Self::InvalidKey => write!(f, "invalid key"),
            Self::UnterminatedQuote => write!(f, "unterminated quote"),
        }
    }
}
