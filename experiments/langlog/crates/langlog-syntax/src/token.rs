use std::borrow::Cow;

use crate::span::Span;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

impl Token {
    pub fn new(kind: TokenKind, span: Span) -> Self {
        Self { kind, span }
    }

    pub fn tag(&self) -> TokenTag {
        self.kind.tag()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    Fn,
    Let,
    Mut,
    If,
    Else,
    Match,
    For,
    In,
    Return,
    Observe,
    True,
    False,
    Identifier(String),
    IntLiteral(u64),
    Underscore,
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Comma,
    Colon,
    Semi,
    Arrow,
    FatArrow,
    Eq,
    EqEq,
    Bang,
    BangEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    AndAnd,
    OrOr,
    DotDot,
    Eof,
}

impl TokenKind {
    pub fn tag(&self) -> TokenTag {
        match self {
            Self::Fn => TokenTag::Fn,
            Self::Let => TokenTag::Let,
            Self::Mut => TokenTag::Mut,
            Self::If => TokenTag::If,
            Self::Else => TokenTag::Else,
            Self::Match => TokenTag::Match,
            Self::For => TokenTag::For,
            Self::In => TokenTag::In,
            Self::Return => TokenTag::Return,
            Self::Observe => TokenTag::Observe,
            Self::True => TokenTag::True,
            Self::False => TokenTag::False,
            Self::Identifier(_) => TokenTag::Identifier,
            Self::IntLiteral(_) => TokenTag::IntLiteral,
            Self::Underscore => TokenTag::Underscore,
            Self::LParen => TokenTag::LParen,
            Self::RParen => TokenTag::RParen,
            Self::LBrace => TokenTag::LBrace,
            Self::RBrace => TokenTag::RBrace,
            Self::LBracket => TokenTag::LBracket,
            Self::RBracket => TokenTag::RBracket,
            Self::Comma => TokenTag::Comma,
            Self::Colon => TokenTag::Colon,
            Self::Semi => TokenTag::Semi,
            Self::Arrow => TokenTag::Arrow,
            Self::FatArrow => TokenTag::FatArrow,
            Self::Eq => TokenTag::Eq,
            Self::EqEq => TokenTag::EqEq,
            Self::Bang => TokenTag::Bang,
            Self::BangEq => TokenTag::BangEq,
            Self::Lt => TokenTag::Lt,
            Self::LtEq => TokenTag::LtEq,
            Self::Gt => TokenTag::Gt,
            Self::GtEq => TokenTag::GtEq,
            Self::Plus => TokenTag::Plus,
            Self::Minus => TokenTag::Minus,
            Self::Star => TokenTag::Star,
            Self::Slash => TokenTag::Slash,
            Self::Percent => TokenTag::Percent,
            Self::AndAnd => TokenTag::AndAnd,
            Self::OrOr => TokenTag::OrOr,
            Self::DotDot => TokenTag::DotDot,
            Self::Eof => TokenTag::Eof,
        }
    }

    pub fn describe(&self) -> Cow<'static, str> {
        self.tag().describe()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TokenTag {
    Fn,
    Let,
    Mut,
    If,
    Else,
    Match,
    For,
    In,
    Return,
    Observe,
    True,
    False,
    Identifier,
    IntLiteral,
    Underscore,
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Comma,
    Colon,
    Semi,
    Arrow,
    FatArrow,
    Eq,
    EqEq,
    Bang,
    BangEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    AndAnd,
    OrOr,
    DotDot,
    Eof,
}

impl TokenTag {
    pub fn describe(self) -> Cow<'static, str> {
        match self {
            Self::Fn => Cow::Borrowed("`fn`"),
            Self::Let => Cow::Borrowed("`let`"),
            Self::Mut => Cow::Borrowed("`mut`"),
            Self::If => Cow::Borrowed("`if`"),
            Self::Else => Cow::Borrowed("`else`"),
            Self::Match => Cow::Borrowed("`match`"),
            Self::For => Cow::Borrowed("`for`"),
            Self::In => Cow::Borrowed("`in`"),
            Self::Return => Cow::Borrowed("`return`"),
            Self::Observe => Cow::Borrowed("`observe`"),
            Self::True => Cow::Borrowed("`true`"),
            Self::False => Cow::Borrowed("`false`"),
            Self::Identifier => Cow::Borrowed("identifier"),
            Self::IntLiteral => Cow::Borrowed("integer literal"),
            Self::Underscore => Cow::Borrowed("`_`"),
            Self::LParen => Cow::Borrowed("`(`"),
            Self::RParen => Cow::Borrowed("`)`"),
            Self::LBrace => Cow::Borrowed("`{`"),
            Self::RBrace => Cow::Borrowed("`}`"),
            Self::LBracket => Cow::Borrowed("`[`"),
            Self::RBracket => Cow::Borrowed("`]`"),
            Self::Comma => Cow::Borrowed("`,`"),
            Self::Colon => Cow::Borrowed("`:`"),
            Self::Semi => Cow::Borrowed("`;`"),
            Self::Arrow => Cow::Borrowed("`->`"),
            Self::FatArrow => Cow::Borrowed("`=>`"),
            Self::Eq => Cow::Borrowed("`=`"),
            Self::EqEq => Cow::Borrowed("`==`"),
            Self::Bang => Cow::Borrowed("`!`"),
            Self::BangEq => Cow::Borrowed("`!=`"),
            Self::Lt => Cow::Borrowed("`<`"),
            Self::LtEq => Cow::Borrowed("`<=`"),
            Self::Gt => Cow::Borrowed("`>`"),
            Self::GtEq => Cow::Borrowed("`>=`"),
            Self::Plus => Cow::Borrowed("`+`"),
            Self::Minus => Cow::Borrowed("`-`"),
            Self::Star => Cow::Borrowed("`*`"),
            Self::Slash => Cow::Borrowed("`/`"),
            Self::Percent => Cow::Borrowed("`%`"),
            Self::AndAnd => Cow::Borrowed("`&&`"),
            Self::OrOr => Cow::Borrowed("`||`"),
            Self::DotDot => Cow::Borrowed("`..`"),
            Self::Eof => Cow::Borrowed("end of file"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{TokenKind, TokenTag};

    #[test]
    fn token_kind_describe_handles_identifiers_and_literals() {
        assert_eq!(
            TokenKind::Identifier("name".into()).describe(),
            "identifier"
        );
        assert_eq!(TokenKind::IntLiteral(7).describe(), "integer literal");
        assert_eq!(TokenKind::Fn.describe(), "`fn`");
    }

    #[test]
    fn token_tag_describe_covers_symbols_and_eof() {
        assert_eq!(TokenTag::Underscore.describe(), "`_`");
        assert_eq!(TokenTag::BangEq.describe(), "`!=`");
        assert_eq!(TokenTag::LtEq.describe(), "`<=`");
        assert_eq!(TokenTag::GtEq.describe(), "`>=`");
        assert_eq!(TokenTag::Slash.describe(), "`/`");
        assert_eq!(TokenTag::Percent.describe(), "`%`");
        assert_eq!(TokenTag::Eof.describe(), "end of file");
    }
}
