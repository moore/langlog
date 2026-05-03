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
