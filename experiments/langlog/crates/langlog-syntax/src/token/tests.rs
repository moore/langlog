use super::{TokenKind, TokenTag};

//= SPEC.md#llg-diag-02-rendered-syntax-diagnostics
//= type=test
//# Token descriptions used in diagnostics MUST name identifiers, integer literals, and keywords.
#[test]
fn requirement_llg_diag_02_token_kind_describe_handles_identifiers_literals_and_keywords() {
    assert_eq!(
        TokenKind::Identifier("name".into()).describe(),
        "identifier"
    );
    assert_eq!(TokenKind::IntLiteral(7).describe(), "integer literal");
    assert_eq!(TokenKind::Fn.describe(), "`fn`");
}

//= SPEC.md#llg-diag-02-rendered-syntax-diagnostics
//= type=test
//# Token descriptions used in diagnostics MUST name punctuation, operators, and end of file.
#[test]
fn requirement_llg_diag_02_token_tag_describe_covers_symbols_and_eof() {
    assert_eq!(TokenTag::Underscore.describe(), "`_`");
    assert_eq!(TokenTag::BangEq.describe(), "`!=`");
    assert_eq!(TokenTag::LtEq.describe(), "`<=`");
    assert_eq!(TokenTag::GtEq.describe(), "`>=`");
    assert_eq!(TokenTag::Slash.describe(), "`/`");
    assert_eq!(TokenTag::Percent.describe(), "`%`");
    assert_eq!(TokenTag::Eof.describe(), "end of file");
}
