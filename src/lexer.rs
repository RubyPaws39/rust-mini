use crate::error::{MiniError, Result, Span};
use crate::token::{Token, TokenKind};

pub struct Lexer<'a> {
    chars: Vec<char>,
    pos: usize,
    line: usize,
    column: usize,
    _source: &'a str,
}

impl<'a> Lexer<'a> {
    pub fn new(source: &'a str) -> Self {
        Self {
            chars: source.chars().collect(),
            pos: 0,
            line: 1,
            column: 1,
            _source: source,
        }
    }

    pub fn lex(mut self) -> Result<Vec<Token>> {
        let mut tokens = Vec::new();
        while let Some(ch) = self.peek() {
            match ch {
                ' ' | '\t' | '\r' => {
                    self.advance();
                }
                '\n' => {
                    self.advance();
                }
                '/' if self.peek_next() == Some('/') => {
                    while self.peek().is_some() && self.peek() != Some('\n') {
                        self.advance();
                    }
                }
                '0'..='9' => tokens.push(self.number()?),
                '"' => tokens.push(self.string()?),
                '\'' => tokens.push(self.lifetime()?),
                'a'..='z' | 'A'..='Z' | '_' => tokens.push(self.ident_or_keyword()),
                '(' => tokens.push(self.single(TokenKind::LParen)),
                ')' => tokens.push(self.single(TokenKind::RParen)),
                '{' => tokens.push(self.single(TokenKind::LBrace)),
                '}' => tokens.push(self.single(TokenKind::RBrace)),
                '[' => tokens.push(self.single(TokenKind::LBracket)),
                ']' => tokens.push(self.single(TokenKind::RBracket)),
                ';' => tokens.push(self.single(TokenKind::Semi)),
                ',' => tokens.push(self.single(TokenKind::Comma)),
                '.' if self.peek_next() == Some('.') => tokens.push(self.double(TokenKind::DotDot)),
                '.' => tokens.push(self.single(TokenKind::Dot)),
                ':' if self.peek_next() == Some(':') => {
                    tokens.push(self.double(TokenKind::ColonColon))
                }
                ':' => tokens.push(self.single(TokenKind::Colon)),
                '=' if self.peek_next() == Some('>') => {
                    tokens.push(self.double(TokenKind::FatArrow))
                }
                '+' => tokens.push(self.single(TokenKind::Plus)),
                '*' => tokens.push(self.single(TokenKind::Star)),
                '%' => tokens.push(self.single(TokenKind::Percent)),
                '-' if self.peek_next() == Some('>') => tokens.push(self.double(TokenKind::Arrow)),
                '-' => tokens.push(self.single(TokenKind::Minus)),
                '=' if self.peek_next() == Some('=') => tokens.push(self.double(TokenKind::EqEq)),
                '=' => tokens.push(self.single(TokenKind::Eq)),
                '!' if self.peek_next() == Some('=') => tokens.push(self.double(TokenKind::BangEq)),
                '!' => tokens.push(self.single(TokenKind::Bang)),
                '<' if self.peek_next() == Some('=') => tokens.push(self.double(TokenKind::LtEq)),
                '<' => tokens.push(self.single(TokenKind::Lt)),
                '>' if self.peek_next() == Some('=') => tokens.push(self.double(TokenKind::GtEq)),
                '>' => tokens.push(self.single(TokenKind::Gt)),
                '&' if self.peek_next() == Some('&') => tokens.push(self.double(TokenKind::AndAnd)),
                '&' => tokens.push(self.single(TokenKind::Amp)),
                '|' if self.peek_next() == Some('|') => tokens.push(self.double(TokenKind::OrOr)),
                '?' => tokens.push(self.single(TokenKind::Question)),
                '/' => tokens.push(self.single(TokenKind::Slash)),
                _ => {
                    return Err(MiniError::lex(
                        format!("unexpected character `{}`", ch),
                        self.span(),
                    ))
                }
            }
        }
        tokens.push(Token::new(TokenKind::Eof, self.span()));
        Ok(tokens)
    }

    fn number(&mut self) -> Result<Token> {
        let span = self.span();
        let mut text = String::new();
        while let Some(ch @ '0'..='9') = self.peek() {
            text.push(ch);
            self.advance();
        }
        if self.peek() == Some('.') && matches!(self.peek_next(), Some('0'..='9')) {
            text.push('.');
            self.advance();
            while let Some(ch @ '0'..='9') = self.peek() {
                text.push(ch);
                self.advance();
            }
            let value = text
                .parse::<f64>()
                .map_err(|_| MiniError::lex("float literal out of range", span))?;
            return Ok(Token::new(TokenKind::Float(value.to_bits()), span));
        }
        let value = text
            .parse::<i64>()
            .map_err(|_| MiniError::lex("integer literal out of range", span))?;
        Ok(Token::new(TokenKind::Int(value), span))
    }

    fn string(&mut self) -> Result<Token> {
        let span = self.span();
        self.advance();
        let mut text = String::new();
        while let Some(ch) = self.peek() {
            match ch {
                '"' => {
                    self.advance();
                    return Ok(Token::new(TokenKind::String(text), span));
                }
                '\\' => {
                    self.advance();
                    let escaped = match self.peek() {
                        Some('n') => '\n',
                        Some('t') => '\t',
                        Some('"') => '"',
                        Some('\\') => '\\',
                        Some(other) => other,
                        None => return Err(MiniError::lex("unterminated string literal", span)),
                    };
                    text.push(escaped);
                    self.advance();
                }
                _ => {
                    text.push(ch);
                    self.advance();
                }
            }
        }
        Err(MiniError::lex("unterminated string literal", span))
    }

    fn lifetime(&mut self) -> Result<Token> {
        let span = self.span();
        self.advance();
        let mut name = String::new();
        while let Some(ch) = self.peek() {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                name.push(ch);
                self.advance();
            } else {
                break;
            }
        }
        if name.is_empty() {
            return Err(MiniError::lex("expected lifetime name after `'`", span));
        }
        Ok(Token::new(TokenKind::Lifetime(name), span))
    }

    fn ident_or_keyword(&mut self) -> Token {
        let span = self.span();
        let mut text = String::new();
        while let Some(ch) = self.peek() {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                text.push(ch);
                self.advance();
            } else {
                break;
            }
        }
        let kind = match text.as_str() {
            "fn" => TokenKind::Fn,
            "let" => TokenKind::Let,
            "mut" => TokenKind::Mut,
            "if" => TokenKind::If,
            "else" => TokenKind::Else,
            "while" => TokenKind::While,
            "loop" => TokenKind::Loop,
            "for" => TokenKind::For,
            "in" => TokenKind::In,
            "break" => TokenKind::Break,
            "continue" => TokenKind::Continue,
            "return" => TokenKind::Return,
            "struct" => TokenKind::Struct,
            "enum" => TokenKind::Enum,
            "impl" => TokenKind::Impl,
            "trait" => TokenKind::Trait,
            "pub" => TokenKind::Pub,
            "match" => TokenKind::Match,
            "mod" => TokenKind::Mod,
            "use" => TokenKind::Use,
            "true" => TokenKind::True,
            "false" => TokenKind::False,
            "i64" => TokenKind::TypeI64,
            "f64" => TokenKind::TypeF64,
            "bool" => TokenKind::TypeBool,
            "str" => TokenKind::TypeStr,
            "String" => TokenKind::TypeString,
            _ => TokenKind::Ident(text),
        };
        Token::new(kind, span)
    }

    fn single(&mut self, kind: TokenKind) -> Token {
        let span = self.span();
        self.advance();
        Token::new(kind, span)
    }

    fn double(&mut self, kind: TokenKind) -> Token {
        let span = self.span();
        self.advance();
        self.advance();
        Token::new(kind, span)
    }

    fn span(&self) -> Span {
        Span::new(self.line, self.column)
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn peek_next(&self) -> Option<char> {
        self.chars.get(self.pos + 1).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let ch = self.peek()?;
        self.pos += 1;
        if ch == '\n' {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }
        Some(ch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lexes_rust_like_tokens() {
        let tokens = Lexer::new("fn main() { let mut x: i64 = 1; // ok\n }")
            .lex()
            .unwrap();
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Fn));
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Mut));
        assert!(tokens.iter().any(|t| t.kind == TokenKind::TypeI64));
    }
}
