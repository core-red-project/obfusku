//! Conformance tests for `obfusku-syntax::lexer` directly, independent
//! of the parser — `CONCRETE_SYMBOLIC_GRAMMAR.md` §2/§5. Numeric-literal
//! edge cases in particular had no dedicated coverage anywhere in the
//! workspace before this file (only exercised incidentally, and never at
//! the exact boundary between `IntLiteral`/`RealLiteral`).

use obfusku_diagnostics::SourceMap;
use obfusku_syntax::lexer::{self, TokenKind};

fn tokenize_ok(source: &str) -> Vec<TokenKind> {
    let mut map = SourceMap::new();
    let id = map.add_file(source);
    lexer::tokenize(source, id)
        .unwrap_or_else(|d| panic!("expected successful lex, got: {d:?}"))
        .into_iter()
        .map(|t| t.kind)
        .collect()
}

// ── §5: `RealLiteral ::= digit+ "." digit+ (("e"|"E") ("+"|"-")? digit+)?`
//    — the exponent suffix is only reachable after the mandatory
//    fractional part; `IntLiteral ::= digit+` has no exponent production
//    at all. ───────────────────────────────────────────────────────────

#[test]
fn bare_int_lexes_as_a_single_int_token() {
    assert_eq!(tokenize_ok("5"), vec![TokenKind::Int(5), TokenKind::Eof]);
}

#[test]
fn dotted_real_lexes_as_a_single_real_token() {
    assert_eq!(
        tokenize_ok("5.5"),
        vec![TokenKind::Real(5.5), TokenKind::Eof]
    );
}

#[test]
fn dotted_real_with_exponent_lexes_as_a_single_real_token() {
    assert_eq!(
        tokenize_ok("5.0e10"),
        vec![TokenKind::Real(5.0e10), TokenKind::Eof]
    );
}

#[test]
fn dotted_real_with_signed_uppercase_exponent_lexes_as_a_single_real_token() {
    assert_eq!(
        tokenize_ok("5.5E-3"),
        vec![TokenKind::Real(5.5E-3), TokenKind::Eof]
    );
}

#[test]
fn integer_directly_followed_by_an_exponent_suffix_is_not_a_real_literal() {
    // §5's own production has no path from `IntLiteral` (`digit+`, no
    // dot) to an exponent at all — `5e10` must NOT collapse into one
    // `Real` token. The grammar's own "longest match among defined
    // productions" implies the digit run stops at `5`, and `e10` is then
    // lexed fresh as whatever it actually is: an ordinary identifier.
    assert_eq!(
        tokenize_ok("5e10"),
        vec![
            TokenKind::Int(5),
            TokenKind::Ident("e10".to_string()),
            TokenKind::Eof
        ]
    );
}

#[test]
fn integer_directly_followed_by_an_uppercase_exponent_suffix_is_not_a_real_literal() {
    assert_eq!(
        tokenize_ok("5E10"),
        vec![
            TokenKind::Int(5),
            TokenKind::Ident("E10".to_string()),
            TokenKind::Eof
        ]
    );
}

#[test]
fn integer_followed_by_e_then_a_non_exponent_identifier_is_not_a_real_literal() {
    let tokens = tokenize_ok("5euros");
    assert_eq!(tokens[0], TokenKind::Int(5));
    assert_eq!(tokens[1], TokenKind::Ident("euros".to_string()));
}
