//! Conformance tests for `obfusku-typecheck` against the Core subset it
//! covers. Built directly against `obfusku_core::ast`, not through
//! `obfusku-syntax`, so these prove the type checker works independent
//! of the parser.

use obfusku_core::ast::{Binding, Expr, Literal, MatchArm, Module, Pattern, TypeDecl, VariantDecl};
use obfusku_core::types::Type as CoreType;
use obfusku_diagnostics::{Diagnostic, SourceId, Span};
use obfusku_typecheck::{check, TypeResult};

fn sp() -> Span {
    Span {
        source: SourceId(0),
        start: 0,
        end: 0,
    }
}

fn int(v: i64) -> Expr {
    Expr::Lit(Literal::Int(v), sp())
}
fn real(v: f64) -> Expr {
    Expr::Lit(Literal::Real(v), sp())
}
fn string(v: &str) -> Expr {
    Expr::Lit(Literal::Str(v.to_string()), sp())
}
fn boolean(v: bool) -> Expr {
    Expr::Lit(Literal::Bool(v), sp())
}
fn unit() -> Expr {
    Expr::Lit(Literal::Unit, sp())
}
fn var(name: &str) -> Expr {
    Expr::Var(name.to_string(), sp())
}
fn lambda(param: &str, body: Expr) -> Expr {
    Expr::Lambda {
        param: param.to_string(),
        param_ty: None,
        body: Box::new(body),
        span: sp(),
    }
}
fn annotated_lambda(param: &str, ty: CoreType, body: Expr) -> Expr {
    Expr::Lambda {
        param: param.to_string(),
        param_ty: Some(ty),
        body: Box::new(body),
        span: sp(),
    }
}
fn apply(f: Expr, a: Expr) -> Expr {
    Expr::Apply {
        func: Box::new(f),
        arg: Box::new(a),
        span: sp(),
    }
}
fn mutcell(init: Expr) -> Expr {
    Expr::MutCell {
        initial: Box::new(init),
        span: sp(),
    }
}
fn mutread(cell: Expr) -> Expr {
    Expr::MutRead {
        cell: Box::new(cell),
        span: sp(),
    }
}
fn mutrebind(cell: Expr, value: Expr) -> Expr {
    Expr::MutRebind {
        cell: Box::new(cell),
        new_value: Box::new(value),
        span: sp(),
    }
}

fn binding(name: &str, value: Expr) -> Binding {
    Binding {
        name: name.to_string(),
        value,
        exported: false,
        span: sp(),
        declared_type: None,
    }
}

fn let_expr(name: &str, value: Expr, body: Expr) -> Expr {
    Expr::Let {
        name: name.to_string(),
        value: Box::new(value),
        body: Box::new(body),
        span: sp(),
    }
}

fn letrec_expr(members: Vec<Binding>, body: Expr) -> Expr {
    Expr::LetRec {
        bindings: members,
        body: Box::new(body),
        span: sp(),
    }
}

fn module(bindings: Vec<Binding>) -> Module {
    Module {
        type_decls: Vec::new(),
        bindings: bindings
            .into_iter()
            .map(obfusku_core::ast::BindingGroup::Let)
            .collect(),
    }
}

fn check_ok(bindings: Vec<Binding>) -> obfusku_typecheck::TypedModule {
    check(&module(bindings))
        .unwrap_or_else(|ds| panic!("expected success, got errors: {}", describe(&ds)))
}

fn check_err(bindings: Vec<Binding>) -> Vec<Diagnostic> {
    match check(&module(bindings)) {
        Err(ds) => ds,
        Ok(_) => panic!("expected a type error, but the module checked successfully"),
    }
}

// ---------------------------------------------------------------------
// A duplicate top-level binding name (category-1 audit finding #26):
// this crate has no `SourceMap`, so it can never format the *first*
// occurrence's location as `line:col` itself — the fix is a separate
// `Note` diagnostic at that span (rendered normally by whoever does
// have a `SourceMap`), not a raw byte offset spliced into message
// text.
// ---------------------------------------------------------------------

#[test]
fn duplicate_top_level_binding_reports_a_separate_note_at_the_first_occurrence() {
    let first_span = Span {
        source: SourceId(0),
        start: 10,
        end: 11,
    };
    let second_span = Span {
        source: SourceId(0),
        start: 30,
        end: 31,
    };
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

// ── ADT test fixtures ────────────────────────────────────────────────
// Hand-built to match what obfusku-syntax's real lowering produces
// (§9): a TypeDecl plus one synthesized Binding per variant.

fn module_with_adts(type_decls: Vec<TypeDecl>, bindings: Vec<Binding>) -> Module {
    Module {
        type_decls,
        bindings: bindings
            .into_iter()
            .map(obfusku_core::ast::BindingGroup::Let)
            .collect(),
    }
}

fn check_ok_adt(
    type_decls: Vec<TypeDecl>,
    bindings: Vec<Binding>,
) -> obfusku_typecheck::TypedModule {
    check(&module_with_adts(type_decls, bindings))
        .unwrap_or_else(|ds| panic!("expected success, got errors: {}", describe(&ds)))
}

fn check_err_adt(type_decls: Vec<TypeDecl>, bindings: Vec<Binding>) -> Vec<Diagnostic> {
    match check(&module_with_adts(type_decls, bindings)) {
        Err(ds) => ds,
        Ok(_) => panic!("expected a type error, but the module checked successfully"),
    }
}

fn variant(tag: &str, fields: Vec<CoreType>) -> VariantDecl {
    VariantDecl {
        tag: tag.to_string(),
        fields,
        span: sp(),
    }
}

fn type_decl(tag: &str, type_params: Vec<&str>, variants: Vec<VariantDecl>) -> TypeDecl {
    TypeDecl {
        tag: tag.to_string(),
        type_params: type_params.into_iter().map(String::from).collect(),
        variants,
        span: sp(),
    }
}

/// `Option<t> = { Some(t) | None }` — the user's own suggested first
/// example, matched exactly.
fn option_type_decl() -> TypeDecl {
    type_decl(
        "Option",
        vec!["t"],
        vec![
            variant("Some", vec![CoreType::Param("t".to_string())]),
            variant("None", vec![]),
        ],
    )
}

/// The two constructor bindings `obfusku-syntax` would synthesize for
/// `option_type_decl()` — a unary `Lambda` for `Some`, a bare
/// `Constructor` value (no `Lambda` wrapper) for nullary `None`.
fn option_constructor_bindings() -> Vec<Binding> {
    vec![
        Binding {
            name: "Some".to_string(),
            value: Expr::Lambda {
                param: "$field0".to_string(),
                param_ty: None,
                body: Box::new(Expr::Constructor {
                    tag: "Some".to_string(),
                    args: vec![var("$field0")],
                    span: sp(),
                }),
                span: sp(),
            },
            exported: true,
            span: sp(),
            declared_type: None,
        },
        Binding {
            name: "None".to_string(),
            value: Expr::Constructor {
                tag: "None".to_string(),
                args: vec![],
                span: sp(),
            },
            exported: true,
            span: sp(),
            declared_type: None,
        },
    ]
}

/// `Result<t, e> = { Ok(t) | Err(e) }` — `t` and `e` independently
/// parameterized, per the user's explicit requirement to verify this,
/// not just Optional's single-parameter case.
fn result_type_decl() -> TypeDecl {
    type_decl(
        "Result",
        vec!["t", "e"],
        vec![
            variant("Ok", vec![CoreType::Param("t".to_string())]),
            variant("Err", vec![CoreType::Param("e".to_string())]),
        ],
    )
}

fn unary_constructor_binding(tag: &str) -> Binding {
    Binding {
        name: tag.to_string(),
        value: Expr::Lambda {
            param: "$field0".to_string(),
            param_ty: None,
            body: Box::new(Expr::Constructor {
                tag: tag.to_string(),
                args: vec![var("$field0")],
                span: sp(),
            }),
            span: sp(),
        },
        exported: true,
        span: sp(),
        declared_type: None,
    }
}

fn result_constructor_bindings() -> Vec<Binding> {
    vec![
        unary_constructor_binding("Ok"),
        unary_constructor_binding("Err"),
    ]
}

fn pvar(name: &str) -> Pattern {
    Pattern::Var(name.to_string(), sp())
}
fn pwild() -> Pattern {
    Pattern::Wildcard(sp())
}
fn plit_int(v: i64) -> Pattern {
    Pattern::Lit(Literal::Int(v), sp())
}
fn plit_bool(v: bool) -> Pattern {
    Pattern::Lit(Literal::Bool(v), sp())
}
fn pctor(tag: &str, args: Vec<Pattern>) -> Pattern {
    Pattern::Constructor {
        tag: tag.to_string(),
        args,
        span: sp(),
    }
}
fn arm(pattern: Pattern, result: Expr) -> MatchArm {
    MatchArm {
        pattern,
        result,
        span: sp(),
    }
}
fn match_expr(scrutinee: Expr, arms: Vec<MatchArm>) -> Expr {
    Expr::Match {
        scrutinee: Box::new(scrutinee),
        arms,
        span: sp(),
    }
}

fn describe(ds: &[Diagnostic]) -> String {
    ds.iter()
        .map(|d| d.message.clone())
        .collect::<Vec<_>>()
        .join("; ")
}

fn monomorphic_type(typed: &obfusku_typecheck::TypedModule, name: &str) -> CoreType {
    let b = typed
        .bindings
        .iter()
        .find(|b| b.name == name)
        .unwrap_or_else(|| panic!("no binding named '{name}' in typed module"));
    match &b.ty {
        TypeResult::Monomorphic(t) => t.clone(),
        TypeResult::Polymorphic(s) => panic!(
            "'{name}' is polymorphic ({:?}), expected monomorphic",
            s.vars
        ),
    }
}

// ── basic inference ──────────────────────────────────────────────────

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

// ── failure cases ────────────────────────────────────────────────────

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

// ── mutability: T vs Cell<T> ─────────────────────────────────────────

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

// ── value restriction (SEMANTIC_CORE.md §19.1) — mandatory ─────────────

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

// ── Cell<T> invariance ───────────────────────────────────────────────

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

// ── diagnostics: spans from the existing Core representation ──────────

#[test]
fn type_error_carries_the_source_span_from_the_core_expression() {
    let bad_span = Span {
        source: SourceId(7),
        start: 100,
        end: 110,
    };
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

// ── ADTs: polymorphic constructors ──────────────────────────────────────

#[test]
fn constructor_is_polymorphic_and_reusable_at_different_types() {
    // Some : ∀t. t → Option<t> — each application instantiates fresh,
    // exactly like `id`.
    let mut bindings = option_constructor_bindings();
    bindings.push(binding("a", apply(var("Some"), int(5))));
    bindings.push(binding("b", apply(var("Some"), string("hi"))));
    let typed = check_ok_adt(vec![option_type_decl()], bindings);
    assert_eq!(
        monomorphic_type(&typed, "a"),
        CoreType::Adt("Option".into(), vec![CoreType::Int])
    );
    assert_eq!(
        monomorphic_type(&typed, "b"),
        CoreType::Adt("Option".into(), vec![CoreType::Str])
    );
}

#[test]
fn nullary_constructor_has_the_adt_type_directly_no_function_wrapper() {
    let mut bindings = option_constructor_bindings();
    bindings.push(binding("n", var("None")));
    let typed = check_ok_adt(vec![option_type_decl()], bindings);
    // None : ∀t. Option<t> — a polymorphic *value*, not a function.
    match &typed.bindings.iter().find(|b| b.name == "n").unwrap().ty {
        TypeResult::Polymorphic(scheme) => {
            assert!(!scheme.vars.is_empty());
            assert!(matches!(scheme.ty, obfusku_typecheck::Type::Adt(ref n, _) if n == "Option"));
        }
        other => panic!("expected 'n' to be polymorphic (via None), got {other:?}"),
    }
}

#[test]
fn constructor_field_type_mismatch_is_rejected() {
    let intbox = type_decl("IntBox", vec![], vec![variant("Box", vec![CoreType::Int])]);
    let box_ctor = Binding {
        name: "Box".to_string(),
        value: Expr::Lambda {
            param: "$field0".to_string(),
            param_ty: None,
            body: Box::new(Expr::Constructor {
                tag: "Box".to_string(),
                args: vec![var("$field0")],
                span: sp(),
            }),
            span: sp(),
        },
        exported: true,
        span: sp(),
        declared_type: None,
    };
    // Caught at the ordinary Apply boundary: Box's type is already the
    // concrete `Int → IntBox`, so a String argument fails like any
    // wrongly-typed call.
    let ds = check_err_adt(
        vec![intbox],
        vec![box_ctor, binding("bad", apply(var("Box"), string("nope")))],
    );
    assert!(
        describe(&ds).contains("argument type mismatch: expected Int, found String"),
        "{}",
        describe(&ds)
    );
}

#[test]
fn constructor_node_itself_rejects_a_mismatched_field_type() {
    // The Constructor-typing code path directly, bypassing the Lambda/
    // Apply layer entirely — this is what a malformed constructor body
    // would hit (the Lambda/Apply check above can never reach this,
    // since it always catches the mismatch first for an ordinary call).
    let intbox = type_decl("IntBox", vec![], vec![variant("Box", vec![CoreType::Int])]);
    let bindings = vec![binding(
        "bad",
        Expr::Constructor {
            tag: "Box".to_string(),
            args: vec![string("nope")],
            span: sp(),
        },
    )];
    let ds = check_err_adt(vec![intbox], bindings);
    assert!(
        describe(&ds).contains("field type mismatch: expected Int, found String"),
        "{}",
        describe(&ds)
    );
}

// ── Match / Pattern typing ──────────────────────────────────────────────

#[test]
fn match_over_option_binds_the_pattern_variable_at_the_correct_type() {
    let mut bindings = option_constructor_bindings();
    bindings.push(binding("x", apply(var("Some"), int(5))));
    bindings.push(binding(
        "y",
        match_expr(
            var("x"),
            vec![
                arm(pctor("Some", vec![pvar("n")]), var("n")),
                arm(pctor("None", vec![]), int(0)),
            ],
        ),
    ));
    let typed = check_ok_adt(vec![option_type_decl()], bindings);
    assert_eq!(monomorphic_type(&typed, "y"), CoreType::Int);
}

#[test]
fn match_arms_with_incompatible_result_types_are_rejected() {
    let mut bindings = option_constructor_bindings();
    bindings.push(binding("x", apply(var("Some"), int(5))));
    bindings.push(binding(
        "y",
        match_expr(
            var("x"),
            vec![
                arm(pctor("Some", vec![pvar("n")]), var("n")),
                arm(pctor("None", vec![]), string("zero")),
            ],
        ),
    ));
    let ds = check_err_adt(vec![option_type_decl()], bindings);
    assert!(
        describe(&ds).contains("match arms have incompatible types"),
        "{}",
        describe(&ds)
    );
}

#[test]
fn wildcard_arm_alone_is_always_exhaustive() {
    let mut bindings = option_constructor_bindings();
    bindings.push(binding("x", apply(var("Some"), int(5))));
    bindings.push(binding(
        "y",
        match_expr(var("x"), vec![arm(pwild(), int(1))]),
    ));
    let typed = check_ok_adt(vec![option_type_decl()], bindings);
    assert_eq!(monomorphic_type(&typed, "y"), CoreType::Int);
}

// ── exhaustiveness (SEMANTIC_CORE.md §17) — mandatory ───────────────────

#[test]
fn exhaustive_match_over_closed_adt_succeeds() {
    let mut bindings = option_constructor_bindings();
    bindings.push(binding("x", apply(var("Some"), int(5))));
    bindings.push(binding(
        "y",
        match_expr(
            var("x"),
            vec![
                arm(pctor("Some", vec![pvar("n")]), var("n")),
                arm(pctor("None", vec![]), int(0)),
            ],
        ),
    ));
    assert!(check(&module_with_adts(vec![option_type_decl()], bindings)).is_ok());
}

#[test]
fn non_exhaustive_match_names_the_missing_case() {
    let mut bindings = option_constructor_bindings();
    bindings.push(binding("x", apply(var("Some"), int(5))));
    bindings.push(binding(
        "y",
        match_expr(
            var("x"),
            vec![arm(pctor("Some", vec![pvar("n")]), var("n"))],
        ),
    ));
    let ds = check_err_adt(vec![option_type_decl()], bindings);
    assert!(
        describe(&ds).contains("non-exhaustive match: missing case(s) for None"),
        "{}",
        describe(&ds)
    );
}

#[test]
fn nested_exhaustiveness_succeeds_when_every_position_is_covered() {
    // Option<Option<Int>> — the nested case SEMANTIC_CORE.md §17
    // requires recursing into, not just checking the top level.
    let mut bindings = option_constructor_bindings();
    bindings.push(binding(
        "xx",
        apply(var("Some"), apply(var("Some"), int(5))),
    ));
    bindings.push(binding(
        "y",
        match_expr(
            var("xx"),
            vec![
                arm(
                    pctor("Some", vec![pctor("Some", vec![pvar("n")])]),
                    var("n"),
                ),
                arm(pctor("Some", vec![pctor("None", vec![])]), int(0)),
                arm(pctor("None", vec![]), int(0)),
            ],
        ),
    ));
    assert!(check(&module_with_adts(vec![option_type_decl()], bindings)).is_ok());
}

#[test]
fn nested_exhaustiveness_catches_a_missing_inner_case() {
    // Top level covers Some/None; the INNER position (Some's own field)
    // does not cover None — the recursive check, not the top-level one,
    // must be what fires here.
    let mut bindings = option_constructor_bindings();
    bindings.push(binding(
        "xx",
        apply(var("Some"), apply(var("Some"), int(5))),
    ));
    bindings.push(binding(
        "y",
        match_expr(
            var("xx"),
            vec![
                arm(
                    pctor("Some", vec![pctor("Some", vec![pvar("n")])]),
                    var("n"),
                ),
                arm(pctor("None", vec![]), int(0)),
                // Missing: Some(None) — the inner Option's None case.
            ],
        ),
    ));
    let ds = check_err_adt(vec![option_type_decl()], bindings);
    assert!(
        describe(&ds).contains("non-exhaustive match: missing case(s) for None"),
        "{}",
        describe(&ds)
    );
}

#[test]
fn bool_exhaustiveness_requires_both_true_and_false() {
    let bindings = vec![binding(
        "y",
        match_expr(boolean(true), vec![arm(plit_bool(true), int(1))]),
    )];
    let ds = check_err(bindings);
    assert!(
        describe(&ds).contains("both 'true' and 'false'"),
        "{}",
        describe(&ds)
    );
}

#[test]
fn bool_exhaustiveness_succeeds_when_both_covered() {
    let bindings = vec![binding(
        "y",
        match_expr(
            boolean(true),
            vec![arm(plit_bool(true), int(1)), arm(plit_bool(false), int(0))],
        ),
    )];
    assert!(check(&module(bindings)).is_ok());
}

#[test]
fn open_type_match_requires_a_wildcard() {
    let bindings = vec![binding(
        "y",
        match_expr(
            int(5),
            vec![arm(plit_int(0), int(0)), arm(plit_int(1), int(1))],
        ),
    )];
    let ds = check_err(bindings);
    assert!(
        describe(&ds).contains("infinitely many values"),
        "{}",
        describe(&ds)
    );
}

#[test]
fn open_type_match_succeeds_with_a_wildcard() {
    let bindings = vec![binding(
        "y",
        match_expr(int(5), vec![arm(plit_int(0), int(0)), arm(pwild(), int(1))]),
    )];
    assert!(check(&module(bindings)).is_ok());
}

// ── unreachable arms (diagnostic aid, not spec-mandated — Warning) ──────

#[test]
fn duplicate_tag_arm_produces_an_unreachable_warning() {
    let mut bindings = option_constructor_bindings();
    bindings.push(binding("x", apply(var("Some"), int(5))));
    bindings.push(binding(
        "y",
        match_expr(
            var("x"),
            vec![
                arm(pctor("Some", vec![pvar("n")]), var("n")),
                arm(pctor("Some", vec![pvar("m")]), var("m")),
                arm(pctor("None", vec![]), int(0)),
            ],
        ),
    ));
    let typed = check_ok_adt(vec![option_type_decl()], bindings);
    assert!(
        typed
            .warnings
            .iter()
            .any(|w| w.message.contains("unreachable") && w.message.contains("Some")),
        "{:?}",
        typed.warnings
    );
}

#[test]
fn arm_after_a_catchall_produces_an_unreachable_warning() {
    let mut bindings = option_constructor_bindings();
    bindings.push(binding("x", apply(var("Some"), int(5))));
    bindings.push(binding(
        "y",
        match_expr(
            var("x"),
            vec![
                arm(pvar("anything"), int(1)),
                arm(pctor("None", vec![]), int(0)),
            ],
        ),
    ));
    let typed = check_ok_adt(vec![option_type_decl()], bindings);
    assert!(
        typed
            .warnings
            .iter()
            .any(|w| w.message.contains("unreachable")),
        "{:?}",
        typed.warnings
    );
}

// ── malformed patterns/constructors ─────────────────────────────────────

#[test]
fn unknown_constructor_in_pattern_is_rejected() {
    let mut bindings = option_constructor_bindings();
    bindings.push(binding("x", apply(var("Some"), int(5))));
    bindings.push(binding(
        "y",
        match_expr(var("x"), vec![arm(pctor("Bogus", vec![]), int(0))]),
    ));
    let ds = check_err_adt(vec![option_type_decl()], bindings);
    assert!(
        describe(&ds).contains("unknown constructor 'Bogus'"),
        "{}",
        describe(&ds)
    );
}

#[test]
fn wrong_arity_constructor_pattern_is_rejected() {
    let mut bindings = option_constructor_bindings();
    bindings.push(binding("x", apply(var("Some"), int(5))));
    bindings.push(binding(
        "y",
        match_expr(
            var("x"),
            vec![
                arm(pctor("Some", vec![pvar("a"), pvar("b")]), int(0)),
                arm(pctor("None", vec![]), int(0)),
            ],
        ),
    ));
    let ds = check_err_adt(vec![option_type_decl()], bindings);
    assert!(
        describe(&ds).contains("expects 1 argument(s), found 2"),
        "{}",
        describe(&ds)
    );
}

#[test]
fn constructor_used_as_a_syntactic_value_is_recursively_checked() {
    // Some(5) is Apply-shaped at the point of use — an Apply-shaped
    // binding is never generalized regardless of what's inside (§19.1).
    let mut bindings = option_constructor_bindings();
    bindings.push(binding("a", apply(var("Some"), int(5))));
    let typed = check_ok_adt(vec![option_type_decl()], bindings);
    match &typed.bindings.iter().find(|b| b.name == "a").unwrap().ty {
        TypeResult::Monomorphic(_) => {}
        other => panic!("expected 'a' to be monomorphic (Apply-shaped), got {other:?}"),
    }
}

// ── Optional<T> / Result<T, E> — ordinary ADT instances, no special-case
//    language feature involved ─────────────────────────────────────────

#[test]
fn optional_under_its_real_name_behaves_identically_to_the_earlier_option_fixture() {
    // Deliberately a *different* tag ("Optional", not "Option") from the
    // fixture used throughout the ADT slice — proving the mechanism
    // isn't accidentally keyed to that one specific name.
    let optional = type_decl(
        "Optional",
        vec!["t"],
        vec![
            variant("Some", vec![CoreType::Param("t".to_string())]),
            variant("None", vec![]),
        ],
    );
    let mut bindings = option_constructor_bindings();
    bindings.push(binding("a", apply(var("Some"), int(5))));
    let typed = check_ok_adt(vec![optional], bindings);
    assert_eq!(
        monomorphic_type(&typed, "a"),
        CoreType::Adt("Optional".into(), vec![CoreType::Int])
    );
}

// ── Result<T, E>: construction, independent T/E instantiation ─────────

#[test]
fn ok_and_err_construct_and_independently_instantiate_their_type_parameter() {
    // Ok(5)/Err("boom") are Apply-shaped, so each is monomorphic with
    // its other parameter free until pinned by an actual later use.
    let mut bindings = result_constructor_bindings();
    bindings.push(binding("success", apply(var("Ok"), int(5))));
    bindings.push(binding("failure", apply(var("Err"), string("boom"))));
    // Pin success's E to String, and failure's T to Int, via ordinary
    // Match usage — independently, in different arms, of each other.
    bindings.push(binding(
        "pin_success",
        match_expr(
            var("success"),
            vec![
                arm(pctor("Ok", vec![pwild()]), string("")),
                arm(pctor("Err", vec![pvar("e")]), var("e")),
            ],
        ),
    ));
    bindings.push(binding(
        "pin_failure",
        match_expr(
            var("failure"),
            vec![
                arm(pctor("Ok", vec![pvar("t")]), var("t")),
                arm(pctor("Err", vec![pwild()]), int(0)),
            ],
        ),
    ));
    let typed = check_ok_adt(vec![result_type_decl()], bindings);
    assert_eq!(
        monomorphic_type(&typed, "success"),
        CoreType::Adt("Result".into(), vec![CoreType::Int, CoreType::Str])
    );
    assert_eq!(
        monomorphic_type(&typed, "failure"),
        CoreType::Adt("Result".into(), vec![CoreType::Int, CoreType::Str])
    );
}

#[test]
fn result_int_string_infers_both_parameters_together_when_both_are_used() {
    // A function using a Result at both arms simultaneously pins both T
    // and E to concrete types from ordinary unification — no
    // Result-specific inference rule involved.
    let mut bindings = result_constructor_bindings();
    bindings.push(binding(
        "describe",
        lambda(
            "r",
            match_expr(
                var("r"),
                vec![
                    arm(pctor("Ok", vec![pvar("n")]), var("n")),
                    arm(pctor("Err", vec![pvar("msg")]), int(0)),
                ],
            ),
        ),
    ));
    bindings.push(binding(
        "y",
        apply(var("describe"), apply(var("Ok"), int(5))),
    ));
    let typed = check_ok_adt(vec![result_type_decl()], bindings);
    assert_eq!(monomorphic_type(&typed, "y"), CoreType::Int);
}

#[test]
fn nested_result_type_checks_and_exhausts_recursively() {
    // Result<Result<Int, Int>, Int> — nested exhaustiveness across both
    // the outer and inner Ok/Err at once.
    let mut bindings = result_constructor_bindings();
    bindings.push(binding("inner_ok", apply(var("Ok"), int(5))));
    bindings.push(binding("outer", apply(var("Ok"), var("inner_ok"))));
    bindings.push(binding(
        "y",
        match_expr(
            var("outer"),
            vec![
                arm(pctor("Ok", vec![pctor("Ok", vec![pvar("n")])]), var("n")),
                arm(
                    pctor("Ok", vec![pctor("Err", vec![pvar("msg")])]),
                    var("msg"),
                ),
                arm(pctor("Err", vec![pvar("b")]), var("b")),
            ],
        ),
    ));
    let typed = check_ok_adt(vec![result_type_decl()], bindings);
    assert_eq!(monomorphic_type(&typed, "y"), CoreType::Int);
}

#[test]
fn nested_result_exhaustiveness_catches_a_missing_inner_case() {
    let mut bindings = result_constructor_bindings();
    bindings.push(binding("inner_ok", apply(var("Ok"), int(5))));
    bindings.push(binding("outer", apply(var("Ok"), var("inner_ok"))));
    bindings.push(binding(
        "y",
        match_expr(
            var("outer"),
            vec![
                arm(pctor("Ok", vec![pctor("Ok", vec![pvar("n")])]), var("n")),
                // Missing: Ok(Err(_)) — the inner Result's Err case.
                arm(pctor("Err", vec![pvar("b")]), int(0)),
            ],
        ),
    ));
    let ds = check_err_adt(vec![result_type_decl()], bindings);
    assert!(
        describe(&ds).contains("non-exhaustive match: missing case(s) for Err"),
        "{}",
        describe(&ds)
    );
}

#[test]
fn incomplete_result_match_names_the_missing_case() {
    let mut bindings = result_constructor_bindings();
    bindings.push(binding("r", apply(var("Ok"), int(5))));
    bindings.push(binding(
        "y",
        match_expr(var("r"), vec![arm(pctor("Ok", vec![pvar("n")]), var("n"))]),
    ));
    let ds = check_err_adt(vec![result_type_decl()], bindings);
    assert!(
        describe(&ds).contains("non-exhaustive match: missing case(s) for Err"),
        "{}",
        describe(&ds)
    );
}

#[test]
fn result_payload_type_mismatch_is_rejected() {
    // A fixed (non-generic) context forcing Result<Int, String> — Err's
    // payload here is an Int, not the required String.
    let pair = type_decl(
        "Pair",
        vec![],
        vec![variant(
            "MkPair",
            vec![CoreType::Adt(
                "Result".to_string(),
                vec![CoreType::Int, CoreType::Str],
            )],
        )],
    );
    let mkpair = unary_constructor_binding("MkPair");
    let mut bindings = result_constructor_bindings();
    bindings.push(mkpair);
    bindings.push(binding(
        "bad",
        apply(var("MkPair"), apply(var("Err"), int(42))),
    ));
    let ds = check_err_adt(vec![result_type_decl(), pair], bindings);
    assert!(
        describe(&ds).contains("expected String, found Int")
            || describe(&ds).contains("expected Result<Int, String>"),
        "{}",
        describe(&ds)
    );
}

#[test]
fn result_composed_with_optional_both_directions_type_check() {
    // Two independently-declared ADTs composing with no special glue.
    let mut bindings = option_constructor_bindings();
    bindings.extend(result_constructor_bindings());
    bindings.push(binding(
        "a", // Result<Optional<Int>, ?>
        apply(var("Ok"), apply(var("Some"), int(5))),
    ));
    bindings.push(binding(
        "pin_a",
        match_expr(
            var("a"),
            vec![
                arm(pctor("Ok", vec![pwild()]), string("")),
                arm(pctor("Err", vec![pvar("e")]), var("e")),
            ],
        ),
    ));
    bindings.push(binding(
        "b", // Optional<Result<Int, ?>>
        apply(var("Some"), apply(var("Ok"), int(5))),
    ));
    bindings.push(binding(
        "pin_b",
        match_expr(
            var("b"),
            vec![
                arm(pctor("Some", vec![pctor("Ok", vec![pwild()])]), string("")),
                arm(pctor("Some", vec![pctor("Err", vec![pvar("e")])]), var("e")),
                arm(pctor("None", vec![]), string("")),
            ],
        ),
    ));
    let typed = check_ok_adt(vec![option_type_decl(), result_type_decl()], bindings);
    match &typed.bindings.iter().find(|b| b.name == "a").unwrap().ty {
        TypeResult::Monomorphic(CoreType::Adt(name, args)) => {
            assert_eq!(name, "Result");
            assert!(matches!(&args[0], CoreType::Adt(n, _) if n == "Option"));
            assert_eq!(args[1], CoreType::Str);
        }
        other => panic!("unexpected type for 'a': {other:?}"),
    }
    match &typed.bindings.iter().find(|b| b.name == "b").unwrap().ty {
        TypeResult::Monomorphic(CoreType::Adt(name, args)) => {
            assert_eq!(name, "Option");
            assert!(matches!(&args[0], CoreType::Adt(n, _) if n == "Result"));
        }
        other => panic!("unexpected type for 'b': {other:?}"),
    }
}

// ── value restriction (§19.1) applies to Result exactly as to any ADT ──

#[test]
fn result_constructor_binding_generalizes_like_any_other_lambda() {
    // `Ok` itself generalizes fully (∀t,e. t → Result<t,e>), reusable
    // like `id`; each use is Apply-shaped and individually monomorphic.
    let mut bindings = result_constructor_bindings();
    bindings.push(binding("a", apply(var("Ok"), int(5))));
    bindings.push(binding("b", apply(var("Ok"), string("hi"))));
    bindings.push(binding(
        "pin_a",
        match_expr(
            var("a"),
            vec![
                arm(pctor("Ok", vec![pwild()]), string("")),
                arm(pctor("Err", vec![pvar("e")]), var("e")),
            ],
        ),
    ));
    bindings.push(binding(
        "pin_b",
        match_expr(
            var("b"),
            vec![
                arm(pctor("Ok", vec![pwild()]), string("")),
                arm(pctor("Err", vec![pvar("e")]), var("e")),
            ],
        ),
    ));
    let typed = check_ok_adt(vec![result_type_decl()], bindings);
    assert_eq!(
        monomorphic_type(&typed, "a"),
        CoreType::Adt("Result".into(), vec![CoreType::Int, CoreType::Str])
    );
    assert_eq!(
        monomorphic_type(&typed, "b"),
        CoreType::Adt("Result".into(), vec![CoreType::Str, CoreType::Str])
    );
}

#[test]
fn apply_shaped_result_expression_does_not_generalize() {
    // Mirrors `wrapped` exactly, with Result substituted for the
    // identity-function example — no special-casing needed to make the
    // value restriction hold here too. Asserted directly against the
    // TypeResult representation, not indirectly via a second call site.
    let mut bindings = result_constructor_bindings();
    bindings.push(binding(
        "wrapped",
        apply(lambda("x", apply(var("Ok"), var("x"))), int(5)),
    ));
    bindings.push(binding(
        "pin",
        match_expr(
            var("wrapped"),
            vec![
                arm(pctor("Ok", vec![pwild()]), string("")),
                arm(pctor("Err", vec![pvar("e")]), var("e")),
            ],
        ),
    ));
    let typed = check_ok_adt(vec![result_type_decl()], bindings);
    match &typed
        .bindings
        .iter()
        .find(|b| b.name == "wrapped")
        .unwrap()
        .ty
    {
        TypeResult::Monomorphic(CoreType::Adt(name, args)) => {
            assert_eq!(name, "Result");
            assert_eq!(args[0], CoreType::Int);
            assert_eq!(args[1], CoreType::Str);
        }
        other => {
            panic!("expected 'wrapped' to be a monomorphic Result<Int, String>, got {other:?}")
        }
    }
}

// ── Expected failure (Result) vs exceptional failure (Raise/Catch) —
//    two genuinely different mechanisms (§6/§15). Result stays an
//    ordinary value with no implicit unwrapping. ────────────────────

#[test]
fn err_value_has_no_special_type_representation_distinct_from_any_other_adt() {
    // Comes back through the exact same
    // `TypeResult::Monomorphic(CoreType::Adt(...))` shape any ordinary
    // ADT value uses — no separate "exceptional" type representation.
    let mut bindings = result_constructor_bindings();
    bindings.push(binding("e", apply(var("Err"), string("boom"))));
    bindings.push(binding(
        "pin",
        match_expr(
            var("e"),
            vec![
                arm(pctor("Ok", vec![pvar("t")]), var("t")),
                arm(pctor("Err", vec![pwild()]), string("")),
            ],
        ),
    ));
    let typed = check_ok_adt(vec![result_type_decl()], bindings);
    match &typed.bindings.iter().find(|b| b.name == "e").unwrap().ty {
        TypeResult::Monomorphic(CoreType::Adt(name, args)) => {
            assert_eq!(name, "Result");
            assert_eq!(args[1], CoreType::Str);
        }
        other => panic!("expected an ordinary monomorphic Adt type, got {other:?}"),
    }
}

#[test]
fn result_value_is_not_implicitly_convertible_to_its_payload_type() {
    // No auto-unwrap: a context requiring a concrete Int rejects a
    // Result<Int, String> outright.
    let intbox = type_decl("IntBox", vec![], vec![variant("Box", vec![CoreType::Int])]);
    let mut bindings = result_constructor_bindings();
    bindings.push(unary_constructor_binding("Box"));
    bindings.push(binding("bad", apply(var("Box"), apply(var("Ok"), int(5)))));
    let ds = check_err_adt(vec![result_type_decl(), intbox], bindings);
    assert!(
        describe(&ds).contains("argument type mismatch: expected Int, found Result"),
        "{}",
        describe(&ds)
    );
}

// ---------------------------------------------------------------------
// LetRec groups (`BindingGroup::LetRec`, §11/§8.2) — built directly
// against `BindingGroup`, the same isolated-crate pattern as this file.
// ---------------------------------------------------------------------

fn check_err_letrec(members: Vec<Binding>) -> Vec<Diagnostic> {
    let module = Module {
        type_decls: Vec::new(),
        bindings: vec![obfusku_core::ast::BindingGroup::LetRec(members)],
    };
    match check(&module) {
        Err(ds) => ds,
        Ok(_) => panic!("expected a type error, but the module checked successfully"),
    }
}

#[test]
fn self_referential_type_equation_in_a_letrec_group_is_rejected_by_the_occurs_check() {
    // f = λx → λy → f(x) forces R = (type of y) -> R — an infinite
    // type. The LetRec slot's presence must not bypass the occurs check.
    let f_body = lambda("y", apply(var("f"), var("x")));
    let f = binding("f", lambda("x", f_body));
    let ds = check_err_letrec(vec![f]);
    assert!(describe(&ds).contains("infinite type"), "{}", describe(&ds));
}

#[test]
fn letrec_group_members_are_mutually_visible_regardless_of_order() {
    // g = λx → x ; f = λx → g(x) — f references g, declared *after* it
    // within the same group. No error expected.
    let g = binding("g", lambda("x", var("x")));
    let f = binding("f", lambda("x", apply(var("g"), var("x"))));
    let module = Module {
        type_decls: Vec::new(),
        bindings: vec![obfusku_core::ast::BindingGroup::LetRec(vec![f, g])],
    };
    assert!(check(&module).is_ok());
}

// ---------------------------------------------------------------------
// A `LetRec` member's `declared_type` mentioning a free `CoreType::Param`
// (the surface `λf(x: t): t → …` shape a `FunctionDeclaration` lowers
// to) used to panic in `instantiate_core_type` — the constructor-field
// path already substituted a fresh variable per declared type
// parameter; the `LetRec` path called `instantiate_core_type` with an
// empty map instead. This section is the regression coverage for the
// fix: each declared free `Param` now gets its own fresh unification
// variable, exactly like a constructor field's `type_params` already
// did — no new generic-function mechanism, just closing that asymmetry.
// ---------------------------------------------------------------------

fn declared_binding(name: &str, value: Expr, declared_type: CoreType) -> Binding {
    Binding {
        name: name.to_string(),
        value,
        exported: false,
        span: sp(),
        declared_type: Some(declared_type),
    }
}

fn check_ok_letrec(members: Vec<Binding>, rest: Vec<Binding>) -> obfusku_typecheck::TypedModule {
    let mut bindings = vec![obfusku_core::ast::BindingGroup::LetRec(members)];
    bindings.extend(rest.into_iter().map(obfusku_core::ast::BindingGroup::Let));
    let module = Module {
        type_decls: Vec::new(),
        bindings,
    };
    check(&module).unwrap_or_else(|ds| panic!("expected success, got errors: {}", describe(&ds)))
}

// ---------------------------------------------------------------------
// P1-3a: `ValueDeclaration`'s `(":" Type)?` annotation (CONCRETE_SYMBOLIC_
// GRAMMAR.md §8.1) is a real constraint on a `Let`-group binding, not
// merely stored and ignored — this is exactly the shape of gap P0-A was,
// so it gets the same "does it actually constrain" regression coverage.
// ---------------------------------------------------------------------

#[test]
fn declared_value_type_rejects_an_incompatible_value() {
    let x = declared_binding("x", string("hi"), CoreType::Int);
    let module = Module {
        type_decls: Vec::new(),
        bindings: vec![obfusku_core::ast::BindingGroup::Let(x)],
    };
    match check(&module) {
        Err(errs) => assert!(!errs.is_empty()),
        Ok(_) => panic!("declared Int, actual Str must be rejected"),
    }
}

#[test]
fn declared_value_type_accepts_a_compatible_value() {
    let x = declared_binding("x", int(5), CoreType::Int);
    let module = Module {
        type_decls: Vec::new(),
        bindings: vec![obfusku_core::ast::BindingGroup::Let(x)],
    };
    let typed = check(&module).unwrap_or_else(|ds| panic!("expected success: {}", describe(&ds)));
    assert_eq!(monomorphic_type(&typed, "x"), CoreType::Int);
}

#[test]
fn undeclared_value_type_is_unaffected_and_still_infers_normally() {
    // No `declared_type` at all (the pre-P1-3a default) must behave
    // exactly as before — the new unify-against-declared step is only
    // reached when `declared_type` is `Some`.
    let x = binding("x", int(5));
    let module = Module {
        type_decls: Vec::new(),
        bindings: vec![obfusku_core::ast::BindingGroup::Let(x)],
    };
    let typed = check(&module).unwrap_or_else(|ds| panic!("expected success: {}", describe(&ds)));
    assert_eq!(monomorphic_type(&typed, "x"), CoreType::Int);
}

// ---------------------------------------------------------------------
// P0: anonymous `Lambda`'s optional `Parameter` annotation
// (`Expr::Lambda.param_ty`) is a real constraint on that one parameter,
// never a generalization rule of its own — SEMANTIC_CORE.md §19.1's
// value restriction already fully governs generalization, unconditionally
// on syntactic form (`Lambda` always qualifies). A concrete annotation
// simply leaves nothing free to generalize over; a bare type-variable
// annotation behaves like today's already-free unification variable.
// ---------------------------------------------------------------------

#[test]
fn annotated_lambda_parameter_rejects_an_incompatible_body() {
    // λ(x: Int) → x ✚ "x" — Int is fine for x, but the body's own use is
    // what should fail, proving the annotation is a real constraint that
    // participates in the body's own inference, not just checked after
    // the fact and discarded.
    let bad_body = Expr::BinaryOp {
        op: obfusku_core::ast::BinOp::Add,
        lhs: Box::new(var("x")),
        rhs: Box::new(string("x")),
        span: sp(),
    };
    let f = binding("f", annotated_lambda("x", CoreType::Int, bad_body));
    let module = Module {
        type_decls: Vec::new(),
        bindings: vec![obfusku_core::ast::BindingGroup::Let(f)],
    };
    match check(&module) {
        Err(errs) => assert!(!errs.is_empty()),
        Ok(_) => panic!("Int + Str inside the body must be rejected"),
    }
}

#[test]
fn annotated_lambda_parameter_accepts_a_compatible_body() {
    let f = binding("id", annotated_lambda("x", CoreType::Int, var("x")));
    let module = Module {
        type_decls: Vec::new(),
        bindings: vec![obfusku_core::ast::BindingGroup::Let(f)],
    };
    let typed = check(&module).unwrap_or_else(|ds| panic!("expected success: {}", describe(&ds)));
    assert_eq!(
        monomorphic_type(&typed, "id"),
        CoreType::Function(Box::new(CoreType::Int), Box::new(CoreType::Int))
    );
}

#[test]
fn unannotated_lambda_parameter_still_generalizes_as_before() {
    // λ(x) → x, applied at two different types via the ordinary
    // generalization path — proves the new annotated arm didn't disturb
    // the pre-existing unannotated one.
    let id = binding("id", lambda("x", var("x")));
    let a = binding("a", apply(var("id"), int(5)));
    let b = binding("b", apply(var("id"), string("hi")));
    let module = Module {
        type_decls: Vec::new(),
        bindings: vec![
            obfusku_core::ast::BindingGroup::Let(id),
            obfusku_core::ast::BindingGroup::Let(a),
            obfusku_core::ast::BindingGroup::Let(b),
        ],
    };
    let typed = check(&module).unwrap_or_else(|ds| panic!("expected success: {}", describe(&ds)));
    assert_eq!(monomorphic_type(&typed, "a"), CoreType::Int);
    assert_eq!(monomorphic_type(&typed, "b"), CoreType::Str);
}

#[test]
fn two_lambda_parameters_sharing_a_renamed_type_variable_are_forced_equal() {
    // Simulates what `obfusku-syntax::desugar`'s renaming produces for
    // `λ(x: t, y: t) → x` — both parameters annotated with the identical
    // `Type::Param` name, so applying them at incompatible types must
    // fail even though each parameter's own annotation, read in
    // isolation, permits any type.
    let shared = CoreType::Param("$tv0".to_string());
    let inner = annotated_lambda("y", shared.clone(), var("x"));
    let outer = annotated_lambda("x", shared, inner);
    let f = binding("f", outer);
    let apply_both = binding("result", apply(apply(var("f"), int(1)), string("hi")));
    let module = Module {
        type_decls: Vec::new(),
        bindings: vec![
            obfusku_core::ast::BindingGroup::Let(f),
            obfusku_core::ast::BindingGroup::Let(apply_both),
        ],
    };
    match check(&module) {
        Err(errs) => assert!(!errs.is_empty()),
        Ok(_) => panic!("x: Int and y: Str must be rejected — both share the same declared 't'"),
    }
}

#[test]
fn two_lambda_parameters_with_independently_renamed_type_variables_do_not_share() {
    // The contrasting case: two DIFFERENT renamed names (what desugar
    // produces for two separately-written nested Lambdas that happen to
    // reuse the same surface spelling) — applying them at different
    // types must succeed.
    let inner = annotated_lambda("y", CoreType::Param("$tv1".to_string()), var("x"));
    let outer = annotated_lambda("x", CoreType::Param("$tv0".to_string()), inner);
    let f = binding("f", outer);
    let apply_both = binding("result", apply(apply(var("f"), int(1)), string("hi")));
    let module = Module {
        type_decls: Vec::new(),
        bindings: vec![
            obfusku_core::ast::BindingGroup::Let(f),
            obfusku_core::ast::BindingGroup::Let(apply_both),
        ],
    };
    let typed = check(&module).unwrap_or_else(|ds| panic!("expected success: {}", describe(&ds)));
    assert_eq!(monomorphic_type(&typed, "result"), CoreType::Int);
}

#[test]
fn checker_lambda_param_vars_cache_keeps_two_independently_renamed_names_apart() {
    // Post-fix audit case, at the `Checker` level directly: `f` and `g`
    // are two SEPARATE top-level `Let` bindings (siblings, not nested),
    // each carrying its own `param_ty` — desugar would give these
    // genuinely distinct renamed names (`$tv0`/`$tv1`), never the same
    // one, precisely because they come from two unrelated surface
    // parameter lists. Simulated directly here to prove
    // `lambda_param_vars` (the checker-lifetime cache) treats distinct
    // keys as fully independent: applying `f` at Int and `g` at Str in
    // the same `Checker` must both succeed.
    let f = binding(
        "f",
        annotated_lambda("x", CoreType::Param("$tv0".to_string()), var("x")),
    );
    let g = binding(
        "g",
        annotated_lambda("y", CoreType::Param("$tv1".to_string()), var("y")),
    );
    let a = binding("a", apply(var("f"), int(5)));
    let result = binding("result", apply(var("g"), string("hi")));
    let module = Module {
        type_decls: Vec::new(),
        bindings: vec![
            obfusku_core::ast::BindingGroup::Let(f),
            obfusku_core::ast::BindingGroup::Let(g),
            obfusku_core::ast::BindingGroup::Let(a),
            obfusku_core::ast::BindingGroup::Let(result),
        ],
    };
    let typed = check(&module).unwrap_or_else(|ds| panic!("expected success: {}", describe(&ds)));
    assert_eq!(monomorphic_type(&typed, "result"), CoreType::Str);
}

#[test]
fn module_level_generic_identity_no_longer_panics_and_infers_correctly() {
    // λid(x: t): t → x — used to panic with "unbound type parameter
    // 't' in a declared field type" before the fix.
    let id = declared_binding(
        "id",
        lambda("x", var("x")),
        CoreType::Function(
            Box::new(CoreType::Param("t".to_string())),
            Box::new(CoreType::Param("t".to_string())),
        ),
    );
    let typed = check_ok_letrec(vec![id], vec![binding("result", apply(var("id"), int(5)))]);
    assert_eq!(monomorphic_type(&typed, "result"), CoreType::Int);
}

#[test]
fn module_level_generic_identity_generalizes_and_is_reusable_at_different_types() {
    // Same `id` as above, applied at Int and then at Str — proof the
    // fix produces a genuinely generalized scheme (fresh instantiation
    // per call site), not merely "no panic, pinned to the first use."
    let id = declared_binding(
        "id",
        lambda("x", var("x")),
        CoreType::Function(
            Box::new(CoreType::Param("t".to_string())),
            Box::new(CoreType::Param("t".to_string())),
        ),
    );
    let typed = check_ok_letrec(
        vec![id],
        vec![
            binding("a", apply(var("id"), int(5))),
            binding("b", apply(var("id"), string("hi"))),
        ],
    );
    assert_eq!(monomorphic_type(&typed, "a"), CoreType::Int);
    assert_eq!(monomorphic_type(&typed, "b"), CoreType::Str);
}

#[test]
fn module_level_generic_function_with_two_independent_type_parameters() {
    // λconst(x: t, y: u): t → x — two distinct free type parameters in
    // one signature; `u` never appears in the return type at all, so
    // it must still get its own fresh variable rather than colliding
    // with `t`'s or being left unbound.
    let const_fn = declared_binding(
        "constFn",
        lambda("x", lambda("y", var("x"))),
        CoreType::Function(
            Box::new(CoreType::Param("t".to_string())),
            Box::new(CoreType::Function(
                Box::new(CoreType::Param("u".to_string())),
                Box::new(CoreType::Param("t".to_string())),
            )),
        ),
    );
    let typed = check_ok_letrec(
        vec![const_fn],
        vec![binding(
            "result",
            apply(apply(var("constFn"), int(5)), string("ignored")),
        )],
    );
    assert_eq!(monomorphic_type(&typed, "result"), CoreType::Int);
}

#[test]
fn module_level_recursive_generic_function_type_checks() {
    // repeatIdentity(n: Nat, x: t): t → self-recursive *and* generic —
    // exactly the shape map/filter/fold need (recursion via LetRec,
    // genericity via a declared free Param).
    let nat_decl = type_decl(
        "Nat",
        vec![],
        vec![
            variant("Zero", vec![]),
            variant("Succ", vec![CoreType::Adt("Nat".into(), vec![])]),
        ],
    );
    let zero = binding(
        "Zero",
        Expr::Constructor {
            tag: "Zero".to_string(),
            args: vec![],
            span: sp(),
        },
    );
    let succ = binding(
        "Succ",
        lambda(
            "n",
            Expr::Constructor {
                tag: "Succ".to_string(),
                args: vec![var("n")],
                span: sp(),
            },
        ),
    );
    let repeat_identity = declared_binding(
        "repeatIdentity",
        lambda(
            "n",
            lambda(
                "x",
                match_expr(
                    var("n"),
                    vec![
                        arm(pctor("Zero", vec![]), var("x")),
                        arm(
                            pctor("Succ", vec![pvar("k")]),
                            apply(apply(var("repeatIdentity"), var("k")), var("x")),
                        ),
                    ],
                ),
            ),
        ),
        CoreType::Function(
            Box::new(CoreType::Adt("Nat".into(), vec![])),
            Box::new(CoreType::Function(
                Box::new(CoreType::Param("t".to_string())),
                Box::new(CoreType::Param("t".to_string())),
            )),
        ),
    );

    let mut bindings = vec![
        obfusku_core::ast::BindingGroup::Let(zero),
        obfusku_core::ast::BindingGroup::Let(succ),
        obfusku_core::ast::BindingGroup::LetRec(vec![repeat_identity]),
    ];
    bindings.push(obfusku_core::ast::BindingGroup::Let(binding(
        "result",
        apply(
            apply(var("repeatIdentity"), apply(var("Succ"), var("Zero"))),
            int(42),
        ),
    )));
    let module = Module {
        type_decls: vec![nat_decl],
        bindings,
    };
    let typed = check(&module)
        .unwrap_or_else(|ds| panic!("expected success, got errors: {}", describe(&ds)));
    assert_eq!(monomorphic_type(&typed, "result"), CoreType::Int);
}

#[test]
fn module_level_generic_function_misuse_is_a_diagnostic_never_a_panic() {
    // `id`'s own group-internal (not yet generalized) instantiation is
    // pinned to `Int` by `bad`'s first use, then `bad`'s own declared
    // type (`Str`) contradicts that — a genuine type error the fix must
    // still report cleanly, never as a panic, exactly like the
    // pre-existing concrete-type version of this same check
    // (`letrec_declared_type_is_unified_before_the_body_is_inferred`).
    let id = declared_binding(
        "id",
        lambda("x", var("x")),
        CoreType::Function(
            Box::new(CoreType::Param("t".to_string())),
            Box::new(CoreType::Param("t".to_string())),
        ),
    );
    let bad = declared_binding("bad", apply(var("id"), int(5)), CoreType::Str);
    let ds = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        check(&Module {
            type_decls: Vec::new(),
            bindings: vec![obfusku_core::ast::BindingGroup::LetRec(vec![id, bad])],
        })
    })) {
        Ok(Err(ds)) => ds,
        Ok(Ok(_)) => panic!("expected a type error, but the module checked successfully"),
        Err(_) => panic!("a genuinely ill-typed generic usage must be a diagnostic, not a panic"),
    };
    assert!(!describe(&ds).is_empty());
}

// The local (expression-level) `Expr::LetRec` path had the identical
// bug, fixed the same way — one regression test for that path too.
#[test]
fn local_letrec_generic_declared_type_no_longer_panics() {
    let typed = check_ok(vec![binding(
        "result",
        letrec_expr(
            vec![declared_binding(
                "id",
                lambda("x", var("x")),
                CoreType::Function(
                    Box::new(CoreType::Param("t".to_string())),
                    Box::new(CoreType::Param("t".to_string())),
                ),
            )],
            apply(var("id"), int(7)),
        ),
    )]);
    assert_eq!(monomorphic_type(&typed, "result"), CoreType::Int);
}

#[test]
fn local_letrec_two_arg_function_x_plus_y_type_checks() {
    // Same P0-A shape as the module-level test below, through the
    // local (expression-level) `Expr::LetRec` path instead.
    let typed = check_ok(vec![binding(
        "result",
        letrec_expr(
            vec![declared_binding(
                "add",
                lambda("x", lambda("y", binop(BinOp::Add, var("x"), var("y")))),
                CoreType::Function(
                    Box::new(CoreType::Int),
                    Box::new(CoreType::Function(
                        Box::new(CoreType::Int),
                        Box::new(CoreType::Int),
                    )),
                ),
            )],
            apply(apply(var("add"), int(2)), int(3)),
        ),
    )]);
    assert_eq!(monomorphic_type(&typed, "result"), CoreType::Int);
}

// ---------------------------------------------------------------------
// P0-A regression matrix: a declared `FunctionType`'s own parameter
// types must reach each nested `Lambda`'s own parameter *before* its
// body is inferred, not only get checked against the whole signature
// afterward — `infer_lambda_chain_against`'s fix. Every case below is
// chosen to be impossible to pass by accident via an "operand order"
// workaround (unlike `1 ✚ n`/`0 ✚ acc ✚ n`, found scattered through
// this session's earlier tests as a symptom of the very bug fixed
// here): neither operand in any of these is a literal.
// ---------------------------------------------------------------------

#[test]
fn two_arg_function_x_plus_y_type_checks() {
    // The exact shape that used to fail: `λadd(x: ⟁, y: ⟁): ⟁ → x ✚ y`.
    let add = declared_binding(
        "add",
        lambda("x", lambda("y", binop(BinOp::Add, var("x"), var("y")))),
        CoreType::Function(
            Box::new(CoreType::Int),
            Box::new(CoreType::Function(
                Box::new(CoreType::Int),
                Box::new(CoreType::Int),
            )),
        ),
    );
    let typed = check_ok_letrec(
        vec![add],
        vec![binding("result", apply(apply(var("add"), int(2)), int(3)))],
    );
    assert_eq!(monomorphic_type(&typed, "result"), CoreType::Int);
}

#[test]
fn two_arg_function_y_plus_x_type_checks_identically() {
    // Operand order must not matter — this used to fail exactly the
    // same way `x ✚ y` did, proving the old bug was never really about
    // "which side," only "neither side is ever resolved yet."
    let add = declared_binding(
        "add",
        lambda("x", lambda("y", binop(BinOp::Add, var("y"), var("x")))),
        CoreType::Function(
            Box::new(CoreType::Int),
            Box::new(CoreType::Function(
                Box::new(CoreType::Int),
                Box::new(CoreType::Int),
            )),
        ),
    );
    let typed = check_ok_letrec(
        vec![add],
        vec![binding("result", apply(apply(var("add"), int(2)), int(3)))],
    );
    assert_eq!(monomorphic_type(&typed, "result"), CoreType::Int);
}

#[test]
fn recursive_two_arg_function_uses_operator_on_its_own_accumulator_param() {
    // A genuinely recursive two-argument function whose body applies an
    // operator directly to its own (non-literal) accumulator parameter
    // — the shape every `0 ✚ acc ✚ n` workaround in this session's
    // stdlib/native tests existed only to route around.
    let nat_decl = type_decl(
        "Nat",
        vec![],
        vec![
            variant("Zero", vec![]),
            variant("Succ", vec![CoreType::Adt("Nat".into(), vec![])]),
        ],
    );
    let zero = binding(
        "Zero",
        Expr::Constructor {
            tag: "Zero".to_string(),
            args: vec![],
            span: sp(),
        },
    );
    let succ = binding(
        "Succ",
        lambda(
            "n",
            Expr::Constructor {
                tag: "Succ".to_string(),
                args: vec![var("n")],
                span: sp(),
            },
        ),
    );
    let sum_to = declared_binding(
        "sumTo",
        lambda(
            "n",
            lambda(
                "acc",
                match_expr(
                    var("n"),
                    vec![
                        arm(pctor("Zero", vec![]), var("acc")),
                        arm(
                            pctor("Succ", vec![pvar("k")]),
                            apply(
                                apply(var("sumTo"), var("k")),
                                binop(BinOp::Add, var("acc"), int(1)),
                            ),
                        ),
                    ],
                ),
            ),
        ),
        CoreType::Function(
            Box::new(CoreType::Adt("Nat".into(), vec![])),
            Box::new(CoreType::Function(
                Box::new(CoreType::Int),
                Box::new(CoreType::Int),
            )),
        ),
    );
    let mut bindings = vec![
        obfusku_core::ast::BindingGroup::Let(zero),
        obfusku_core::ast::BindingGroup::Let(succ),
        obfusku_core::ast::BindingGroup::LetRec(vec![sum_to]),
    ];
    bindings.push(obfusku_core::ast::BindingGroup::Let(binding(
        "result",
        apply(
            apply(
                var("sumTo"),
                apply(var("Succ"), apply(var("Succ"), var("Zero"))),
            ),
            int(0),
        ),
    )));
    let module = Module {
        type_decls: vec![nat_decl],
        bindings,
    };
    let typed = check(&module)
        .unwrap_or_else(|ds| panic!("expected success, got errors: {}", describe(&ds)));
    assert_eq!(monomorphic_type(&typed, "result"), CoreType::Int);
}

#[test]
fn two_arg_function_with_mismatched_operand_types_is_still_a_real_diagnostic() {
    // The fix must not weaken error detection: `x: ⟁, y: ⌘` used with
    // `x ✚ y` is genuinely ill-typed (§20.2 has no Int+Str case) and
    // must still be reported, never silently accepted or panicked on.
    let add = declared_binding(
        "add",
        lambda("x", lambda("y", binop(BinOp::Add, var("x"), var("y")))),
        CoreType::Function(
            Box::new(CoreType::Int),
            Box::new(CoreType::Function(
                Box::new(CoreType::Str),
                Box::new(CoreType::Int),
            )),
        ),
    );
    let ds = check_err_letrec(vec![add]);
    assert!(!describe(&ds).is_empty());
}

// ---------------------------------------------------------------------
// Operators (`SEMANTIC_CORE.md` §20.2) — closed dispatch table, no
// typeclasses, no implicit coercion. Division/modulo-by-zero is
// deliberately NOT checked here (a runtime concern, §15.3) — these
// tests only ever assert types, never evaluate anything.
// ---------------------------------------------------------------------

use obfusku_core::ast::{BinOp, UnOp};

fn binop(op: BinOp, lhs: Expr, rhs: Expr) -> Expr {
    Expr::BinaryOp {
        op,
        lhs: Box::new(lhs),
        rhs: Box::new(rhs),
        span: sp(),
    }
}

fn unop(op: UnOp, operand: Expr) -> Expr {
    Expr::UnaryOp {
        op,
        operand: Box::new(operand),
        span: sp(),
    }
}

// ── P0: the four tests called out explicitly ───────────────────────────

#[test]
fn p0_int_plus_int_is_int() {
    let typed = check_ok(vec![binding("result", binop(BinOp::Add, int(1), int(2)))]);
    assert_eq!(monomorphic_type(&typed, "result"), CoreType::Int);
}

#[test]
fn p0_int_plus_real_is_rejected_no_coercion() {
    let ds = check_err(vec![binding(
        "result",
        binop(BinOp::Add, int(1), real(2.0)),
    )]);
    assert!(!describe(&ds).is_empty());
}

#[test]
fn p0_lambda_equality_is_a_static_type_error_not_runtime() {
    // λ(x) → x == λ(x) → x — comparing two function values.
    let f = lambda("x", var("x"));
    let ds = check_err(vec![binding(
        "result",
        binop(BinOp::Eq, f.clone(), lambda("y", var("y"))),
    )]);
    assert!(describe(&ds).contains("no equality"), "{}", describe(&ds));
}

#[test]
fn p0_and_is_already_match_by_the_time_typecheck_sees_it_no_special_short_circuit_rule() {
    // `∧` is already Core `Match` by the time this reaches typecheck —
    // ordinary Match typing handles it, nothing operator-specific.
    let expensive = apply(var("expensive"), int(1));
    let core_and = Expr::Match {
        scrutinee: Box::new(boolean(true)),
        arms: vec![
            MatchArm {
                pattern: Pattern::Lit(Literal::Bool(true), sp()),
                result: expensive,
                span: sp(),
            },
            MatchArm {
                pattern: Pattern::Lit(Literal::Bool(false), sp()),
                result: boolean(false),
                span: sp(),
            },
        ],
        span: sp(),
    };
    let typed = check_ok(vec![
        binding("expensive", lambda("ignored", boolean(true))),
        binding("result", core_and),
    ]);
    assert_eq!(monomorphic_type(&typed, "result"), CoreType::Bool);
}

// ── full closed table ───────────────────────────────────────────────────

#[test]
fn add_real_real_is_real() {
    let typed = check_ok(vec![binding(
        "result",
        binop(BinOp::Add, real(1.0), real(2.0)),
    )]);
    assert_eq!(monomorphic_type(&typed, "result"), CoreType::Real);
}

#[test]
fn add_str_str_is_str_concatenation() {
    let typed = check_ok(vec![binding(
        "result",
        binop(BinOp::Add, string("a"), string("b")),
    )]);
    assert_eq!(monomorphic_type(&typed, "result"), CoreType::Str);
}

#[test]
fn add_bool_bool_is_rejected_bool_not_in_the_closed_set() {
    let ds = check_err(vec![binding(
        "result",
        binop(BinOp::Add, boolean(true), boolean(false)),
    )]);
    assert!(
        describe(&ds).contains("closed operator table"),
        "{}",
        describe(&ds)
    );
}

#[test]
fn sub_mul_div_accept_int_or_real_symmetrically() {
    for op in [BinOp::Sub, BinOp::Mul, BinOp::Div] {
        let typed_int = check_ok(vec![binding("result", binop(op, int(4), int(2)))]);
        assert_eq!(monomorphic_type(&typed_int, "result"), CoreType::Int);
        let typed_real = check_ok(vec![binding("result", binop(op, real(4.0), real(2.0)))]);
        assert_eq!(monomorphic_type(&typed_real, "result"), CoreType::Real);
    }
}

#[test]
fn sub_mixed_int_real_is_rejected() {
    let ds = check_err(vec![binding(
        "result",
        binop(BinOp::Sub, int(4), real(2.0)),
    )]);
    assert!(!describe(&ds).is_empty());
}

#[test]
fn sub_on_str_is_rejected_str_only_valid_for_add() {
    let ds = check_err(vec![binding(
        "result",
        binop(BinOp::Sub, string("a"), string("b")),
    )]);
    assert!(
        describe(&ds).contains("closed operator table"),
        "{}",
        describe(&ds)
    );
}

#[test]
fn modulo_is_int_only() {
    let typed = check_ok(vec![binding("result", binop(BinOp::Mod, int(7), int(3)))]);
    assert_eq!(monomorphic_type(&typed, "result"), CoreType::Int);

    let ds = check_err(vec![binding(
        "result",
        binop(BinOp::Mod, real(7.0), real(3.0)),
    )]);
    assert!(!describe(&ds).is_empty());
}

#[test]
fn division_by_the_literal_zero_still_type_checks_fine() {
    // The specific "typecheck does not evaluate constants" property:
    // 1 ÷ 0 is perfectly well-typed. DivisionByZero belongs to runtime.
    let typed = check_ok(vec![binding("result", binop(BinOp::Div, int(1), int(0)))]);
    assert_eq!(monomorphic_type(&typed, "result"), CoreType::Int);
}

#[test]
fn comparison_operators_accept_int_or_real_and_produce_bool() {
    for op in [BinOp::Lt, BinOp::Gt, BinOp::Le, BinOp::Ge] {
        let typed = check_ok(vec![binding("result", binop(op, int(1), int(2)))]);
        assert_eq!(monomorphic_type(&typed, "result"), CoreType::Bool);
    }
}

#[test]
fn comparison_rejects_mixed_int_real() {
    let ds = check_err(vec![binding("result", binop(BinOp::Lt, int(1), real(2.0)))]);
    assert!(!describe(&ds).is_empty());
}

#[test]
fn comparison_rejects_non_numeric_types() {
    let ds = check_err(vec![binding(
        "result",
        binop(BinOp::Lt, string("a"), string("b")),
    )]);
    assert!(
        describe(&ds).contains("closed operator table"),
        "{}",
        describe(&ds)
    );
}

#[test]
fn equality_works_for_any_single_matching_equatable_type() {
    let typed_int = check_ok(vec![binding("result", binop(BinOp::Eq, int(1), int(1)))]);
    assert_eq!(monomorphic_type(&typed_int, "result"), CoreType::Bool);

    let typed_str = check_ok(vec![binding(
        "result",
        binop(BinOp::Eq, string("a"), string("a")),
    )]);
    assert_eq!(monomorphic_type(&typed_str, "result"), CoreType::Bool);

    let typed_bool = check_ok(vec![binding(
        "result",
        binop(BinOp::NotEq, boolean(true), boolean(false)),
    )]);
    assert_eq!(monomorphic_type(&typed_bool, "result"), CoreType::Bool);
}

#[test]
fn equality_rejects_cross_type_comparison_no_coercion() {
    let ds = check_err(vec![binding("result", binop(BinOp::Eq, int(1), real(1.0)))]);
    assert!(!describe(&ds).is_empty());
}

#[test]
fn xor_is_bool_only() {
    let typed = check_ok(vec![binding(
        "result",
        binop(BinOp::Xor, boolean(true), boolean(false)),
    )]);
    assert_eq!(monomorphic_type(&typed, "result"), CoreType::Bool);

    let ds = check_err(vec![binding("result", binop(BinOp::Xor, int(1), int(0)))]);
    assert!(!describe(&ds).is_empty());
}

#[test]
fn unary_not_is_bool_only() {
    let typed = check_ok(vec![binding("result", unop(UnOp::Not, boolean(true)))]);
    assert_eq!(monomorphic_type(&typed, "result"), CoreType::Bool);

    let ds = check_err(vec![binding("result", unop(UnOp::Not, int(1)))]);
    assert!(!describe(&ds).is_empty());
}

#[test]
fn unary_negation_accepts_int_or_real_preserving_the_operand_type() {
    let typed_int = check_ok(vec![binding("result", unop(UnOp::Neg, int(5)))]);
    assert_eq!(monomorphic_type(&typed_int, "result"), CoreType::Int);

    let typed_real = check_ok(vec![binding("result", unop(UnOp::Neg, real(5.0)))]);
    assert_eq!(monomorphic_type(&typed_real, "result"), CoreType::Real);

    let ds = check_err(vec![binding("result", unop(UnOp::Neg, boolean(true)))]);
    assert!(!describe(&ds).is_empty());
}

#[test]
fn operator_type_error_carries_the_source_span_from_the_offending_expression() {
    let bad = binop(BinOp::Add, int(1), boolean(true));
    let ds = check_err(vec![binding("result", bad)]);
    assert_eq!(ds[0].primary, sp());
}

// ── LocalBinding: `Expr::Let`/`Expr::LetRec` (`SEMANTIC_CORE.md` §11,
//    §19.1) ─────────────────────────────────────────────────────────

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

// ── Type-arity checking: a bare `Tag` never infers implicit type
//    arguments; a wrong arity is a real, reported error. ─────────────

#[test]
fn self_referential_generic_variant_field_needs_explicit_type_application() {
    // `List t = { Nil | Cons(t, List<t>) }`, correctly using
    // `CoreType::Adt("List", [Param("t")])` for the recursive field
    // (the `List ▷ t` surface form) — must build and type-check cleanly.
    let list_decl = type_decl(
        "List",
        vec!["t"],
        vec![
            variant("Nil", vec![]),
            variant(
                "Cons",
                vec![
                    CoreType::Param("t".to_string()),
                    CoreType::Adt("List".to_string(), vec![CoreType::Param("t".to_string())]),
                ],
            ),
        ],
    );
    let nil = binding(
        "Nil",
        Expr::Constructor {
            tag: "Nil".to_string(),
            args: vec![],
            span: sp(),
        },
    );
    let cons = binding(
        "Cons",
        lambda(
            "h",
            lambda(
                "t",
                Expr::Constructor {
                    tag: "Cons".to_string(),
                    args: vec![var("h"), var("t")],
                    span: sp(),
                },
            ),
        ),
    );
    let xs = binding("xs", apply(apply(var("Cons"), int(1)), var("Nil")));
    let _typed = check_ok_adt(vec![list_decl], vec![nil, cons, xs]);
}

#[test]
fn bare_self_reference_with_wrong_arity_is_a_real_type_error() {
    // Same shape, but the recursive field is `List` (bare, zero
    // arguments) where `List` is declared with one type parameter —
    // must be rejected at registry-build time, not silently accepted as
    // "the zero-argument `List`."
    let list_decl = type_decl(
        "List",
        vec!["t"],
        vec![
            variant("Nil", vec![]),
            variant(
                "Cons",
                vec![
                    CoreType::Param("t".to_string()),
                    CoreType::Adt("List".to_string(), vec![]),
                ],
            ),
        ],
    );
    let ds = check_err_adt(vec![list_decl], vec![]);
    assert!(
        describe(&ds).contains("expects 1 type argument"),
        "{}",
        describe(&ds)
    );
}

// ── Array<T> (`SEMANTIC_CORE.md` §9.2) ─────────────────────────────────

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
fn array_literal_infers_int_array_from_homogeneous_elements() {
    let typed = check_ok(vec![binding("xs", array_lit(vec![int(1), int(2), int(3)]))]);
    assert_eq!(
        monomorphic_type(&typed, "xs"),
        CoreType::Adt("Array".to_string(), vec![CoreType::Int])
    );
}

#[test]
fn array_literal_with_mixed_element_types_is_rejected() {
    let ds = check_err(vec![binding("xs", array_lit(vec![int(1), boolean(true)]))]);
    assert!(!describe(&ds).is_empty());
}

#[test]
fn index_into_an_int_array_produces_int() {
    let typed = check_ok(vec![binding(
        "result",
        index(array_lit(vec![int(1), int(2)]), int(0)),
    )]);
    assert_eq!(monomorphic_type(&typed, "result"), CoreType::Int);
}

#[test]
fn index_with_a_non_int_index_is_rejected() {
    let ds = check_err(vec![binding(
        "result",
        index(array_lit(vec![int(1)]), string("nope")),
    )]);
    assert!(!describe(&ds).is_empty());
}

#[test]
fn indexing_a_non_array_value_is_rejected() {
    let ds = check_err(vec![binding("result", index(int(5), int(0)))]);
    assert!(!describe(&ds).is_empty());
}

#[test]
fn empty_array_literal_type_checks_with_an_ambiguous_element_type_error() {
    // No context here at all, so this must be the same "ambiguous"
    // diagnostic any other never-pinned type variable gets.
    let ds = check_err(vec![binding("xs", array_lit(vec![]))]);
    assert!(describe(&ds).contains("ambiguous"), "{}", describe(&ds));
}

#[test]
fn empty_array_literal_element_type_resolves_from_later_context() {
    let typed = check_ok(vec![
        binding("xs", array_lit(vec![])),
        binding("result", index(var("xs"), int(0))),
        binding("pin", binop(BinOp::Add, int(1), var("result"))),
    ]);
    assert_eq!(monomorphic_type(&typed, "pin"), CoreType::Int);
}

#[test]
fn letrec_group_does_not_leak_its_members_past_the_local_body() {
    let ds = check_err(vec![
        binding(
            "result",
            letrec_expr(vec![binding("loop", lambda("n", var("n")))], var("loop")),
        ),
        binding("outside", var("loop")),
    ]);
    assert!(!describe(&ds).is_empty());
}

// ── `check_with_prelude` (imports, §20) ─────────────────────────────

#[test]
fn prelude_names_are_in_scope_before_the_module_is_checked() {
    let module = Module {
        type_decls: Vec::new(),
        bindings: vec![obfusku_core::ast::BindingGroup::Let(binding(
            "result",
            apply(var("imported"), int(5)),
        ))],
    };
    let mut prelude = obfusku_typecheck::Prelude::new();
    let fn_ty = CoreType::Function(Box::new(CoreType::Int), Box::new(CoreType::Int));
    prelude.insert(
        "imported".to_string(),
        obfusku_typecheck::Scheme::monomorphic(obfusku_typecheck::instantiate_core_type(
            &fn_ty,
            &std::collections::HashMap::new(),
        )),
    );
    let typed = obfusku_typecheck::check_with_prelude(&module, prelude)
        .unwrap_or_else(|ds| panic!("expected success, got errors: {}", describe(&ds)));
    assert_eq!(monomorphic_type(&typed, "result"), CoreType::Int);
}

#[test]
fn a_local_binding_reusing_a_prelude_name_is_a_static_error() {
    let module = Module {
        type_decls: Vec::new(),
        bindings: vec![obfusku_core::ast::BindingGroup::Let(binding(
            "imported",
            int(1),
        ))],
    };
    let mut prelude = obfusku_typecheck::Prelude::new();
    prelude.insert(
        "imported".to_string(),
        obfusku_typecheck::Scheme::monomorphic(obfusku_typecheck::instantiate_core_type(
            &CoreType::Int,
            &std::collections::HashMap::new(),
        )),
    );
    let ds = match obfusku_typecheck::check_with_prelude(&module, prelude) {
        Err(ds) => ds,
        Ok(_) => panic!("expected a duplicate-name error"),
    };
    assert!(
        describe(&ds).contains("already bound by the ambient prelude"),
        "{}",
        describe(&ds)
    );
}

#[test]
fn check_without_a_prelude_behaves_exactly_like_check() {
    let typed = check_ok(vec![binding("x", int(5))]);
    assert_eq!(monomorphic_type(&typed, "x"), CoreType::Int);
}

// ---------------------------------------------------------------------
// P0-B regression: a `Prelude` entry's `Scheme` is minted by a
// *different* `Checker` than the one about to consume it (`stdlib`,
// `natives`, and an imported module's own separate `check_with_prelude`
// call all do exactly this in production) — its `TypeVarId`s mean
// nothing to the receiving checker's own fresh-variable counter, which
// independently starts from zero every time. `check_with_prelude` must
// rebind every incoming `Scheme` to its own local variables
// (`Checker::normalize_scheme`) before it ever reaches `generalize`'s
// environment scan — otherwise a purely numeric coincidence between a
// foreign scheme's "bound" variable and this checker's own live,
// resolved variable can silently strip a local generic function of its
// polymorphism.
// ---------------------------------------------------------------------

/// Produces a real foreign `Scheme` the same way `stdlib`/`natives`
/// actually do: run a completely independent `check()` (its own,
/// separately-numbered `Checker`) over a small generic module and pull
/// its `Polymorphic` result back out — not hand-constructed, since
/// `TypeVarId` isn't public outside this crate (by design).
fn foreign_generic_identity_prelude() -> obfusku_typecheck::Prelude {
    let foreign = check_ok_letrec(
        vec![declared_binding(
            "foreignId",
            lambda("x", var("x")),
            CoreType::Function(
                Box::new(CoreType::Param("t".to_string())),
                Box::new(CoreType::Param("t".to_string())),
            ),
        )],
        vec![],
    );
    let mut prelude = obfusku_typecheck::Prelude::new();
    match &foreign
        .bindings
        .iter()
        .find(|b| b.name == "foreignId")
        .unwrap()
        .ty
    {
        TypeResult::Polymorphic(scheme) => {
            prelude.insert("foreignId".to_string(), scheme.clone());
        }
        other => panic!("expected foreignId to be polymorphic, got {other:?}"),
    }
    prelude
}

#[test]
fn a_local_generic_function_still_generalizes_with_a_foreign_generic_scheme_in_the_prelude() {
    let prelude = foreign_generic_identity_prelude();
    let id = declared_binding(
        "id",
        lambda("x", var("x")),
        CoreType::Function(
            Box::new(CoreType::Param("t".to_string())),
            Box::new(CoreType::Param("t".to_string())),
        ),
    );
    let module = Module {
        type_decls: Vec::new(),
        bindings: vec![
            obfusku_core::ast::BindingGroup::LetRec(vec![id]),
            obfusku_core::ast::BindingGroup::Let(binding("a", apply(var("id"), int(5)))),
            obfusku_core::ast::BindingGroup::Let(binding("b", apply(var("id"), string("hi")))),
        ],
    };
    let typed = obfusku_typecheck::check_with_prelude(&module, prelude)
        .unwrap_or_else(|ds| panic!("expected success, got errors: {}", describe(&ds)));
    assert_eq!(monomorphic_type(&typed, "a"), CoreType::Int);
    assert_eq!(monomorphic_type(&typed, "b"), CoreType::Str);
}

#[test]
fn an_unused_local_generic_function_type_checks_cleanly_with_a_foreign_generic_scheme_present() {
    // The "declared but never used" symptom found during investigation:
    // an otherwise-valid, unused generic function must not become a
    // spurious "ambiguous type" error just because an unrelated foreign
    // generic scheme sits in the same prelude.
    let prelude = foreign_generic_identity_prelude();
    let id = declared_binding(
        "id",
        lambda("x", var("x")),
        CoreType::Function(
            Box::new(CoreType::Param("t".to_string())),
            Box::new(CoreType::Param("t".to_string())),
        ),
    );
    let module = Module {
        type_decls: Vec::new(),
        bindings: vec![
            obfusku_core::ast::BindingGroup::LetRec(vec![id]),
            obfusku_core::ast::BindingGroup::Let(binding("result", int(5))),
        ],
    };
    let typed = obfusku_typecheck::check_with_prelude(&module, prelude)
        .unwrap_or_else(|ds| panic!("expected success, got errors: {}", describe(&ds)));
    assert_eq!(monomorphic_type(&typed, "result"), CoreType::Int);
}

#[test]
fn foreign_prelude_scheme_itself_is_usable_and_still_polymorphic_after_normalization() {
    // Normalization must preserve quantification, not just avoid
    // collisions — the foreign scheme itself must still work as a
    // genuinely polymorphic value once it's in scope.
    let prelude = foreign_generic_identity_prelude();
    let module = Module {
        type_decls: Vec::new(),
        bindings: vec![
            obfusku_core::ast::BindingGroup::Let(binding("a", apply(var("foreignId"), int(5)))),
            obfusku_core::ast::BindingGroup::Let(binding(
                "b",
                apply(var("foreignId"), string("hi")),
            )),
        ],
    };
    let typed = obfusku_typecheck::check_with_prelude(&module, prelude)
        .unwrap_or_else(|ds| panic!("expected success, got errors: {}", describe(&ds)));
    assert_eq!(monomorphic_type(&typed, "a"), CoreType::Int);
    assert_eq!(monomorphic_type(&typed, "b"), CoreType::Str);
}
