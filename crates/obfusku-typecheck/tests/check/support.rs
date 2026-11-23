//! Builders shared by the themed test files.

pub use obfusku_core::ast::{
    BinOp, Binding, Expr, Literal, MatchArm, Module, Pattern, TypeDecl, UnOp, VariantDecl,
};
pub use obfusku_core::types::Type as CoreType;
pub use obfusku_diagnostics::{Diagnostic, SourceId, Span};
pub use obfusku_typecheck::{TypeResult, check};
pub use std::rc::Rc;

pub fn sp() -> Span {
    Span::new(SourceId(0), 0, 0)
}

pub fn int(v: i64) -> Expr {
    Expr::Lit(Literal::Int(v), sp())
}

pub fn real(v: f64) -> Expr {
    Expr::Lit(Literal::Real(v), sp())
}

pub fn string(v: &str) -> Expr {
    Expr::Lit(Literal::Str(v.to_string()), sp())
}

pub fn boolean(v: bool) -> Expr {
    Expr::Lit(Literal::Bool(v), sp())
}

pub fn unit() -> Expr {
    Expr::Lit(Literal::Unit, sp())
}

pub fn var(name: &str) -> Expr {
    Expr::Var(name.to_string(), sp())
}

pub fn lambda(param: &str, body: Expr) -> Expr {
    Expr::Lambda {
        param: param.to_string(),
        param_ty: None,
        body: Rc::new(body),
        span: sp(),
    }
}

pub fn annotated_lambda(param: &str, ty: CoreType, body: Expr) -> Expr {
    Expr::Lambda {
        param: param.to_string(),
        param_ty: Some(ty),
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

pub fn mutcell(init: Expr) -> Expr {
    Expr::MutCell {
        initial: Box::new(init),
        span: sp(),
    }
}

pub fn mutread(cell: Expr) -> Expr {
    Expr::MutRead {
        cell: Box::new(cell),
        span: sp(),
    }
}

pub fn mutrebind(cell: Expr, value: Expr) -> Expr {
    Expr::MutRebind {
        cell: Box::new(cell),
        new_value: Box::new(value),
        span: sp(),
    }
}

pub fn binding(name: &str, value: Expr) -> Binding {
    Binding {
        name: name.to_string(),
        value,
        is_exported: false,
        span: sp(),
        declared_type: None,
    }
}

pub fn let_expr(name: &str, value: Expr, body: Expr) -> Expr {
    Expr::Let {
        name: name.to_string(),
        value: Box::new(value),
        body: Rc::new(body),
        span: sp(),
    }
}

pub fn letrec_expr(members: Vec<Binding>, body: Expr) -> Expr {
    Expr::LetRec {
        bindings: members,
        body: Rc::new(body),
        span: sp(),
    }
}

pub fn module(bindings: Vec<Binding>) -> Module {
    Module {
        type_decls: Vec::new(),
        bindings: bindings
            .into_iter()
            .map(obfusku_core::ast::BindingGroup::Let)
            .collect(),
    }
}

pub fn check_ok(bindings: Vec<Binding>) -> obfusku_typecheck::TypedModule {
    check(&module(bindings))
        .unwrap_or_else(|ds| panic!("expected success, got errors: {}", describe(&ds)))
}

pub fn check_err(bindings: Vec<Binding>) -> Vec<Diagnostic> {
    match check(&module(bindings)) {
        Err(ds) => ds,
        Ok(_) => panic!("expected a type error, but the module checked successfully"),
    }
}

pub fn module_with_adts(type_decls: Vec<TypeDecl>, bindings: Vec<Binding>) -> Module {
    Module {
        type_decls,
        bindings: bindings
            .into_iter()
            .map(obfusku_core::ast::BindingGroup::Let)
            .collect(),
    }
}

pub fn check_ok_adt(
    type_decls: Vec<TypeDecl>,
    bindings: Vec<Binding>,
) -> obfusku_typecheck::TypedModule {
    check(&module_with_adts(type_decls, bindings))
        .unwrap_or_else(|ds| panic!("expected success, got errors: {}", describe(&ds)))
}

pub fn check_err_adt(type_decls: Vec<TypeDecl>, bindings: Vec<Binding>) -> Vec<Diagnostic> {
    match check(&module_with_adts(type_decls, bindings)) {
        Err(ds) => ds,
        Ok(_) => panic!("expected a type error, but the module checked successfully"),
    }
}

pub fn variant(tag: &str, fields: Vec<CoreType>) -> VariantDecl {
    VariantDecl {
        tag: tag.to_string(),
        fields,
        field_names: None,
        span: sp(),
    }
}

pub fn type_decl(tag: &str, type_params: Vec<&str>, variants: Vec<VariantDecl>) -> TypeDecl {
    TypeDecl {
        tag: tag.to_string(),
        type_params: type_params.into_iter().map(String::from).collect(),
        variants,
        span: sp(),
    }
}

/// `Option<t> = { Some(t) | None }` — the user's own suggested first
/// example, matched exactly.
pub fn option_type_decl() -> TypeDecl {
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
pub fn option_constructor_bindings() -> Vec<Binding> {
    vec![
        Binding {
            name: "Some".to_string(),
            value: Expr::Lambda {
                param: "$field0".to_string(),
                param_ty: None,
                body: Rc::new(Expr::Constructor {
                    tag: "Some".to_string(),
                    args: vec![var("$field0")],
                    span: sp(),
                }),
                span: sp(),
            },
            is_exported: true,
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
            is_exported: true,
            span: sp(),
            declared_type: None,
        },
    ]
}

/// `Result<t, e> = { Ok(t) | Err(e) }` — `t` and `e` independently
/// parameterized, per the user's explicit requirement to verify this,
/// not just Optional's single-parameter case.
pub fn result_type_decl() -> TypeDecl {
    type_decl(
        "Result",
        vec!["t", "e"],
        vec![
            variant("Ok", vec![CoreType::Param("t".to_string())]),
            variant("Err", vec![CoreType::Param("e".to_string())]),
        ],
    )
}

pub fn unary_constructor_binding(tag: &str) -> Binding {
    Binding {
        name: tag.to_string(),
        value: Expr::Lambda {
            param: "$field0".to_string(),
            param_ty: None,
            body: Rc::new(Expr::Constructor {
                tag: tag.to_string(),
                args: vec![var("$field0")],
                span: sp(),
            }),
            span: sp(),
        },
        is_exported: true,
        span: sp(),
        declared_type: None,
    }
}

pub fn result_constructor_bindings() -> Vec<Binding> {
    vec![
        unary_constructor_binding("Ok"),
        unary_constructor_binding("Err"),
    ]
}

pub fn pvar(name: &str) -> Pattern {
    Pattern::Var(name.to_string(), sp())
}

pub fn pwild() -> Pattern {
    Pattern::Wildcard(sp())
}

pub fn plit_int(v: i64) -> Pattern {
    Pattern::Lit(Literal::Int(v), sp())
}

pub fn plit_bool(v: bool) -> Pattern {
    Pattern::Lit(Literal::Bool(v), sp())
}

pub fn pctor(tag: &str, args: Vec<Pattern>) -> Pattern {
    Pattern::Constructor {
        tag: tag.to_string(),
        args,
        span: sp(),
    }
}

pub fn arm(pattern: Pattern, result: Expr) -> MatchArm {
    MatchArm {
        pattern,
        result: Rc::new(result),
        span: sp(),
    }
}

pub fn match_expr(scrutinee: Expr, arms: Vec<MatchArm>) -> Expr {
    Expr::Match {
        scrutinee: Box::new(scrutinee),
        arms,
        span: sp(),
    }
}

pub fn describe(ds: &[Diagnostic]) -> String {
    ds.iter()
        .map(|d| d.message.clone())
        .collect::<Vec<_>>()
        .join("; ")
}

pub fn monomorphic_type(typed: &obfusku_typecheck::TypedModule, name: &str) -> CoreType {
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

pub fn check_err_letrec(members: Vec<Binding>) -> Vec<Diagnostic> {
    let module = Module {
        type_decls: Vec::new(),
        bindings: vec![obfusku_core::ast::BindingGroup::LetRec(members)],
    };
    match check(&module) {
        Err(ds) => ds,
        Ok(_) => panic!("expected a type error, but the module checked successfully"),
    }
}

pub fn declared_binding(name: &str, value: Expr, declared_type: CoreType) -> Binding {
    Binding {
        name: name.to_string(),
        value,
        is_exported: false,
        span: sp(),
        declared_type: Some(declared_type),
    }
}

pub fn check_ok_letrec(
    members: Vec<Binding>,
    rest: Vec<Binding>,
) -> obfusku_typecheck::TypedModule {
    let mut bindings = vec![obfusku_core::ast::BindingGroup::LetRec(members)];
    bindings.extend(rest.into_iter().map(obfusku_core::ast::BindingGroup::Let));
    let module = Module {
        type_decls: Vec::new(),
        bindings,
    };
    check(&module).unwrap_or_else(|ds| panic!("expected success, got errors: {}", describe(&ds)))
}

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

pub fn array_lit(elements: Vec<Expr>) -> Expr {
    Expr::ArrayLiteral {
        elements,
        span: sp(),
    }
}

pub fn index(array: Expr, idx: Expr) -> Expr {
    Expr::Index {
        array: Box::new(array),
        index: Box::new(idx),
        span: sp(),
    }
}

/// Produces a real foreign `Scheme` the same way `stdlib`/`natives`
/// actually do: run a completely independent `check()` (its own,
/// separately-numbered `Checker`) over a small generic module and pull
/// its `Polymorphic` result back out — not hand-constructed, since
/// `TypeVarId` isn't public outside this crate (by design).
pub fn foreign_generic_identity_prelude() -> obfusku_typecheck::Prelude {
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

pub fn binop_expr(op: obfusku_core::ast::BinOp, lhs: Expr, rhs: Expr) -> Expr {
    Expr::BinaryOp {
        op,
        lhs: Box::new(lhs),
        rhs: Box::new(rhs),
        span: sp(),
    }
}

pub fn field(base: Expr, name: &str) -> Expr {
    Expr::Field {
        base: Box::new(base),
        name: name.to_string(),
        span: sp(),
    }
}

pub fn update(base: Expr, fields: Vec<(&str, Expr)>) -> Expr {
    Expr::Update {
        base: Box::new(base),
        fields: fields
            .into_iter()
            .map(|(n, e)| (n.to_string(), e))
            .collect(),
        span: sp(),
    }
}

pub fn point_decl() -> TypeDecl {
    type_decl(
        "P",
        vec![],
        vec![VariantDecl {
            tag: "P".to_string(),
            fields: vec![CoreType::Int, CoreType::Int],
            field_names: Some(vec!["x".to_string(), "y".to_string()]),
            span: sp(),
        }],
    )
}

pub fn point(x: i64, y: i64) -> Expr {
    Expr::Constructor {
        tag: "P".to_string(),
        args: vec![int(x), int(y)],
        span: sp(),
    }
}

pub fn module_with(types: Vec<TypeDecl>, bindings: Vec<Binding>) -> Module {
    Module {
        type_decls: types,
        bindings: bindings
            .into_iter()
            .map(obfusku_core::ast::BindingGroup::Let)
            .collect(),
    }
}
