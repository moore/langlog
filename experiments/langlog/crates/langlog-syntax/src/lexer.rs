use crate::diagnostic::{Diagnostic, Label};
use crate::span::SourceFile;
use crate::token::{Token, TokenKind};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexedSource {
    pub source: SourceFile,
    pub tokens: Vec<Token>,
    pub diagnostics: Vec<Diagnostic>,
}

pub fn lex(path: impl Into<std::path::PathBuf>, contents: impl Into<String>) -> LexedSource {
    lex_source(SourceFile::new(path, contents))
}

//= SPEC.md#llg-diag-01-source-span-preservation
//# The front end MUST preserve byte spans for tokens and syntax nodes or derive them from spanned children without reparsing source text.
pub fn lex_source(source: SourceFile) -> LexedSource {
    Lexer::new(source).lex()
}

struct Lexer {
    source: SourceFile,
    offset: usize,
    tokens: Vec<Token>,
    diagnostics: Vec<Diagnostic>,
}

impl Lexer {
    fn new(source: SourceFile) -> Self {
        Self {
            source,
            offset: 0,
            tokens: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    fn lex(mut self) -> LexedSource {
        while let Some(ch) = self.peek_char() {
            match ch {
                ch if ch.is_whitespace() => {
                    self.bump_char();
                }
                '/' => {
                    if self.peek_next('/') {
                        self.skip_line_comment();
                    } else if self.peek_next('*') {
                        self.skip_block_comment();
                    } else {
                        self.push_simple(TokenKind::Slash, 1);
                    }
                }
                'a'..='z' | 'A'..='Z' | '_' => self.lex_identifier_or_keyword(),
                '0'..='9' => self.lex_integer(),
                '(' => self.push_simple(TokenKind::LParen, 1),
                ')' => self.push_simple(TokenKind::RParen, 1),
                '{' => self.push_simple(TokenKind::LBrace, 1),
                '}' => self.push_simple(TokenKind::RBrace, 1),
                '[' => self.push_simple(TokenKind::LBracket, 1),
                ']' => self.push_simple(TokenKind::RBracket, 1),
                ',' => self.push_simple(TokenKind::Comma, 1),
                ':' => self.push_simple(TokenKind::Colon, 1),
                ';' => self.push_simple(TokenKind::Semi, 1),
                '+' => self.push_simple(TokenKind::Plus, 1),
                '*' => self.push_simple(TokenKind::Star, 1),
                '%' => self.push_simple(TokenKind::Percent, 1),
                '-' => {
                    if self.peek_next('>') {
                        self.push_compound(TokenKind::Arrow, 2);
                    } else {
                        self.push_simple(TokenKind::Minus, 1);
                    }
                }
                '=' => {
                    if self.peek_next('=') {
                        self.push_compound(TokenKind::EqEq, 2);
                    } else if self.peek_next('>') {
                        self.push_compound(TokenKind::FatArrow, 2);
                    } else {
                        self.push_simple(TokenKind::Eq, 1);
                    }
                }
                '!' => {
                    if self.peek_next('=') {
                        self.push_compound(TokenKind::BangEq, 2);
                    } else {
                        self.push_simple(TokenKind::Bang, 1);
                    }
                }
                '<' => {
                    if self.peek_next('=') {
                        self.push_compound(TokenKind::LtEq, 2);
                    } else {
                        self.push_simple(TokenKind::Lt, 1);
                    }
                }
                '>' => {
                    if self.peek_next('=') {
                        self.push_compound(TokenKind::GtEq, 2);
                    } else {
                        self.push_simple(TokenKind::Gt, 1);
                    }
                }
                '&' => {
                    if self.peek_next('&') {
                        self.push_compound(TokenKind::AndAnd, 2);
                    } else {
                        self.report_unexpected_char('&', "expected `&&` for logical and");
                        self.bump_char();
                    }
                }
                '|' => {
                    if self.peek_next('|') {
                        self.push_compound(TokenKind::OrOr, 2);
                    } else {
                        self.report_unexpected_char('|', "expected `||` for logical or");
                        self.bump_char();
                    }
                }
                '.' => {
                    if self.peek_next('.') {
                        self.push_compound(TokenKind::DotDot, 2);
                    } else {
                        self.report_unexpected_char('.', "unexpected `.`");
                        self.bump_char();
                    }
                }
                other => {
                    self.report_unexpected_char(other, "unrecognized character");
                    self.bump_char();
                }
            }
        }

        let eof_span = self.source.eof_span();
        self.tokens.push(Token::new(TokenKind::Eof, eof_span));

        LexedSource {
            source: self.source,
            tokens: self.tokens,
            diagnostics: self.diagnostics,
        }
    }

    fn peek_char(&self) -> Option<char> {
        self.source.contents()[self.offset..].chars().next()
    }

    fn bump_char(&mut self) -> Option<char> {
        let ch = self.peek_char()?;
        self.offset += ch.len_utf8();
        Some(ch)
    }

    fn peek_next(&self, expected: char) -> bool {
        let mut chars = self.source.contents()[self.offset..].chars();
        matches!((chars.next(), chars.next()), (Some(_), Some(ch)) if ch == expected)
    }

    fn push_simple(&mut self, kind: TokenKind, width: usize) {
        let start = self.offset;
        self.offset += width;
        let span = self.source.span(start, self.offset);
        self.tokens.push(Token::new(kind, span));
    }

    fn push_compound(&mut self, kind: TokenKind, width: usize) {
        self.push_simple(kind, width);
    }

    fn skip_line_comment(&mut self) {
        //= SPEC.md#llg-lex-01-comments
        //# The lexer MUST ignore line comments beginning with `//`.
        self.bump_char();
        self.bump_char();

        while let Some(ch) = self.peek_char() {
            if ch == '\n' {
                break;
            }
            self.bump_char();
        }
    }

    fn skip_block_comment(&mut self) {
        //= SPEC.md#llg-lex-01-comments
        //# The lexer MUST ignore block comments delimited by `/*` and `*/`.
        //= SPEC.md#llg-lex-01-comments
        //# The lexer MUST support nested block comments.
        let start = self.offset;
        self.bump_char();
        self.bump_char();
        let mut depth = 1usize;

        while let Some(ch) = self.peek_char() {
            if ch == '/' && self.peek_next('*') {
                self.bump_char();
                self.bump_char();
                depth += 1;
                continue;
            }

            if ch == '*' && self.peek_next('/') {
                self.bump_char();
                self.bump_char();
                depth -= 1;
                if depth == 0 {
                    return;
                }
                continue;
            }

            self.bump_char();
        }

        let span = self.source.span(start, self.source.len());
        self.diagnostics.push(
            //= SPEC.md#llg-lex-01-comments
            //# The lexer MUST report an error for an unterminated block comment.
            Diagnostic::error("unterminated block comment")
                .with_label(Label::primary(span, "comment starts here")),
        );
    }

    fn lex_identifier_or_keyword(&mut self) {
        //= SPEC.md#llg-lex-02-identifiers-and-literals
        //# Identifiers MUST begin with an ASCII letter or `_` and MAY continue with ASCII letters, digits, or `_`.
        //= SPEC.md#llg-lex-03-reserved-keywords
        //# The phase 1 keyword set MUST reserve `fn`, `let`, `mut`, `if`, `else`, `match`, `for`, `in`, `return`, `observe`, `true`, and `false`.
        //= SPEC.md#llg-lex-02-identifiers-and-literals
        //# Boolean literals MUST include `true` and `false`.
        let start = self.offset;
        self.bump_char();

        while let Some(ch) = self.peek_char() {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                self.bump_char();
            } else {
                break;
            }
        }

        let span = self.source.span(start, self.offset);
        let text = self.source.span_text(span).unwrap_or_default();
        let kind = match text {
            "fn" => TokenKind::Fn,
            "let" => TokenKind::Let,
            "mut" => TokenKind::Mut,
            "if" => TokenKind::If,
            "else" => TokenKind::Else,
            "match" => TokenKind::Match,
            "for" => TokenKind::For,
            "in" => TokenKind::In,
            "return" => TokenKind::Return,
            "observe" => TokenKind::Observe,
            "true" => TokenKind::True,
            "false" => TokenKind::False,
            "_" => TokenKind::Underscore,
            _ => TokenKind::Identifier(text.to_owned()),
        };

        self.tokens.push(Token::new(kind, span));
    }

    fn lex_integer(&mut self) {
        //= SPEC.md#llg-lex-02-identifiers-and-literals
        //# Integer literals MUST be parsed as unsigned base-10 integers.
        let start = self.offset;
        self.bump_char();

        while matches!(self.peek_char(), Some(ch) if ch.is_ascii_digit()) {
            self.bump_char();
        }

        let span = self.source.span(start, self.offset);
        let text = self.source.span_text(span).unwrap_or_default();

        match text.parse::<u64>() {
            Ok(value) => self
                .tokens
                .push(Token::new(TokenKind::IntLiteral(value), span)),
            Err(_) => {
                self.diagnostics.push(
                    Diagnostic::error("integer literal does not fit in `u64`")
                        .with_label(Label::primary(span, "literal is too large")),
                );
                self.tokens.push(Token::new(TokenKind::IntLiteral(0), span));
            }
        }
    }

    fn report_unexpected_char(&mut self, ch: char, message: &str) {
        //= SPEC.md#llg-lex-04-lexical-error-diagnostics
        //# Lexical diagnostics for invalid characters MUST include a primary span covering the offending character.
        let start = self.offset;
        let end = start + ch.len_utf8();
        let span = self.source.span(start, end);
        self.diagnostics.push(
            Diagnostic::error(format!("unexpected character `{ch}`"))
                .with_label(Label::primary(span, message)),
        );
    }
}

#[cfg(test)]
mod tests {
    use crate::lexer::lex;
    use crate::token::TokenTag;

    #[test]
    fn lexes_keywords_comments_and_symbols() {
        let lexed = lex(
            "demo.llg",
            "fn main() -> u32 { /* block */ let mut value = 1 + 2; // line\n return value; }",
        );

        let tags: Vec<_> = lexed.tokens.iter().map(|token| token.tag()).collect();
        assert!(lexed.diagnostics.is_empty());
        assert_eq!(
            tags,
            vec![
                TokenTag::Fn,
                TokenTag::Identifier,
                TokenTag::LParen,
                TokenTag::RParen,
                TokenTag::Arrow,
                TokenTag::Identifier,
                TokenTag::LBrace,
                TokenTag::Let,
                TokenTag::Mut,
                TokenTag::Identifier,
                TokenTag::Eq,
                TokenTag::IntLiteral,
                TokenTag::Plus,
                TokenTag::IntLiteral,
                TokenTag::Semi,
                TokenTag::Return,
                TokenTag::Identifier,
                TokenTag::Semi,
                TokenTag::RBrace,
                TokenTag::Eof,
            ]
        );
    }
}
