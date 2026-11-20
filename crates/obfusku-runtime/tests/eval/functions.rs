//! Literals, functions, currying and closure capture.

use super::support::*;

#[test]
fn literals_evaluate_to_themselves() {
    assert!(matches!(run(int(42)), Value::Int(42)));
    assert!(matches!(run(real(1.5)), Value::Real(v) if v == 1.5));
    assert!(matches!(run(str_("hi")), Value::Str(ref s) if &**s == "hi"));
    assert!(matches!(run(boolean(true)), Value::Bool(true)));
}

// ---------------------------------------------------------------------
// Functions, application, currying, partial application
// ---------------------------------------------------------------------

#[test]
fn identity_function_returns_its_argument() {
    let id = lambda("x", var("x"));
    assert_eq!(as_int(run(apply(id, int(7)))), 7);
}

#[test]
fn curried_two_argument_function_via_nested_lambdas() {
    // const_ = λx → λy → x
    let const_ = lambda("x", lambda("y", var("x")));
    assert_eq!(as_int(run(apply2(const_, int(1), int(2)))), 1);
}

#[test]
fn partial_application_produces_a_reusable_closure() {
    let add_via_const_pattern = lambda("x", lambda("y", var("x")));
    let module = run_module(vec![
        ("give1", apply(add_via_const_pattern, int(1))),
        ("a", apply(var("give1"), int(100))),
        ("b", apply(var("give1"), int(200))),
    ]);
    // last binding ("b") should still be 1, proving the same partially
    // applied closure works across multiple calls without state bleed.
    assert_eq!(as_int(module), 1);
}

#[test]
fn calling_a_non_function_value_is_a_runtime_error() {
    let module = Module {
        type_decls: vec![],
        bindings: vec![obfusku_core::ast::BindingGroup::Let(
            obfusku_core::ast::Binding {
                name: "bad".to_string(),
                value: apply(int(5), int(1)),
                exported: false,
                span: sp(),
                declared_type: None,
            },
        )],
    };
    assert!(evaluate(&module).is_err());
}

// ---------------------------------------------------------------------
// Lexical closures: capture, shadowing, isolation, nesting
// ---------------------------------------------------------------------

#[test]
fn closure_captures_free_variable_from_definition_site_not_call_site() {
    // make_adder = λx → λy → x   (ignores y, returns the captured x)
    let make = lambda("x", lambda("y", var("x")));
    let module = run_module(vec![
        ("addFive", apply(make.clone(), int(5))),
        // calling addFive with a *different* value bound to `x` in this
        // scope must not leak in — closures are lexical, not dynamic.
        ("x", int(999)),
        ("result", apply(var("addFive"), int(1))),
    ]);
    assert_eq!(as_int(module), 5);
}

#[test]
fn nested_closures_each_capture_independently() {
    // outer = λx → λy → λz → x   — three levels deep, innermost still
    // sees the outermost binding.
    let inner = lambda("z", var("x"));
    let mid = lambda("y", inner);
    let outer = lambda("x", mid);
    let module = run_module(vec![(
        "result",
        apply(apply(apply(outer, int(1)), int(2)), int(3)),
    )]);
    assert_eq!(as_int(module), 1);
}

#[test]
fn two_calls_to_the_same_closure_are_isolated() {
    // λx → x, called twice with different arguments must not interfere.
    let id = lambda("x", var("x"));
    let module = run_module(vec![
        ("id", id),
        ("a", apply(var("id"), int(1))),
        ("b", apply(var("id"), int(2))),
    ]);
    assert_eq!(as_int(module), 2);
    // and re-confirm the first call's result independently:
    let module2 = run_module(vec![
        ("id2", lambda("x", var("x"))),
        ("a", apply(var("id2"), int(11))),
    ]);
    assert_eq!(as_int(module2), 11);
}

// ---------------------------------------------------------------------
// Mutation: cell identity, default (live) vs explicit capture
// ---------------------------------------------------------------------
