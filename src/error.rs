use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub line: usize,
    pub column: usize,
}

impl Span {
    pub fn new(line: usize, column: usize) -> Self {
        Self { line, column }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorKind {
    Lex,
    Parse,
    Type,
    Borrow,
    Runtime,
}

#[derive(Debug, Clone)]
pub struct MiniError {
    pub kind: ErrorKind,
    pub message: String,
    pub span: Option<Span>,
}

pub type Result<T> = std::result::Result<T, MiniError>;

impl MiniError {
    pub fn lex(message: impl Into<String>, span: Span) -> Self {
        Self {
            kind: ErrorKind::Lex,
            message: message.into(),
            span: Some(span),
        }
    }

    pub fn parse(message: impl Into<String>, span: Span) -> Self {
        Self {
            kind: ErrorKind::Parse,
            message: message.into(),
            span: Some(span),
        }
    }

    pub fn type_error(message: impl Into<String>, span: Option<Span>) -> Self {
        Self {
            kind: ErrorKind::Type,
            message: message.into(),
            span,
        }
    }

    pub fn borrow(message: impl Into<String>, span: Option<Span>) -> Self {
        Self {
            kind: ErrorKind::Borrow,
            message: message.into(),
            span,
        }
    }

    pub fn runtime(message: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::Runtime,
            message: message.into(),
            span: None,
        }
    }

    fn prefix(&self) -> &'static str {
        match self.kind {
            ErrorKind::Lex => "lex error",
            ErrorKind::Parse => "parse error",
            ErrorKind::Type => "type error",
            ErrorKind::Borrow => "borrowcheck error",
            ErrorKind::Runtime => "runtime error",
        }
    }

    pub fn render_with_source(&self, source: &str) -> String {
        let mut rendered = self.to_string();
        if let Some(span) = self.span {
            if let Some(line) = source.lines().nth(span.line.saturating_sub(1)) {
                rendered.push('\n');
                rendered.push_str(line);
                rendered.push('\n');
                rendered.push_str(&" ".repeat(span.column.saturating_sub(1)));
                rendered.push('^');
            }
        }
        rendered
    }
}

impl fmt::Display for MiniError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.span {
            Some(span) => write!(
                f,
                "{} at {}:{}: {}",
                self.prefix(),
                span.line,
                span.column,
                self.message
            ),
            None => write!(f, "{}: {}", self.prefix(), self.message),
        }
    }
}

impl std::error::Error for MiniError {}
