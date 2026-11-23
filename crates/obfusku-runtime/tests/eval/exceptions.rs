//! `raise` and `catch`: propagation, nesting and handlers.

use super::support::*;

#[test]
fn raise_without_catch_is_an_uncaught_error() {
    let module = Module {
        type_decls: vec![],
        bindings: vec![obfusku_core::ast::BindingGroup::Let(
            obfusku_core::ast::Binding {
                name: "bad".to_string(),
                value: raise(str_("boom")),
                is_exported: false,
                span: sp(),
                declared_type: None,
            },
        )],
    };
    assert!(evaluate(&module).is_err());
}

#[test]
fn catch_intercepts_a_raise_and_runs_the_handler() {
    let expr = catch(raise(int(42)), "e", var("e"));
    assert_eq!(as_int(run(expr)), 42);
}

#[test]
fn catch_with_no_raise_evaluates_to_the_body() {
    let expr = catch(int(1), "e", int(999));
    assert_eq!(as_int(run(expr)), 1);
}

#[test]
fn nested_catch_only_the_innermost_active_handler_intercepts() {
    // catch(catch(raise(1), "e", raise(var e)), "e2", var e2)
    // inner handler re-raises what it caught; outer handler catches that.
    let inner = catch(raise(int(1)), "e", raise(var("e")));
    let outer = catch(inner, "e2", var("e2"));
    assert_eq!(as_int(run(outer)), 1);
}

#[test]
fn raise_inside_a_handler_is_not_caught_by_the_same_catch() {
    // The handler re-raises; there is no *outer* catch, so this must be
    // an uncaught error, proving the frame deactivated before the
    // handler ran (per §15.1).
    let expr = catch(raise(int(1)), "e", raise(int(2)));
    let module = Module {
        type_decls: vec![],
        bindings: vec![obfusku_core::ast::BindingGroup::Let(
            obfusku_core::ast::Binding {
                name: "bad".to_string(),
                value: expr,
                is_exported: false,
                span: sp(),
                declared_type: None,
            },
        )],
    };
    assert!(evaluate(&module).is_err());
}

#[test]
fn catch_around_a_function_call_that_raises_deep_inside() {
    // thrower = λ_ → raise(5)
    // catch(thrower(unit), "e", e)
    let thrower = lambda("_", raise(int(5)));
    let module = run_module(vec![
        ("thrower", thrower),
        (
            "result",
            catch(apply(var("thrower"), boolean(true)), "e", var("e")),
        ),
    ]);
    assert_eq!(as_int(module), 5);
}

// ---------------------------------------------------------------------
// Cross-feature composition
// ---------------------------------------------------------------------
