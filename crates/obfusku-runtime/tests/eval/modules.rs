//! Preludes, exported bindings and dropping deep values.

use super::support::*;

#[test]
fn prelude_bindings_are_visible_to_the_module() {
    let module = Module {
        type_decls: vec![],
        bindings: vec![obfusku_core::ast::BindingGroup::Let(mk_binding(
            "result",
            apply(var("imported"), int(5)),
        ))],
    };
    let bindings = evaluate_with_prelude(
        &module,
        vec![(
            "imported".to_string(),
            obfusku_runtime::closure_from_core("x", &binop(BinOp::Add, var("x"), int(1))),
        )],
    )
    .expect("evaluation should not fail");
    let (_, _, result_value) = bindings
        .into_iter()
        .find(|(n, _, _)| n == "result")
        .unwrap();
    assert_eq!(as_int(result_value), 6);
}

#[test]
fn evaluate_with_prelude_returns_every_top_level_binding_with_its_export_flag() {
    let module = Module {
        type_decls: vec![],
        bindings: vec![
            obfusku_core::ast::BindingGroup::Let(obfusku_core::ast::Binding {
                name: "priv".to_string(),
                value: int(1),
                exported: false,
                span: sp(),
                declared_type: None,
            }),
            obfusku_core::ast::BindingGroup::Let(obfusku_core::ast::Binding {
                name: "pub_".to_string(),
                value: int(2),
                exported: true,
                span: sp(),
                declared_type: None,
            }),
        ],
    };
    let bindings =
        evaluate_with_prelude(&module, std::iter::empty()).expect("evaluation should not fail");
    assert_eq!(bindings.len(), 2);
    assert_eq!(bindings[0].0, "priv");
    assert!(!bindings[0].1);
    assert_eq!(bindings[1].0, "pub_");
    assert!(bindings[1].1);
}

#[test]
fn dropping_a_long_cons_chain_needs_no_deep_stack() {
    use std::rc::Rc;
    // 300k nodes would need megabytes of stack to free recursively.
    let handle = std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(|| {
            let mut list = Value::Unit;
            for i in 0..300_000 {
                list = Value::Adt {
                    tag: Rc::from("⁝"),
                    args: Rc::from(vec![Value::Int(i), list]),
                };
            }
            drop(list);
        })
        .unwrap();
    handle.join().unwrap();
}
