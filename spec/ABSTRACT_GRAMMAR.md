# Obfusku — Abstract Grammar

Status: **established** — structural, zero concrete glyphs. This document
answers "what constructions exist and how do they compose," not "what do
they look like." It sits between `SEMANTIC_CORE.md` (frozen) and
`CONCRETE_SYMBOLIC_GRAMMAR.md` (frozen).

**Method, stated once and enforced throughout:** every production below
carries a *Desugars to* note pointing at the Core section it reduces to.
If a surface construction can't be given such a note honestly, that is a
signal to go back and investigate the Core — not license to quietly patch
the Core to make the grammar's life easier. This document found no case
requiring that; §7 shows the work at the points where friction seemed most
likely, rather than asserting it by fiat.

---

## 1. Top-level structure

```
Program      ::= Module+
Module       ::= Declaration* Seal
Declaration  ::= ValueDeclaration
               | FunctionDeclaration
               | TypeDeclaration
               | ImportDeclaration
```

A `Module` is a named sequence of declarations (Core §20). `Import`/export
are declaration-level, not expression-level — they affect name resolution
across module boundaries only, never evaluation (Core §20, unchanged here).

**`Seal`** (added by `GLYPH_SYSTEM_DESIGN.md` §10.1) closes a `Module`'s
declaration sequence — the one place in this grammar with no other
termination signal (unlike an expression body or a delimited list, a bare
sequence of declarations has no value or list-structure of its own to mark
its end). Mandatory, not optional: its function is a completeness
assertion, and an optional one wouldn't be one.

---

## 2. Declarations

### 2.1 `ValueDeclaration`

```
ValueDeclaration ::= Mutability? Name TypeAnnotation? "=" Expression
Mutability        ::= "mut" | (absent, meaning immutable)
TypeAnnotation     ::= present only at module boundaries; inferred locally
```

Desugars to `Let(name, value, ...)` (Core §11), or, when `Mutability` is
present, to `Let(name, MutCell(value), ...)` (Core §12) with subsequent
uses of `name` desugared per Core §12's reference/read distinction.
`TypeAnnotation` is required only where Core §4 already requires
explicitness (module-boundary bindings, function signatures) — never at
local scope, matching the existing "explicit at boundaries, inferred
locally" commitment.

### 2.2 `FunctionDeclaration` — deliberately distinct from `ValueDeclaration`

```
FunctionDeclaration ::= Name Parameter+ ReturnType? "=" Expression
```

This is its own production, not a `ValueDeclaration` whose RHS happens to
be a `Lambda` — and that distinction is load-bearing, not stylistic.
**Self- and mutual recursion are reachable only through groups of adjacent
`FunctionDeclaration`s**, which desugar to a single `LetRec` (Core §11).
An ordinary `ValueDeclaration` never participates in a recursive group.
This encodes the Core's `LetRec`-is-`Lambda`-only restriction (Core §11)
directly in the grammar's structure: it is not even syntactically possible
to write the unsound "recursive non-function value" case the Core rules
out, because there is no grammatical path from `ValueDeclaration` into a
recursive binding group at all.

### 2.3 `TypeDeclaration`

```
TypeDeclaration ::= Name TypeParameter*  "=" TypeBody
TypeBody         ::= SumBody | ProductBody
SumBody          ::= Variant+
Variant          ::= Tag PositionalFields
ProductBody       ::= RecordBody | TupleBody
RecordBody        ::= NamedField+
TupleBody         ::= PositionalFields
```

Each `Variant`/product declaration introduces a constructor function of
matching arity (Core §9) — this is the same declaration form for ADT sum
variants, tuples, and records; only whether fields are named or positional
differs (§4 below resolves the previously-deferred record-construction
question using exactly this asymmetry).

---

## 3. Expressions

The sketch this document works from listed `Literal, Reference,
Application, Lambda, Binding, Constructor, Match`. Two categories are
added here that the Core requires but that list didn't name —
**mutation** and **exceptional control** are genuine semantic categories
(Core §12, §15), not sub-cases of the other seven, and a grammar that
omitted them wouldn't actually cover the frozen Core. Naming this
explicitly rather than folding them in silently.

```
Expression ::= Literal
             | Reference
             | Application
             | PipeExpression
             | Lambda
             | LocalBinding
             | Conditional
             | Constructor
             | Match
             | MutationExpression
             | ExceptionExpression
```

### 3.1 `Literal` → `Lit` (Core §1)
Int, Real, String, Bool, Unit literals. `Real` literals include any
surface representation that can produce `NaN`/`Infinity`, per Core §15.3 —
handled by the same rule as any other `Real` literal, no special case.

### 3.2 `Reference` → `Var` (Core §1)
A name lookup. At the Core level this always returns the raw bound value
(Core §12); whether a *surface* reference to a mut-bound name reads its
contents or takes a handle to the cell is resolved by which of the two
forms below is used, not by `Reference` itself.

### 3.3 `Application` → curried `Apply` chain (Core §5, §6)

```
Application ::= Callable "(" Argument* ")"
Argument     ::= Expression | Hole
```

`Hole` marks an explicit argument-hole for partial application (Core
§6/§2.2 of the design draft) — under-supplying arguments entirely is
already covered by the same rule with implied trailing holes, so no
separate production is needed for that case.

### 3.4 `PipeExpression` — its own category, though it fully desugars away

```
PipeExpression      ::= Expression "PIPE" StageExpression
StageExpression      ::= Callable                      -- bare-callable case
                        | Expression-containing PipeReference
PipeReference        ::= (the flowing value inside a StageExpression)
```

Two desugarings, exactly per Core §7: bare-callable form → ordinary
`Apply(stage, subject)`; a `StageExpression` containing `PipeReference` →
`Apply(Lambda(fresh, stage'), subject)`, with `PipeReference` bound to
`fresh` (Core §8). **Scoping is structural, not a special grammar rule**:
each `PipeExpression` node's `StageExpression` is its own binding region
for `PipeReference` — an inner `PipeExpression` nested within introduces
its own region, shadowing the outer one for any reference lexically
inside it. This is ordinary nested-scope structure, the same shape as
nested `Lambda`s; §7 below confirms this was checked, not assumed.

### 3.5 `Lambda` → curried `Lambda` chain (Core §1, §5)

```
Lambda ::= Parameter+ "=>" Expression
```

Multi-parameter surface syntax over the Core's single-parameter chain,
same relationship as multi-argument `Application` to `Apply`.

### 3.6 `LocalBinding` → `Let`/`LetRec` (Core §11)

```
LocalBinding ::= ValueDeclaration Expression
               | FunctionDeclaration+ Expression   -- adjacent, mutually recursive
```

Fully expression-oriented (design draft §3): a local binding is an
expression whose value is its body's value, not a statement. The same
`ValueDeclaration`/`FunctionDeclaration` split from §2.1/§2.2 applies
locally, for the same reason — recursion stays reachable only through
`FunctionDeclaration` groups.

### 3.7 `Conditional` → sugar over `Match` on `Bool` (Core §16, design draft §5.2)

```
Conditional ::= "if" Expression "then" Expression "else" Expression
```

No `Conditional` node exists in the Core at all — this desugars directly
to a two-arm exhaustive `Match` against `true`/`false` (Core §16's
desugaring table already states this; restated here as the grammar
production that triggers it).

### 3.8 `Constructor` → `Constructor(tag, args)` (Core §1, §9)

```
Constructor ::= PositionalConstruction | NamedConstruction
PositionalConstruction ::= Tag "(" Expression* ")"
NamedConstruction        ::= Tag "{" (FieldName ":" Expression)* "}"
```

`PositionalConstruction` covers ADT variants and tuples directly — it
already is `Constructor(tag, args)`. `NamedConstruction` (records) is new
grammar work — see §4, since the design draft explicitly left record
construction syntax open.

### 3.9 `Match` → `Match` (Core §1, §5, §10, §17)

```
Match     ::= "match" Expression MatchArm+
MatchArm ::= Pattern "->" Expression
```

Exhaustiveness (Core §17) and the closed/open type distinction are
checker-level properties computed from the `Pattern`s present against the
scrutinee's `TypeDeclaration` — nothing about this needs separate grammar
support; §7 confirms this was checked, not assumed.

### 3.10 `MutationExpression` → `MutRead`/`MutRebind`/cell-reference (Core §12, §12.1)

```
MutationExpression ::= ReadForm | RebindForm | ReferenceForm
ReadForm            ::= (an ordinary Reference to a mut-bound name)
RebindForm           ::= Name "<-" Expression
ReferenceForm        ::= "ref" Name        -- explicit handle-taking
```

`ReadForm` is not a distinct token — it's what an ordinary `Reference` to
a mut-bound name desugars to (`MutRead(Var(name))`), per Core §12.
`RebindForm` desugars to `MutRebind`. `ReferenceForm` is the explicit
mechanism required wherever Core §13's closure-capture rule or §12.1's
cell-as-value passing needs a real handle rather than a snapshot — and per
Core §13's now-explicit requirement, **a closure body using `RebindForm`
on a free variable is a static error unless that variable was captured via
`ReferenceForm`**, checked against this grammar's own structure (which
form of reference was used at the capture site is directly visible in the
parsed tree).

### 3.11 `ExceptionExpression` → `Raise`/`Catch` (Core §15)

```
ExceptionExpression ::= RaiseForm | CatchForm
RaiseForm             ::= "raise" Expression         -- Expression : Exception
CatchForm              ::= "catch" Expression "with" Lambda
```

`RaiseForm`'s argument must type-check as `Exception` (Core §15.2) —
`raise(42)` is a grammatically valid `RaiseForm` but a type error, exactly
as intended. `CatchForm`'s handler is an ordinary `Lambda`; nothing in the
grammar represents "the currently active `Catch` frame" because that's a
dynamic-extent property of evaluation (Core §15.1), not a structural one —
ordinary lexical nesting of `CatchForm` is all the grammar needs to
provide, and resolution semantics live entirely in the Core, already
frozen.

---

## 4. Patterns

```
Pattern ::= PVar | PWildcard | PLit
          | PPositionalConstructor
          | PNamedConstructor
PPositionalConstructor ::= Tag "(" Pattern* ")"
PNamedConstructor         ::= Tag "{" (FieldName ":" Pattern)* "}"
```

Directly mirrors Core §1/§10: `Pattern` is confirmed here as its own
grammar category, not an `Expression` variant, matching the resolved
"two grammars sharing a name namespace" answer from Core §10. `Cell`
(Core §12.1) has no constructor productions and therefore never appears
under `PPositionalConstructor`/`PNamedConstructor` — only `PVar`/
`PWildcard` are legal against a `Cell`-typed scrutinee, which the grammar
enforces simply by `Cell` never having a `TypeDeclaration` to draw
constructor patterns from.

**`PNamedConstructor` is new — resolving the design draft's deferred
record-construction question.** Both `NamedConstruction` (§3.8) and
`PNamedConstructor` canonicalize to the Core's positional
`Constructor`/`PConstructor` forms by reordering fields according to the
order declared in the type's `RecordBody` (§2.3) — named syntax at the
surface, positional underneath, with zero new Core mechanism. This is the
concrete answer the deferred item was waiting for: it wasn't a hard
question, just one the grammar work needed to actually reach before it had
an obvious answer.

---

## 5. Types

```
Type ::= BaseType
       | FunctionType
       | TypeApplication
       | TypeReference
BaseType         ::= Int | Real | String | Bool | Unit
FunctionType     ::= Type "->" Type
TypeApplication ::= Type Type              -- e.g. a wrapper applied to a base type
TypeReference     ::= Name                  -- a declared type, possibly generic-instantiated
```

`TypeApplication` is the surface-grammar shadow of Core §3/§4/§19's
type-level application and its invisible kind bookkeeping — the grammar
only needs ordinary application-shaped structure here; kind-checking
itself stays entirely off the surface, per Core §4's explicit requirement
that kinds never become source-level syntax. No explicit type-level
application syntax (`f<Int>`-shaped) exists at the surface, matching Core
§19's stated restriction.

---

## 6. Module boundaries

```
ImportDeclaration ::= "import" ModuleName
ExportMarker        ::= modifier attached to a Declaration, not a separate form
```

Matches Core §20 exactly: export is metadata on a binding, resolved at
name-resolution time, never an expression-level or evaluation-level
construct.

---

## 7. Friction check — where the grammar might have strained the Core, and didn't

Checked deliberately rather than assumed clean, per the standing rule that
friction gets investigated, not patched away:

- **`LetRec`'s `Lambda`-only restriction** (Core §11): resolved by making
  `FunctionDeclaration` a separate production from `ValueDeclaration`
  (§2.2) — the restriction becomes a fact about the grammar's shape, not
  an extra check bolted on afterward.
- **`◈`/`PipeReference` scoping** (Core §8): resolved by ordinary nested
  binding-region structure (§3.4) — no special pipeline-aware grammar
  machinery needed, confirming the Core's own finding that this was never
  a pipeline-specific rule to begin with.
- **The value restriction** (Core §19.1): needs no grammar support at
  all — it's a type-checking rule over which `Expression` variant a
  `ValueDeclaration`'s RHS happens to be, and the grammar already
  distinguishes `Literal`/`Lambda`/`Constructor`/`Reference` from every
  other `Expression` variant by construction.
- **Exhaustiveness's closed/open distinction** (Core §17): needs no
  grammar support either — it's computed from a scrutinee's
  `TypeDeclaration` against the `Pattern`s present in a `Match`, entirely
  at the checker level.
- **Record construction** (previously deferred): turned out to need one
  small, genuine grammar decision (§3.8, §4) rather than any Core
  revision — resolved by canonicalizing named fields to the existing
  positional `Constructor` form.

No case required weakening or reinterpreting anything in
`SEMANTIC_CORE.md`. The next document is a Concrete Symbolic Grammar —
choosing how these already-fixed structures are actually written.
