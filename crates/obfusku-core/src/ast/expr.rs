//! The Core expression language (`SEMANTIC_CORE.md` §1).
//!
//! Small on purpose: every surface feature — multi-argument functions,
//! partial application (§6), pipelines (§7, §8), `∧`/`∨`, local bindings —
//! lowers to the forms below, so the checker and the evaluator each have
//! one rule per form and nothing else to keep in step with the surface.

use super::literal::Literal;
use super::operator::{BinOp, UnOp};
use super::pattern::MatchArm;
use crate::types::Type;
use obfusku_diagnostics::Span;
use std::rc::Rc;

/// A Core expression. Every variant carries the [`Span`] of the source it
/// came from, so a diagnostic raised anywhere downstream still points at text.
///
/// Bodies shared with closures at runtime (`Lambda`, `Catch` handler, `Let`
/// and `LetRec` bodies, match-arm results) are `Rc<Expr>`: the evaluator
/// captures them without deep-cloning the tree.
#[derive(Debug, Clone)]
pub enum Expr {
    /// A name reference.
    Var(String, Span),
    /// A literal value.
    Lit(Literal, Span),
    /// A function of exactly one parameter. Multi-parameter surface forms
    /// curry into chains of these (§1), which is what makes partial
    /// application (§6) need no rule of its own.
    Lambda {
        /// The parameter's name.
        param: String,
        /// §7.3's optional parameter annotation, a real constraint the
        /// checker unifies against. Each free `Param` name in it is already
        /// module-unique (desugar renames per parameter list), so the
        /// checker may safely cache it for its whole lifetime.
        param_ty: Option<Type>,
        /// The function body.
        body: Rc<Expr>,
        /// The whole lambda.
        span: Span,
    },
    /// Application of a function to exactly one argument (§5).
    Apply {
        /// The function; evaluated first.
        func: Box<Expr>,
        /// The argument; evaluated second, strictly.
        arg: Box<Expr>,
        /// The whole application.
        span: Span,
    },
    /// §12 — allocates a cell holding `initial`. Not a syntactic value for
    /// §19.1's generalization rule: it is an allocation, so a binding to it
    /// is never generalized.
    MutCell {
        /// The cell's first contents.
        initial: Box<Expr>,
        /// The whole expression.
        span: Span,
    },
    /// §12 — reads the current contents of a cell.
    MutRead {
        /// An expression of type `Cell<T>`.
        cell: Box<Expr>,
        /// The whole expression.
        span: Span,
    },
    /// §12 — replaces the cell's contents; evaluates to `Unit`.
    MutRebind {
        /// An expression of type `Cell<T>`; evaluated first.
        cell: Box<Expr>,
        /// The new contents, of type `T`; evaluated second.
        new_value: Box<Expr>,
        /// The whole expression.
        span: Span,
    },
    /// §9 — a fully-applied constructor value. Reached only from inside a
    /// synthesized constructor function's body: surface construction syntax
    /// lowers through ordinary [`Expr::Apply`] instead, so partial
    /// construction is partial application and needs no special case.
    Constructor {
        /// The constructor's tag.
        tag: String,
        /// One argument per field, evaluated left to right.
        args: Vec<Expr>,
        /// The whole expression.
        span: Span,
    },
    /// §10 — selects the first arm whose pattern matches. Exhaustiveness is
    /// checked statically (§17); a fall-through at runtime raises
    /// `NonExhaustiveMatch`.
    Match {
        /// The value being inspected; evaluated once.
        scrutinee: Box<Expr>,
        /// The arms, tried in order.
        arms: Vec<MatchArm>,
        /// The whole expression.
        span: Span,
    },
    /// §15/§15.1 — raises `value`, unwinding dynamically to the nearest
    /// active [`Expr::Catch`].
    Raise {
        /// An `Exception` (§15.2).
        value: Box<Expr>,
        /// The whole expression.
        span: Span,
    },
    /// §15.1 — evaluates `body`; if it raises, binds the exception to
    /// `handler_param` and evaluates `handler_body`.
    ///
    /// The handler frame is already deactivated when `handler_body` runs, so
    /// a raise inside the handler is **not** caught by this same `Catch`.
    /// Calls inside `body` fall outside the §14.1 tail-call guarantee,
    /// because the frame must stay to catch what they raise.
    Catch {
        /// The protected expression.
        body: Box<Expr>,
        /// Name the raised exception is bound to in the handler.
        handler_param: String,
        /// Runs only if `body` raised.
        handler_body: Rc<Expr>,
        /// The whole expression.
        span: Span,
    },
    /// §20.2 — evaluates `lhs` then `rhs`, with no short-circuiting.
    BinaryOp {
        /// The operator.
        op: BinOp,
        /// Left operand; evaluated first.
        lhs: Box<Expr>,
        /// Right operand; evaluated second.
        rhs: Box<Expr>,
        /// The whole expression.
        span: Span,
    },
    /// §20.2 — a unary operator applied to one operand.
    UnaryOp {
        /// The operator.
        op: UnOp,
        /// The operand.
        operand: Box<Expr>,
        /// The whole expression.
        span: Span,
    },
    /// §11 — an ordinary, non-recursive binding. `value` is *not* in its own
    /// scope; `name` is in scope only within `body`.
    Let {
        /// The bound name.
        name: String,
        /// The bound value.
        value: Box<Expr>,
        /// Where `name` is visible.
        body: Rc<Expr>,
        /// The whole expression.
        span: Span,
    },
    /// §11 — a recursive group. Every member is in scope within every other
    /// member's value and in `body`; every member's value must be a
    /// [`Expr::Lambda`] (enforced at construction by desugaring, not
    /// re-checked here), which is what keeps recursion well-founded.
    LetRec {
        /// The group's members.
        bindings: Vec<super::binding::Binding>,
        /// Where the members are visible.
        body: Rc<Expr>,
        /// The whole expression.
        span: Span,
    },
    /// §9.2 — elements evaluate strictly, left to right.
    ArrayLiteral {
        /// The elements; all of one type.
        elements: Vec<Expr>,
        /// The whole expression.
        span: Span,
    },
    /// §9.2 — evaluates `array` before `index`; an out-of-range index raises
    /// `InvalidOperation` at runtime.
    Index {
        /// An `Array<T>`.
        array: Box<Expr>,
        /// An `Int`.
        index: Box<Expr>,
        /// The whole expression.
        span: Span,
    },
    /// §9.3 — record field read, `base.name`. Resolved against the record
    /// declaration by the checker and by tag at runtime. A record is a
    /// one-variant ADT, so this is sugar over positional access rather than
    /// a second data representation.
    Field {
        /// A record value.
        base: Box<Expr>,
        /// The field being read.
        name: String,
        /// The whole expression.
        span: Span,
    },
    /// §9.3 — persistent record update, `base.{name: value, …}`: a new value
    /// of the same record with the listed fields replaced and `base`
    /// untouched.
    Update {
        /// The record being copied.
        base: Box<Expr>,
        /// The replaced fields, in written order.
        fields: Vec<(String, Expr)>,
        /// The whole expression.
        span: Span,
    },
}

impl Expr {
    /// The source span of the whole expression.
    #[must_use]
    pub fn span(&self) -> Span {
        match self {
            Expr::Var(_, s) | Expr::Lit(_, s) => *s,
            Expr::Lambda { span: s, .. } | Expr::Apply { span: s, .. } => *s,
            Expr::MutCell { span: s, .. }
            | Expr::MutRead { span: s, .. }
            | Expr::MutRebind { span: s, .. } => *s,
            Expr::Constructor { span: s, .. } | Expr::Match { span: s, .. } => *s,
            Expr::Raise { span: s, .. } | Expr::Catch { span: s, .. } => *s,
            Expr::BinaryOp { span: s, .. } | Expr::UnaryOp { span: s, .. } => *s,
            Expr::Let { span: s, .. } | Expr::LetRec { span: s, .. } => *s,
            Expr::ArrayLiteral { span: s, .. } | Expr::Index { span: s, .. } => *s,
            Expr::Field { span: s, .. } | Expr::Update { span: s, .. } => *s,
        }
    }

    /// The direct sub-expressions, in the order they evaluate.
    ///
    /// This is the one place that knows every variant's shape, so a pass that
    /// only needs to *visit* the tree (a capability scan, a free-variable
    /// walk) is three lines instead of a 15-arm match of its own. The match
    /// is exhaustive on purpose: adding an `Expr` variant fails to compile
    /// here, and nowhere a pass could silently skip it.
    ///
    /// Binders are not reported: `Let`'s name or a `Lambda`'s parameter is
    /// not an expression. Patterns of a `Match` are not either; only the arm
    /// results are.
    #[must_use]
    pub fn children(&self) -> Vec<&Expr> {
        match self {
            Expr::Var(..) | Expr::Lit(..) => Vec::new(),
            Expr::Lambda { body, .. } => vec![body],
            Expr::Apply { func, arg, .. } => vec![func, arg],
            Expr::MutCell { initial, .. } => vec![initial],
            Expr::MutRead { cell, .. } => vec![cell],
            Expr::MutRebind {
                cell, new_value, ..
            } => vec![cell, new_value],
            Expr::Constructor { args, .. } => args.iter().collect(),
            Expr::Match {
                scrutinee, arms, ..
            } => std::iter::once(&**scrutinee)
                .chain(arms.iter().map(|a| &*a.result))
                .collect(),
            Expr::Raise { value, .. } => vec![value],
            Expr::Catch {
                body, handler_body, ..
            } => vec![body, handler_body],
            Expr::BinaryOp { lhs, rhs, .. } => vec![lhs, rhs],
            Expr::UnaryOp { operand, .. } => vec![operand],
            Expr::Let { value, body, .. } => vec![value, body],
            Expr::LetRec { bindings, body, .. } => bindings
                .iter()
                .map(|b| &b.value)
                .chain(std::iter::once(&**body))
                .collect(),
            Expr::ArrayLiteral { elements, .. } => elements.iter().collect(),
            Expr::Index { array, index, .. } => vec![array, index],
            Expr::Field { base, .. } => vec![base],
            Expr::Update { base, fields, .. } => std::iter::once(&**base)
                .chain(fields.iter().map(|(_, v)| v))
                .collect(),
        }
    }

    /// Calls `visit` on this expression and every descendant, parents before
    /// children.
    pub fn walk<'a>(&'a self, visit: &mut impl FnMut(&'a Expr)) {
        visit(self);
        for child in self.children() {
            child.walk(visit);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::binding::Binding;
    use crate::ast::pattern::Pattern;

    fn sp() -> Span {
        Span::default()
    }

    fn int(n: i64) -> Expr {
        Expr::Lit(Literal::Int(n), sp())
    }

    fn var(n: &str) -> Expr {
        Expr::Var(n.to_string(), sp())
    }

    fn lambda(param: &str, body: Expr) -> Expr {
        Expr::Lambda {
            param: param.to_string(),
            param_ty: None,
            body: Rc::new(body),
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

    fn names(e: &Expr) -> Vec<String> {
        let mut out = Vec::new();
        e.walk(&mut |x| {
            if let Expr::Var(n, _) = x {
                out.push(n.clone());
            }
        });
        out
    }

    #[test]
    fn leaves_have_no_children() {
        assert!(int(1).children().is_empty());
        assert!(var("x").children().is_empty());
    }

    #[test]
    fn apply_children_follow_evaluation_order() {
        let e = apply(var("f"), var("a"));
        assert_eq!(names(&e), ["f", "a"]);
    }

    #[test]
    fn walk_visits_parents_before_children_and_reaches_every_depth() {
        let e = apply(lambda("x", apply(var("g"), var("x"))), int(1));
        let mut kinds = Vec::new();
        e.walk(&mut |x| {
            kinds.push(match x {
                Expr::Apply { .. } => "apply",
                Expr::Lambda { .. } => "lambda",
                Expr::Var(..) => "var",
                Expr::Lit(..) => "lit",
                _ => "other",
            });
        });
        assert_eq!(kinds, ["apply", "lambda", "apply", "var", "var", "lit"]);
    }

    #[test]
    fn match_reports_scrutinee_then_each_arm_result_but_not_patterns() {
        let arm = |r: Expr| MatchArm {
            pattern: Pattern::Var("p".to_string(), sp()),
            result: Rc::new(r),
            span: sp(),
        };
        let e = Expr::Match {
            scrutinee: Box::new(var("s")),
            arms: vec![arm(var("a")), arm(var("b"))],
            span: sp(),
        };
        assert_eq!(names(&e), ["s", "a", "b"]);
    }

    #[test]
    fn letrec_reports_member_values_then_body() {
        let member = |n: &str, v: Expr| Binding {
            name: n.to_string(),
            value: v,
            is_exported: false,
            span: sp(),
            declared_type: None,
        };
        let e = Expr::LetRec {
            bindings: vec![
                member("f", lambda("x", var("g"))),
                member("g", lambda("y", var("f"))),
            ],
            body: Rc::new(var("body")),
            span: sp(),
        };
        assert_eq!(names(&e), ["g", "f", "body"]);
    }

    #[test]
    fn let_reports_value_then_body_and_catch_body_then_handler() {
        let l = Expr::Let {
            name: "n".to_string(),
            value: Box::new(var("v")),
            body: Rc::new(var("b")),
            span: sp(),
        };
        assert_eq!(names(&l), ["v", "b"]);
        let c = Expr::Catch {
            body: Box::new(var("protected")),
            handler_param: "e".to_string(),
            handler_body: Rc::new(var("handler")),
            span: sp(),
        };
        assert_eq!(names(&c), ["protected", "handler"]);
    }

    #[test]
    fn update_reports_base_then_field_values_in_written_order() {
        let e = Expr::Update {
            base: Box::new(var("r")),
            fields: vec![("a".into(), var("x")), ("b".into(), var("y"))],
            span: sp(),
        };
        assert_eq!(names(&e), ["r", "x", "y"]);
    }

    #[test]
    fn collections_and_operators_report_every_operand() {
        let arr = Expr::ArrayLiteral {
            elements: vec![var("a"), var("b")],
            span: sp(),
        };
        let idx = Expr::Index {
            array: Box::new(arr),
            index: Box::new(var("i")),
            span: sp(),
        };
        let bin = Expr::BinaryOp {
            op: BinOp::Add,
            lhs: Box::new(idx),
            rhs: Box::new(Expr::UnaryOp {
                op: UnOp::Neg,
                operand: Box::new(var("n")),
                span: sp(),
            }),
            span: sp(),
        };
        assert_eq!(names(&bin), ["a", "b", "i", "n"]);
    }

    #[test]
    fn mutation_forms_report_their_cell_and_value() {
        let e = Expr::MutRebind {
            cell: Box::new(Expr::MutRead {
                cell: Box::new(Expr::MutCell {
                    initial: Box::new(var("init")),
                    span: sp(),
                }),
                span: sp(),
            }),
            new_value: Box::new(var("next")),
            span: sp(),
        };
        assert_eq!(names(&e), ["init", "next"]);
    }

    #[test]
    fn span_reads_the_variants_own_span() {
        let s = Span::new(obfusku_diagnostics::SourceId(0), 1, 4);
        assert_eq!(Expr::Var("x".into(), s).span(), s);
        assert_eq!(
            Expr::Raise {
                value: Box::new(int(0)),
                span: s
            }
            .span(),
            s
        );
    }
}
