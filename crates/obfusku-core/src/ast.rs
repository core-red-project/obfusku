//! Core expression and pattern grammar — `spec/SEMANTIC_CORE.md` §1.

use obfusku_diagnostics::Span;
use std::rc::Rc;

#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    Int(i64),
    Real(f64),
    Str(String),
    Bool(bool),
    Unit,
}

/// `∧`/`∨` are absent: they desugar to `Match` at the `obfusku-syntax`
/// boundary, so short-circuiting falls out of `Match`'s evaluation rule
/// instead of a bespoke conditional-evaluation path here (§16, §20.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Lt,
    Gt,
    Le,
    Ge,
    Eq,
    NotEq,
    Xor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Not,
    Neg,
}

/// `Lambda`/`Apply` are always single-parameter/single-argument (§1);
/// multi-parameter/argument surface forms curry into chains of these
/// during lowering.
#[derive(Debug, Clone)]
pub enum Expr {
    Var(String, Span),
    Lit(Literal, Span),
    Lambda {
        param: String,
        /// §7.3's optional `Parameter` annotation — a real constraint
        /// on this parameter. Each free `Param` name is already
        /// module-unique (desugar's per-parameter-list renaming), so
        /// `lambda_param_vars` safely caches it for the checker's lifetime.
        param_ty: Option<crate::types::Type>,
        body: Rc<Expr>,
        span: Span,
    },
    Apply {
        func: Box<Expr>,
        arg: Box<Expr>,
        span: Span,
    },
    /// §12 — allocates a cell holding `initial`. Not a syntactic value
    /// for §19.1's generalization rule (`Apply`-shaped, an allocation).
    MutCell {
        initial: Box<Expr>,
        span: Span,
    },
    MutRead {
        cell: Box<Expr>,
        span: Span,
    },
    /// §12 — replaces the cell's contents; evaluates to `Unit`.
    MutRebind {
        cell: Box<Expr>,
        new_value: Box<Expr>,
        span: Span,
    },
    /// §9 — a fully-applied constructor value, reached only from inside
    /// a synthesized constructor function's body. Surface construction
    /// syntax lowers through ordinary `Apply` instead.
    Constructor {
        tag: String,
        args: Vec<Expr>,
        span: Span,
    },
    Match {
        scrutinee: Box<Expr>,
        arms: Vec<MatchArm>,
        span: Span,
    },
    /// §15/§15.1 — raises `value`, unwinding dynamically to the nearest
    /// active `Catch`.
    Raise {
        value: Box<Expr>,
        span: Span,
    },
    /// §15.1 — evaluates `body`; on raise, the handler frame is already
    /// deactivated before `handler_body` runs, so a raise inside it is
    /// not caught by this same `Catch`. Calls within `body` are outside
    /// the §14.1 TCO guarantee.
    Catch {
        body: Box<Expr>,
        handler_param: String,
        handler_body: Rc<Expr>,
        span: Span,
    },
    /// §20.2 — evaluates `lhs` then `rhs`, no short-circuiting.
    BinaryOp {
        op: BinOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
        span: Span,
    },
    UnaryOp {
        op: UnOp,
        operand: Box<Expr>,
        span: Span,
    },
    /// §11 — ordinary non-recursive binding; `value` is not in its own
    /// scope, `name` is in scope only within `body`.
    Let {
        name: String,
        value: Box<Expr>,
        body: Rc<Expr>,
        span: Span,
    },
    /// §11 — every member is in scope within every other member's value
    /// and in `body`. Every member's value must be a `Lambda`.
    LetRec {
        bindings: Vec<Binding>,
        body: Rc<Expr>,
        span: Span,
    },
    /// §9.2 — elements evaluate strictly, left to right.
    ArrayLiteral {
        elements: Vec<Expr>,
        span: Span,
    },
    /// §9.2 — evaluates `array` before `index`; out-of-bounds raises
    /// `Exception::InvalidOperation` at runtime.
    Index {
        array: Box<Expr>,
        index: Box<Expr>,
        span: Span,
    },
    /// Record field read — `base.name`. Resolved against the record
    /// declaration (`VariantDecl::field_names`) by the type checker, and
    /// by tag at runtime; a record is a one-variant ADT, so this is
    /// sugar over positional access, not a second data representation.
    Field {
        base: Box<Expr>,
        name: String,
        span: Span,
    },
    /// Persistent record update — `base.{name: value, …}` yields a new
    /// value of the same record with the listed fields replaced.
    Update {
        base: Box<Expr>,
        fields: Vec<(String, Expr)>,
        span: Span,
    },
}

impl Expr {
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
}

#[derive(Debug, Clone)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub result: Rc<Expr>,
    pub span: Span,
}

/// Separate from [`Expr`] — patterns and expressions are two grammars
/// sharing a constructor-tag namespace, not one grammar with patterns
/// embedded as an expression subtype (§10).
#[derive(Debug, Clone)]
pub enum Pattern {
    Var(String, Span),
    Wildcard(Span),
    Lit(Literal, Span),
    Constructor {
        tag: String,
        args: Vec<Pattern>,
        span: Span,
    },
}

impl Pattern {
    #[must_use]
    pub fn span(&self) -> Span {
        match self {
            Pattern::Var(_, s) | Pattern::Wildcard(s) | Pattern::Lit(_, s) => *s,
            Pattern::Constructor { span: s, .. } => *s,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Binding {
    pub name: String,
    pub value: Expr,
    pub exported: bool,
    pub span: Span,
    /// The surface type annotation, if any (`FunctionDeclaration`'s
    /// mandatory annotation is the only current source) — a constraint
    /// for `obfusku-typecheck` to unify against; `obfusku-runtime` never
    /// reads it.
    pub declared_type: Option<crate::types::Type>,
}

#[derive(Debug, Clone)]
pub struct TypeDecl {
    pub tag: String,
    pub type_params: Vec<String>,
    pub variants: Vec<VariantDecl>,
    pub span: Span,
}

/// Field types may reference [`crate::types::Type::Param`] for one of
/// the enclosing [`TypeDecl`]'s `type_params`, substituted at each use.
#[derive(Debug, Clone)]
pub struct VariantDecl {
    pub tag: String,
    pub fields: Vec<crate::types::Type>,
    /// `Some(names)` — one per field, in declared order — only for a
    /// record's single variant; `None` for sum variants and tuples.
    pub field_names: Option<Vec<String>>,
    pub span: Span,
}

/// Group membership is decided once in `obfusku-syntax::desugar` from
/// declaration form alone (`ValueDeclaration` vs. adjacent
/// `FunctionDeclaration` runs) — never reconstructed from a binding's
/// value shape or a dependency/SCC analysis.
#[derive(Debug, Clone)]
pub enum BindingGroup {
    /// Ordinary, non-recursive — not in its own scope.
    Let(Binding),
    /// Every member is in scope within every other's value (each must
    /// be a `Lambda`, enforced by construction, not re-checked here). A
    /// single-element group is direct self-recursion — it does not
    /// collapse to `Let`, whose value is never in its own scope.
    LetRec(Vec<Binding>),
}

impl BindingGroup {
    #[must_use]
    pub fn bindings(&self) -> &[Binding] {
        match self {
            BindingGroup::Let(b) => std::slice::from_ref(b),
            BindingGroup::LetRec(bs) => bs,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Module {
    pub type_decls: Vec<TypeDecl>,
    pub bindings: Vec<BindingGroup>,
}
