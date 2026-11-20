//! `˚` bindings, reads, rebinds and closure capture.

use super::support::*;

#[test]
fn immutable_binding_still_lowers_normally() {
    // Regression: adding mutability support must not disturb this.
    let module = lower_ok("x ≔ 5\n❧");
    assert!(matches!(
        flat(&module)[0].value,
        Expr::Lit(Literal::Int(5), _)
    ));
}

#[test]
fn mutable_binding_lowers_to_mutcell() {
    let module = lower_ok("x ≔˚ 5\n❧");
    match &flat(&module)[0].value {
        Expr::MutCell { initial, .. } => {
            assert!(matches!(**initial, Expr::Lit(Literal::Int(5), _)));
        }
        other => panic!("expected a MutCell, got {other:?}"),
    }
}

#[test]
fn mutable_and_immutable_bindings_are_distinguishable_by_core_shape() {
    // No separate boolean flag on Binding — mutability is entirely
    // represented by whether the value is MutCell-wrapped, matching
    // SEMANTIC_CORE.md's own framing of mutability as a property of the
    // value's shape, not a side tag.
    let module = lower_ok("x ≔ 1\ny ≔˚ 1\n❧");
    assert!(!matches!(flat(&module)[0].value, Expr::MutCell { .. }));
    assert!(matches!(flat(&module)[1].value, Expr::MutCell { .. }));
}

#[test]
fn ordinary_reference_to_a_mutable_binding_lowers_to_mutread() {
    let module = lower_ok("x ≔˚ 5\ny ≔ x\n❧");
    match &flat(&module)[1].value {
        Expr::MutRead { cell, .. } => {
            assert!(matches!(**cell, Expr::Var(ref n, _) if n == "x"));
        }
        other => panic!("expected a MutRead, got {other:?}"),
    }
}

#[test]
fn explicit_reference_form_never_reads() {
    // ˚x always lowers to bare Var(x). Parenthesized: "≔" immediately
    // followed by "˚" is always the Mutability modifier, never a bare
    // ReferenceForm value, so this position needs parens.
    let module = lower_ok("x ≔˚ 5\ny ≔ (˚x)\n❧");
    assert!(matches!(&flat(&module)[1].value, Expr::Var(n, _) if n == "x"));
}

#[test]
fn rebind_lowers_to_mutrebind_against_the_raw_cell() {
    // Plain (non-explicit) spelling: valid here because this rebind is
    // NOT inside any Lambda — no closure boundary is being crossed, so
    // no explicit capture is required (SEMANTIC_CORE.md §13 only
    // constrains references crossing INTO a closure).
    let module = lower_ok("x ≔˚ 5\ny ≔ x ⚙︎ 9\n❧");
    match &flat(&module)[1].value {
        Expr::MutRebind {
            cell, new_value, ..
        } => {
            assert!(matches!(**cell, Expr::Var(ref n, _) if n == "x"));
            assert!(matches!(**new_value, Expr::Lit(Literal::Int(9), _)));
        }
        other => panic!("expected a MutRebind, got {other:?}"),
    }
}

#[test]
fn mutable_capture_follows_frozen_capture_semantics() {
    // SEMANTIC_CORE.md §13: explicit ('˚') capture is what makes
    // mutation-through-closure representable at all. `˚counter ⚙︎ step`
    // is the single-occurrence idiom that both captures and mutates —
    // this must lower cleanly, with no static rejection.
    let module = lower_ok("counter ≔˚ 0\nbump ≔ λ(step) → ˚counter ⚙︎ step\n❧");
    match &flat(&module)[1].value {
        Expr::Lambda { body, .. } => {
            assert!(matches!(**body, Expr::MutRebind { .. }));
        }
        other => panic!("expected a Lambda, got {other:?}"),
    }
}

#[test]
fn mutation_through_a_default_captured_reference_is_rejected_statically() {
    // §13: rebinding a mut-bound free variable not explicitly
    // '˚'-captured is a static error — the default-capture reference
    // desugars to a read of the cell's contents, not the cell itself,
    // and `⚙︎` needs the cell.
    let msg = lower_err("counter ≔˚ 0\nbump ≔ λ(step) → counter ⚙︎ step\n❧");
    assert!(msg.contains("desugars to a read"), "message was: {msg}");
}

#[test]
fn rebind_of_a_never_mutable_name_is_rejected() {
    let msg = lower_err("x ≔ 5\ny ≔ λ(z) → ˚x ⚙︎ z\n❧");
    assert!(msg.contains("not a mutable binding"), "message was: {msg}");
}

#[test]
fn nested_lambda_capture_is_not_inherited_from_the_outer_one() {
    // SEMANTIC_CORE.md §13: "nesting does not propagate reference-capture
    // status." The OUTER lambda explicitly captures x (and rebinds it),
    // but the INNER lambda's own plain rebind of the same free variable
    // must still fail — it never captured x itself.
    let msg = lower_err("x ≔˚ 0\nf ≔ λ(a) → ˚x ⚙︎ a ▷ (λ(b) → x ⚙︎ b)\n❧");
    assert!(msg.contains("desugars to a read"), "message was: {msg}");
}

// ── ADTs, Match, and Pattern lowering (SEMANTIC_CORE.md §9/§10) ────────
