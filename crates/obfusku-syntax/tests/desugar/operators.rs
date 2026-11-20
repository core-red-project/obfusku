//! Operator lowering, including short-circuit `∧` and `∨`.

use super::support::*;

#[test]
fn arithmetic_operator_lowers_to_core_binaryop_with_matching_op() {
    let module = lower_ok("x ≔ 1 ✚ 2\n❧");
    match &flat(&module)[0].value {
        Expr::BinaryOp { op, lhs, rhs, .. } => {
            assert_eq!(*op, CoreBinOp::Add);
            assert!(matches!(**lhs, Expr::Lit(Literal::Int(1), _)));
            assert!(matches!(**rhs, Expr::Lit(Literal::Int(2), _)));
        }
        other => panic!("expected a BinaryOp, got {other:?}"),
    }
}

#[test]
fn every_non_short_circuit_binop_lowers_to_the_matching_core_op() {
    let cases: &[(&str, CoreBinOp)] = &[
        ("1 ☠︎ 2", CoreBinOp::Sub),
        ("1 ✱ 2", CoreBinOp::Mul),
        ("1 ÷ 2", CoreBinOp::Div),
        ("1 ⌗ 2", CoreBinOp::Mod),
        ("1 < 2", CoreBinOp::Lt),
        ("1 > 2", CoreBinOp::Gt),
        ("1 \u{2264} 2", CoreBinOp::Le),
        ("1 \u{2265} 2", CoreBinOp::Ge),
        ("1 \u{2261} 2", CoreBinOp::Eq),
        ("1 \u{2260} 2", CoreBinOp::NotEq),
        ("◉ ⊻ ◎", CoreBinOp::Xor),
    ];
    for (src, expected) in cases {
        let module = lower_ok(&format!("x ≔ {src}\n❧"));
        match &flat(&module)[0].value {
            Expr::BinaryOp { op, .. } => assert_eq!(op, expected, "for {src:?}"),
            other => panic!("expected a BinaryOp for {src:?}, got {other:?}"),
        }
    }
}

#[test]
fn unary_operators_lower_to_core_unaryop() {
    let module = lower_ok("x ≔ ¬◉\n❧");
    match &flat(&module)[0].value {
        Expr::UnaryOp { op, operand, .. } => {
            assert_eq!(*op, CoreUnOp::Not);
            assert!(matches!(**operand, Expr::Lit(Literal::Bool(true), _)));
        }
        other => panic!("expected a UnaryOp, got {other:?}"),
    }

    let module2 = lower_ok("x ≔ −5\n❧");
    match &flat(&module2)[0].value {
        Expr::UnaryOp { op, .. } => assert_eq!(*op, CoreUnOp::Neg),
        other => panic!("expected a UnaryOp, got {other:?}"),
    }
}

#[test]
fn precedence_structure_survives_lowering_to_core() {
    // 1 ✚ 2 ✱ 3  must still be Add(1, Mul(2, 3)) once in Core, not
    // reassociated or flattened during lowering.
    let module = lower_ok("x ≔ 1 ✚ 2 ✱ 3\n❧");
    match &flat(&module)[0].value {
        Expr::BinaryOp { op, lhs, rhs, .. } => {
            assert_eq!(*op, CoreBinOp::Add);
            assert!(matches!(**lhs, Expr::Lit(Literal::Int(1), _)));
            match &**rhs {
                Expr::BinaryOp { op: inner_op, .. } => assert_eq!(*inner_op, CoreBinOp::Mul),
                other => panic!("expected inner BinaryOp, got {other:?}"),
            }
        }
        other => panic!("expected a BinaryOp, got {other:?}"),
    }
}

#[test]
fn and_never_reaches_core_as_a_binaryop_it_becomes_match() {
    // `∧` must lower to `Expr::Match`, never `Expr::BinaryOp` (§20.2) —
    // no `BinaryOp` node should appear anywhere in the tree.
    let module = lower_ok("x ≔ ◉ ∧ ◎\n❧");
    let value = &flat(&module)[0].value;
    assert!(
        !contains_binaryop(value),
        "∧ must never lower to Expr::BinaryOp anywhere in the tree, got {value:?}"
    );
    match value {
        Expr::Match {
            scrutinee, arms, ..
        } => {
            assert!(matches!(**scrutinee, Expr::Lit(Literal::Bool(true), _)));
            assert_eq!(arms.len(), 2);
            assert!(matches!(
                arms[0].pattern,
                obfusku_core::ast::Pattern::Lit(Literal::Bool(true), _)
            ));
            assert!(matches!(
                arms[1].pattern,
                obfusku_core::ast::Pattern::Lit(Literal::Bool(false), _)
            ));
            // Zero(false) arm's own result must be the second operand,
            // not a further-nested BinaryOp.
            assert!(matches!(
                *arms[0].result,
                Expr::Lit(Literal::Bool(false), _)
            ));
            assert!(matches!(
                *arms[1].result,
                Expr::Lit(Literal::Bool(false), _)
            ));
        }
        other => panic!("expected a Match, got {other:?}"),
    }
}

#[test]
fn or_never_reaches_core_as_a_binaryop_it_becomes_match() {
    let module = lower_ok("x ≔ ◎ ∨ ◉\n❧");
    let value = &flat(&module)[0].value;
    assert!(
        !contains_binaryop(value),
        "∨ must never lower to Expr::BinaryOp, got {value:?}"
    );
    assert!(matches!(value, Expr::Match { .. }));
}

#[test]
fn and_or_short_circuit_structure_names_the_correct_operand_as_the_conditional_branch() {
    // a ∧ b: True arm evaluates to b (the second operand); False arm is
    // the literal False, never touching b — this is what makes it
    // short-circuit once evaluated, checked here at the Core-shape level
    // (evaluation itself is a later slice).
    let module = lower_ok("x ≔ p ∧ q\n❧");
    match &flat(&module)[0].value {
        Expr::Match {
            scrutinee, arms, ..
        } => {
            assert!(matches!(**scrutinee, Expr::Var(ref n, _) if n == "p"));
            assert!(matches!(*arms[0].result, Expr::Var(ref n, _) if n == "q"));
            assert!(matches!(
                *arms[1].result,
                Expr::Lit(Literal::Bool(false), _)
            ));
        }
        other => panic!("expected a Match, got {other:?}"),
    }

    let module2 = lower_ok("y ≔ p ∨ q\n❧");
    match &flat(&module2)[0].value {
        Expr::Match { arms, .. } => {
            assert!(matches!(*arms[0].result, Expr::Lit(Literal::Bool(true), _)));
            assert!(matches!(*arms[1].result, Expr::Var(ref n, _) if n == "q"));
        }
        other => panic!("expected a Match, got {other:?}"),
    }
}

/// Recursively checks whether any `Expr::BinaryOp` appears anywhere in
/// the tree — used specifically to prove `∧`/`∨` leave no trace of
/// themselves as `BinaryOp` after lowering, not just that the top-level
/// node happens to be `Match`.
fn contains_binaryop(expr: &Expr) -> bool {
    match expr {
        Expr::BinaryOp { .. } => true,
        Expr::UnaryOp { operand, .. } => contains_binaryop(operand),
        Expr::Apply { func, arg, .. } => contains_binaryop(func) || contains_binaryop(arg),
        Expr::Lambda { body, .. } => contains_binaryop(body),
        Expr::MutCell { initial, .. } => contains_binaryop(initial),
        Expr::MutRead { cell, .. } => contains_binaryop(cell),
        Expr::MutRebind {
            cell, new_value, ..
        } => contains_binaryop(cell) || contains_binaryop(new_value),
        Expr::Constructor { args, .. } => args.iter().any(contains_binaryop),
        Expr::Match {
            scrutinee, arms, ..
        } => contains_binaryop(scrutinee) || arms.iter().any(|a| contains_binaryop(&a.result)),
        Expr::Raise { value, .. } => contains_binaryop(value),
        Expr::Catch {
            body, handler_body, ..
        } => contains_binaryop(body) || contains_binaryop(handler_body),
        Expr::Var(..) | Expr::Lit(..) => false,
        Expr::Let { value, body, .. } => contains_binaryop(value) || contains_binaryop(body),
        Expr::LetRec { bindings, body, .. } => {
            bindings.iter().any(|b| contains_binaryop(&b.value)) || contains_binaryop(body)
        }
        Expr::ArrayLiteral { elements, .. } => elements.iter().any(contains_binaryop),
        Expr::Index { array, index, .. } => contains_binaryop(array) || contains_binaryop(index),
        Expr::Field { base, .. } => contains_binaryop(base),
        Expr::Update { base, fields, .. } => {
            contains_binaryop(base) || fields.iter().any(|(_, e)| contains_binaryop(e))
        }
    }
}

// ---------------------------------------------------------------------
// Raise / Catch lowering (`SEMANTIC_CORE.md` §15.2's built-in `Exception`
// constructors, synthesized just like a user ADT's).
// ---------------------------------------------------------------------
