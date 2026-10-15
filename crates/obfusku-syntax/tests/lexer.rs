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
fn an_exponent_without_a_fraction_makes_a_real_literal() {
    // `ADR-027`: the exponent no longer needs the fractional part.
    assert_eq!(
        tokenize_ok("5e10 5E-2"),
        vec![TokenKind::Real(5e10), TokenKind::Real(5e-2), TokenKind::Eof]
    );
}

#[test]
fn radix_prefixes_and_digit_separators() {
    assert_eq!(
        tokenize_ok("0xFF 0b1010 0o17 1_000 0x_1"),
        vec![
            TokenKind::Int(255),
            TokenKind::Int(10),
            TokenKind::Int(15),
            TokenKind::Int(1000),
            TokenKind::Int(0),
            TokenKind::Ident("x_1".to_string()),
            TokenKind::Eof
        ]
    );
}

#[test]
fn a_trailing_separator_is_not_part_of_the_literal() {
    assert_eq!(
        tokenize_ok("1_"),
        vec![TokenKind::Int(1), TokenKind::Wildcard, TokenKind::Eof]
    );
}

#[test]
fn two_to_the_63_lexes_as_the_minimum_for_the_parser_to_police() {
    assert_eq!(
        tokenize_ok("9223372036854775808"),
        vec![TokenKind::Int(i64::MIN), TokenKind::Eof]
    );
    let mut map = SourceMap::new();
    let src = "9223372036854775809";
    let id = map.add_file(src);
    assert!(lexer::tokenize(src, id).is_err());
}

#[test]
fn unicode_escapes_name_scalar_values() {
    assert_eq!(
        tokenize_ok(r#""\u{41}\u{1F44D}""#),
        vec![TokenKind::Str("A👍".to_string()), TokenKind::Eof]
    );
    for bad in [r#""\u{110000}""#, r#""\u{D800}""#, r#""\u41""#, r#""\u{}""#] {
        let mut map = SourceMap::new();
        let id = map.add_file(bad);
        assert!(lexer::tokenize(bad, id).is_err(), "{bad}");
    }
}

#[test]
fn integer_followed_by_e_then_a_non_exponent_identifier_is_not_a_real_literal() {
    let tokens = tokenize_ok("5euros");
    assert_eq!(tokens[0], TokenKind::Int(5));
    assert_eq!(tokens[1], TokenKind::Ident("euros".to_string()));
}
