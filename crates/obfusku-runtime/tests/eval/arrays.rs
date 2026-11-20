//! Array literals, indexing and equality.

use super::support::*;

fn array_lit(elements: Vec<Expr>) -> Expr {
    Expr::ArrayLiteral {
        elements,
        span: sp(),
    }
}
fn index(array: Expr, idx: Expr) -> Expr {
    Expr::Index {
        array: Box::new(array),
        index: Box::new(idx),
        span: sp(),
    }
}

#[test]
fn array_literal_evaluates_its_elements_and_produces_an_array_value() {
    let v = run(array_lit(vec![int(1), int(2), int(3)]));
    match &v {
        Value::Array(elems) => {
            assert_eq!(elems.len(), 3);
            assert!(matches!(elems.get(0), Some(Value::Int(1))));
            assert!(matches!(elems.get(2), Some(Value::Int(3))));
        }
        other => panic!("expected an Array, got {other:?}"),
    }
}

#[test]
fn empty_array_literal_evaluates_to_an_empty_array() {
    let v = run(array_lit(vec![]));
    match &v {
        Value::Array(elems) => assert!(elems.is_empty()),
        other => panic!("expected an Array, got {other:?}"),
    }
}

#[test]
fn indexing_reads_the_element_at_position() {
    let v = run(index(array_lit(vec![int(10), int(20), int(30)]), int(1)));
    assert_eq!(as_int(v), 20);
}

#[test]
fn out_of_bounds_index_raises_invalidoperation() {
    let module = Module {
        type_decls: vec![],
        bindings: vec![obfusku_core::ast::BindingGroup::Let(mk_binding(
            "result",
            index(array_lit(vec![int(1), int(2)]), int(5)),
        ))],
    };
    match evaluate(&module) {
        Err(d) => assert!(d.message.contains("uncaught exception")),
        Ok(v) => panic!("expected an error, got {v:?}"),
    }
}

#[test]
fn negative_index_is_also_out_of_bounds() {
    let module = Module {
        type_decls: vec![],
        bindings: vec![obfusku_core::ast::BindingGroup::Let(mk_binding(
            "result",
            index(array_lit(vec![int(1)]), int(-1)),
        ))],
    };
    assert!(evaluate(&module).is_err());
}

#[test]
fn array_equality_is_structural_elementwise() {
    let a = array_lit(vec![int(1), int(2)]);
    let b = array_lit(vec![int(1), int(2)]);
    assert!(matches!(run(binop(BinOp::Eq, a, b)), Value::Bool(true)));
}

#[test]
fn array_inequality_by_length_or_element() {
    let a = array_lit(vec![int(1), int(2)]);
    let b = array_lit(vec![int(1), int(3)]);
    assert!(matches!(run(binop(BinOp::NotEq, a, b)), Value::Bool(true)));
}

#[test]
fn out_of_bounds_index_is_an_ordinary_catchable_exception() {
    let v = run(catch(index(array_lit(vec![int(1)]), int(9)), "e", int(0)));
    assert_eq!(as_int(v), 0);
}

// ── `evaluate_with_prelude` (imports) ───────────────────────────────
