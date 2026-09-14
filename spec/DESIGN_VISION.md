# Obfusku — Design Vision

Status: **established** — see §10 for the remaining open items. Nothing
here obligates any part of the old `src/`, `LANGUAGE_SPEC.md`, or grimoires
to survive; they are historical evidence of intent, not constraints. Glyphs
have since been chosen — see `GLYPH_SYSTEM_DESIGN.md` and
`CONCRETE_SYMBOLIC_GRAMMAR.md`.

Core philosophy carried forward from the original project:

> Programming without syntactic words — an alternative programming experience
> based on symbols.

---

## 0. Design Laws

These are language-level constraints on every decision below and every
decision still to come. They are normative, not aspirational.

1. Symbolic, not cryptic.
2. New constructs should compose existing primitives whenever possible.
3. A symbol has stable semantic meaning.
4. Syntax should expose semantic distinctions rather than hide them.
5. The language should minimize memorization of arbitrary glyphs.
6. Equivalent semantic concepts should have visually related syntax.
7. Evaluation must be deterministic.
8. Type information should be statically checkable.
9. Runtime behavior must not depend on implementation accidents.
10. The language specification is authoritative; implementation is
    subordinate.
11. No feature exists solely because conventional languages have it.

The normative test for law 1: **can someone who has learned a small set of
primitive glyphs and a few combination rules read a symbol they've never seen
composed before?** If a construct fails that test, it doesn't belong.

---

## 1. Identity

Obfusku is a **statically typed, fully expression-oriented, pipeline-first**
language where **pattern matching over algebraic data types is the primary
control-flow mechanism**, and where the entire symbolic grammar reduces to
one compositional primitive (§2) applied across value, type, and declaration
registers.

---

## 2. The application primitive — and where the analogy actually holds

Value and type composition are the same operation: type constructors are
functions one level up, in a system with kinds (`Array` has kind `* → *`),
so `Int ▷ Array` **is** application, not an analogy to it — and this is
order-sensitive in both registers, the same way. Declaration modifiers do
not share this property: `x ▷ mut ▷ export` and `x ▷ export ▷ mut` denote
the same declaration, so modifiers are a commutative *attachment*, not an
order-sensitive application.

> **One application primitive, `subject ▷ transformer`, shared by the value
> and type registers** (justified by an implicit kind system underneath the
> type checker) **— plus a separate, deliberately visually-related
> annotation mechanism for declarations**, sharing the connective by design
> (Design Law 6: related concepts, related syntax) without claiming to be
> the same operation underneath.

```
x ▷ f              -- value:       f(x)               [application]
Int ▷ Array        -- type:        Array<Int>          [application, one kind up]
x ▷ mut            -- declaration: x, marked mutable   [annotation, commutative]
f ▷ export         -- declaration: f, marked exported  [annotation, commutative]
```

Chaining in the application registers composes as `x ▷ f ▷ g = g(f(x))`,
order-sensitive, identically for values and types. Chaining in the
declaration register is flag-attachment — order-independent by nature, and
the spec should say so plainly rather than imply otherwise.

Direct, multi-argument calls keep ordinary call syntax — `f(a, b, c)` — as
the base case. `▷` is specifically the *chaining/composition* form on top of
that base case, not a replacement for it (this is what keeps `a + b` from
ever needing to become `a ▷ (+ b)` — see §2.1).

### 2.1 Why this doesn't turn everything into a pipeline

`▷` is defined as pure sugar: `x ▷ f ≡ f(x)`. An ordinary complete expression
like `a + b` is already a value; there's nothing to chain, so nothing forces
it through `▷`. The connective only earns its place when you're composing
*more than one* transformer in sequence. This keeps "pipeline-oriented"
(the identity in §1) from collapsing into "everything-is-a-pipeline" (the
failure mode flagged in review) — the distinction is structural, not a style
convention.

### 2.2 Partial application vs. pipeline reference — two distinct devices, one formal rule for the first

Under-supplying arguments and explicit holes are not two mechanisms —
they're one rule: *a call with fewer arguments than the
function's arity, or with explicit `•` markers, or both, returns a function
over the missing positions, in left-to-right order of occurrence.*
`f(a)` on an arity-3 `f` is exactly `f(a, •, •)` with trailing holes
implied.

This one rule, combined with the ordinary application rule in §2, already
gives arbitrary-position piping for free — **no separate "pipeline stage"
grammar is needed.** `x ▷ f(a, •, c)` evaluates `f(a,•,c)` to a one-argument
closure, then applies `x` to it via the same `▷` = apply rule, yielding
`f(a, x, c)`. A "pipeline stage" is not its own grammatical category; it's
an ordinary partially-applied function value.

- **`•`** (placeholder, pending real glyph) — argument hole in a direct
  call, per the rule above. Never opens a new lambda scope.
- **`◈`** (placeholder) — pipeline-value reference. Unlike `•`, this *can*
  appear anywhere inside an arbitrary sub-expression (`◈ > 0`, `f(◈) + 1`)
  and implicitly wraps the whole enclosing expression in a one-argument
  lambda. **Scoping rule:** `◈` closes over the smallest enclosing
  pipeline-stage argument expression, so nested stages
  (`x ▷ filter(◈ > 0) ▷ map(◈ * 2)`) don't leak into each other.

`◈` and `•` answer genuinely different questions — "what's flowing" (an
implicit-lambda trigger) vs. "where does an argument go in this specific
call" (a hole with no new scope) — and are kept separate deliberately, not
out of caution.

---

## 3. Expressions, not statements

Obfusku is **fully expression-oriented**: every construct produces a value.
There is no separate statement category.

- A block's value is its last expression.
- `if`/`match` are expressions (§5).
- Mutation is an expression whose value is the unit value.
- There is no general loop construct: combinators over built-in
  collections plus ADT-and-`match` recursion is the design, not a
  placeholder awaiting a loop keyword (`GLYPH_SYSTEM_DESIGN.md` carries no
  loop-delimiter glyph).
- There is no `return`/early-exit form: the exceptional channel
  (`Raise`/`Catch`, `SEMANTIC_CORE.md` §15) is the only way to
  short-circuit; `GLYPH_SYSTEM_DESIGN.md` §7.2 states this has no place in
  an expression-oriented Core.

This gives Obfusku the property **everything meaningful produces a value**,
which is what makes §2's chaining mechanism apply uniformly — there's never
a "you can't pipe this, it's just a statement" exception to remember.

---

## 4. Values and types

- Static types; sigil-declared **only at module-boundary bindings and
  function signatures**. Local/internal code is inferred (resolves review
  point 1 — explicit typing lives at the boundaries where it communicates to
  a reader, not on every intermediate expression).
- Primitive types: Int, Real, String, Bool, Unit (the value produced by a
  mutation expression, §3).
- **Values are persistent/structurally immutable.**
  No operation mutates an existing object in place; an "update" produces a
  new value (`arr ▷ set(i, v)` returns a new array). The only mutable cell
  in the language is a `mut`-marked *binding*, which can be rebound to point
  at a new value. This closes the closure-capture ambiguity in §7: for
  ordinary (non-`mut`) bindings, "capture by value" and "share the GC
  reference" are unobservably identical, since the referenced object can
  never change out from under the closure.
- **Equality is structural**, defined over data types (primitives, tuples,
  records, ADTs) by the persistent-value model above. **Function values do
  not support equality** — this is an explicit carve-out from §8's closed
  operator semantics, since meaningful function equality is generally
  undecidable once partial application constructs closures dynamically.
- Collections are **homogeneous only** — `Array<T>` (composed as
  `T ▷ Array` per §2). No heterogeneous arrays.
- **No universal null.** Absence is `Optional<T>` (`T ▷ Optional`), not a
  separate untyped value.
- **Generics: small and deliberate.** Type parameters are allowed on
  user-defined ADTs and functions; no higher-kinded types, no macro-level
  metaprogramming. This means `Array<T>`, `Optional<T>`, `Result<T, E>` are
  not magic built-ins — they're ordinary library-defined generic ADTs, using
  the exact same mechanism a user gets for `Box<T>`, `Tree<T>`, `Map<K, V>`.
  This removes the asymmetry flagged in review: there is one generic-type
  mechanism, not "built-ins get generics, users don't."
- **Products: tuples and records are both first-class, and are genuinely
  distinct** (not two syntaxes over one type):
  - **Tuple** — anonymous, positional, structurally typed
    (`(Int, String)`). Lightweight, ideal as pipeline-intermediate shape.
  - **Record** — named fields, nominally typed, declared as part of a named
    type. Ideal for structured domain data.
- **Sum types (ADTs)** — user-definable tagged variants, exhaustiveness-
  checked at compile time when matched (§5). `Result<T, E>` is an ordinary
  sum type in the standard vocabulary (§6).

---

## 5. Pattern matching over ADTs

Matching is how sum types are consumed at all — not an alternative to `if`
bolted on afterward. Exhaustiveness is enforced statically: a new variant
added to a matched type turns every non-exhaustive match into a compile
error, not a silent no-op.

```
type Shape =
  | Circle(Real)
  | Rectangle(Real, Real)

area(s: Shape) -> Real =
  match s
    Circle(r)      → π * r * r
    Rectangle(w,h) → w * h
```

(Glyphs are placeholders throughout this document — `match`/`→`/`|` stand in
for symbols not yet chosen, per the deferred decision in §10.)

### 5.1 Constructors are ordinary first-class functions

Each variant constructor is an ordinary function of matching arity,
`Args → ADT`, first-class like any other function value — so `Circle`
alone is a valid function reference and `arr ▷ map(Circle)`,
`Rectangle(3, •)` both work with no new mechanism, just the rules already
defined in §2. Record construction uses its own labeled form (`RecordBody`,
`ABSTRACT_GRAMMAR.md` §4), since records are field-named rather than
positional.

### 5.2 `if` is sugar over `match` on `Bool`

Taking "match is primary" literally would force every boolean conditional
through a two-armed `match`, which is needlessly heavy for the most common
control-flow case in any program. `if c → a else → b` desugars to an
exhaustive match on `true`/`false` — one true mechanism underneath,
ergonomic surface syntax on top, the same pattern used for `▷` in §2.1.
Design law: **pattern matching is primary for data-driven control flow,
not necessarily the only conditional syntax.**

---

## 6. Errors

Two distinct mechanisms, not one undifferentiated exception system:

- **Expected failure → `Result<T, E>`.** An ordinary ADT (§4), consumed via
  pattern matching or chained via `▷` combinators. Covers "this can
  reasonably fail" (parse, lookup, divide).
- **Exceptional failure → a narrow unwind mechanism** for genuine
  programmer-error conditions (contract violations, unreachable states) —
  not used for ordinary control flow.


---

## 7. Functions and binding

- Bindings **immutable by default**; a modifier composed via `▷` (§2) marks
  mutable: `x ▷ mut`.
- Function signatures explicit at module boundaries (§4); bodies inferred.
- Fixed arity, no variadics.
- **Partial application** is a core primitive, formally defined in §2.2 —
  required because pipeline stages are partially-applied functions.
- **Closures capture immutably/by-value by default.** Capturing a binding
  *by reference, for mutation* requires an explicit modifier at the capture
  site — never implicit just because the underlying binding happens to be
  mutable. Shared mutable state via closures must be visible in the syntax
  at the point of capture, not an incidental consequence of a binding's
  declared mutability elsewhere. Under §4's persistent-value commitment,
  value-capture and reference-sharing are unobservably identical for
  ordinary bindings, so the only place aliasing is ever visible is exactly
  the `mut`-capture case this rule singles out.
- **Named functions may self-reference** (ordinary recursion) — a stated
  special case of binding, not a violation of immutable-by-default. Mutual
  recursion between functions in the same scope is resolved via `LetRec`
  binding groups (`SEMANTIC_CORE.md` §11).

---

## 8. Operators

**User-defined operator semantics are forbidden. Operators have
language-defined, closed polymorphic semantics.** `+` is
defined by the language over a fixed set of types (Int, Real, and other
types where addition has one unambiguous meaning) — not user-redefinable,
but not artificially restricted to a single monomorphic type either. This is
a meaningfully different (and stronger) principle than "no operator
overloading," which would have forced `Int` and `Real` addition to be
unrelated operations just to keep the "fixed meaning" rule technically true.

---

## 9. Evaluation, memory, modules

- **Evaluation**: eager, strict, left-to-right, deterministic. No laziness.
- **Memory**: automatic. The language has no observable finalization order,
  no user-visible ownership or lifetime construct, and the implementation
  language's own memory model must not be visible to Obfusku programs. The
  specific reclamation strategy (reference counting, tracing GC, or otherwise)
  is an implementation concern, not a language commitment.
- **Modules**: symbol-addressable namespaces; export is a modifier composed
  via `▷` at the definition site (§2), not a separate export-list
  declaration.

---

## 10. Status of items once listed here as deferred

0. **`◈` scoping rule — resolved.** Codified in `CONCRETE_SYMBOLIC_GRAMMAR.md`
   §8.2 (closes over the smallest enclosing pipeline-stage argument
   expression; nested stages do not leak into each other).
0. **Record construction syntax — resolved.** `ABSTRACT_GRAMMAR.md` §4 and
   `CONCRETE_SYMBOLIC_GRAMMAR.md` define the labeled `RecordBody`
   construction form.
0. **Mutual recursion / binding-group semantics — resolved.**
   `SEMANTIC_CORE.md` §8/§11 defines `LetRec` binding groups, reached via
   adjacent `FunctionDeclaration`s (`CONCRETE_SYMBOLIC_GRAMMAR.md` §8.2).
1. **Glyph selection — resolved.** See `GLYPH_SYSTEM_DESIGN.md` for the
   chosen vocabulary and `CONCRETE_SYMBOLIC_GRAMMAR.md` for its concrete
   grammar.
2. **Loop value semantics — resolved.** No general loop construct exists in
   the Core: combinators over built-in collections plus `match`+recursion
   over user ADTs is the design, not a placeholder. `GLYPH_SYSTEM_DESIGN.md`
   accordingly carries no loop-delimiter glyphs.
3. **Recursion vs. fold-as-idiom — open.** Standard-library iteration idiom
   is not yet written; see `LANGUAGE_SPEC.md` §5.
4. **CLI / artifact model — open.** The module *syntax* is defined
   (`ABSTRACT_GRAMMAR.md` §20, `CONCRETE_SYMBOLIC_GRAMMAR.md` §14), but the
   artifact/packaging/distribution model is not; see `LANGUAGE_SPEC.md` §5.
5. **What (if anything) survives from the old symbol table — resolved.**
   `GLYPH_SYSTEM_DESIGN.md` records which historical glyphs were kept,
   rejected, or repurposed.
