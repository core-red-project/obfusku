//! Expressions as written (`CONCRETE_SYMBOLIC_GRAMMAR.md` §7, §12).

use super::{BinOp, FunctionDeclaration, MatchArm, TypeRef, UnOp, ValueDeclaration};
use obfusku_diagnostics::Span;

/// An expression.
///
/// Unlike the Core's `Expr`, this keeps every sugar form. The comments on
/// each variant say what the desugarer turns it into.
#[derive(Debug, Clone)]
pub enum Expression {
    /// An integer literal.
    Int(i64, Span),
    /// A real literal.
    Real(f64, Span),
    /// A string literal, escapes already resolved.
    Str(String, Span),
    /// `◉` or `◎`.
    Bool(bool, Span),
    /// `∅` in expression position: the one `Unit` *value*.
    ///
    /// The same glyph is the `Unit` type in a type position; position alone
    /// tells them apart, as for every other base-type glyph. It does not
    /// reopen zero-parameter functions: `λ() → …` stays rejected; this only
    /// makes `Unit` a value an ordinary one-argument call can pass.
    Unit(Span),
    /// A name.
    Reference(String, Span),
    /// §7.3: anonymous `λ(params) → body`. Curried into one Core lambda per
    /// parameter.
    Lambda {
        /// The parameters; never empty.
        params: Vec<Parameter>,
        /// The body.
        body: Box<Expression>,
        /// The whole lambda.
        span: Span,
    },
    /// §7.2: `callable(args)`.
    ///
    /// Juxtaposition is greedy: a callable immediately followed by `(…)`
    /// always continues the same application. An argument may be a `•` hole;
    /// the desugarer turns an application with holes into a curried closure
    /// over just those positions, left to right. A hole never opens a lambda
    /// scope of its own: one inside a *nested* call's argument list belongs
    /// to that call.
    Application {
        /// The function.
        callable: Box<Expression>,
        /// The arguments; never empty.
        args: Vec<Argument>,
        /// The whole call.
        span: Span,
    },
    /// §12: `subject ▷ stage`.
    Pipe {
        /// The value piped in.
        subject: Box<Expression>,
        /// What receives it.
        stage: Box<Stage>,
        /// The whole pipe.
        span: Span,
    },
    /// `◈`: the piped value.
    ///
    /// Legal anywhere an expression is; whether an enclosing stage exists is
    /// the desugarer's check, not the parser's.
    PipeRef(Span),
    /// §7.7: `name ⚙︎ value`, or `˚name ⚙︎ value` (`explicit`) when the `˚`
    /// also marks explicit capture for the enclosing closure (§13).
    Rebind {
        /// The cell being overwritten.
        name: String,
        /// Just the name.
        name_span: Span,
        /// The new contents.
        value: Box<Expression>,
        /// Whether `˚` was written.
        explicit: bool,
        /// The whole rebind.
        span: Span,
    },
    /// §7.7: `˚name`, the raw cell with no implicit read.
    ExplicitRef(String, Span),
    /// §11: `⟡ scrutinee { arm ⟢ arm … }`.
    Match {
        /// The matched value.
        scrutinee: Box<Expression>,
        /// The arms; never empty.
        arms: Vec<MatchArm>,
        /// The whole match.
        span: Span,
    },
    /// A binary operator application.
    ///
    /// Every operator produces this uniformly, `∧` and `∨` included; the
    /// desugarer turns those two into `Match`.
    BinaryOp {
        /// The operator.
        op: BinOp,
        /// Left operand.
        lhs: Box<Expression>,
        /// Right operand.
        rhs: Box<Expression>,
        /// The whole expression.
        span: Span,
    },
    /// A prefix operator application.
    UnaryOp {
        /// The operator.
        op: UnOp,
        /// The operand.
        operand: Box<Expression>,
        /// The whole expression.
        span: Span,
    },
    /// §7.8: `"☄" Expression`. The operand must be an `Exception`; the
    /// checker enforces that, not the grammar.
    Raise(Box<Expression>, Span),
    /// §7.8: `"☊" Expression Lambda`.
    ///
    /// `handler` is parsed as an ordinary expression; the desugarer requires
    /// it to be a single-parameter lambda.
    Catch {
        /// The protected expression.
        body: Box<Expression>,
        /// The handler.
        handler: Box<Expression>,
        /// The whole catch.
        span: Span,
    },
    /// §7.6: record construction `Tag { name: value ⟢ … }`.
    ///
    /// Positional construction (tuples, sum variants) needs no node: it is an
    /// ordinary application with an uppercase callable. Fields are in source
    /// order and are reordered into the declared order by the desugarer.
    NamedConstruction {
        /// The record's tag.
        tag: String,
        /// `(field name, value)` in the order written.
        fields: Vec<(String, Expression)>,
        /// The whole construction.
        span: Span,
    },
    /// §7.4: `name ≔ value` followed by the expression it scopes over.
    /// Lowers to Core `Let`.
    LocalValue {
        /// The binding.
        decl: Box<ValueDeclaration>,
        /// The scope.
        body: Box<Expression>,
        /// The whole local binding.
        span: Span,
    },
    /// §7.4: a maximal run of adjacent local function declarations followed
    /// by the expression they scope over. Lowers to Core `LetRec`.
    LocalFunctions {
        /// The functions, in order; never empty.
        decls: Vec<FunctionDeclaration>,
        /// The scope.
        body: Box<Expression>,
        /// The whole local group.
        span: Span,
    },
    /// §7.9: `[a, b, …]`. Empty is legal, unlike a call or a lambda.
    ArrayLiteral {
        /// The elements.
        elements: Vec<Expression>,
        /// The whole literal.
        span: Span,
    },
    /// §7.9: `array[index]`.
    Index {
        /// The array.
        array: Box<Expression>,
        /// The position.
        index: Box<Expression>,
        /// The whole expression.
        span: Span,
    },
    /// `base.name`: a record field read.
    Field {
        /// The record.
        base: Box<Expression>,
        /// The field.
        name: String,
        /// The whole expression.
        span: Span,
    },
    /// `base.{ name: value ⟢ … }`: a persistent record update.
    Update {
        /// The record being copied.
        base: Box<Expression>,
        /// `(field name, new value)` in the order written.
        fields: Vec<(String, Expression)>,
        /// The whole update.
        span: Span,
    },
}

impl Expression {
    /// The whole expression.
    #[must_use]
    pub fn span(&self) -> Span {
        match self {
            Expression::Int(_, s)
            | Expression::Real(_, s)
            | Expression::Str(_, s)
            | Expression::Bool(_, s)
            | Expression::Unit(s)
            | Expression::Reference(_, s)
            | Expression::Lambda { span: s, .. }
            | Expression::Application { span: s, .. }
            | Expression::Pipe { span: s, .. }
            | Expression::PipeRef(s)
            | Expression::Rebind { span: s, .. }
            | Expression::ExplicitRef(_, s)
            | Expression::Match { span: s, .. }
            | Expression::BinaryOp { span: s, .. }
            | Expression::UnaryOp { span: s, .. }
            | Expression::Raise(_, s)
            | Expression::Catch { span: s, .. }
            | Expression::NamedConstruction { span: s, .. }
            | Expression::LocalValue { span: s, .. }
            | Expression::LocalFunctions { span: s, .. }
            | Expression::ArrayLiteral { span: s, .. }
            | Expression::Index { span: s, .. }
            | Expression::Field { span: s, .. }
            | Expression::Update { span: s, .. } => *s,
        }
    }

    /// Every direct sub-expression, in source order.
    ///
    /// Includes a lambda's body, a catch handler, a pipe stage's expression
    /// and every local function's body; excludes `•` holes and patterns,
    /// which are not expressions. The match is exhaustive on purpose: a new
    /// variant must say what it contains here, instead of being skipped by
    /// each analysis that walks the tree.
    ///
    /// Analyses differ only in *where they stop* (a closure's own captures, a
    /// nested pipe stage), and express that by returning early on those
    /// variants before recursing through this.
    #[must_use]
    pub fn children(&self) -> Vec<&Expression> {
        match self {
            Expression::Int(..)
            | Expression::Real(..)
            | Expression::Str(..)
            | Expression::Bool(..)
            | Expression::Unit(..)
            | Expression::Reference(..)
            | Expression::PipeRef(..)
            | Expression::ExplicitRef(..) => Vec::new(),
            Expression::Lambda { body, .. } => vec![body],
            Expression::Application { callable, args, .. } => {
                let mut out = vec![&**callable];
                out.extend(args.iter().filter_map(|a| match a {
                    Argument::Expr(e) => Some(e),
                    Argument::Hole(_) => None,
                }));
                out
            }
            Expression::Pipe { subject, stage, .. } => vec![subject, stage.expression()],
            Expression::Rebind { value, .. } => vec![value],
            Expression::Match {
                scrutinee, arms, ..
            } => {
                let mut out = vec![&**scrutinee];
                out.extend(arms.iter().map(|arm| &arm.result));
                out
            }
            Expression::BinaryOp { lhs, rhs, .. } => vec![lhs, rhs],
            Expression::UnaryOp { operand, .. } => vec![operand],
            Expression::Raise(value, _) => vec![value],
            Expression::Catch { body, handler, .. } => vec![body, handler],
            Expression::NamedConstruction { fields, .. } => fields.iter().map(|(_, e)| e).collect(),
            Expression::LocalValue { decl, body, .. } => vec![&decl.value, body],
            Expression::LocalFunctions { decls, body, .. } => {
                let mut out: Vec<&Expression> = decls.iter().map(|d| &d.body).collect();
                out.push(body);
                out
            }
            Expression::ArrayLiteral { elements, .. } => elements.iter().collect(),
            Expression::Index { array, index, .. } => vec![array, index],
            Expression::Field { base, .. } => vec![base],
            Expression::Update { base, fields, .. } => {
                let mut out = vec![&**base];
                out.extend(fields.iter().map(|(_, e)| e));
                out
            }
        }
    }
}

/// `Argument ::= Expression | "•"` (§7.2): one slot of an application.
#[derive(Debug, Clone)]
pub enum Argument {
    /// An ordinary argument.
    Expr(Expression),
    /// `•`: a position to be filled later (partial application).
    Hole(Span),
}

impl Argument {
    /// The argument's source range.
    #[must_use]
    pub fn span(&self) -> Span {
        match self {
            Argument::Expr(e) => e.span(),
            Argument::Hole(s) => *s,
        }
    }
}

/// `name (":" Type)?` (§7.3): an anonymous lambda's parameter.
///
/// The annotation is optional here, unlike
/// [`FnParameter`](super::FnParameter)'s: it is inferred when absent.
#[derive(Debug, Clone)]
pub struct Parameter {
    /// The parameter name.
    pub name: String,
    /// The optional annotation.
    pub ty: Option<TypeRef>,
    /// The whole parameter.
    pub span: Span,
}

/// What follows a `▷` (§12).
///
/// The distinction is *syntactic*, never dependent on whether `◈` actually
/// occurs inside: a parenthesized stage is always an expression stage, a
/// bare one is always a callable. That keeps the meaning of `x ▷ (f)`
/// independent of what `f` contains.
#[derive(Debug, Clone)]
pub enum Stage {
    /// `subject ▷ f`: apply `f` to the subject.
    Callable(Expression),
    /// `subject ▷ (expr)`: `expr` with `◈` bound to the subject.
    Expr(Expression),
}

impl Stage {
    /// The stage's source range.
    #[must_use]
    pub fn span(&self) -> Span {
        self.expression().span()
    }

    /// The expression inside, whichever kind of stage it is.
    #[must_use]
    pub fn expression(&self) -> &Expression {
        match self {
            Stage::Callable(e) | Stage::Expr(e) => e,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use obfusku_diagnostics::SourceId;

    fn at(n: u32) -> Span {
        Span::new(SourceId(0), n, n + 1)
    }

    fn int(n: i64) -> Expression {
        Expression::Int(n, at(n as u32))
    }

    fn texts(e: &Expression) -> Vec<i64> {
        e.children()
            .iter()
            .map(|c| match c {
                Expression::Int(v, _) => *v,
                other => panic!("expected an Int child, got {other:?}"),
            })
            .collect()
    }

    #[test]
    fn leaves_have_no_children() {
        for leaf in [int(1), Expression::Unit(at(0)), Expression::PipeRef(at(0))] {
            assert!(leaf.children().is_empty());
        }
    }

    #[test]
    fn a_binary_operation_lists_left_then_right() {
        let e = Expression::BinaryOp {
            op: BinOp::Add,
            lhs: Box::new(int(1)),
            rhs: Box::new(int(2)),
            span: at(0),
        };
        assert_eq!(texts(&e), [1, 2]);
    }

    #[test]
    fn an_application_lists_the_callee_then_expression_arguments_but_not_holes() {
        let e = Expression::Application {
            callable: Box::new(int(1)),
            args: vec![
                Argument::Expr(int(2)),
                Argument::Hole(at(9)),
                Argument::Expr(int(3)),
            ],
            span: at(0),
        };
        assert_eq!(texts(&e), [1, 2, 3]);
    }

    #[test]
    fn a_pipe_lists_the_subject_and_the_stage_expression() {
        let e = Expression::Pipe {
            subject: Box::new(int(1)),
            stage: Box::new(Stage::Expr(int(2))),
            span: at(0),
        };
        assert_eq!(texts(&e), [1, 2]);
    }

    #[test]
    fn a_catch_lists_body_then_handler() {
        let e = Expression::Catch {
            body: Box::new(int(1)),
            handler: Box::new(int(2)),
            span: at(0),
        };
        assert_eq!(texts(&e), [1, 2]);
    }

    #[test]
    fn record_forms_list_their_field_values_in_order() {
        let fields = vec![("a".to_string(), int(2)), ("b".to_string(), int(3))];
        let construction = Expression::NamedConstruction {
            tag: "P".into(),
            fields: fields.clone(),
            span: at(0),
        };
        assert_eq!(texts(&construction), [2, 3]);
        let update = Expression::Update {
            base: Box::new(int(1)),
            fields,
            span: at(0),
        };
        assert_eq!(texts(&update), [1, 2, 3]);
    }

    #[test]
    fn a_local_value_lists_its_value_then_its_scope() {
        let decl = ValueDeclaration {
            name: "x".into(),
            name_span: at(0),
            ty: None,
            mutable: false,
            exported: false,
            value: int(1),
            span: at(0),
        };
        let e = Expression::LocalValue {
            decl: Box::new(decl),
            body: Box::new(int(2)),
            span: at(0),
        };
        assert_eq!(texts(&e), [1, 2]);
    }

    #[test]
    fn a_stage_exposes_its_expression_and_span() {
        let callable = Stage::Callable(int(5));
        assert_eq!(callable.span(), at(5));
        assert!(matches!(callable.expression(), Expression::Int(5, _)));
        let expr = Stage::Expr(int(6));
        assert!(matches!(expr.expression(), Expression::Int(6, _)));
    }

    #[test]
    fn holes_and_arguments_report_their_spans() {
        assert_eq!(Argument::Hole(at(4)).span(), at(4));
        assert_eq!(Argument::Expr(int(7)).span(), at(7));
    }

    #[test]
    fn every_expression_reports_its_span() {
        assert_eq!(int(3).span(), at(3));
        assert_eq!(Expression::Reference("x".into(), at(8)).span(), at(8));
        assert_eq!(Expression::Raise(Box::new(int(1)), at(2)).span(), at(2));
    }
}
