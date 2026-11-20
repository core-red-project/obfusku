//! Builders for Core AST fixtures and the `run*` helpers every test uses.
//!
//! Tests build Core directly: `obfusku-runtime` deliberately has no
//! dependency on the parser, so fixtures cannot come from source text.

pub use obfusku_core::ast::{BinOp, Expr, MatchArm, Module, Pattern, UnOp};
pub use obfusku_diagnostics::{SourceId, Span};
pub use obfusku_runtime::{Value, evaluate, evaluate_with_prelude};
pub use std::rc::Rc;

pub fn sp() -> Span {
    Span {
        source: SourceId(0),
        start: 0,
        end: 0,
    }
}

pub fn var(n: &str) -> Expr {
    Expr::Var(n.to_string(), sp())
}
pub fn int(v: i64) -> Expr {
    Expr::Lit(obfusku_core::ast::Literal::Int(v), sp())
}
pub fn real(v: f64) -> Expr {
    Expr::Lit(obfusku_core::ast::Literal::Real(v), sp())
}
pub fn str_(v: &str) -> Expr {
    Expr::Lit(obfusku_core::ast::Literal::Str(v.to_string()), sp())
}
pub fn boolean(v: bool) -> Expr {
    Expr::Lit(obfusku_core::ast::Literal::Bool(v), sp())
}
pub fn lambda(param: &str, body: Expr) -> Expr {
    Expr::Lambda {
        param: param.to_string(),
        param_ty: None,
        body: Rc::new(body),
        span: sp(),
    }
}
pub fn apply(f: Expr, a: Expr) -> Expr {
    Expr::Apply {
        func: Box::new(f),
        arg: Box::new(a),
        span: sp(),
    }
}
pub fn apply2(f: Expr, a: Expr, b: Expr) -> Expr {
    apply(apply(f, a), b)
}
pub fn mutcell(initial: Expr) -> Expr {
    Expr::MutCell {
        initial: Box::new(initial),
        span: sp(),
    }
}
pub fn mutread(cell: Expr) -> Expr {
    Expr::MutRead {
        cell: Box::new(cell),
        span: sp(),
    }
}
pub fn mutrebind(cell: Expr, new_value: Expr) -> Expr {
    Expr::MutRebind {
        cell: Box::new(cell),
        new_value: Box::new(new_value),
        span: sp(),
    }
}
pub fn ctor(tag: &str, args: Vec<Expr>) -> Expr {
    Expr::Constructor {
        tag: tag.to_string(),
        args,
        span: sp(),
    }
}
pub fn match_(scrutinee: Expr, arms: Vec<(Pattern, Expr)>) -> Expr {
    Expr::Match {
        scrutinee: Box::new(scrutinee),
        arms: arms
            .into_iter()
            .map(|(pattern, result)| MatchArm {
                pattern,
                result: Rc::new(result),
                span: sp(),
            })
            .collect(),
        span: sp(),
    }
}
pub fn pvar(n: &str) -> Pattern {
    Pattern::Var(n.to_string(), sp())
}
pub fn pwild() -> Pattern {
    Pattern::Wildcard(sp())
}
pub fn pctor(tag: &str, args: Vec<Pattern>) -> Pattern {
    Pattern::Constructor {
        tag: tag.to_string(),
        args,
        span: sp(),
    }
}
pub fn raise(value: Expr) -> Expr {
    Expr::Raise {
        value: Box::new(value),
        span: sp(),
    }
}
pub fn catch(body: Expr, handler_param: &str, handler_body: Expr) -> Expr {
    Expr::Catch {
        body: Box::new(body),
        handler_param: handler_param.to_string(),
        handler_body: Rc::new(handler_body),
        span: sp(),
    }
}

/// A one-binding module whose value is `expr` — the common case for
/// these tests, where we only care about the last binding's result.
pub fn mk_binding(name: &str, value: Expr) -> obfusku_core::ast::Binding {
    obfusku_core::ast::Binding {
        name: name.to_string(),
        value,
        exported: false,
        span: sp(),
        declared_type: None,
    }
}

pub fn run(expr: Expr) -> Value {
    let module = Module {
        type_decls: vec![],
        bindings: vec![obfusku_core::ast::BindingGroup::Let(mk_binding(
            "result", expr,
        ))],
    };
    evaluate(&module).expect("evaluation should not fail")
}

/// Every entry becomes its own ordinary `Let` slot (sequential, not
/// mutually visible). Tests needing a `LetRec` group build `Module`
/// directly with `BindingGroup::LetRec`.
pub fn run_module(bindings: Vec<(&str, Expr)>) -> Value {
    let module = Module {
        type_decls: vec![],
        bindings: bindings
            .into_iter()
            .map(|(name, value)| obfusku_core::ast::BindingGroup::Let(mk_binding(name, value)))
            .collect(),
    };
    evaluate(&module).expect("evaluation should not fail")
}

/// `letrec_members` become one `BindingGroup::LetRec`; `then` bindings
/// follow as ordinary sequential `Let`s.
pub fn run_letrec(letrec_members: Vec<(&str, Expr)>, then: Vec<(&str, Expr)>) -> Value {
    let mut bindings = vec![obfusku_core::ast::BindingGroup::LetRec(
        letrec_members
            .into_iter()
            .map(|(name, value)| mk_binding(name, value))
            .collect(),
    )];
    bindings.extend(
        then.into_iter()
            .map(|(name, value)| obfusku_core::ast::BindingGroup::Let(mk_binding(name, value))),
    );
    let module = Module {
        type_decls: vec![],
        bindings,
    };
    evaluate(&module).expect("evaluation should not fail")
}

pub fn as_int(v: Value) -> i64 {
    match v {
        Value::Int(i) => i,
        other => panic!("expected Int, got {other:?}"),
    }
}

// ---------------------------------------------------------------------
// Literals and basic evaluation
// ---------------------------------------------------------------------

pub fn binop(op: BinOp, lhs: Expr, rhs: Expr) -> Expr {
    Expr::BinaryOp {
        op,
        lhs: Box::new(lhs),
        rhs: Box::new(rhs),
        span: sp(),
    }
}
pub fn unop(op: UnOp, operand: Expr) -> Expr {
    Expr::UnaryOp {
        op,
        operand: Box::new(operand),
        span: sp(),
    }
}
