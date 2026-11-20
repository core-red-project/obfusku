//! `let` and `letrec`: scoping, shadowing and mutual recursion.

use super::support::*;

fn let_(name: &str, value: Expr, body: Expr) -> Expr {
    Expr::Let {
        name: name.to_string(),
        value: Box::new(value),
        body: Rc::new(body),
        span: sp(),
    }
}

fn letrec_(members: Vec<(&str, Expr)>, body: Expr) -> Expr {
    Expr::LetRec {
        bindings: members.into_iter().map(|(n, v)| mk_binding(n, v)).collect(),
        body: Rc::new(body),
        span: sp(),
    }
}

#[test]
fn let_evaluates_value_then_binds_it_for_the_body() {
    let expr = let_("x", int(5), binop(BinOp::Add, var("x"), int(1)));
    assert_eq!(as_int(run(expr)), 6);
}

#[test]
fn nested_let_shadowing_evaluates_the_innermost_binding() {
    let expr = let_("x", int(1), let_("x", int(2), var("x")));
    assert_eq!(as_int(run(expr)), 2);
}

#[test]
fn let_mutable_binding_supports_mutation_scoped_to_its_body() {
    let expr = let_(
        "x",
        mutcell(int(5)),
        apply(lambda("_", mutread(var("x"))), mutrebind(var("x"), int(9))),
    );
    assert_eq!(as_int(run(expr)), 9);
}

#[test]
fn letrec_supports_self_recursion_via_tail_calls_at_real_depth() {
    // The reason `LetRec` (not `Let`) exists at all: `countdown`'s own
    // name must be visible inside its own body. Run deep enough (20,000)
    // that this would stack-overflow if `body`'s tail continuation
    // (§14.1) weren't actually looping in the trampoline.
    let countdown = lambda(
        "n",
        match_(
            var("n"),
            vec![
                (
                    Pattern::Lit(obfusku_core::ast::Literal::Int(0), sp()),
                    int(0),
                ),
                (
                    pwild(),
                    apply(var("countdown"), binop(BinOp::Sub, var("n"), int(1))),
                ),
            ],
        ),
    );
    let expr = letrec_(
        vec![("countdown", countdown)],
        apply(var("countdown"), int(20_000)),
    );
    assert_eq!(as_int(run(expr)), 0);
}

#[test]
fn letrec_members_are_mutually_visible_and_can_call_each_other() {
    let is_even = lambda(
        "n",
        match_(
            var("n"),
            vec![
                (
                    Pattern::Lit(obfusku_core::ast::Literal::Int(0), sp()),
                    boolean(true),
                ),
                (
                    pwild(),
                    apply(var("isOdd"), binop(BinOp::Sub, var("n"), int(1))),
                ),
            ],
        ),
    );
    let is_odd = lambda(
        "n",
        match_(
            var("n"),
            vec![
                (
                    Pattern::Lit(obfusku_core::ast::Literal::Int(0), sp()),
                    boolean(false),
                ),
                (
                    pwild(),
                    apply(var("isEven"), binop(BinOp::Sub, var("n"), int(1))),
                ),
            ],
        ),
    );
    let expr = letrec_(
        vec![("isEven", is_even), ("isOdd", is_odd)],
        apply(var("isEven"), int(10)),
    );
    assert!(matches!(run(expr), Value::Bool(true)));
}

#[test]
fn letrec_group_does_not_leak_its_members_outside_the_local_body() {
    // `countdown` is only visible within the `LetRec`'s own `body` — a
    // later, unrelated reference must be an unbound-variable failure,
    // proving the group's frame is a genuinely nested child scope.
    let module = Module {
        type_decls: vec![],
        bindings: vec![
            obfusku_core::ast::BindingGroup::Let(mk_binding(
                "first",
                letrec_(
                    vec![("countdown", lambda("n", var("n")))],
                    apply(var("countdown"), int(1)),
                ),
            )),
            obfusku_core::ast::BindingGroup::Let(mk_binding("second", var("countdown"))),
        ],
    };
    assert!(evaluate(&module).is_err());
}

// ── ArrayLiteral / Index (`SEMANTIC_CORE.md` §9.2) ─────────────────────
