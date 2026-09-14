//! Inference engine — environment, unification, generalization, and the
//! per-`Expr` typing rules (`SEMANTIC_CORE.md` §17).

use crate::adt::{check_type_arity, AdtRegistry};
use crate::types::{Scheme, Type, TypeVarId};
use obfusku_core::ast::{BinOp, Expr, Literal, MatchArm, Pattern, UnOp};
use obfusku_core::types::Type as CoreType;
use obfusku_diagnostics::{Diagnostic, Severity, Span};
use std::collections::{HashMap, HashSet};

pub type TypeEnv = HashMap<String, Scheme>;

fn err(span: Span, message: impl Into<String>) -> Diagnostic {
    Diagnostic {
        severity: Severity::Error,
        message: message.into(),
        primary: span,
    }
}

/// The surface glyph for a `BinOp`, used for diagnostic messages only.
fn op_glyph(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "✚",
        BinOp::Sub => "☠︎",
        BinOp::Mul => "✱",
        BinOp::Div => "÷",
        BinOp::Mod => "⌗",
        BinOp::Lt => "<",
        BinOp::Gt => ">",
        BinOp::Le => "<=",
        BinOp::Ge => ">=",
        BinOp::Eq => "==",
        BinOp::NotEq => "!=",
        BinOp::Xor => "⊻",
    }
}

/// §18: a type with a function-typed field anywhere has no equality.
fn type_contains_function(ty: &Type) -> bool {
    match ty {
        Type::Function(..) => true,
        Type::Cell(inner) => type_contains_function(inner),
        Type::Adt(_, args) => args.iter().any(type_contains_function),
        Type::Int | Type::Real | Type::Str | Type::Bool | Type::Unit | Type::Var(_) => false,
    }
}

fn warn(span: Span, message: impl Into<String>) -> Diagnostic {
    Diagnostic {
        severity: Severity::Warning,
        message: message.into(),
        primary: span,
    }
}

pub struct Checker {
    subst: HashMap<TypeVarId, Type>,
    next_var: u32,
    adt_registry: AdtRegistry,
    /// Non-fatal diagnostics — currently just unreachable-arm detection.
    pub warnings: Vec<Diagnostic>,
    /// One fresh unification variable per `Expr::Lambda.param_ty` free
    /// `Type::Param` name. Cached checker-wide because desugaring guarantees
    /// parameter names are module-unique.
    lambda_param_vars: HashMap<String, Type>,
}

impl Checker {
    pub fn new(adt_registry: AdtRegistry) -> Self {
        Checker {
            subst: HashMap::new(),
            next_var: 0,
            adt_registry,
            warnings: Vec::new(),
            lambda_param_vars: HashMap::new(),
        }
    }

    pub(crate) fn adt_registry(&self) -> &AdtRegistry {
        &self.adt_registry
    }

    pub fn fresh(&mut self) -> Type {
        let id = TypeVarId(self.next_var);
        self.next_var += 1;
        Type::Var(id)
    }

    /// Applies the accumulated substitution, chasing `Var` chains fully.
    pub fn resolve(&self, ty: &Type) -> Type {
        match ty {
            Type::Var(v) => match self.subst.get(v) {
                Some(t) => self.resolve(t),
                None => ty.clone(),
            },
            Type::Function(a, b) => {
                Type::Function(Box::new(self.resolve(a)), Box::new(self.resolve(b)))
            }
            Type::Cell(t) => Type::Cell(Box::new(self.resolve(t))),
            Type::Adt(name, args) => {
                Type::Adt(name.clone(), args.iter().map(|a| self.resolve(a)).collect())
            }
            other => other.clone(),
        }
    }

    fn occurs(&self, v: TypeVarId, ty: &Type) -> bool {
        let mut vars = HashSet::new();
        self.resolve(ty).free_vars(&mut vars);
        vars.contains(&v)
    }

    pub fn unify(&mut self, a: &Type, b: &Type, span: Span) -> Result<(), Diagnostic> {
        let a = self.resolve(a);
        let b = self.resolve(b);
        match (&a, &b) {
            (Type::Var(v1), Type::Var(v2)) if v1 == v2 => Ok(()),
            (Type::Var(v), t) | (t, Type::Var(v)) => {
                if self.occurs(*v, t) {
                    Err(err(
                        span,
                        format!("infinite type: {} occurs within itself", t.describe()),
                    ))
                } else {
                    self.subst.insert(*v, t.clone());
                    Ok(())
                }
            }
            (Type::Int, Type::Int)
            | (Type::Real, Type::Real)
            | (Type::Str, Type::Str)
            | (Type::Bool, Type::Bool)
            | (Type::Unit, Type::Unit) => Ok(()),
            (Type::Function(a1, r1), Type::Function(a2, r2)) => {
                self.unify(a1, a2, span)?;
                self.unify(r1, r2, span)
            }
            (Type::Cell(t1), Type::Cell(t2)) => self.unify(t1, t2, span),
            (Type::Adt(n1, args1), Type::Adt(n2, args2))
                if n1 == n2 && args1.len() == args2.len() =>
            {
                for (x, y) in args1.iter().zip(args2.iter()) {
                    self.unify(x, y, span)?;
                }
                Ok(())
            }
            _ => Err(err(
                span,
                format!("expected {}, found {}", a.describe(), b.describe()),
            )),
        }
    }

    /// Fresh-substitutes only `scheme.vars`; a monomorphic scheme
    /// (`vars: []`) is a no-op, returning the same type.
    pub fn instantiate(&mut self, scheme: &Scheme) -> Type {
        let mapping: HashMap<TypeVarId, Type> =
            scheme.vars.iter().map(|v| (*v, self.fresh())).collect();
        substitute(&scheme.ty, &mapping)
    }

    /// Rebinds a `Scheme` minted by a different `Checker` to this
    /// checker's own fresh `TypeVarId`s, preserving quantification.
    ///
    /// `TypeVarId`s are checker-local. Normalizing prevents foreign IDs
    /// from colliding with local variables during generalization.
    /// Returns another polymorphic `Scheme`.
    pub fn normalize_scheme(&mut self, scheme: &Scheme) -> Scheme {
        let mapping: HashMap<TypeVarId, Type> =
            scheme.vars.iter().map(|v| (*v, self.fresh())).collect();
        let ty = substitute(&scheme.ty, &mapping);
        let vars: Vec<TypeVarId> = scheme
            .vars
            .iter()
            .map(|v| match &mapping[v] {
                Type::Var(id) => *id,
                _ => unreachable!("fresh() always returns Type::Var"),
            })
            .collect();

        // A `Scheme` crossing this boundary must be fully closed over
        // its own `vars` — any other free variable left in `ty` after
        // substitution is a foreign `TypeVarId` that was never in
        // `scheme.vars` to begin with, i.e. a scheme that was already
        // broken before it got here (see `Scheme`'s own boundary
        // invariant). Letting that slip through silently would just
        // relocate the original bug one call earlier.
        let mut remaining = HashSet::new();
        ty.free_vars(&mut remaining);
        assert!(
            remaining.iter().all(|v| vars.contains(v)),
            "normalize_scheme: scheme was not closed over its own declared \
             `vars` — {scheme:?} still has free variable(s) outside {vars:?} \
             after normalization"
        );

        Scheme { vars, ty }
    }

    /// §19.1: quantifies over every variable free in `ty` but not in
    /// `env`. Callers must only invoke this when `is_syntactic_value`
    /// permits it.
    pub fn generalize(&self, ty: &Type, env: &TypeEnv) -> Scheme {
        let resolved = self.resolve(ty);
        let mut ty_vars = HashSet::new();
        resolved.free_vars(&mut ty_vars);

        let mut env_vars = HashSet::new();
        for scheme in env.values() {
            let mut scheme_ty_vars = HashSet::new();
            self.resolve(&scheme.ty).free_vars(&mut scheme_ty_vars);
            for v in scheme_ty_vars {
                if !scheme.vars.contains(&v) {
                    env_vars.insert(v);
                }
            }
        }

        let vars: Vec<TypeVarId> = ty_vars.difference(&env_vars).copied().collect();
        Scheme { vars, ty: resolved }
    }

    pub fn infer(&mut self, expr: &Expr, env: &TypeEnv) -> Result<Type, Vec<Diagnostic>> {
        match expr {
            Expr::Lit(Literal::Int(_), _) => Ok(Type::Int),
            Expr::Lit(Literal::Real(_), _) => Ok(Type::Real),
            Expr::Lit(Literal::Str(_), _) => Ok(Type::Str),
            Expr::Lit(Literal::Bool(_), _) => Ok(Type::Bool),
            Expr::Lit(Literal::Unit, _) => Ok(Type::Unit),

            Expr::Var(name, span) => match env.get(name) {
                Some(scheme) => Ok(self.instantiate(scheme)),
                None => Err(vec![err(*span, format!("unknown variable '{name}'"))]),
            },

            Expr::Lambda {
                param,
                param_ty: None,
                body,
                ..
            } => {
                let param_ty = self.fresh();
                let mut inner_env = env.clone();
                inner_env.insert(param.clone(), Scheme::monomorphic(param_ty.clone()));
                let body_ty = self.infer(body, &inner_env)?;
                Ok(Type::Function(Box::new(param_ty), Box::new(body_ty)))
            }

            // §7.3 optional parameter annotation: unified with the parameter's
            // type. Type variable names are pre-renamed by desugar to be unique.
            Expr::Lambda {
                param,
                param_ty: Some(declared),
                body,
                span,
            } => {
                let mut arity_errors = Vec::new();
                check_type_arity(declared, &self.adt_registry.adts, *span, &mut arity_errors);
                if !arity_errors.is_empty() {
                    return Err(arity_errors);
                }
                let mut free = Vec::new();
                crate::adt::free_params(declared, &mut free);
                let mut fresh_map: HashMap<String, Type> = HashMap::new();
                for name in free {
                    let v = match self.lambda_param_vars.get(&name) {
                        Some(existing) => existing.clone(),
                        None => {
                            let v = self.fresh();
                            self.lambda_param_vars.insert(name.clone(), v.clone());
                            v
                        }
                    };
                    fresh_map.insert(name, v);
                }
                let declared_internal = instantiate_core_type(declared, &fresh_map);
                let mut inner_env = env.clone();
                inner_env.insert(
                    param.clone(),
                    Scheme::monomorphic(declared_internal.clone()),
                );
                let body_ty = self.infer(body, &inner_env)?;
                Ok(Type::Function(
                    Box::new(declared_internal),
                    Box::new(body_ty),
                ))
            }

            Expr::Apply { func, arg, span } => {
                let func_ty = self.infer(func, env)?;
                let arg_ty = self.infer(arg, env)?;

                match self.resolve(&func_ty) {
                    Type::Function(param_ty, return_ty) => {
                        self.unify(&param_ty, &arg_ty, *span).map_err(|_| {
                            vec![err(
                                *span,
                                format!(
                                    "argument type mismatch: expected {}, found {}",
                                    self.resolve(&param_ty).describe(),
                                    self.resolve(&arg_ty).describe()
                                ),
                            )]
                        })?;
                        Ok(self.resolve(&return_ty))
                    }
                    Type::Var(_) => {
                        let result_ty = self.fresh();
                        let expected =
                            Type::Function(Box::new(arg_ty.clone()), Box::new(result_ty.clone()));
                        self.unify(&func_ty, &expected, *span)
                            .map_err(|d| vec![d])?;
                        Ok(self.resolve(&result_ty))
                    }
                    other => Err(vec![err(
                        *span,
                        format!(
                            "cannot apply a value of type {} as a function",
                            other.describe()
                        ),
                    )]),
                }
            }

            Expr::MutCell { initial, .. } => {
                let inner_ty = self.infer(initial, env)?;
                Ok(Type::Cell(Box::new(inner_ty)))
            }

            Expr::MutRead { cell, span } => {
                let cell_ty = self.infer(cell, env)?;
                match self.resolve(&cell_ty) {
                    Type::Cell(inner) => Ok(*inner),
                    Type::Var(v) => {
                        let inner = self.fresh();
                        self.subst.insert(v, Type::Cell(Box::new(inner.clone())));
                        Ok(inner)
                    }
                    other => Err(vec![err(
                        *span,
                        format!("cannot read: {} is not a mutable cell", other.describe()),
                    )]),
                }
            }

            Expr::MutRebind {
                cell,
                new_value,
                span,
            } => {
                let cell_ty = self.infer(cell, env)?;
                let value_ty = self.infer(new_value, env)?;
                match self.resolve(&cell_ty) {
                    Type::Cell(inner) => {
                        self.unify(&inner, &value_ty, *span).map_err(|_| {
                            vec![err(
                                *span,
                                format!(
                                    "cannot rebind: cell holds {}, but the new value has type {}",
                                    self.resolve(&inner).describe(),
                                    self.resolve(&value_ty).describe()
                                ),
                            )]
                        })?;
                    }
                    Type::Var(v) => {
                        self.subst.insert(v, Type::Cell(Box::new(value_ty.clone())));
                    }
                    other => {
                        return Err(vec![err(
                            *span,
                            format!("cannot rebind: {} is not a mutable cell", other.describe()),
                        )])
                    }
                }
                Ok(Type::Unit)
            }

            Expr::Constructor { tag, args, span } => {
                let variant = self
                    .adt_registry
                    .variants
                    .get(tag)
                    .cloned()
                    .ok_or_else(|| vec![err(*span, format!("unknown constructor '{tag}'"))])?;
                if args.len() != variant.field_types.len() {
                    return Err(vec![err(
                        *span,
                        format!(
                            "constructor '{tag}' expects {} argument(s), found {}",
                            variant.field_types.len(),
                            args.len()
                        ),
                    )]);
                }
                let fresh_map: HashMap<String, Type> = variant
                    .type_params
                    .iter()
                    .map(|p| (p.clone(), self.fresh()))
                    .collect();
                for (arg, field_ty) in args.iter().zip(variant.field_types.iter()) {
                    let arg_ty = self.infer(arg, env)?;
                    let expected = instantiate_core_type(field_ty, &fresh_map);
                    self.unify(&arg_ty, &expected, *span).map_err(|_| {
                        vec![err(
                            *span,
                            format!(
                                "constructor '{tag}' field type mismatch: expected {}, found {}",
                                self.resolve(&expected).describe(),
                                self.resolve(&arg_ty).describe()
                            ),
                        )]
                    })?;
                }
                let instantiated_params: Vec<Type> = variant
                    .type_params
                    .iter()
                    .map(|p| fresh_map[p].clone())
                    .collect();
                Ok(Type::Adt(variant.adt_name.clone(), instantiated_params))
            }

            Expr::Match {
                scrutinee,
                arms,
                span,
            } => self.infer_match(scrutinee, arms, *span, env),

            // §15.2: operand must be Exception; Raise's own type unifies
            // with whatever the context expects (fresh, unconstrained).
            Expr::Raise { value, span } => {
                let value_ty = self.infer(value, env)?;
                let exception_ty = Type::Adt("Exception".to_string(), vec![]);
                self.unify(&exception_ty, &value_ty, *span)
                    .map_err(|d| vec![d])?;
                Ok(self.fresh())
            }
            // Body and handler must agree on a result type; the handler
            // parameter is bound at Exception.
            Expr::Catch {
                body,
                handler_param,
                handler_body,
                span,
            } => {
                let body_ty = self.infer(body, env)?;
                let exception_ty = Type::Adt("Exception".to_string(), vec![]);
                let mut handler_env = env.clone();
                handler_env.insert(handler_param.clone(), Scheme::monomorphic(exception_ty));
                let handler_ty = self.infer(handler_body, &handler_env)?;
                self.unify(&body_ty, &handler_ty, *span)
                    .map_err(|d| vec![d])?;
                Ok(body_ty)
            }

            Expr::BinaryOp { op, lhs, rhs, span } => self.infer_binop(*op, lhs, rhs, *span, env),
            Expr::UnaryOp { op, operand, span } => self.infer_unop(*op, operand, *span, env),

            Expr::Let {
                name, value, body, ..
            } => {
                let value_ty = self.infer(value, env)?;
                let scheme = if is_syntactic_value(value) {
                    self.generalize(&value_ty, env)
                } else {
                    Scheme::monomorphic(value_ty)
                };
                let mut inner_env = env.clone();
                inner_env.insert(name.clone(), scheme);
                self.infer(body, &inner_env)
            }

            // Fresh placeholder slots (declared_type unified in before
            // the body is inferred), then infer+unify every member
            // against its own slot, generalizing only afterward.
            Expr::LetRec { bindings, body, .. } => {
                let mut slots: TypeEnv = TypeEnv::new();
                // See `infer_lambda_chain_against`'s doc comment — kept
                // past this loop so the second pass can push each
                // declared parameter type *into* its `Lambda`, not only
                // check the whole signature after the body is inferred.
                let mut declared_internals: HashMap<&str, Type> = HashMap::new();
                for m in bindings {
                    let v = self.fresh();
                    if let Some(declared) = &m.declared_type {
                        let mut arity_errors = Vec::new();
                        check_type_arity(
                            declared,
                            &self.adt_registry.adts,
                            m.span,
                            &mut arity_errors,
                        );
                        if !arity_errors.is_empty() {
                            return Err(arity_errors);
                        }
                        // Instantiate declaration type variables to fresh variables.
                        let mut free = Vec::new();
                        crate::adt::free_params(declared, &mut free);
                        let fresh_map: HashMap<String, Type> =
                            free.into_iter().map(|p| (p, self.fresh())).collect();
                        let declared_internal = instantiate_core_type(declared, &fresh_map);
                        self.unify(&v, &declared_internal, m.span)
                            .map_err(|d| vec![d])?;
                        declared_internals.insert(m.name.as_str(), declared_internal);
                    }
                    slots.insert(m.name.clone(), Scheme::monomorphic(v));
                }
                let mut group_env = env.clone();
                group_env.extend(slots.iter().map(|(k, v)| (k.clone(), v.clone())));

                let mut inferred_members = Vec::with_capacity(bindings.len());
                for m in bindings {
                    let inferred = match declared_internals.get(m.name.as_str()) {
                        Some(declared_internal) => self.infer_lambda_chain_against(
                            &m.value,
                            declared_internal,
                            &group_env,
                        )?,
                        None => self.infer(&m.value, &group_env)?,
                    };
                    if let Some(slot) = slots.get(&m.name) {
                        if let Type::Var(id) = slot.ty {
                            self.unify(&Type::Var(id), &inferred, m.span)
                                .map_err(|d| vec![d])?;
                        }
                    }
                    inferred_members.push((m, inferred));
                }

                let mut body_env = env.clone();
                for (m, inferred) in inferred_members {
                    let scheme = self.generalize(&inferred, &body_env);
                    body_env.insert(m.name.clone(), scheme);
                }
                self.infer(body, &body_env)
            }

            // Homogeneous: every element unifies against one shared
            // type; an empty literal leaves it an unconstrained variable.
            Expr::ArrayLiteral { elements, span } => {
                let elem_ty = self.fresh();
                for e in elements {
                    let e_ty = self.infer(e, env)?;
                    self.unify(&elem_ty, &e_ty, *span).map_err(|_| {
                        vec![err(
                            *span,
                            format!(
                                "array elements must all share one type: expected {}, found {}",
                                self.resolve(&elem_ty).describe(),
                                self.resolve(&e_ty).describe()
                            ),
                        )]
                    })?;
                }
                Ok(Type::Adt("Array".to_string(), vec![elem_ty]))
            }

            // Out-of-bounds is a runtime condition, never checked here.
            Expr::Index { array, index, span } => {
                let array_ty = self.infer(array, env)?;
                let index_ty = self.infer(index, env)?;
                self.unify(&index_ty, &Type::Int, *span).map_err(|_| {
                    vec![err(
                        *span,
                        format!(
                            "array index must be Int, found {}",
                            self.resolve(&index_ty).describe()
                        ),
                    )]
                })?;
                let elem_ty = self.fresh();
                let expected_array_ty = Type::Adt("Array".to_string(), vec![elem_ty.clone()]);
                self.unify(&array_ty, &expected_array_ty, *span)
                    .map_err(|_| {
                        vec![err(
                            *span,
                            format!(
                                "cannot index: {} is not an Array",
                                self.resolve(&array_ty).describe()
                            ),
                        )]
                    })?;
                Ok(self.resolve(&elem_ty))
            }
        }
    }

    /// Infers a `LetRec` member's curried `Lambda` chain against its
    /// declared `FunctionType`, peeling one parameter type off `expected`
    /// per nested `Lambda` and seeding it into that parameter's scope.
    ///
    /// Seeding parameter types before inferring the body ensures operators
    /// that require resolved types (such as arithmetic) have concrete types
    /// available during body inference.
    pub(crate) fn infer_lambda_chain_against(
        &mut self,
        expr: &Expr,
        expected: &Type,
        env: &TypeEnv,
    ) -> Result<Type, Vec<Diagnostic>> {
        match (expr, expected) {
            (Expr::Lambda { param, body, .. }, Type::Function(param_ty, ret_ty)) => {
                let mut inner_env = env.clone();
                inner_env.insert(param.clone(), Scheme::monomorphic((**param_ty).clone()));
                let body_ty = self.infer_lambda_chain_against(body, ret_ty, &inner_env)?;
                Ok(Type::Function(param_ty.clone(), Box::new(body_ty)))
            }
            _ => {
                let actual = self.infer(expr, env)?;
                self.unify(&actual, expected, expr.span())
                    .map_err(|d| vec![d])?;
                Ok(actual)
            }
        }
    }

    /// §20.2's closed operator table. Division/modulo by zero is a
    /// runtime condition (§15.3), not checked here.
    fn infer_binop(
        &mut self,
        op: BinOp,
        lhs: &Expr,
        rhs: &Expr,
        span: Span,
        env: &TypeEnv,
    ) -> Result<Type, Vec<Diagnostic>> {
        let lhs_ty = self.infer(lhs, env)?;
        match op {
            BinOp::Eq | BinOp::NotEq => {
                let rhs_ty = self.infer(rhs, env)?;
                self.unify(&lhs_ty, &rhs_ty, span).map_err(|d| vec![d])?;
                let resolved = self.resolve(&lhs_ty);
                if type_contains_function(&resolved) {
                    return Err(vec![err(
                        span,
                        format!(
                            "'{}' has no equality — a type with a function-typed value anywhere \
                             in its structure has no generated equality (SEMANTIC_CORE.md §18)",
                            resolved.describe()
                        ),
                    )]);
                }
                Ok(Type::Bool)
            }
            BinOp::Add => {
                let resolved_lhs = self.resolve(&lhs_ty);
                let candidate = match resolved_lhs {
                    Type::Int | Type::Real | Type::Str => resolved_lhs,
                    other => {
                        return Err(vec![err(
                            span,
                            format!(
                                "'✚' is only defined for Int, Real, or Str (SEMANTIC_CORE.md \
                                 §20.2's closed operator table) — found {}",
                                other.describe()
                            ),
                        )])
                    }
                };
                let rhs_ty = self.infer(rhs, env)?;
                self.unify(&candidate, &rhs_ty, span).map_err(|d| vec![d])?;
                Ok(candidate)
            }
            BinOp::Sub | BinOp::Mul | BinOp::Div => {
                let candidate = self.numeric_dispatch(&lhs_ty, op_glyph(op), span)?;
                let rhs_ty = self.infer(rhs, env)?;
                self.unify(&candidate, &rhs_ty, span).map_err(|d| vec![d])?;
                Ok(candidate)
            }
            BinOp::Mod => {
                self.unify(&Type::Int, &lhs_ty, span).map_err(|d| vec![d])?;
                let rhs_ty = self.infer(rhs, env)?;
                self.unify(&Type::Int, &rhs_ty, span).map_err(|d| vec![d])?;
                Ok(Type::Int)
            }
            BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge => {
                let candidate = self.numeric_dispatch(&lhs_ty, op_glyph(op), span)?;
                let rhs_ty = self.infer(rhs, env)?;
                self.unify(&candidate, &rhs_ty, span).map_err(|d| vec![d])?;
                Ok(Type::Bool)
            }
            BinOp::Xor => {
                self.unify(&Type::Bool, &lhs_ty, span)
                    .map_err(|d| vec![d])?;
                let rhs_ty = self.infer(rhs, env)?;
                self.unify(&Type::Bool, &rhs_ty, span)
                    .map_err(|d| vec![d])?;
                Ok(Type::Bool)
            }
        }
    }

    fn infer_unop(
        &mut self,
        op: UnOp,
        operand: &Expr,
        span: Span,
        env: &TypeEnv,
    ) -> Result<Type, Vec<Diagnostic>> {
        let operand_ty = self.infer(operand, env)?;
        match op {
            UnOp::Not => {
                self.unify(&Type::Bool, &operand_ty, span)
                    .map_err(|d| vec![d])?;
                Ok(Type::Bool)
            }
            UnOp::Neg => self.numeric_dispatch(&operand_ty, "−", span),
        }
    }

    fn numeric_dispatch(
        &mut self,
        ty: &Type,
        glyph: &str,
        span: Span,
    ) -> Result<Type, Vec<Diagnostic>> {
        match self.resolve(ty) {
            t @ (Type::Int | Type::Real) => Ok(t),
            other => Err(vec![err(
                span,
                format!(
                    "'{glyph}' is only defined for Int or Real (SEMANTIC_CORE.md §20.2's closed \
                     operator table) — found {}",
                    other.describe()
                ),
            )]),
        }
    }

    fn infer_match(
        &mut self,
        scrutinee: &Expr,
        arms: &[MatchArm],
        span: Span,
        env: &TypeEnv,
    ) -> Result<Type, Vec<Diagnostic>> {
        let scrutinee_ty = self.infer(scrutinee, env)?;

        let mut result_ty: Option<Type> = None;
        let mut seen_tags: HashSet<&str> = HashSet::new();
        let mut seen_catchall = false;

        for arm in arms {
            if seen_catchall {
                self.warnings.push(warn(
                    arm.span,
                    "unreachable match arm: a previous wildcard/variable arm already covers \
                     every remaining case",
                ));
            } else {
                match &arm.pattern {
                    Pattern::Constructor { tag, .. } => {
                        if !seen_tags.insert(tag.as_str()) {
                            self.warnings.push(warn(
                                arm.span,
                                format!(
                                    "unreachable match arm: '{tag}' is already covered by an \
                                     earlier arm"
                                ),
                            ));
                        }
                    }
                    Pattern::Var(..) | Pattern::Wildcard(..) => seen_catchall = true,
                    Pattern::Lit(..) => {}
                }
            }

            let mut arm_env = env.clone();
            self.check_pattern(&arm.pattern, &scrutinee_ty, &mut arm_env)?;
            let arm_ty = self.infer(&arm.result, &arm_env)?;
            match &result_ty {
                None => result_ty = Some(arm_ty),
                Some(rt) => {
                    let rt = rt.clone();
                    self.unify(&rt, &arm_ty, arm.span).map_err(|_| {
                        vec![err(
                            arm.span,
                            format!(
                                "match arms have incompatible types: expected {}, found {}",
                                self.resolve(&rt).describe(),
                                self.resolve(&arm_ty).describe()
                            ),
                        )]
                    })?;
                }
            }
        }

        let patterns: Vec<&Pattern> = arms.iter().map(|a| &a.pattern).collect();
        self.check_exhaustive(&scrutinee_ty, &patterns, span)
            .map_err(|d| vec![d])?;

        // MatchArm parsing requires at least one arm.
        Ok(result_ty.expect("Match always has at least one arm"))
    }

    fn check_pattern(
        &mut self,
        pattern: &Pattern,
        expected_ty: &Type,
        env: &mut TypeEnv,
    ) -> Result<(), Vec<Diagnostic>> {
        match pattern {
            Pattern::Var(name, _) => {
                env.insert(name.clone(), Scheme::monomorphic(expected_ty.clone()));
                Ok(())
            }
            Pattern::Wildcard(_) => Ok(()),
            Pattern::Lit(lit, span) => {
                let lit_ty = match lit {
                    Literal::Int(_) => Type::Int,
                    Literal::Real(_) => Type::Real,
                    Literal::Str(_) => Type::Str,
                    Literal::Bool(_) => Type::Bool,
                    Literal::Unit => Type::Unit,
                };
                self.unify(expected_ty, &lit_ty, *span).map_err(|_| {
                    vec![err(
                        *span,
                        format!(
                            "pattern type mismatch: expected {}, found {}",
                            self.resolve(expected_ty).describe(),
                            lit_ty.describe()
                        ),
                    )]
                })
            }
            Pattern::Constructor { tag, args, span } => {
                let variant = self
                    .adt_registry
                    .variants
                    .get(tag)
                    .cloned()
                    .ok_or_else(|| {
                        vec![err(
                            *span,
                            format!("unknown constructor '{tag}' in pattern"),
                        )]
                    })?;
                if args.len() != variant.field_types.len() {
                    return Err(vec![err(
                        *span,
                        format!(
                            "pattern for '{tag}' expects {} argument(s), found {}",
                            variant.field_types.len(),
                            args.len()
                        ),
                    )]);
                }
                let fresh_map: HashMap<String, Type> = variant
                    .type_params
                    .iter()
                    .map(|p| (p.clone(), self.fresh()))
                    .collect();
                let adt_ty = Type::Adt(
                    variant.adt_name.clone(),
                    variant
                        .type_params
                        .iter()
                        .map(|p| fresh_map[p].clone())
                        .collect(),
                );
                self.unify(expected_ty, &adt_ty, *span).map_err(|_| {
                    vec![err(
                        *span,
                        format!(
                            "pattern for '{tag}' does not match scrutinee type {}",
                            self.resolve(expected_ty).describe()
                        ),
                    )]
                })?;
                for (arg_pat, field_ty) in args.iter().zip(variant.field_types.iter()) {
                    let expected_field = instantiate_core_type(field_ty, &fresh_map);
                    self.check_pattern(arg_pat, &expected_field, env)?;
                }
                Ok(())
            }
        }
    }

    /// §17: closed types (ADT, `Bool`) are exhaustive when every
    /// variant/value is covered, or a var/wildcard arm is present; open
    /// types (`Int`, `Real`, `Str`) always require the latter.
    fn check_exhaustive(
        &self,
        scrutinee_ty: &Type,
        patterns: &[&Pattern],
        span: Span,
    ) -> Result<(), Diagnostic> {
        if patterns
            .iter()
            .any(|p| matches!(p, Pattern::Var(..) | Pattern::Wildcard(..)))
        {
            return Ok(());
        }
        match self.resolve(scrutinee_ty) {
            Type::Adt(adt_name, type_args) => {
                self.check_exhaustive_adt(&adt_name, &type_args, patterns, span)
            }
            Type::Int | Type::Real | Type::Str => Err(err(
                span,
                "non-exhaustive match: this type has infinitely many values; a wildcard or \
                 variable arm is required",
            )),
            Type::Bool => {
                let covered: HashSet<bool> = patterns
                    .iter()
                    .filter_map(|p| match p {
                        Pattern::Lit(Literal::Bool(b), _) => Some(*b),
                        _ => None,
                    })
                    .collect();
                if covered.len() == 2 {
                    Ok(())
                } else {
                    Err(err(
                        span,
                        "non-exhaustive match: both 'true' and 'false' must be covered, or a \
                         wildcard/variable arm added",
                    ))
                }
            }
            // Function/Cell/Unit/unresolved Var: permissive, unaddressed.
            _ => Ok(()),
        }
    }

    /// `type_args` are the scrutinee's actual type arguments at this
    /// position (e.g. `[Int]` for `Option<Int>`) — needed to instantiate
    /// each field's declared `Param` placeholder before recursing.
    fn check_exhaustive_adt(
        &self,
        adt_name: &str,
        type_args: &[Type],
        patterns: &[&Pattern],
        span: Span,
    ) -> Result<(), Diagnostic> {
        let Some(adt_info) = self.adt_registry.adts.get(adt_name) else {
            return Ok(()); // unresolved ADT name; permissive fallback
        };
        let covered: HashSet<&str> = patterns
            .iter()
            .filter_map(|p| match p {
                Pattern::Constructor { tag, .. } => Some(tag.as_str()),
                _ => None,
            })
            .collect();
        let missing: Vec<&str> = adt_info
            .variants
            .iter()
            .map(String::as_str)
            .filter(|t| !covered.contains(t))
            .collect();
        if !missing.is_empty() {
            return Err(err(
                span,
                format!(
                    "non-exhaustive match: missing case(s) for {}",
                    missing.join(", ")
                ),
            ));
        }

        for tag in &adt_info.variants {
            let variant = &self.adt_registry.variants[tag];
            let param_map: HashMap<String, Type> = variant
                .type_params
                .iter()
                .cloned()
                .zip(type_args.iter().cloned())
                .collect();
            for (field_idx, field_ty) in variant.field_types.iter().enumerate() {
                let subpatterns: Vec<&Pattern> = patterns
                    .iter()
                    .filter_map(|p| match p {
                        Pattern::Constructor { tag: t, args, .. } if t == tag => {
                            Some(&args[field_idx])
                        }
                        _ => None,
                    })
                    .collect();
                if subpatterns.is_empty()
                    || subpatterns
                        .iter()
                        .any(|p| matches!(p, Pattern::Var(..) | Pattern::Wildcard(..)))
                {
                    continue;
                }
                let instantiated = self.resolve(&instantiate_core_type(field_ty, &param_map));
                match instantiated {
                    Type::Adt(sub_name, sub_args) => {
                        self.check_exhaustive_adt(&sub_name, &sub_args, &subpatterns, span)?;
                    }
                    _ => {
                        return Err(err(
                            span,
                            format!(
                                "non-exhaustive match: field {field_idx} of '{tag}' requires a \
                                 wildcard or variable pattern"
                            ),
                        ))
                    }
                }
            }
        }
        Ok(())
    }
}

fn substitute(ty: &Type, mapping: &HashMap<TypeVarId, Type>) -> Type {
    match ty {
        Type::Var(v) => mapping.get(v).cloned().unwrap_or_else(|| ty.clone()),
        Type::Function(a, b) => Type::Function(
            Box::new(substitute(a, mapping)),
            Box::new(substitute(b, mapping)),
        ),
        Type::Cell(t) => Type::Cell(Box::new(substitute(t, mapping))),
        Type::Adt(name, args) => Type::Adt(
            name.clone(),
            args.iter().map(|a| substitute(a, mapping)).collect(),
        ),
        other => other.clone(),
    }
}

/// `obfusku_core::types::Type` → this crate's internal `Type`,
/// substituting each `Param` per `fresh_map`.
pub fn instantiate_core_type(core_ty: &CoreType, fresh_map: &HashMap<String, Type>) -> Type {
    match core_ty {
        CoreType::Int => Type::Int,
        CoreType::Real => Type::Real,
        CoreType::Str => Type::Str,
        CoreType::Bool => Type::Bool,
        CoreType::Unit => Type::Unit,
        CoreType::Function(a, b) => Type::Function(
            Box::new(instantiate_core_type(a, fresh_map)),
            Box::new(instantiate_core_type(b, fresh_map)),
        ),
        CoreType::Cell(t) => Type::Cell(Box::new(instantiate_core_type(t, fresh_map))),
        CoreType::Adt(name, args) => Type::Adt(
            name.clone(),
            args.iter()
                .map(|a| instantiate_core_type(a, fresh_map))
                .collect(),
        ),
        CoreType::Param(name) => fresh_map
            .get(name)
            .cloned()
            .unwrap_or_else(|| panic!("unbound type parameter '{name}' in a declared field type")),
    }
}

/// §19.1: only these shapes are eligible for generalization.
/// `Constructor` is eligible only when every argument is, recursively.
pub fn is_syntactic_value(expr: &Expr) -> bool {
    match expr {
        Expr::Lit(..) | Expr::Var(..) | Expr::Lambda { .. } => true,
        Expr::Constructor { args, .. } => args.iter().all(is_syntactic_value),
        Expr::Apply { .. }
        | Expr::MutCell { .. }
        | Expr::MutRead { .. }
        | Expr::MutRebind { .. }
        | Expr::Match { .. }
        | Expr::Raise { .. }
        | Expr::Catch { .. }
        | Expr::BinaryOp { .. }
        | Expr::UnaryOp { .. }
        | Expr::Let { .. }
        | Expr::LetRec { .. }
        | Expr::ArrayLiteral { .. }
        | Expr::Index { .. } => false,
    }
}
