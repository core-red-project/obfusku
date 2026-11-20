//! Resource limits and syntax errors: what is rejected, and with which message.

use super::support::*;

/// Debug frames are large and a deep tree drops recursively; the test
/// thread's default stack is not enough.
fn on_big_stack(f: fn()) {
    std::thread::Builder::new()
        .stack_size(1 << 30)
        .spawn(f)
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn nesting_beyond_the_limit_is_a_diagnostic_not_a_stack_overflow() {
    on_big_stack(|| {
        let src = format!("x ≔ {}1{}\n❧\n", "(".repeat(5000), ")".repeat(5000));
        assert!(parse_err(&src).contains("nesting is deeper"));
    });
}

#[test]
fn a_chain_beyond_the_limit_is_a_diagnostic() {
    on_big_stack(|| {
        let src = format!("x ≔ 1{}\n❧\n", " ✚ 1".repeat(100_001));
        assert!(parse_err(&src).contains("chained operators"));
    });
}

// ── valid programs ─────────────────────────────────────────────────────

#[test]
fn missing_seal_is_rejected() {
    let msg = parse_err("x ≔ 5");
    assert!(msg.contains('❧'), "message was: {msg}");
}

#[test]
fn empty_input_is_rejected() {
    let msg = parse_err("");
    assert!(msg.contains('❧'), "message was: {msg}");
}

#[test]
fn unterminated_string_is_rejected() {
    let msg = parse_err("x ≔ \"abc");
    assert!(msg.contains("unterminated"), "message was: {msg}");
}

#[test]
fn ascii_plus_is_never_valid_obfusku_syntax() {
    // The glyph '✚', never ASCII '+' — no ASCII-operator fallback.
    let msg = parse_err("x ≔ 1 + 2\n❧");
    assert!(msg.contains("unexpected character"), "message was: {msg}");
}

#[test]
fn reversed_modifier_order_is_rejected() {
    // §8.1's production is Mutability? Export?, in that fixed order —
    // reversing them is a parse error.
    let msg = parse_err("x ≔⟳˚ 5\n❧");
    assert!(
        msg.contains("expected a name after '˚'"),
        "message was: {msg}"
    );
}

#[test]
fn unknown_identifier_where_a_declaration_or_seal_is_expected_is_rejected() {
    let msg = parse_err("x ≔ 5\n)\n❧");
    assert!(msg.contains("expected a declaration"), "message was: {msg}");
}

// ── P1-3a: ValueDeclaration's `(":" Type)?` annotation (§8.1) ──────────

#[test]
fn match_missing_closing_brace_is_rejected() {
    let msg = parse_err("y ≔ ⟡ x { Some(n) → n\n❧");
    assert!(msg.contains("'}'"), "message was: {msg}");
}

#[test]
fn type_declaration_missing_bind_is_rejected() {
    let msg = parse_err("Shape { Circle(⧆) }\n❧");
    assert!(msg.contains("'≔'"), "message was: {msg}");
}

#[test]
fn zero_parameter_lambda_is_rejected() {
    // CONCRETE_SYMBOLIC_GRAMMAR.md §7.3: excluded at the grammar level,
    // not merely unimplemented — there is no Unit value for a
    // zero-parameter Lambda to lower to.
    let msg = parse_err("f ≔ λ() → 1\n❧");
    assert!(msg.contains("zero-parameter lambdas"), "message was: {msg}");
}

#[test]
fn zero_argument_application_is_rejected() {
    // CONCRETE_SYMBOLIC_GRAMMAR.md §7.2: same reasoning as above.
    let msg = parse_err("y ≔ f()\n❧");
    assert!(
        msg.contains("zero-argument application"),
        "message was: {msg}"
    );
}

// ---------------------------------------------------------------------
// FunctionDeclaration — `CONCRETE_SYMBOLIC_GRAMMAR.md` §8.2
// ---------------------------------------------------------------------
