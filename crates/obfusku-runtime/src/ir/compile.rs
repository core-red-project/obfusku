//! Core AST to [`Node`]: name resolution.

use super::node::{Arm, Node, Pat};
use super::{Globals, intern};
use crate::persistent::Text;
use crate::value::Value;
use obfusku_core::ast::{Expr, Literal, Pattern};
use std::rc::Rc;

/// The runtime value of a literal.
#[must_use]
pub fn literal_value(lit: &Literal) -> Value {
    match lit {
        Literal::Int(v) => Value::Int(*v),
        Literal::Real(v) => Value::Real(*v),
        Literal::Str(v) => Value::Str(Text::from(v.as_str())),
        Literal::Bool(v) => Value::Bool(*v),
        Literal::Unit => Value::Unit,
    }
}

/// Compiles Core expressions, resolving each name against the enclosing
/// binders first and the top-level [`Globals`] second.
pub struct Compiler<'a> {
    /// Top-level names; extended by the caller as bindings are evaluated.
    pub globals: &'a mut Globals,
    /// Enclosing binders, innermost last; one entry per runtime frame, so a
    /// name's position here is exactly its `(depth, slot)` at run time.
    scopes: Vec<Vec<String>>,
}

impl<'a> Compiler<'a> {
    /// A compiler with no local binders in scope.
    pub fn new(globals: &'a mut Globals) -> Self {
        Compiler {
            globals,
            scopes: Vec::new(),
        }
    }

    /// `(depth, slot)` of the innermost binder called `name`. Within one
    /// frame the *last* match wins, so a pattern that repeats a name binds
    /// its later occurrence.
    fn resolve(&self, name: &str) -> Option<(u32, u32)> {
        for (depth, frame) in self.scopes.iter().rev().enumerate() {
            if let Some(slot) = frame.iter().rposition(|n| n == name) {
                return Some((depth as u32, slot as u32));
            }
        }
        None
    }

    /// Compiles inside a new frame binding `names`.
    fn scoped<T>(&mut self, names: Vec<String>, f: impl FnOnce(&mut Self) -> T) -> T {
        self.scopes.push(names);
        let out = f(self);
        self.scopes.pop();
        out
    }

    /// Compiles `e`. `hint` names a `Lambda` bound directly to a name, so
    /// call traces can say which function was running.
    pub fn expr(&mut self, e: &Expr, hint: Option<&str>) -> Node {
        match e {
            Expr::Var(name, span) => match self.resolve(name) {
                Some((depth, slot)) => Node::Local { depth, slot },
                None => match self.globals.get(name) {
                    Some(cell) => Node::Global(Rc::clone(cell), *span),
                    None => Node::Unbound(Rc::from(name.as_str()), *span),
                },
            },
            Expr::Lit(lit, _) => Node::Lit(literal_value(lit)),
            Expr::Lambda { param, body, .. } => {
                // A curried declaration is a lambda chain: every link
                // carries the declared name.
                let inner = matches!(**body, Expr::Lambda { .. })
                    .then_some(hint)
                    .flatten();
                let body = self.scoped(vec![param.clone()], |c| c.expr(body, inner));
                Node::Lambda {
                    body: Rc::new(body),
                    name: Rc::from(hint.unwrap_or("λ")),
                }
            }
            Expr::Apply { func, arg, span } => Node::Apply {
                func: Box::new(self.expr(func, None)),
                arg: Box::new(self.expr(arg, None)),
                span: *span,
            },
            Expr::MutCell { initial, .. } => Node::MutCell(Box::new(self.expr(initial, None))),
            Expr::MutRead { cell, span } => Node::MutRead(Box::new(self.expr(cell, None)), *span),
            Expr::MutRebind {
                cell,
                new_value,
                span,
            } => Node::MutRebind {
                cell: Box::new(self.expr(cell, None)),
                value: Box::new(self.expr(new_value, None)),
                span: *span,
            },
            Expr::Constructor { tag, args, .. } => Node::Constructor {
                tag: intern(tag),
                args: args.iter().map(|a| self.expr(a, None)).collect(),
            },
            Expr::Match {
                scrutinee, arms, ..
            } => {
                let scrutinee = Box::new(self.expr(scrutinee, None));
                let arms = arms.iter().map(|arm| self.arm(arm)).collect();
                Node::Match { scrutinee, arms }
            }
            Expr::Raise { value, span } => Node::Raise(Box::new(self.expr(value, None)), *span),
            Expr::Catch {
                body,
                handler_param,
                handler_body,
                ..
            } => Node::Catch {
                body: Box::new(self.expr(body, None)),
                handler: Rc::new(
                    self.scoped(vec![handler_param.clone()], |c| c.expr(handler_body, None)),
                ),
            },
            Expr::BinaryOp { op, lhs, rhs, span } => Node::Binary {
                op: *op,
                lhs: Box::new(self.expr(lhs, None)),
                rhs: Box::new(self.expr(rhs, None)),
                span: *span,
            },
            Expr::UnaryOp { op, operand, span } => Node::Unary {
                op: *op,
                operand: Box::new(self.expr(operand, None)),
                span: *span,
            },
            Expr::Let {
                name, value, body, ..
            } => {
                let value = Box::new(self.expr(value, Some(name)));
                let body = self.scoped(vec![name.clone()], |c| c.expr(body, None));
                Node::Let {
                    value,
                    body: Rc::new(body),
                }
            }
            Expr::LetRec { bindings, body, .. } => {
                let names: Vec<String> = bindings.iter().map(|b| b.name.clone()).collect();
                self.scoped(names, |c| {
                    let values = bindings
                        .iter()
                        .map(|b| c.expr(&b.value, Some(&b.name)))
                        .collect();
                    let body = c.expr(body, None);
                    Node::LetRec {
                        values,
                        body: Rc::new(body),
                    }
                })
            }
            Expr::ArrayLiteral { elements, .. } => {
                Node::Array(elements.iter().map(|x| self.expr(x, None)).collect())
            }
            Expr::Index { array, index, span } => Node::Index {
                array: Box::new(self.expr(array, None)),
                index: Box::new(self.expr(index, None)),
                span: *span,
            },
            Expr::Field { base, name, span } => Node::Field {
                base: Box::new(self.expr(base, None)),
                name: name.clone(),
                span: *span,
            },
            Expr::Update { base, fields, span } => Node::Update {
                base: Box::new(self.expr(base, None)),
                fields: fields
                    .iter()
                    .map(|(n, v)| (n.clone(), self.expr(v, None)))
                    .collect(),
                span: *span,
            },
        }
    }

    /// One match arm: the pattern's binders become the arm's frame, in the
    /// order [`Pattern::binders`] lists them, which is the order the
    /// evaluator pushes matched values.
    fn arm(&mut self, arm: &obfusku_core::ast::MatchArm) -> Arm {
        let names: Vec<String> = arm
            .pattern
            .binders()
            .into_iter()
            .map(str::to_string)
            .collect();
        let vars = names.len() as u32;
        let result = if vars == 0 {
            self.expr(&arm.result, None)
        } else {
            self.scoped(names, |c| c.expr(&arm.result, None))
        };
        Arm {
            pattern: lower_pattern(&arm.pattern),
            vars,
            result: Rc::new(result),
        }
    }
}

fn lower_pattern(p: &Pattern) -> Pat {
    match p {
        Pattern::Wildcard(_) => Pat::Wild,
        Pattern::Var(..) => Pat::Var,
        Pattern::Lit(lit, _) => Pat::Lit(literal_value(lit)),
        Pattern::Constructor { tag, args, .. } => Pat::Con {
            tag: intern(tag),
            args: args.iter().map(lower_pattern).collect(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use obfusku_core::ast::{BinOp, MatchArm};
    use obfusku_diagnostics::Span;

    fn sp() -> Span {
        Span::default()
    }
    fn var(n: &str) -> Expr {
        Expr::Var(n.into(), sp())
    }
    fn int(v: i64) -> Expr {
        Expr::Lit(Literal::Int(v), sp())
    }
    fn lambda(param: &str, body: Expr) -> Expr {
        Expr::Lambda {
            param: param.into(),
            param_ty: None,
            body: Rc::new(body),
            span: sp(),
        }
    }
    fn compile(e: &Expr) -> Node {
        Compiler::new(&mut Globals::new()).expr(e, None)
    }

    #[test]
    fn a_parameter_resolves_to_depth_zero() {
        let Node::Lambda { body, .. } = compile(&lambda("x", var("x"))) else {
            panic!("expected a lambda");
        };
        assert!(matches!(*body, Node::Local { depth: 0, slot: 0 }));
    }

    #[test]
    fn an_outer_parameter_resolves_one_frame_up() {
        let Node::Lambda { body, .. } = compile(&lambda("x", lambda("y", var("x")))) else {
            panic!("expected a lambda");
        };
        let Node::Lambda { body, .. } = &*body else {
            panic!("expected an inner lambda");
        };
        assert!(matches!(**body, Node::Local { depth: 1, slot: 0 }));
    }

    #[test]
    fn a_shadowing_binder_wins() {
        let Node::Lambda { body, .. } = compile(&lambda("x", lambda("x", var("x")))) else {
            panic!("expected a lambda");
        };
        let Node::Lambda { body, .. } = &*body else {
            panic!("expected an inner lambda");
        };
        assert!(matches!(**body, Node::Local { depth: 0, slot: 0 }));
    }

    #[test]
    fn a_known_global_becomes_a_shared_cell() {
        let mut globals = Globals::new();
        let cell = crate::ir::GlobalCell::new("g", Some(Value::Int(1)));
        globals.insert("g".into(), Rc::clone(&cell));
        let node = Compiler::new(&mut globals).expr(&var("g"), None);
        assert!(matches!(node, Node::Global(c, _) if Rc::ptr_eq(&c, &cell)));
    }

    #[test]
    fn an_unknown_name_compiles_to_unbound() {
        assert!(matches!(compile(&var("nope")), Node::Unbound(n, _) if &*n == "nope"));
    }

    #[test]
    fn a_bound_lambda_carries_its_name_through_the_whole_curried_chain() {
        let e = lambda("a", lambda("b", int(0)));
        let node = Compiler::new(&mut Globals::new()).expr(&e, Some("f"));
        let Node::Lambda { body, name } = node else {
            panic!("expected a lambda");
        };
        assert_eq!(&*name, "f");
        assert!(matches!(&*body, Node::Lambda { name, .. } if &**name == "f"));
    }

    #[test]
    fn an_anonymous_lambda_is_called_lambda() {
        assert!(
            matches!(compile(&lambda("x", int(0))), Node::Lambda { name, .. } if &*name == "λ")
        );
    }

    #[test]
    fn match_arm_binders_fill_slots_left_to_right() {
        let pattern = Pattern::Constructor {
            tag: "Pair".into(),
            args: vec![
                Pattern::Var("a".into(), sp()),
                Pattern::Var("b".into(), sp()),
            ],
            span: sp(),
        };
        let e = Expr::Match {
            scrutinee: Box::new(int(0)),
            arms: vec![MatchArm {
                pattern,
                result: Rc::new(Expr::BinaryOp {
                    op: BinOp::Sub,
                    lhs: Box::new(var("a")),
                    rhs: Box::new(var("b")),
                    span: sp(),
                }),
                span: sp(),
            }],
            span: sp(),
        };
        let Node::Match { arms, .. } = compile(&e) else {
            panic!("expected a match");
        };
        assert_eq!(arms[0].vars, 2);
        let Node::Binary { lhs, rhs, .. } = &*arms[0].result else {
            panic!("expected a binary node");
        };
        assert!(matches!(**lhs, Node::Local { depth: 0, slot: 0 }));
        assert!(matches!(**rhs, Node::Local { depth: 0, slot: 1 }));
    }

    #[test]
    fn an_arm_without_binders_pushes_no_frame() {
        let e = Expr::Match {
            scrutinee: Box::new(int(0)),
            arms: vec![MatchArm {
                pattern: Pattern::Wildcard(sp()),
                result: Rc::new(int(1)),
                span: sp(),
            }],
            span: sp(),
        };
        let Node::Match { arms, .. } = compile(&e) else {
            panic!("expected a match");
        };
        assert_eq!(arms[0].vars, 0);
    }

    #[test]
    fn literals_convert_to_their_values() {
        assert!(matches!(literal_value(&Literal::Int(3)), Value::Int(3)));
        assert!(matches!(
            literal_value(&Literal::Bool(true)),
            Value::Bool(true)
        ));
        assert!(matches!(literal_value(&Literal::Unit), Value::Unit));
        assert!(matches!(&literal_value(&Literal::Str("a".into())), Value::Str(t) if t == "a"));
    }
}
