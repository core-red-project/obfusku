//! Precedence, associativity and the operator glyphs.

use super::support::*;

fn expr_of(module: &obfusku_syntax::ast::Module) -> &Expression {
    let Declaration::Value(decl) = &module.declarations[0] else {
        panic!("expected a Value declaration")
    };
    &decl.value
}

fn as_binop(e: &Expression) -> (BinOp, &Expression, &Expression) {
    match e {
        Expression::BinaryOp { op, lhs, rhs, .. } => (*op, lhs, rhs),
        other => panic!("expected a BinaryOp, got {other:?}"),
    }
}

fn as_int_lit(e: &Expression) -> i64 {
    match e {
        Expression::Int(v, _) => *v,
        other => panic!("expected an Int literal, got {other:?}"),
    }
}

#[test]
fn all_operator_tokens_lex_without_error() {
    // One pass proving every glyph from the frozen grammar actually
    // lexes — including the two-codepoint `☠︎` and the two-ASCII-char
    // compound comparison/equality tokens.
    let _ = parse_ok(
        "x ≔ 1 ✚ 2 ☠︎ 3 ✱ 4 ÷ 5 ⌗ 6\n\
         y ≔ (1 < 2) ∧ (3 ≤ 4) ∧ (5 > 6) ∧ (7 ≥ 8)\n\
         z ≔ (1 ≡ 2) ∧ (3 ≠ 4)\n\
         w ≔ ¬◉ ∨ ◎ ⊻ ◉\n\
         n ≔ −5\n❧",
    );
}

#[test]
fn multiplicative_binds_tighter_than_additive() {
    // 1 ✚ 2 ✱ 3  ==  1 ✚ (2 ✱ 3), not (1 ✚ 2) ✱ 3
    let module = parse_ok("x ≔ 1 ✚ 2 ✱ 3\n❧");
    let (op, lhs, rhs) = as_binop(expr_of(&module));
    assert_eq!(op, BinOp::Add);
    assert_eq!(as_int_lit(lhs), 1);
    let (inner_op, inner_lhs, inner_rhs) = as_binop(rhs);
    assert_eq!(inner_op, BinOp::Mul);
    assert_eq!(as_int_lit(inner_lhs), 2);
    assert_eq!(as_int_lit(inner_rhs), 3);
}

#[test]
fn additive_is_left_associative() {
    // 1 ✚ 2 ☠︎ 3  ==  (1 ✚ 2) ☠︎ 3
    let module = parse_ok("x ≔ 1 ✚ 2 ☠︎ 3\n❧");
    let (op, lhs, rhs) = as_binop(expr_of(&module));
    assert_eq!(op, BinOp::Sub);
    assert_eq!(as_int_lit(rhs), 3);
    let (inner_op, inner_lhs, inner_rhs) = as_binop(lhs);
    assert_eq!(inner_op, BinOp::Add);
    assert_eq!(as_int_lit(inner_lhs), 1);
    assert_eq!(as_int_lit(inner_rhs), 2);
}

#[test]
fn additive_binds_tighter_than_comparison() {
    // 1 ✚ 2 < 3 ✱ 4  ==  (1 ✚ 2) < (3 ✱ 4)
    let module = parse_ok("x ≔ 1 ✚ 2 < 3 ✱ 4\n❧");
    let (op, lhs, rhs) = as_binop(expr_of(&module));
    assert_eq!(op, BinOp::Lt);
    assert_eq!(as_binop(lhs).0, BinOp::Add);
    assert_eq!(as_binop(rhs).0, BinOp::Mul);
}

#[test]
fn comparison_binds_tighter_than_equality() {
    let module = parse_ok("x ≔ 1 < 2 ≡ 3 > 4\n❧");
    let (op, lhs, rhs) = as_binop(expr_of(&module));
    assert_eq!(op, BinOp::Eq);
    assert_eq!(as_binop(lhs).0, BinOp::Lt);
    assert_eq!(as_binop(rhs).0, BinOp::Gt);
}

#[test]
fn equality_binds_tighter_than_and() {
    let module = parse_ok("x ≔ 1 ≡ 2 ∧ 3 ≠ 4\n❧");
    let (op, lhs, rhs) = as_binop(expr_of(&module));
    assert_eq!(op, BinOp::And);
    assert_eq!(as_binop(lhs).0, BinOp::Eq);
    assert_eq!(as_binop(rhs).0, BinOp::NotEq);
}

#[test]
fn and_binds_tighter_than_xor_binds_tighter_than_or() {
    // a ∧ b ∨ c ⊻ d  ==  (a ∧ b) ∨ (c ⊻ d)
    let module = parse_ok("x ≔ 1 ∧ 2 ∨ 3 ⊻ 4\n❧");
    let (op, lhs, rhs) = as_binop(expr_of(&module));
    assert_eq!(op, BinOp::Or);
    assert_eq!(as_binop(lhs).0, BinOp::And);
    assert_eq!(as_binop(rhs).0, BinOp::Xor);
}

#[test]
fn logical_operators_are_left_associative() {
    let module = parse_ok("x ≔ 1 ∨ 2 ∨ 3\n❧");
    let (op, lhs, rhs) = as_binop(expr_of(&module));
    assert_eq!(op, BinOp::Or);
    assert_eq!(as_int_lit(rhs), 3);
    assert_eq!(as_binop(lhs).0, BinOp::Or);
}

#[test]
fn comparison_does_not_chain() {
    let msg = parse_err("x ≔ 1 < 2 < 3\n❧");
    assert!(msg.contains("do not chain"), "{msg}");
}

#[test]
fn equality_does_not_chain() {
    let msg = parse_err("x ≔ 1 ≡ 2 ≡ 3\n❧");
    assert!(msg.contains("do not chain"), "{msg}");
}

#[test]
fn mixed_comparison_operators_still_do_not_chain() {
    let msg = parse_err("x ≔ 1 ≤ 2 ≥ 3\n❧");
    assert!(msg.contains("do not chain"), "{msg}");
}

#[test]
fn unary_negation_binds_tighter_than_multiplicative() {
    // −2 ✱ 3  ==  (−2) ✱ 3
    let module = parse_ok("x ≔ −2 ✱ 3\n❧");
    let (op, lhs, rhs) = as_binop(expr_of(&module));
    assert_eq!(op, BinOp::Mul);
    assert!(matches!(lhs, Expression::UnaryOp { op: UnOp::Neg, .. }));
    assert_eq!(as_int_lit(rhs), 3);
}

#[test]
fn unary_not_nests_right_to_left() {
    // ¬¬◉  ==  ¬(¬◉)
    let module = parse_ok("x ≔ ¬¬◉\n❧");
    match expr_of(&module) {
        Expression::UnaryOp {
            op: UnOp::Not,
            operand,
            ..
        } => {
            assert!(matches!(
                **operand,
                Expression::UnaryOp { op: UnOp::Not, .. }
            ));
        }
        other => panic!("expected nested UnaryOp, got {other:?}"),
    }
}

#[test]
fn binary_subtract_and_unary_negate_are_distinct_glyphs_no_ambiguity() {
    // 5 ☠︎ −2  ==  5 minus (negate 2) — both glyphs present, unambiguous
    // since '☠︎' (binary) and '−' (unary) are different tokens entirely.
    let module = parse_ok("x ≔ 5 ☠︎ −2\n❧");
    let (op, lhs, rhs) = as_binop(expr_of(&module));
    assert_eq!(op, BinOp::Sub);
    assert_eq!(as_int_lit(lhs), 5);
    assert!(matches!(rhs, Expression::UnaryOp { op: UnOp::Neg, .. }));
}

#[test]
fn application_binds_tighter_than_unary() {
    // ¬f(x)  ==  ¬(f(x)), not (¬f)(x)
    let module = parse_ok("y ≔ ¬f(x)\n❧");
    match expr_of(&module) {
        Expression::UnaryOp {
            op: UnOp::Not,
            operand,
            ..
        } => {
            assert!(matches!(**operand, Expression::Application { .. }));
        }
        other => panic!("expected UnaryOp wrapping an Application, got {other:?}"),
    }
}

#[test]
fn pipe_wraps_the_whole_operator_ladder_as_its_subject() {
    // 1 ✚ 2 ▷ f  ==  (1 ✚ 2) ▷ f — pipe's subject descends through the
    // full ladder, per §13's "▷ wraps the ladder" resolution.
    let module = parse_ok("y ≔ 1 ✚ 2 ▷ f\n❧");
    match expr_of(&module) {
        Expression::Pipe { subject, .. } => {
            assert_eq!(as_binop(subject).0, BinOp::Add);
        }
        other => panic!("expected a Pipe, got {other:?}"),
    }
}

#[test]
fn string_concatenation_operator_parses_as_add() {
    let module = parse_ok("x ≔ \"a\" ✚ \"b\"\n❧");
    assert_eq!(as_binop(expr_of(&module)).0, BinOp::Add);
}

#[test]
fn modulo_operator_parses() {
    let module = parse_ok("x ≔ 7 ⌗ 3\n❧");
    assert_eq!(as_binop(expr_of(&module)).0, BinOp::Mod);
}

#[test]
fn bare_lt_gt_are_distinct_from_le_ge_tokens() {
    // '<' alone parses as a comparison; '<=' must NOT be lexed as '<'
    // followed by a stray '=' (which would be a lex error, since bare
    // '=' is not otherwise valid).
    let module = parse_ok("x ≔ 1 ≤ 2\n❧");
    assert_eq!(as_binop(expr_of(&module)).0, BinOp::Le);
    let module2 = parse_ok("x ≔ 1 < 2\n❧");
    assert_eq!(as_binop(expr_of(&module2)).0, BinOp::Lt);
}

#[test]
fn bare_ascii_equals_alone_is_a_lex_error() {
    // '=' alone (not part of '==') is not a valid token anywhere in the
    // grammar — confirms no accidental partial-match fallback exists.
    let msg = parse_err("x ≔ 1 = 2\n❧");
    assert!(msg.contains("unexpected character"), "{msg}");
}

// ── NFC source validation — `CONCRETE_SYMBOLIC_GRAMMAR.md` §2 ─────────

#[test]
fn non_nfc_source_is_a_lexical_error() {
    // 'e' + U+0301 COMBINING ACUTE ACCENT (NFD-decomposed "é"), inside an
    // otherwise ordinary string literal — must be rejected before any
    // token is produced, not silently renormalized.
    let msg = parse_err("x \u{2254} \"caf\u{65}\u{301}\"\n\u{2767}\n");
    assert!(msg.contains("Normalization Form C"), "{msg}");
}

#[test]
fn nfc_source_with_a_precomposed_accent_is_unaffected() {
    // The NFC-composed form of the same character ("é" as one codepoint,
    // U+00E9) must lex with no error at all.
    let module = parse_ok("x \u{2254} \"caf\u{e9}\"\n\u{2767}\n");
    assert_eq!(module.declarations.len(), 1);
}

// ---------------------------------------------------------------------
// Raise / Catch — `CONCRETE_SYMBOLIC_GRAMMAR.md` §7.8, `SEMANTIC_CORE.md`
// §15/§15.1/§15.2.
// ---------------------------------------------------------------------
