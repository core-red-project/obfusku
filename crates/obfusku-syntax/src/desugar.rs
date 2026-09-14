//! Surface AST → Core AST — `spec/SEMANTIC_CORE.md` §16's consolidated
//! desugaring table, `spec/CONCRETE_SYMBOLIC_GRAMMAR.md` §7/§12/§7.7.
//!
//! **`◈` scoping** (§8): a bare-callable `Stage` passes the ambient
//! `pipe_ref` through unchanged; an expression `Stage` generates a
//! fresh synthetic name and lowers its body with that as the new
//! ambient value, shadowing the outer one lexically inside it.
//!
//! **Mutability** (§12/§12.1/§13):
//! - `x ≔˚ init` lowers to a binding whose value is `MutCell(init)`.
//! - An ordinary reference to a mut-bound name lowers to
//!   `MutRead(Var(name))`, tracked via a `mut_names` set.
//! - `˚name` always lowers to bare `Var(name)`, no implicit read.
//! - `name ⚙︎ value` lowers to `MutRebind(Var(name), value)`; rejected
//!   if `name` was never mut-bound.
//! - A `Lambda` body that rebinds a mut-bound free variable it didn't
//!   explicitly `˚`-capture within that same lambda is rejected.
//!
//! §13 (amended): capture is completely uniform regardless of default
//! vs. explicit syntax — every closure captures the *same cell object*
//! for a mut-bound free variable. What differs is only what the
//! closure's *body* does with it: default (non-`˚`) capture lowers per
//! `ReadForm`'s literal rule (`MutRead` at the reference site,
//! re-reading the shared cell's current contents at call time, not
//! creation time — there is no snapshot). Explicit (`˚`) capture omits
//! the `MutRead`, handing the caller the cell itself.

use crate::ast::{self as surface, Stage};
use obfusku_core::ast::{self as core, Literal};
use obfusku_diagnostics::{Diagnostic, Severity, Span};
use std::collections::{HashMap, HashSet};

pub fn desugar(surface: &surface::Module) -> Result<core::Module, Vec<Diagnostic>> {
    desugar_with_ambient_mut_names(surface, &HashSet::new())
}

/// Same as [`desugar`], but `ambient_mut_names` are treated as already
/// mut-bound (`≔˚`) before this module's own declarations are lowered —
/// for a caller whose "module" is only a fragment of a larger, ongoing
/// session (an incremental REPL: each accepted line is its own
/// freshly-desugared `Module`, so without this, a *later* line
/// referencing a `Cell` a *previous* line declared would lower to a bare
/// `Var` instead of the `MutRead`/`MutRebind` it needs, since an ordinary
/// `desugar` call has no memory of anything outside its own `surface`
/// argument). Every other caller keeps calling plain [`desugar`],
/// unaffected.
pub fn desugar_with_ambient_mut_names(
    surface: &surface::Module,
    ambient_mut_names: &HashSet<String>,
) -> Result<core::Module, Vec<Diagnostic>> {
    let mut lowerer = Lowerer {
        fresh_counter: 0,
        mut_names: ambient_mut_names.clone(),
        record_fields: HashMap::new(),
    };
    let mut type_decls = Vec::new();
    let mut bindings: Vec<core::BindingGroup> = Vec::new();
    let mut errors = Vec::new();

    // §15.2 — `Exception`'s built-in constructors, synthesized here
    // rather than from a surface `TypeDeclaration`; must stay in sync
    // with `obfusku-typecheck::adt::AdtRegistry::build`.
    for (tag, arity) in [
        ("DivisionByZero", 0),
        ("NonExhaustiveMatch", 0),
        ("InvalidOperation", 1),
        ("Failure", 2),
        ("IntegerOverflow", 0),
    ] {
        bindings.push(core::BindingGroup::Let(synthesize_builtin_constructor(
            tag, arity,
        )));
    }

    // §8.2: a `LetRec` group is one maximal run of adjacent
    // `FunctionDeclaration`s, decided purely from declaration kind.
    let mut run: Vec<core::Binding> = Vec::new();
    let flush_run = |run: &mut Vec<core::Binding>, bindings: &mut Vec<core::BindingGroup>| {
        if !run.is_empty() {
            bindings.push(core::BindingGroup::LetRec(std::mem::take(run)));
        }
    };

    for decl in &surface.declarations {
        match decl {
            surface::Declaration::Function(fd) => {
                match lowerer.lower_function_declaration(fd) {
                    Ok(value) => run.push(core::Binding {
                        name: fd.name.clone(),
                        value,
                        exported: fd.exported,
                        span: fd.span,
                        declared_type: Some(declared_function_type(fd)),
                    }),
                    Err(mut ds) => errors.append(&mut ds),
                }
                continue;
            }
            _ => flush_run(&mut run, &mut bindings),
        }

        match decl {
            surface::Declaration::Function(_) => unreachable!("handled above"),

            // Imports resolve to nothing in Core — the module resolver
            // (outside `obfusku-syntax`, which does no file I/O) acts on
            // `surface.declarations` before `desugar` ever runs, and
            // splices the resolved module's exported bindings in as
            // ordinary pre-existing names. By the time desugar sees an
            // `Import`, it has already served its only purpose.
            surface::Declaration::Import(_) => {}

            // §9: each variant becomes one synthesized constructor
            // binding, always `Let`-bound (never a `LetRec` group).
            surface::Declaration::Type(td) => {
                for variant in &td.variants {
                    if let Some(field_names) = &variant.field_names {
                        lowerer
                            .record_fields
                            .insert(variant.tag.clone(), field_names.clone());
                    }
                }
                type_decls.push(lower_type_decl(td));
                for variant in &td.variants {
                    bindings.push(core::BindingGroup::Let(synthesize_constructor_binding(
                        variant,
                    )));
                }
            }
            surface::Declaration::Value(vd) => {
                let lowered_value = if vd.mutable {
                    lowerer
                        .lower_expr(&vd.value, None)
                        .map(|initial| core::Expr::MutCell {
                            initial: Box::new(initial),
                            span: vd.span,
                        })
                } else {
                    lowerer.lower_expr(&vd.value, None)
                };

                match lowered_value {
                    Ok(value) => {
                        // A `˚` binding's own `Expr` is wrapped in
                        // `MutCell` above, so its inferred type is
                        // `Cell<T>`; the surface annotation names the
                        // element type `T` (`x : Int ≔˚ 0`, not
                        // `x : Cell<Int> ≔˚ 0`), so it needs the same
                        // wrapping to stay a real constraint rather than
                        // a guaranteed mismatch.
                        let declared_type = vd.ty.as_ref().map(|t| {
                            let inner = lower_type_ref(t);
                            if vd.mutable {
                                obfusku_core::types::Type::Cell(Box::new(inner))
                            } else {
                                inner
                            }
                        });
                        bindings.push(core::BindingGroup::Let(core::Binding {
                            name: vd.name.clone(),
                            value,
                            exported: vd.exported,
                            span: vd.span,
                            declared_type,
                        }));
                        if vd.mutable {
                            lowerer.mut_names.insert(vd.name.clone());
                        }
                    }
                    Err(mut ds) => errors.append(&mut ds),
                }
            }
        }
    }
    flush_run(&mut run, &mut bindings);

    if errors.is_empty() {
        Ok(core::Module {
            type_decls,
            bindings,
        })
    } else {
        Err(errors)
    }
}

/// `And`/`Or` never reach here — `lower_expr` turns them into `Match`
/// before calling this, and they're omitted from the match arms
/// deliberately, not left to a wildcard.
fn lower_binop(op: surface::BinOp) -> obfusku_core::ast::BinOp {
    use obfusku_core::ast::BinOp as Core;
    match op {
        surface::BinOp::Add => Core::Add,
        surface::BinOp::Sub => Core::Sub,
        surface::BinOp::Mul => Core::Mul,
        surface::BinOp::Div => Core::Div,
        surface::BinOp::Mod => Core::Mod,
        surface::BinOp::Lt => Core::Lt,
        surface::BinOp::Gt => Core::Gt,
        surface::BinOp::Le => Core::Le,
        surface::BinOp::Ge => Core::Ge,
        surface::BinOp::Eq => Core::Eq,
        surface::BinOp::NotEq => Core::NotEq,
        surface::BinOp::Xor => Core::Xor,
        surface::BinOp::And | surface::BinOp::Or => {
            unreachable!("And/Or are lowered to Match by lower_expr, never reach here")
        }
    }
}

fn lower_unop(op: surface::UnOp) -> obfusku_core::ast::UnOp {
    match op {
        surface::UnOp::Not => obfusku_core::ast::UnOp::Not,
        surface::UnOp::Neg => obfusku_core::ast::UnOp::Neg,
    }
}

/// `λf(x: A, y: B): C → ...` declares `A → (B → C)`.
fn declared_function_type(fd: &surface::FunctionDeclaration) -> obfusku_core::types::Type {
    let mut ty = lower_type_ref(&fd.return_type);
    for p in fd.params.iter().rev() {
        ty = obfusku_core::types::Type::Function(Box::new(lower_type_ref(&p.ty)), Box::new(ty));
    }
    ty
}

/// Collects every free `TypeRef::Var` name in `tr` (deduplicated is not
/// required here — the caller inserts into a `HashMap` via
/// `entry().or_insert_with`, so repeats are harmless).
fn collect_type_var_names(tr: &surface::TypeRef, out: &mut Vec<String>) {
    match tr {
        surface::TypeRef::Base(..) | surface::TypeRef::Named(..) => {}
        surface::TypeRef::Var(name, _) => out.push(name.clone()),
        surface::TypeRef::Function(lhs, rhs, _) | surface::TypeRef::Apply(lhs, rhs, _) => {
            collect_type_var_names(lhs, out);
            collect_type_var_names(rhs, out);
        }
    }
}

/// Substitutes every `Type::Param` name found in `rename` — used to give
/// an anonymous `Lambda`'s parameter-list its own module-unique type-
/// variable names (see `Lowerer::rename_lambda_param_names`'s doc
/// comment for why this has to happen here, at the surface→Core
/// boundary, rather than in `obfusku-typecheck`).
fn rename_type_params(
    ty: obfusku_core::types::Type,
    rename: &HashMap<String, String>,
) -> obfusku_core::types::Type {
    use obfusku_core::types::Type as CoreType;
    match ty {
        CoreType::Param(name) => match rename.get(&name) {
            Some(renamed) => CoreType::Param(renamed.clone()),
            None => CoreType::Param(name),
        },
        CoreType::Function(a, b) => CoreType::Function(
            Box::new(rename_type_params(*a, rename)),
            Box::new(rename_type_params(*b, rename)),
        ),
        CoreType::Cell(t) => CoreType::Cell(Box::new(rename_type_params(*t, rename))),
        CoreType::Adt(name, args) => CoreType::Adt(
            name,
            args.into_iter()
                .map(|a| rename_type_params(a, rename))
                .collect(),
        ),
        other @ (CoreType::Int
        | CoreType::Real
        | CoreType::Str
        | CoreType::Bool
        | CoreType::Unit) => other,
    }
}

fn lower_type_ref(tr: &surface::TypeRef) -> obfusku_core::types::Type {
    use obfusku_core::types::Type as CoreType;
    use surface::BaseType;
    match tr {
        surface::TypeRef::Base(BaseType::Int, _) => CoreType::Int,
        surface::TypeRef::Base(BaseType::Real, _) => CoreType::Real,
        surface::TypeRef::Base(BaseType::Str, _) => CoreType::Str,
        surface::TypeRef::Base(BaseType::Bool, _) => CoreType::Bool,
        surface::TypeRef::Base(BaseType::Unit, _) => CoreType::Unit,
        surface::TypeRef::Var(name, _) => CoreType::Param(name.clone()),
        surface::TypeRef::Named(name, _) => CoreType::Adt(name.clone(), Vec::new()),
        surface::TypeRef::Function(lhs, rhs, _) => {
            CoreType::Function(Box::new(lower_type_ref(lhs)), Box::new(lower_type_ref(rhs)))
        }
        // `Array ▷ Int` folds onto `Adt(name, args)`: each `▷` appends
        // one type argument, so `Result ▷ Int ▷ Str` accumulates two.
        surface::TypeRef::Apply(lhs, rhs, _) => {
            let arg = lower_type_ref(rhs);
            match lower_type_ref(lhs) {
                CoreType::Adt(name, mut args) => {
                    args.push(arg);
                    CoreType::Adt(name, args)
                }
                // Not reachable from any valid program — `▷`'s left
                // side must resolve to a Tag.
                other => other,
            }
        }
    }
}

fn lower_type_decl(td: &surface::TypeDeclaration) -> core::TypeDecl {
    core::TypeDecl {
        tag: td.tag.clone(),
        type_params: td.type_params.clone(),
        variants: td
            .variants
            .iter()
            .map(|v| core::VariantDecl {
                tag: v.tag.clone(),
                fields: v.fields.iter().map(lower_type_ref).collect(),
                span: v.span,
            })
            .collect(),
        span: td.span,
    }
}

/// §9's curried-Lambda-chain construction; `$`-prefixed param names
/// can't collide with real surface identifiers.
fn synthesize_constructor_binding(variant: &surface::VariantDecl) -> core::Binding {
    let span = variant.span;
    let arity = variant.fields.len();
    let param_names: Vec<String> = (0..arity).map(|i| format!("$field{i}")).collect();
    let body = core::Expr::Constructor {
        tag: variant.tag.clone(),
        args: param_names
            .iter()
            .map(|n| core::Expr::Var(n.clone(), span))
            .collect(),
        span,
    };
    let mut value = body;
    for name in param_names.iter().rev() {
        value = core::Expr::Lambda {
            param: name.clone(),
            param_ty: None,
            body: Box::new(value),
            span,
        };
    }
    core::Binding {
        name: variant.tag.clone(),
        value,
        exported: true,
        span,
        declared_type: None,
    }
}

/// Same shape as [`synthesize_constructor_binding`], for a built-in
/// constructor with no corresponding surface `VariantDecl` to read a
/// span from — uses `Span::default()`, since these bindings don't
/// originate from any specific point in the user's source.
fn synthesize_builtin_constructor(tag: &str, arity: usize) -> core::Binding {
    let span = Span::default();
    let param_names: Vec<String> = (0..arity).map(|i| format!("$field{i}")).collect();
    let body = core::Expr::Constructor {
        tag: tag.to_string(),
        args: param_names
            .iter()
            .map(|n| core::Expr::Var(n.clone(), span))
            .collect(),
        span,
    };
    let mut value = body;
    for name in param_names.iter().rev() {
        value = core::Expr::Lambda {
            param: name.clone(),
            param_ty: None,
            body: Box::new(value),
            span,
        };
    }
    core::Binding {
        name: tag.to_string(),
        value,
        exported: true,
        span,
        declared_type: None,
    }
}

fn err(span: Span, message: impl Into<String>) -> Diagnostic {
    Diagnostic {
        severity: Severity::Error,
        message: message.into(),
        primary: span,
    }
}

struct Lowerer {
    fresh_counter: u32,
    /// Names mut-bound so far. Module-level bindings are sequential
    /// (§11), so a later binding sees earlier ones, never the reverse.
    mut_names: HashSet<String>,
    /// Declared field-name order per `RecordBody` `Tag`, for reordering
    /// `NamedConstruction`/`PNamedConstructor` fields into declared
    /// positional order (`ABSTRACT_GRAMMAR.md` §4). A record must be
    /// declared before use.
    record_fields: HashMap<String, Vec<String>>,
}

impl Lowerer {
    /// A synthetic name no surface identifier can ever produce (`$` is
    /// not a legal identifier-start character in `obfusku-syntax`'s
    /// lexer) — safe to use as a pipe binder with zero collision risk
    /// against real user names.
    fn fresh(&mut self) -> String {
        let name = format!("$pipe{}", self.fresh_counter);
        self.fresh_counter += 1;
        name
    }

    /// A module-unique type-variable name — see the `Lambda` lowering
    /// arm's comment for why an anonymous `Lambda`'s own declared type
    /// variables get renamed here rather than kept as their original
    /// surface spelling.
    fn fresh_type_var_name(&mut self) -> String {
        let name = format!("$tv{}", self.fresh_counter);
        self.fresh_counter += 1;
        name
    }

    /// Reorders a `PNamedConstructor`'s source-order `fields` into the
    /// declared order, producing an ordinary `core::Pattern::Constructor`
    /// (§4 of `ABSTRACT_GRAMMAR.md`).
    fn lower_pattern(&self, p: &surface::Pattern) -> Result<core::Pattern, Vec<Diagnostic>> {
        Ok(match p {
            surface::Pattern::Var(name, span) => core::Pattern::Var(name.clone(), *span),
            surface::Pattern::Wildcard(span) => core::Pattern::Wildcard(*span),
            surface::Pattern::Int(v, span) => core::Pattern::Lit(Literal::Int(*v), *span),
            surface::Pattern::Real(v, span) => core::Pattern::Lit(Literal::Real(*v), *span),
            surface::Pattern::Str(v, span) => core::Pattern::Lit(Literal::Str(v.clone()), *span),
            surface::Pattern::Bool(v, span) => core::Pattern::Lit(Literal::Bool(*v), *span),
            surface::Pattern::Constructor { tag, args, span } => {
                let mut core_args = Vec::with_capacity(args.len());
                for a in args {
                    core_args.push(self.lower_pattern(a)?);
                }
                core::Pattern::Constructor {
                    tag: tag.clone(),
                    args: core_args,
                    span: *span,
                }
            }
            surface::Pattern::NamedConstructor { tag, fields, span } => {
                let ordered = self.reorder_named_fields(tag, fields, *span)?;
                let mut core_args = Vec::with_capacity(ordered.len());
                for p in ordered {
                    core_args.push(self.lower_pattern(p)?);
                }
                core::Pattern::Constructor {
                    tag: tag.clone(),
                    args: core_args,
                    span: *span,
                }
            }
        })
    }

    /// Shared reordering logic for both `NamedConstruction` (expressions)
    /// and `PNamedConstructor` (patterns): looks up the declared field
    /// order for `tag`, and returns each source `(name, value)` pair's
    /// `value` in that order — erroring if `tag` isn't a known record, if
    /// a field is missing, or if an unknown field name was given.
    fn reorder_named_fields<'a, T>(
        &self,
        tag: &str,
        fields: &'a [(String, T)],
        span: Span,
    ) -> Result<Vec<&'a T>, Vec<Diagnostic>> {
        let Some(declared_order) = self.record_fields.get(tag) else {
            return Err(vec![err(
                span,
                format!("'{tag}' is not a known record type (no matching RecordBody declaration)"),
            )]);
        };
        let mut by_name: HashMap<&str, &T> = fields.iter().map(|(n, v)| (n.as_str(), v)).collect();
        let mut ordered = Vec::with_capacity(declared_order.len());
        for field_name in declared_order {
            match by_name.remove(field_name.as_str()) {
                Some(v) => ordered.push(v),
                None => {
                    return Err(vec![err(
                        span,
                        format!("missing field '{field_name}' in construction of '{tag}'"),
                    )])
                }
            }
        }
        if let Some((extra_name, _)) = by_name.into_iter().next() {
            return Err(vec![err(
                span,
                format!("'{tag}' has no field named '{extra_name}'"),
            )]);
        }
        Ok(ordered)
    }

    /// Reuses `Lambda` lowering (same explicit-capture/rebind checks,
    /// §13) — a `FunctionDeclaration` is a `Lambda` with a name and
    /// mandatory annotations, not a distinct expression shape.
    fn lower_function_declaration(
        &mut self,
        fd: &surface::FunctionDeclaration,
    ) -> Result<core::Expr, Vec<Diagnostic>> {
        let params: Vec<surface::Parameter> = fd
            .params
            .iter()
            .map(|p| surface::Parameter {
                name: p.name.clone(),
                // `FnParameter.ty` is mandatory and already flows through
                // `core::Binding.declared_type`/`infer_lambda_chain_against` —
                // deliberately not duplicated onto `param_ty` here too, or
                // the same annotation would be checked twice via two mechanisms.
                ty: None,
                span: p.span,
            })
            .collect();
        let synthetic_lambda = surface::Expression::Lambda {
            params,
            body: Box::new(fd.body.clone()),
            span: fd.span,
        };
        self.lower_expr(&synthetic_lambda, None)
    }

    /// §11 — `Let(name, value, body)`. `mut_names` is a flat set shared
    /// by the whole pass, so it's saved/restored around `body` here: a
    /// local binding's mutability must not leak past `body`, nor
    /// outlive a same-named outer mutable binding it shadows.
    fn lower_local_value(
        &mut self,
        decl: &surface::ValueDeclaration,
        body: &surface::Expression,
        pipe_ref: Option<&str>,
        span: Span,
    ) -> Result<core::Expr, Vec<Diagnostic>> {
        let lowered_value = if decl.mutable {
            self.lower_expr(&decl.value, pipe_ref)
                .map(|initial| core::Expr::MutCell {
                    initial: Box::new(initial),
                    span: decl.span,
                })
        } else {
            self.lower_expr(&decl.value, pipe_ref)
        };

        let was_mut = self.mut_names.contains(&decl.name);
        if decl.mutable {
            self.mut_names.insert(decl.name.clone());
        } else {
            self.mut_names.remove(&decl.name);
        }
        let body_result = self.lower_expr(body, pipe_ref);
        if was_mut {
            self.mut_names.insert(decl.name.clone());
        } else {
            self.mut_names.remove(&decl.name);
        }

        let mut errors = Vec::new();
        let value = match lowered_value {
            Ok(v) => Some(v),
            Err(mut ds) => {
                errors.append(&mut ds);
                None
            }
        };
        let core_body = match body_result {
            Ok(b) => Some(b),
            Err(mut ds) => {
                errors.append(&mut ds);
                None
            }
        };
        if !errors.is_empty() {
            return Err(errors);
        }
        Ok(core::Expr::Let {
            name: decl.name.clone(),
            value: Box::new(value.unwrap()),
            body: Box::new(core_body.unwrap()),
            span,
        })
    }

    /// §11 — `LetRec(bindings, body)` for a local `FunctionDeclaration+`
    /// run. Same `mut_names` save/restore as `lower_local_value`.
    fn lower_local_functions(
        &mut self,
        decls: &[surface::FunctionDeclaration],
        body: &surface::Expression,
        pipe_ref: Option<&str>,
        span: Span,
    ) -> Result<core::Expr, Vec<Diagnostic>> {
        let saved: Vec<(String, bool)> = decls
            .iter()
            .map(|fd| (fd.name.clone(), self.mut_names.remove(&fd.name)))
            .collect();

        let mut errors = Vec::new();
        let mut bindings = Vec::new();
        for fd in decls {
            match self.lower_function_declaration(fd) {
                Ok(value) => bindings.push(core::Binding {
                    name: fd.name.clone(),
                    value,
                    exported: false,
                    span: fd.span,
                    declared_type: Some(declared_function_type(fd)),
                }),
                Err(mut ds) => errors.append(&mut ds),
            }
        }
        let body_result = self.lower_expr(body, pipe_ref);

        for (name, was_present) in saved {
            if was_present {
                self.mut_names.insert(name);
            } else {
                self.mut_names.remove(&name);
            }
        }

        let core_body = match body_result {
            Ok(b) => b,
            Err(mut ds) => {
                errors.append(&mut ds);
                return Err(errors);
            }
        };
        if !errors.is_empty() {
            return Err(errors);
        }
        Ok(core::Expr::LetRec {
            bindings,
            body: Box::new(core_body),
            span,
        })
    }

    fn lower_expr(
        &mut self,
        expr: &surface::Expression,
        pipe_ref: Option<&str>,
    ) -> Result<core::Expr, Vec<Diagnostic>> {
        match expr {
            surface::Expression::Int(v, span) => Ok(core::Expr::Lit(Literal::Int(*v), *span)),
            surface::Expression::Real(v, span) => Ok(core::Expr::Lit(Literal::Real(*v), *span)),
            surface::Expression::Str(v, span) => {
                Ok(core::Expr::Lit(Literal::Str(v.clone()), *span))
            }
            surface::Expression::Bool(v, span) => Ok(core::Expr::Lit(Literal::Bool(*v), *span)),
            surface::Expression::Unit(span) => Ok(core::Expr::Lit(Literal::Unit, *span)),

            // §12/§7.7 ReadForm: an ordinary reference to a mut-bound
            // name desugars to MutRead(Var(name)) at this use site;
            // anything else is a plain Var.
            surface::Expression::Reference(name, span) => {
                let var = core::Expr::Var(name.clone(), *span);
                if self.mut_names.contains(name) {
                    Ok(core::Expr::MutRead {
                        cell: Box::new(var),
                        span: *span,
                    })
                } else {
                    Ok(var)
                }
            }

            // §7.7 ReferenceForm: always the raw binding, never a read.
            surface::Expression::ExplicitRef(name, span) => {
                Ok(core::Expr::Var(name.clone(), *span))
            }

            // §7.7 RebindForm: name "⚙︎" value, or "˚" name "⚙︎" value.
            // `explicit` doesn't change the Core shape produced (both
            // spellings lower to the same MutRebind) — it only affects
            // whether this occurrence counts toward the enclosing
            // Lambda's explicit-capture set, checked by the caller.
            surface::Expression::Rebind {
                name,
                name_span,
                value,
                explicit: _,
                span,
            } => {
                if !self.mut_names.contains(name) {
                    return Err(vec![err(
                        *name_span,
                        format!(
                            "cannot rebind '{name}': not a mutable binding (declared with '≔˚')"
                        ),
                    )]);
                }
                let cell = core::Expr::Var(name.clone(), *name_span);
                let new_value = self.lower_expr(value, pipe_ref)?;
                Ok(core::Expr::MutRebind {
                    cell: Box::new(cell),
                    new_value: Box::new(new_value),
                    span: *span,
                })
            }

            surface::Expression::PipeRef(span) => match pipe_ref {
                Some(name) => Ok(core::Expr::Var(name.to_string(), *span)),
                None => Err(vec![err(
                    *span,
                    "'◈' used outside of a pipeline stage — it has no enclosing '▷' to bind to",
                )]),
            },

            surface::Expression::Lambda { params, body, span } => {
                // §13: reject rebinding a mut-bound free variable not
                // explicitly '˚'-captured by this lambda; not descending
                // into nested Lambdas, each checked independently.
                let mut explicit_captures = HashSet::new();
                collect_explicit_captures(body, &mut explicit_captures);
                let mut rebind_targets = Vec::new();
                collect_rebind_targets(body, &mut rebind_targets);
                for (name, target_span) in &rebind_targets {
                    if self.mut_names.contains(name) && !explicit_captures.contains(name) {
                        return Err(vec![err(
                            *target_span,
                            format!(
                                "cannot rebind '{name}' here: an ordinary reference to it \
                                 desugars to a read of its current value, not the cell itself — \
                                 write '˚{name}' where this closure captures it, to get the \
                                 cell reference `⚙︎` needs to rebind through \
                                 (SEMANTIC_CORE.md §13)"
                            ),
                        )]);
                    }
                }

                // A free type-variable name shared across two `Parameter`s
                // of *this one* surface parameter list (`λ(x: t, y: t) →
                // …`) must resolve to the same declared type variable —
                // matching `FunctionDeclaration`'s existing per-signature
                // sharing (`λf(x: t, y: t): t → …` already unifies `t`
                // via one `fresh_map` built once for the whole
                // declaration). But two *separately* written `Lambda`s
                // that happen to reuse the same surface spelling (e.g. an
                // inner `λ(y: t) → …` nested in this one's body) must NOT
                // share — each is its own signature. `obfusku-typecheck`
                // has no way to tell these apart once curried into a flat
                // Core `Lambda` chain (both shapes are structurally
                // identical `Lambda(x, Lambda(y, …))` after currying), so
                // the distinction has to be made *here*, while `params`
                // is still one syntactic unit: every free type-variable
                // name in this list is renamed to a fresh, module-unique
                // name before lowering, so repeats *within* this call
                // share a name and anything outside it can never collide.
                let mut rename: HashMap<String, String> = HashMap::new();
                for p in params {
                    if let Some(ty) = &p.ty {
                        let mut free = Vec::new();
                        collect_type_var_names(ty, &mut free);
                        for name in free {
                            rename
                                .entry(name)
                                .or_insert_with(|| self.fresh_type_var_name());
                        }
                    }
                }

                let core_body = self.lower_expr(body, pipe_ref)?;
                let mut result = core_body;
                for p in params.iter().rev() {
                    result = core::Expr::Lambda {
                        param: p.name.clone(),
                        param_ty: p
                            .ty
                            .as_ref()
                            .map(|t| rename_type_params(lower_type_ref(t), &rename)),
                        body: Box::new(result),
                        span: *span,
                    };
                }
                Ok(result)
            }

            surface::Expression::Application {
                callable,
                args,
                span,
            } => {
                // §7.2/§12.1: a hole never opens a new lambda scope of
                // its own — one fresh parameter per hole, left-to-right,
                // substituted in place, the whole call then curried
                // over just those parameters (outermost = first hole).
                // A hole nested inside one of these *arguments'* own
                // sub-expressions belongs to that inner `Application`
                // instead — already handled independently, since each
                // `Application` node lowers its own `args` on its own.
                let hole_names: Vec<String> = args
                    .iter()
                    .filter(|a| matches!(a, surface::Argument::Hole(_)))
                    .map(|_| self.fresh())
                    .collect();
                let mut hole_iter = hole_names.iter();

                let mut result = self.lower_expr(callable, pipe_ref)?;
                for a in args {
                    let core_arg = match a {
                        surface::Argument::Expr(e) => self.lower_expr(e, pipe_ref)?,
                        surface::Argument::Hole(hole_span) => core::Expr::Var(
                            hole_iter.next().expect("one fresh name per hole").clone(),
                            *hole_span,
                        ),
                    };
                    result = core::Expr::Apply {
                        func: Box::new(result),
                        arg: Box::new(core_arg),
                        span: *span,
                    };
                }
                for name in hole_names.into_iter().rev() {
                    result = core::Expr::Lambda {
                        param: name,
                        param_ty: None,
                        body: Box::new(result),
                        span: *span,
                    };
                }
                Ok(result)
            }

            surface::Expression::Pipe {
                subject,
                stage,
                span,
            } => {
                let core_subject = self.lower_expr(subject, pipe_ref)?;
                match &**stage {
                    Stage::Callable(callable) => {
                        let core_stage = self.lower_expr(callable, pipe_ref)?;
                        Ok(core::Expr::Apply {
                            func: Box::new(core_stage),
                            arg: Box::new(core_subject),
                            span: *span,
                        })
                    }
                    Stage::Expr(inner) => {
                        let fresh = self.fresh();
                        let core_stage_body = self.lower_expr(inner, Some(&fresh))?;
                        Ok(core::Expr::Apply {
                            func: Box::new(core::Expr::Lambda {
                                param: fresh,
                                param_ty: None,
                                body: Box::new(core_stage_body),
                                span: *span,
                            }),
                            arg: Box::new(core_subject),
                            span: *span,
                        })
                    }
                }
            }

            surface::Expression::Match {
                scrutinee,
                arms,
                span,
            } => {
                let core_scrutinee = self.lower_expr(scrutinee, pipe_ref)?;
                let mut core_arms = Vec::new();
                let mut errors = Vec::new();
                for arm in arms {
                    let pattern = match self.lower_pattern(&arm.pattern) {
                        Ok(p) => p,
                        Err(mut ds) => {
                            errors.append(&mut ds);
                            continue;
                        }
                    };
                    match self.lower_expr(&arm.result, pipe_ref) {
                        Ok(result) => core_arms.push(core::MatchArm {
                            pattern,
                            result,
                            span: arm.span,
                        }),
                        Err(mut ds) => errors.append(&mut ds),
                    }
                }
                if !errors.is_empty() {
                    return Err(errors);
                }
                Ok(core::Expr::Match {
                    scrutinee: Box::new(core_scrutinee),
                    arms: core_arms,
                    span: *span,
                })
            }

            // §20.2/§16: `∧`/`∨` desugar to `Match`, never `BinaryOp` —
            // short-circuiting falls out of `Match`'s own evaluation
            // rule. Every other `BinOp` lowers uniformly to `BinaryOp`.
            surface::Expression::BinaryOp { op, lhs, rhs, span } => {
                let core_lhs = self.lower_expr(lhs, pipe_ref)?;
                match op {
                    surface::BinOp::And => {
                        let core_rhs = self.lower_expr(rhs, pipe_ref)?;
                        Ok(core::Expr::Match {
                            scrutinee: Box::new(core_lhs),
                            arms: vec![
                                core::MatchArm {
                                    pattern: core::Pattern::Lit(Literal::Bool(true), *span),
                                    result: core_rhs,
                                    span: *span,
                                },
                                core::MatchArm {
                                    pattern: core::Pattern::Lit(Literal::Bool(false), *span),
                                    result: core::Expr::Lit(Literal::Bool(false), *span),
                                    span: *span,
                                },
                            ],
                            span: *span,
                        })
                    }
                    surface::BinOp::Or => {
                        let core_rhs = self.lower_expr(rhs, pipe_ref)?;
                        Ok(core::Expr::Match {
                            scrutinee: Box::new(core_lhs),
                            arms: vec![
                                core::MatchArm {
                                    pattern: core::Pattern::Lit(Literal::Bool(true), *span),
                                    result: core::Expr::Lit(Literal::Bool(true), *span),
                                    span: *span,
                                },
                                core::MatchArm {
                                    pattern: core::Pattern::Lit(Literal::Bool(false), *span),
                                    result: core_rhs,
                                    span: *span,
                                },
                            ],
                            span: *span,
                        })
                    }
                    other => {
                        let core_op = lower_binop(*other);
                        let core_rhs = self.lower_expr(rhs, pipe_ref)?;
                        Ok(core::Expr::BinaryOp {
                            op: core_op,
                            lhs: Box::new(core_lhs),
                            rhs: Box::new(core_rhs),
                            span: *span,
                        })
                    }
                }
            }
            surface::Expression::UnaryOp { op, operand, span } => {
                let core_operand = self.lower_expr(operand, pipe_ref)?;
                Ok(core::Expr::UnaryOp {
                    op: lower_unop(*op),
                    operand: Box::new(core_operand),
                    span: *span,
                })
            }

            surface::Expression::Raise(value, span) => {
                let core_value = self.lower_expr(value, pipe_ref)?;
                Ok(core::Expr::Raise {
                    value: Box::new(core_value),
                    span: *span,
                })
            }

            // §7.8: `handler` must be a single-parameter `Lambda` — that
            // extracts `handler_param`/`handler_body` for `Expr::Catch`
            // and gets it the same capture/rebind checks any Lambda gets.
            surface::Expression::Catch {
                body,
                handler,
                span,
            } => {
                let core_body = self.lower_expr(body, pipe_ref)?;
                let surface::Expression::Lambda { params, .. } = &**handler else {
                    return Err(vec![err(
                        handler.span(),
                        "a 'catch' handler must be a single-parameter Lambda (e.g. 'λ(e) → ...') \
                         (CONCRETE_SYMBOLIC_GRAMMAR.md §7.8)",
                    )]);
                };
                if params.len() != 1 {
                    return Err(vec![err(
                        handler.span(),
                        format!(
                            "a 'catch' handler must take exactly one parameter, found {}",
                            params.len()
                        ),
                    )]);
                }
                let core_handler = self.lower_expr(handler, pipe_ref)?;
                let core::Expr::Lambda {
                    param: handler_param,
                    body: handler_body,
                    ..
                } = core_handler
                else {
                    unreachable!(
                        "handler was checked to be a single-parameter surface Lambda above"
                    )
                };
                Ok(core::Expr::Catch {
                    body: Box::new(core_body),
                    handler_param,
                    handler_body,
                    span: *span,
                })
            }

            // §4: reorders source-order fields into declared positional
            // order, producing an ordinary `core::Expr::Constructor`.
            surface::Expression::NamedConstruction { tag, fields, span } => {
                let ordered = self.reorder_named_fields(tag, fields, *span)?;
                let mut core_args = Vec::with_capacity(ordered.len());
                let mut errors = Vec::new();
                for value in ordered {
                    match self.lower_expr(value, pipe_ref) {
                        Ok(v) => core_args.push(v),
                        Err(mut ds) => errors.append(&mut ds),
                    }
                }
                if !errors.is_empty() {
                    return Err(errors);
                }
                Ok(core::Expr::Constructor {
                    tag: tag.clone(),
                    args: core_args,
                    span: *span,
                })
            }

            surface::Expression::LocalValue { decl, body, span } => {
                self.lower_local_value(decl, body, pipe_ref, *span)
            }

            surface::Expression::LocalFunctions { decls, body, span } => {
                self.lower_local_functions(decls, body, pipe_ref, *span)
            }

            surface::Expression::ArrayLiteral { elements, span } => {
                let mut core_elements = Vec::with_capacity(elements.len());
                let mut errors = Vec::new();
                for e in elements {
                    match self.lower_expr(e, pipe_ref) {
                        Ok(v) => core_elements.push(v),
                        Err(mut ds) => errors.append(&mut ds),
                    }
                }
                if !errors.is_empty() {
                    return Err(errors);
                }
                Ok(core::Expr::ArrayLiteral {
                    elements: core_elements,
                    span: *span,
                })
            }

            surface::Expression::Index { array, index, span } => {
                let core_array = self.lower_expr(array, pipe_ref)?;
                let core_index = self.lower_expr(index, pipe_ref)?;
                Ok(core::Expr::Index {
                    array: Box::new(core_array),
                    index: Box::new(core_index),
                    span: *span,
                })
            }
        }
    }
}

/// Collects names appearing in `ExplicitRef` ("˚name") anywhere in
/// `expr`, without descending into a nested `Lambda`'s own body.
fn collect_explicit_captures(expr: &surface::Expression, out: &mut HashSet<String>) {
    use surface::Expression::*;
    match expr {
        ExplicitRef(name, _) => {
            out.insert(name.clone());
        }
        Lambda { .. } => {} // its own captures are checked independently
        Int(..) | Real(..) | Str(..) | Bool(..) | Unit(..) | Reference(..) | PipeRef(..) => {}
        Application { callable, args, .. } => {
            collect_explicit_captures(callable, out);
            for a in args {
                if let surface::Argument::Expr(e) = a {
                    collect_explicit_captures(e, out);
                }
            }
        }
        Pipe { subject, stage, .. } => {
            collect_explicit_captures(subject, out);
            match &**stage {
                Stage::Callable(e) | Stage::Expr(e) => collect_explicit_captures(e, out),
            }
        }
        Rebind {
            name,
            value,
            explicit,
            ..
        } => {
            if *explicit {
                out.insert(name.clone());
            }
            collect_explicit_captures(value, out);
        }
        // Pattern-bound names are always ordinary immutable bindings,
        // never mut — only each arm's result expression can contain a
        // relevant explicit capture. Patterns themselves have no
        // sub-expressions to recurse into.
        Match {
            scrutinee, arms, ..
        } => {
            collect_explicit_captures(scrutinee, out);
            for arm in arms {
                collect_explicit_captures(&arm.result, out);
            }
        }
        BinaryOp { lhs, rhs, .. } => {
            collect_explicit_captures(lhs, out);
            collect_explicit_captures(rhs, out);
        }
        UnaryOp { operand, .. } => collect_explicit_captures(operand, out),
        Raise(value, _) => collect_explicit_captures(value, out),
        // `body` is an ordinary expression in this Lambda's scope;
        // `handler` is itself a `Lambda` (checked independently when
        // lowering reaches it, same as the `Lambda { .. } => {}` arm
        // above) so its own body is not descended into from here.
        Catch { body, .. } => collect_explicit_captures(body, out),
        NamedConstruction { fields, .. } => {
            for (_, v) in fields {
                collect_explicit_captures(v, out);
            }
        }
        LocalValue { decl, body, .. } => {
            collect_explicit_captures(&decl.value, out);
            collect_explicit_captures(body, out);
        }
        // Each declaration's own body is checked independently when
        // lowering reaches it (a local `FunctionDeclaration` gets its
        // own capture check, same as `Lambda { .. } => {}` above).
        LocalFunctions { body, .. } => collect_explicit_captures(body, out),
        ArrayLiteral { elements, .. } => {
            for e in elements {
                collect_explicit_captures(e, out);
            }
        }
        Index { array, index, .. } => {
            collect_explicit_captures(array, out);
            collect_explicit_captures(index, out);
        }
    }
}

/// Collects `(name, span)` for every `Rebind` target appearing in
/// `expr`, without descending into a nested `Lambda`'s own body.
fn collect_rebind_targets(expr: &surface::Expression, out: &mut Vec<(String, Span)>) {
    use surface::Expression::*;
    match expr {
        Rebind {
            name,
            name_span,
            value,
            ..
        } => {
            out.push((name.clone(), *name_span));
            collect_rebind_targets(value, out);
        }
        Lambda { .. } => {} // checked independently when lowering reaches it
        Int(..) | Real(..) | Str(..) | Bool(..) | Unit(..) | Reference(..) | PipeRef(..)
        | ExplicitRef(..) => {}
        Application { callable, args, .. } => {
            collect_rebind_targets(callable, out);
            for a in args {
                if let surface::Argument::Expr(e) = a {
                    collect_rebind_targets(e, out);
                }
            }
        }
        Pipe { subject, stage, .. } => {
            collect_rebind_targets(subject, out);
            match &**stage {
                Stage::Callable(e) | Stage::Expr(e) => collect_rebind_targets(e, out),
            }
        }
        Match {
            scrutinee, arms, ..
        } => {
            collect_rebind_targets(scrutinee, out);
            for arm in arms {
                collect_rebind_targets(&arm.result, out);
            }
        }
        BinaryOp { lhs, rhs, .. } => {
            collect_rebind_targets(lhs, out);
            collect_rebind_targets(rhs, out);
        }
        UnaryOp { operand, .. } => collect_rebind_targets(operand, out),
        Raise(value, _) => collect_rebind_targets(value, out),
        Catch { body, .. } => collect_rebind_targets(body, out),
        NamedConstruction { fields, .. } => {
            for (_, v) in fields {
                collect_rebind_targets(v, out);
            }
        }
        LocalValue { decl, body, .. } => {
            collect_rebind_targets(&decl.value, out);
            collect_rebind_targets(body, out);
        }
        LocalFunctions { body, .. } => collect_rebind_targets(body, out),
        ArrayLiteral { elements, .. } => {
            for e in elements {
                collect_rebind_targets(e, out);
            }
        }
        Index { array, index, .. } => {
            collect_rebind_targets(array, out);
            collect_rebind_targets(index, out);
        }
    }
}
