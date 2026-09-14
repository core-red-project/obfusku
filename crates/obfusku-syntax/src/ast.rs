//! Surface AST — `spec/ABSTRACT_GRAMMAR.md` and
//! `spec/CONCRETE_SYMBOLIC_GRAMMAR.md`.

use obfusku_diagnostics::Span;

#[derive(Debug, Clone, Default)]
pub struct Module {
    pub declarations: Vec<Declaration>,
    pub seal: Span,
}

#[derive(Debug, Clone)]
pub enum Declaration {
    Value(ValueDeclaration),
    /// §8.2 — the only surface route to self/mutual recursion; never a
    /// `ValueDeclaration` whose value happens to be a `Lambda`.
    Function(FunctionDeclaration),
    Type(TypeDeclaration),
    /// `ImportDeclaration ::= "⟲" ModuleName` — brings another module's
    /// exported (`⟳`) top-level bindings into scope. `module_name`
    /// resolves to a same-directory `<module_name>.obk` file, resolved
    /// by the CLI/orchestration layer (`obfusku-syntax` itself does no
    /// file I/O) — never lowered to anything itself by
    /// `obfusku-syntax::desugar`, which only records that it occurred.
    Import(ImportDeclaration),
}

impl Declaration {
    pub fn span(&self) -> Span {
        match self {
            Declaration::Value(v) => v.span,
            Declaration::Function(f) => f.span,
            Declaration::Type(t) => t.span,
            Declaration::Import(i) => i.span,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ImportDeclaration {
    pub module_name: String,
    pub name_span: Span,
    pub span: Span,
}

/// §8.2: `"λ" name "(" FnParameter,* ")" ":" Type "→" Expression`. At
/// least one parameter is mandatory; every parameter and the return
/// type carry a mandatory annotation via [`FnParameter`] (never the
/// anonymous `Lambda`'s optional-annotation [`Parameter`]).
#[derive(Debug, Clone)]
pub struct FunctionDeclaration {
    pub name: String,
    pub name_span: Span,
    pub params: Vec<FnParameter>,
    pub return_type: TypeRef,
    pub exported: bool,
    pub body: Expression,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct FnParameter {
    pub name: String,
    pub ty: TypeRef,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct TypeDeclaration {
    pub tag: String,
    pub type_params: Vec<String>,
    pub variants: Vec<VariantDecl>,
    pub span: Span,
}

/// `Variant ::= Tag ( "(" Type? ("," Type)* ")" )?`. Also doubles as the
/// single self-tagged "variant" a `RecordBody`/`TupleBody` produces
/// (same declaration form as an ADT sum variant, per `ABSTRACT_GRAMMAR.md`
/// §2.3) — a `RecordBody` additionally carries `field_names` so
/// `obfusku-syntax::desugar` can reorder a later `NamedConstruction`/
/// `PNamedConstructor`'s fields into declared positional order.
#[derive(Debug, Clone)]
pub struct VariantDecl {
    pub tag: String,
    pub fields: Vec<TypeRef>,
    /// `Some(names)` only for a `RecordBody`'s single synthesized
    /// variant; `None` for `SumBody`/`TupleBody` (purely positional).
    pub field_names: Option<Vec<String>>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum TypeRef {
    /// `BaseType ::= "⟁" | "⧆" | "⌘" | "○" | "∅"` (§9) — a distinct
    /// production from [`TypeRef::Named`], not an alias for the bare
    /// words `Int`/`Real`/`Str`/`Bool`/`Unit` (those are ordinary
    /// `Tag`s).
    Base(BaseType, Span),
    /// An uppercase name — another declared ADT.
    Named(String, Span),
    /// A lowercase name — one of the enclosing declaration's own
    /// `type_params`.
    Var(String, Span),
    /// `FunctionType ::= Type "→" Type`, right-associative.
    Function(Box<TypeRef>, Box<TypeRef>, Span),
    /// `TypeApplication ::= Type "▷" Type`, left-associative, tighter
    /// than `FunctionType`. `Array ▷ Int` = `Array<Int>`.
    Apply(Box<TypeRef>, Box<TypeRef>, Span),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BaseType {
    Int,  // ⟁
    Real, // ⧆
    Str,  // ⌘
    Bool, // ○
    Unit, // ∅
}

impl TypeRef {
    pub fn span(&self) -> Span {
        match self {
            TypeRef::Base(_, s) | TypeRef::Named(_, s) | TypeRef::Var(_, s) => *s,
            TypeRef::Function(_, _, s) | TypeRef::Apply(_, _, s) => *s,
        }
    }
}

/// §8.1: `name "≔" Mutability? Export? Expression`.
#[derive(Debug, Clone)]
pub struct ValueDeclaration {
    pub name: String,
    pub name_span: Span,
    /// `(":" Type)?` (§8.1) — module-boundary-only; always `None` on a
    /// `LocalBinding`'s `ValueDeclaration` (§7.4 has no annotation).
    pub ty: Option<TypeRef>,
    pub mutable: bool,
    pub exported: bool,
    pub value: Expression,
    pub span: Span,
}

/// `Argument ::= Expression | "•"` (§7.2) — one slot in an
/// `Application`'s argument list.
#[derive(Debug, Clone)]
pub enum Argument {
    Expr(Expression),
    Hole(Span),
}

impl Argument {
    pub fn span(&self) -> Span {
        match self {
            Argument::Expr(e) => e.span(),
            Argument::Hole(s) => *s,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Expression {
    Int(i64, Span),
    Real(f64, Span),
    Str(String, Span),
    Bool(bool, Span),
    /// `∅` in expression position — the one Unit *value*, distinct from
    /// `∅` the type (`TypeRef::Base(BaseType::Unit, _)`); the same
    /// glyph plays both roles, disambiguated purely by position, same
    /// as every other base-type glyph never needed a separate value
    /// form before this. Does not reopen zero-parameter functions/
    /// lambdas — `λ() → …` stays rejected; this only makes `Unit` a
    /// value an *ordinary* one-argument application can pass.
    Unit(Span),
    Reference(String, Span),
    /// §7.3 — anonymous `λ(params) → body`.
    Lambda {
        params: Vec<Parameter>,
        body: Box<Expression>,
        span: Span,
    },
    /// §7.2 — greedy juxtaposition: a `Callable` immediately followed
    /// by `(...)` always continues the same `Application`. Each
    /// argument is either an ordinary `Expression` or a `•` hole
    /// (§7.2/§12.1) — desugaring turns any `Application` containing a
    /// hole into a curried closure over just those positions,
    /// left-to-right; it never opens a new lambda scope of its own, so
    /// a hole inside a *nested* call's own argument list belongs to
    /// that inner `Application`, not this one.
    Application {
        callable: Box<Expression>,
        args: Vec<Argument>,
        span: Span,
    },
    /// §12 — `subject ▷ stage`.
    Pipe {
        subject: Box<Expression>,
        stage: Box<Stage>,
        span: Span,
    },
    /// `◈`, the pipeline-value reference. Legal anywhere an `Expression`
    /// is; whether it has an enclosing stage is a desugaring concern.
    PipeRef(Span),
    /// §7.7 `RebindForm` — `name ⚙︎ value`, or `˚name ⚙︎ value`
    /// (`explicit: true`) when `˚` also establishes explicit reference
    /// capture for the enclosing closure (§13).
    Rebind {
        name: String,
        name_span: Span,
        value: Box<Expression>,
        explicit: bool,
        span: Span,
    },
    /// §7.7 `ReferenceForm` — `˚name`, the raw binding with no implicit
    /// deref; closure capture is one consumer, not its only use.
    ExplicitRef(String, Span),
    Match {
        scrutinee: Box<Expression>,
        arms: Vec<MatchArm>,
        span: Span,
    },
    /// Every `BinOp` produces this uniformly at the surface level,
    /// `∧`/`∨` included — desugaring splits those off into `Match`.
    BinaryOp {
        op: BinOp,
        lhs: Box<Expression>,
        rhs: Box<Expression>,
        span: Span,
    },
    UnaryOp {
        op: UnOp,
        operand: Box<Expression>,
        span: Span,
    },
    /// §7.8 — `"☄" Expression`. The operand must type-check as
    /// `Exception`; not enforced by this grammar.
    Raise(Box<Expression>, Span),
    /// §7.8 — `"☊" Expression Lambda`. `handler` is parsed as an
    /// ordinary `Expression`; `desugar` requires it to literally be a
    /// single-parameter `Lambda`.
    Catch {
        body: Box<Expression>,
        handler: Box<Expression>,
        span: Span,
    },
    /// §7.6 record construction — `Tag "{" name ":" Expression ... "}"`.
    /// `PositionalConstruction` (tuples, sum variants) needs no
    /// dedicated node: it's ordinary `Application` with an uppercase
    /// `Callable`. Reordered into `core::Expr::Constructor` by desugar,
    /// per the matching `RecordBody`'s declared field order.
    NamedConstruction {
        tag: String,
        fields: Vec<(String, Expression)>,
        span: Span,
    },
    /// §7.4 — `LocalBinding ::= ValueDeclaration Expression`. Lowers to
    /// Core's `Expr::Let`, the expression form carrying a `body`
    /// continuation (module-level `BindingGroup::Let` has none).
    LocalValue {
        decl: Box<ValueDeclaration>,
        body: Box<Expression>,
        span: Span,
    },
    /// §7.4 — `LocalBinding ::= FunctionDeclaration+ Expression`.
    /// `decls` is one maximal run of adjacent local `FunctionDeclaration`s.
    /// Lowers to Core's `Expr::LetRec`.
    LocalFunctions {
        decls: Vec<FunctionDeclaration>,
        body: Box<Expression>,
        span: Span,
    },
    /// §7.9 — `"[" (Expression ("," Expression)*)? "]"`.
    ArrayLiteral {
        elements: Vec<Expression>,
        span: Span,
    },
    /// §7.9 — `Expression "[" Expression "]"`.
    Index {
        array: Box<Expression>,
        index: Box<Expression>,
        span: Span,
    },
}

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
    And,
    Or,
    Xor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Not,
    Neg,
}

#[derive(Debug, Clone)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub result: Expression,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum Pattern {
    Var(String, Span),
    Wildcard(Span),
    Int(i64, Span),
    Real(f64, Span),
    Str(String, Span),
    Bool(bool, Span),
    /// `PPositionalConstructor ::= Tag "(" Pattern? ("," Pattern)* ")"`.
    Constructor {
        tag: String,
        args: Vec<Pattern>,
        span: Span,
    },
    /// `PNamedConstructor ::= Tag "{" name ":" Pattern ... "}"`.
    /// Reordered into `core::Pattern::Constructor` by desugar, using
    /// the matching `RecordBody`'s declared field order — kept distinct
    /// from `Constructor` here since field order is source order until
    /// then.
    NamedConstructor {
        tag: String,
        fields: Vec<(String, Pattern)>,
        span: Span,
    },
}

impl Pattern {
    pub fn span(&self) -> Span {
        match self {
            Pattern::Var(_, s)
            | Pattern::Wildcard(s)
            | Pattern::Int(_, s)
            | Pattern::Real(_, s)
            | Pattern::Str(_, s)
            | Pattern::Bool(_, s)
            | Pattern::Constructor { span: s, .. }
            | Pattern::NamedConstructor { span: s, .. } => *s,
        }
    }
}

impl Expression {
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
            | Expression::Index { span: s, .. } => *s,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Parameter {
    pub name: String,
    /// `(":" Type)?` (§7.3) — optional, unlike `FnParameter`'s mandatory
    /// annotation; inferred if absent.
    pub ty: Option<TypeRef>,
    pub span: Span,
}

/// §12: bare-callable vs. expression-stage is a syntactic distinction —
/// never depends on whether `◈` actually occurs free inside the stage.
#[derive(Debug, Clone)]
pub enum Stage {
    Callable(Expression),
    Expr(Expression),
}

impl Stage {
    pub fn span(&self) -> Span {
        match self {
            Stage::Callable(e) | Stage::Expr(e) => e.span(),
        }
    }
}
