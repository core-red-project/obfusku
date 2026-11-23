//! Mutable cells: rebinding, capture and sharing.

use super::support::*;

#[test]
fn mutcell_read_and_rebind_round_trip() {
    let module = run_module(vec![
        ("counter", mutcell(int(0))),
        ("_bump", mutrebind(var("counter"), int(42))),
        ("result", mutread(var("counter"))),
    ]);
    assert_eq!(as_int(module), 42);
}

#[test]
fn default_capture_of_a_mutcell_sees_rebinds_made_after_closure_creation() {
    // §13: default capture shares the live cell and re-reads it at call
    // time, so `peek` must observe a rebind made after its creation.
    let peek = lambda("_", mutread(var("counter")));
    let module = run_module(vec![
        ("counter", mutcell(int(0))),
        ("peek", peek),
        ("_rebind", mutrebind(var("counter"), int(99))),
        ("result", apply(var("peek"), boolean(true))),
    ]);
    assert_eq!(as_int(module), 99);
}

#[test]
fn explicit_capture_shares_the_same_cell_as_the_original_binding() {
    // bump = λstep → MutRebind(Var(counter), step)  (explicit-capture
    // rebind — the bare `Var(counter)` here is exactly what the surface
    // `˚counter ⚙︎ step` form desugars to).
    let bump = lambda("step", mutrebind(var("counter"), var("step")));
    let module = run_module(vec![
        ("counter", mutcell(int(0))),
        ("bump", bump),
        ("_call", apply(var("bump"), int(10))),
        ("result", mutread(var("counter"))),
    ]);
    assert_eq!(as_int(module), 10);
}

#[test]
fn multiple_closures_capturing_the_same_cell_observe_each_others_writes() {
    let counter_maker = mutcell(int(0));
    let module = run_module(vec![
        ("counter", counter_maker),
        (
            "bump",
            lambda("step", mutrebind(var("counter"), var("step"))),
        ),
        ("peek", lambda("_", mutread(var("counter")))),
        ("_c1", apply(var("bump"), int(1))),
        ("_c2", apply(var("bump"), int(2))),
        ("result", apply(var("peek"), boolean(true))),
    ]);
    assert_eq!(as_int(module), 2);
}

#[test]
fn reading_a_non_cell_value_is_a_runtime_error() {
    let module = Module {
        type_decls: vec![],
        bindings: vec![obfusku_core::ast::BindingGroup::Let(
            obfusku_core::ast::Binding {
                name: "bad".to_string(),
                value: mutread(int(5)),
                is_exported: false,
                span: sp(),
                declared_type: None,
            },
        )],
    };
    assert!(evaluate(&module).is_err());
}

// ---------------------------------------------------------------------
// ADTs / constructors / pattern matching
// ---------------------------------------------------------------------
