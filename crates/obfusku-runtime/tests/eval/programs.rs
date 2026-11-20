//! Features combined in small programs.

use super::support::*;

#[test]
fn closure_over_a_mutable_cell_combined_with_match() {
    // toggle = λ_ → match MutRead(flag) with
    //            True → MutRebind(flag, False)
    //            False → MutRebind(flag, True)
    // then read it back.
    let toggle_body = match_(
        mutread(var("flag")),
        vec![
            (
                Pattern::Lit(obfusku_core::ast::Literal::Bool(true), sp()),
                mutrebind(var("flag"), boolean(false)),
            ),
            (pwild(), mutrebind(var("flag"), boolean(true))),
        ],
    );
    let module = run_module(vec![
        ("flag", mutcell(boolean(true))),
        ("toggle", lambda("_", toggle_body)),
        ("_t1", apply(var("toggle"), boolean(true))),
        ("_t2", apply(var("toggle"), boolean(true))),
        ("result", mutread(var("flag"))),
    ]);
    // toggled twice from true: true -> false -> true
    assert!(matches!(module, Value::Bool(true)));
}

#[test]
fn mutation_inside_a_catch_handler_is_observed_after_catch_returns() {
    let expr = catch(
        raise(boolean(true)),
        "_e",
        mutrebind(var("counter"), int(1)),
    );
    let module = run_module(vec![
        ("counter", mutcell(int(0))),
        ("_run", expr),
        ("result", mutread(var("counter"))),
    ]);
    assert_eq!(as_int(module), 1);
}

#[test]
fn recursive_function_over_an_adt_combined_with_tail_position_match() {
    // sum-depth style: same shape as the recursion test above but proves
    // ADT + closure + recursion compose without special-casing.
    let succ = |n: Expr| ctor("Succ", vec![n]);
    let n = succ(succ(ctor("Zero", vec![])));
    let is_zero_body = match_(
        var("n"),
        vec![
            (pctor("Zero", vec![]), boolean(true)),
            (
                pctor("Succ", vec![pvar("k")]),
                apply(var("is_zero"), var("k")),
            ),
        ],
    );
    let module = run_letrec(
        vec![("is_zero", lambda("n", is_zero_body))],
        vec![("result", apply(var("is_zero"), n))],
    );
    assert!(matches!(module, Value::Bool(true)));
}

#[test]
fn a_reused_top_level_name_keeps_the_binding_a_closure_resolved_to() {
    // Names resolve when a binding is compiled, so a later binding of the
    // same name does not reach back into an earlier closure.
    // `obfusku-typecheck::check` rejects duplicates outright; this only
    // pins the runtime's own behavior for hand-built Core.
    let capture_x = lambda("ignored", var("x"));
    let module = run_module(vec![
        ("x", int(1)),
        ("capture_x", capture_x),
        ("x", int(2)),
        ("result", apply(var("capture_x"), boolean(true))),
    ]);
    assert_eq!(as_int(module), 1);
}

// ---------------------------------------------------------------------
// Operators (`SEMANTIC_CORE.md` §20.2) — runtime evaluation. Typing is
// obfusku-typecheck's job (already tested there); these tests build
// well-typed Core directly and check actual evaluated results/errors.
// ---------------------------------------------------------------------
