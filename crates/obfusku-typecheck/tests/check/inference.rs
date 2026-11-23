//! Literals, variables, application, cells, the value restriction, local bindings and diagnostics spans.

use super::support::*;

#[test]
fn duplicate_top_level_binding_reports_a_separate_note_at_the_first_occurrence() {
    let first_span = Span::new(SourceId(0), 10, 11);
    let second_span = Span::new(SourceId(0), 30, 31);
    let first = Binding {
        name: "x".to_string(),
        value: int(1),
        exported: false,
        span: first_span,
        declared_type: None,
    };
    let second = Binding {
        name: "x".to_string(),
        value: int(2),
        exported: false,
        span: second_span,
        declared_type: None,
    };
    let ds = check_err(vec![first, second]);

    assert_eq!(ds.len(), 2, "{}", describe(&ds));
    assert!(
        !ds.iter().any(|d| d.message.contains("byte")),
        "{}",
        describe(&ds)
    );

    let error = ds
        .iter()
        .find(|d| d.severity == obfusku_diagnostics::Severity::Error)
        .expect("expected an Error diagnostic");
    assert_eq!(error.primary, second_span);

    let note = ds
        .iter()
        .find(|d| d.severity == obfusku_diagnostics::Severity::Note)
        .expect("expected a Note diagnostic pointing at the first occurrence");
    assert_eq!(
        note.primary, first_span,
        "the Note must point at the *first* binding, not the duplicate"
    );
}

#[test]
fn integer_literal() {
    let typed = check_ok(vec![binding("x", int(5))]);
    assert_eq!(monomorphic_type(&typed, "x"), CoreType::Int);
}

#[test]
#[allow(clippy::approx_constant)]
fn real_literal() {
    let typed = check_ok(vec![binding("x", real(3.14))]);
    assert_eq!(monomorphic_type(&typed, "x"), CoreType::Real);
}

#[test]
fn string_literal() {
    let typed = check_ok(vec![binding("x", string("hello"))]);
    assert_eq!(monomorphic_type(&typed, "x"), CoreType::Str);
}

#[test]
fn boolean_literal() {
    let typed = check_ok(vec![binding("x", boolean(true))]);
    assert_eq!(monomorphic_type(&typed, "x"), CoreType::Bool);
}

#[test]
fn unit_literal_has_unit_type() {
    let typed = check_ok(vec![binding("x", unit())]);
    assert_eq!(monomorphic_type(&typed, "x"), CoreType::Unit);
}

#[test]
fn unit_literal_is_usable_as_an_ordinary_function_argument() {
    // The whole point of adding a Unit *value*: `f(∅)` type-checks like
    // any other one-argument application — no new Apply-typing rule.
    let f = binding("f", lambda("u", var("u")));
    let typed = check_ok(vec![f, binding("result", apply(var("f"), unit()))]);
    assert_eq!(monomorphic_type(&typed, "result"), CoreType::Unit);
}

#[test]
fn variable_lookup() {
    let typed = check_ok(vec![binding("x", int(5)), binding("y", var("x"))]);
    assert_eq!(monomorphic_type(&typed, "y"), CoreType::Int);
}

#[test]
fn lambda_parameter_inference() {
    // λx. x applied to an Int pins the parameter's type to Int through
    // ordinary unification, without any annotation.
    let typed = check_ok(vec![
        binding("id", lambda("x", var("x"))),
        binding("y", apply(var("id"), int(5))),
    ]);
    assert_eq!(monomorphic_type(&typed, "y"), CoreType::Int);
}

#[test]
fn application_inference() {
    let typed = check_ok(vec![
        binding("f", lambda("x", var("x"))),
        binding("y", apply(var("f"), string("hi"))),
    ]);
    assert_eq!(monomorphic_type(&typed, "y"), CoreType::Str);
}

#[test]
fn unknown_variable_is_rejected() {
    let ds = check_err(vec![binding("y", var("nope"))]);
    assert!(
        describe(&ds).contains("unknown variable"),
        "{}",
        describe(&ds)
    );
}

#[test]
fn applying_a_non_function_is_rejected() {
    let ds = check_err(vec![binding("y", apply(int(5), int(3)))]);
    assert!(
        describe(&ds).contains("cannot apply a value of type Int as a function"),
        "{}",
        describe(&ds)
    );
}

#[test]
fn incompatible_argument_type_is_rejected() {
    // `setter`'s parameter type gets pinned to Int by its own body
    // (rebinding an Int-typed cell) — applying it to a String must fail.
    let bindings = vec![
        binding(
            "counter",
            Expr::MutCell {
                initial: Box::new(int(0)),
                span: sp(),
            },
        ),
        binding("setter", lambda("x", mutrebind(var("counter"), var("x")))),
        binding("bad", apply(var("setter"), string("nope"))),
    ];
    let ds = check_err(bindings);
    assert!(
        describe(&ds).contains("argument type mismatch: expected Int, found String"),
        "{}",
        describe(&ds)
    );
}

#[test]
fn inconsistent_binding_is_rejected() {
    // The value-restriction negative case, spelled out as a plain type
    // error: `wrapped` is Apply-shaped (identity applied to identity),
    // so it is NOT generalized — its type is pinned by the first real
    // use (`a`), and the second, incompatible use (`b`) must fail.
    let bindings = vec![
        binding(
            "wrapped",
            apply(lambda("x", var("x")), lambda("y", var("y"))),
        ),
        binding("a", apply(var("wrapped"), int(5))),
        binding("b", apply(var("wrapped"), string("hi"))),
    ];
    let ds = check_err(bindings);
    assert!(
        describe(&ds).contains("expected Int, found String")
            || describe(&ds).contains("argument type mismatch"),
        "{}",
        describe(&ds)
    );
}

#[test]
fn invalid_mutation_is_rejected() {
    let bindings = vec![
        binding(
            "x",
            Expr::MutCell {
                initial: Box::new(int(5)),
                span: sp(),
            },
        ),
        binding("bad", mutrebind(var("x"), string("nope"))),
    ];
    let ds = check_err(bindings);
    assert!(
        describe(&ds).contains("cannot rebind: cell holds Int, but the new value has type String"),
        "{}",
        describe(&ds)
    );
}

#[test]
fn mutcell_produces_cell_of_the_initial_value_type() {
    let typed = check_ok(vec![binding("x", mutcell(int(5)))]);
    assert_eq!(
        monomorphic_type(&typed, "x"),
        CoreType::Cell(Box::new(CoreType::Int))
    );
}

#[test]
fn mutread_unwraps_cell_to_its_element_type() {
    let typed = check_ok(vec![
        binding("x", mutcell(int(5))),
        binding("y", mutread(var("x"))),
    ]);
    // y : Int, NOT Cell<Int> — reading actually removes the Cell wrapper
    // rather than behaving as an ordinary alias/reassignment of x.
    assert_eq!(monomorphic_type(&typed, "y"), CoreType::Int);
}

#[test]
fn mutrebind_produces_unit_not_the_cells_element_type() {
    let typed = check_ok(vec![
        binding("x", mutcell(int(5))),
        binding("y", mutrebind(var("x"), int(9))),
    ]);
    // Mutation is not ordinary reassignment-as-an-expression-value:
    // MutRebind's own result is Unit, never the value written or the
    // cell's element type.
    assert_eq!(monomorphic_type(&typed, "y"), CoreType::Unit);
}

#[test]
fn mutread_on_a_non_cell_is_rejected() {
    let ds = check_err(vec![binding("y", mutread(int(5)))]);
    assert!(
        describe(&ds).contains("cannot read: Int is not a mutable cell"),
        "{}",
        describe(&ds)
    );
}

#[test]
fn syntactic_value_generalizes_and_is_reusable_at_different_types() {
    // λx. x is a Lambda — eligible. Each use below gets its own fresh
    // instantiation, so both succeed even though they disagree on the
    // instantiated type.
    let typed = check_ok(vec![
        binding("id", lambda("x", var("x"))),
        binding("a", apply(var("id"), int(5))),
        binding("b", apply(var("id"), string("hi"))),
    ]);
    assert_eq!(monomorphic_type(&typed, "a"), CoreType::Int);
    assert_eq!(monomorphic_type(&typed, "b"), CoreType::Str);
}

#[test]
fn non_syntactic_value_does_not_generalize_even_though_it_looks_generic() {
    // Same shape as identity, but as an Apply, not a bare Lambda — not
    // a syntactic value (§19.1), so never generalized.
    let bindings = vec![
        binding(
            "wrapped",
            apply(lambda("x", var("x")), lambda("y", var("y"))),
        ),
        binding("a", apply(var("wrapped"), int(5))),
        binding("b", apply(var("wrapped"), string("hi"))),
    ];
    let ds = check_err(bindings);
    assert!(!ds.is_empty());
}

#[test]
fn mutcell_allocation_never_generalizes_even_when_bound_immutably_at_top_level() {
    // If a MutCell-producing binding were generalized, `r` could be
    // written as one type and read as another — a second, incompatible
    // mutation must be rejected.
    let bindings = vec![
        binding("r", mutcell(int(0))),
        binding("_write_int", mutrebind(var("r"), int(1))),
        binding("_write_string", mutrebind(var("r"), string("oops"))),
    ];
    let ds = check_err(bindings);
    assert!(
        describe(&ds).contains("cannot rebind: cell holds Int, but the new value has type String"),
        "{}",
        describe(&ds)
    );
}

#[test]
fn function_wrapping_mutation_is_monomorphic_not_polymorphic() {
    // `setter` is eligible for generalization (a Lambda) but its param
    // type gets pinned to Int by the cell's element type — nothing free
    // remains to quantify over, unlike `id` above.
    let typed = check_ok(vec![
        binding("counter", mutcell(int(0))),
        binding("setter", lambda("x", mutrebind(var("counter"), var("x")))),
    ]);
    match typed
        .bindings
        .iter()
        .find(|b| b.name == "setter")
        .unwrap()
        .ty
    {
        TypeResult::Monomorphic(CoreType::Function(ref p, ref r)) => {
            assert_eq!(**p, CoreType::Int);
            assert_eq!(**r, CoreType::Unit);
        }
        _ => panic!("expected setter : Int -> Unit, monomorphic"),
    }
}

#[test]
fn cell_of_int_and_cell_of_string_do_not_unify() {
    // Direct structural test: Cell<Int> is not compatible with
    // Cell<String> as an argument to MutRebind — no covariance,
    // contravariance, or coercion lets this through.
    let bindings = vec![
        binding("outer", mutcell(mutcell(int(5)))), // outer : Cell<Cell<Int>>
        binding("bad", mutrebind(var("outer"), mutcell(string("x")))),
    ];
    let ds = check_err(bindings);
    assert!(
        describe(&ds).contains("cell holds Cell<Int>, but the new value has type Cell<String>"),
        "{}",
        describe(&ds)
    );
}

#[test]
fn cell_element_type_polymorphism_works_correctly_through_a_generalized_function() {
    // A genuinely polymorphic function over Cell<T> (a Lambda, hence
    // eligible) correctly instantiates fresh at each element type — this
    // is unification working *with* invariance, not around it: each
    // instantiation independently requires its own consistent T.
    let typed = check_ok(vec![
        binding("read_it", lambda("c", mutread(var("c")))),
        binding("x", mutcell(int(5))),
        binding("y", mutcell(string("hi"))),
        binding("a", apply(var("read_it"), var("x"))),
        binding("b", apply(var("read_it"), var("y"))),
    ]);
    assert_eq!(monomorphic_type(&typed, "a"), CoreType::Int);
    assert_eq!(monomorphic_type(&typed, "b"), CoreType::Str);
}

#[test]
fn type_error_carries_the_source_span_from_the_core_expression() {
    let bad_span = Span::new(SourceId(7), 100, 110);
    let bindings = vec![Binding {
        name: "y".to_string(),
        value: Expr::Var("missing".to_string(), bad_span),
        exported: false,
        span: sp(),
        declared_type: None,
    }];
    let ds = check_err(bindings);
    assert_eq!(ds[0].primary, bad_span);
}

#[test]
fn let_binds_name_only_within_its_own_body() {
    let typed = check_ok(vec![binding(
        "result",
        let_expr("x", int(5), binop(BinOp::Add, var("x"), int(1))),
    )]);
    assert_eq!(monomorphic_type(&typed, "result"), CoreType::Int);
}

#[test]
fn let_value_is_not_in_its_own_scope_ordinary_non_recursive() {
    // `x`'s own value expression must not see `x` — referencing it there
    // is an ordinary unknown-variable error, matching §11's "value's
    // scope does not include itself."
    let ds = check_err(vec![binding("result", let_expr("x", var("x"), int(1)))]);
    assert!(!describe(&ds).is_empty());
}

#[test]
fn let_bound_value_generalizes_like_any_other_syntactic_value() {
    // §19.1's value restriction applies inside a `Let` exactly as it
    // does at module level: a `Lambda`-valued local binding generalizes,
    // so it can be used at two different types within its own body.
    let typed = check_ok(vec![binding(
        "result",
        let_expr(
            "id",
            lambda("x", var("x")),
            let_expr(
                "a",
                apply(var("id"), int(5)),
                let_expr("b", apply(var("id"), string("hi")), var("a")),
            ),
        ),
    )]);
    assert_eq!(monomorphic_type(&typed, "result"), CoreType::Int);
}

#[test]
fn nested_let_shadowing_resolves_to_the_innermost_binding() {
    let typed = check_ok(vec![binding(
        "result",
        let_expr("x", int(1), let_expr("x", string("shadowed"), var("x"))),
    )]);
    assert_eq!(monomorphic_type(&typed, "result"), CoreType::Str);
}

#[test]
fn letrec_single_member_supports_self_recursion() {
    // A one-member `LetRec` self-recursing — impossible via `Let` (§11:
    // its value is never in its own scope). A real base case pins the
    // recursive call's type to `Int`.
    let countdown = lambda(
        "n",
        match_expr(
            var("n"),
            vec![
                arm(plit_int(0), int(0)),
                arm(pwild(), apply(var("countdown"), int(0))),
            ],
        ),
    );
    let typed = check_ok(vec![binding(
        "result",
        letrec_expr(
            vec![binding("countdown", countdown)],
            apply(var("countdown"), int(5)),
        ),
    )]);
    assert_eq!(monomorphic_type(&typed, "result"), CoreType::Int);
}

#[test]
fn letrec_members_are_mutually_visible_within_the_group() {
    // isEven/isOdd, each referencing the *other* — real mutual
    // recursion, each with a concrete `Int → Bool` base case, so
    // `result`'s type resolves concretely instead of staying ambiguous.
    let is_even = lambda(
        "n",
        match_expr(
            var("n"),
            vec![
                arm(plit_int(0), boolean(true)),
                arm(pwild(), apply(var("isOdd"), int(0))),
            ],
        ),
    );
    let is_odd = lambda(
        "n",
        match_expr(
            var("n"),
            vec![
                arm(plit_int(0), boolean(false)),
                arm(pwild(), apply(var("isEven"), int(0))),
            ],
        ),
    );
    let typed = check_ok(vec![binding(
        "result",
        letrec_expr(
            vec![binding("isEven", is_even), binding("isOdd", is_odd)],
            apply(var("isEven"), int(4)),
        ),
    )]);
    assert_eq!(monomorphic_type(&typed, "result"), CoreType::Bool);
}

#[test]
fn letrec_declared_type_is_unified_before_the_body_is_inferred() {
    // Mirrors the module-level `FunctionDeclaration` annotation-checking
    // test: a `declared_type` that contradicts the body is a real,
    // caught type error, not silently ignored.
    let contradicting = Binding {
        name: "f".to_string(),
        value: lambda("x", string("not an int")),
        exported: false,
        span: sp(),
        declared_type: Some(CoreType::Function(
            Box::new(CoreType::Int),
            Box::new(CoreType::Int),
        )),
    };
    let ds = check_err(vec![binding(
        "result",
        letrec_expr(vec![contradicting], var("f")),
    )]);
    assert!(!describe(&ds).is_empty());
}
