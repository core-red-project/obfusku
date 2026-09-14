# Obfusku — Semantic Core

Status: **formal semantics, zero concrete glyphs — frozen.** If any sentence in this document requires a symbol
to be understood, it has failed its own purpose. All syntax below is
abstract (named forms: `Apply`, `Lambda`, `Match`, …), not a proposal for
how anything is written.

This revision incorporates every finding from `SEMANTIC_CORE_STRESS_TEST.md`:
one real contradiction (division's error channel) and every load-bearing
missing decision are resolved below; nothing new is introduced beyond what
that pass already surfaced. Two genuinely open forks from that pass
(`Real`/`MutCell` equality) are resolved here with stated reasoning rather
than left open, since a frozen Core can't carry unresolved forks — where a
fork was a real either-way judgment call, the choice made and its
consequence are stated plainly rather than hidden as if it were forced.

## 0. Method: a small core, a larger surface

Obfusku has two layers:

- **Surface syntax** — what a person writes. Includes convenience forms:
  `if`, under-application, argument holes, the pipe connective, implicit
  pipeline-value reference. Every one of these is *sugar*.
- **Core calculus** — the minimal set of forms the semantics is actually
  defined over. Everything in Surface Syntax has a stated desugaring into
  Core. If a surface form can't be desugared, it isn't well-defined yet, and
  doesn't belong in the language.

This document defines the Core, and states the desugaring rules for every
piece of surface sugar decided so far. Glyph selection, later, is choosing
notation for Surface Syntax only — the Core doesn't change when glyphs are
picked, which is the whole point of doing it in this order.

---

## 1. Abstract syntax of the Core

```
Expr :=
  | Var(name)
  | Lit(value)                              -- Int, Real, String, Bool, Unit literals
  | Lambda(param, body: Expr)                -- single-parameter, always
  | Apply(fn: Expr, arg: Expr)               -- single-argument, always
  | Let(name, value: Expr, body: Expr)
  | LetRec(bindings: [(name, Expr)], body: Expr)
  | Constructor(tag, args: [Expr])           -- ADT / tuple / record construction
  | Match(scrutinee: Expr, arms: [(Pattern, Expr)])
  | Raise(value: Expr)
  | Catch(body: Expr, handler: Expr)
  | MutCell(initial: Expr)                   -- allocates a rebindable cell
  | MutRebind(cell: Expr, newValue: Expr)    -- rebinds a cell, yields Unit
  | MutRead(cell: Expr)

Pattern :=
  | PVar(name)
  | PWildcard
  | PLit(value)
  | PConstructor(tag, subpatterns: [Pattern])
```

That's the whole core expression language. Every multi-argument function is
represented as nested single-argument `Lambda`s (a curried chain); every
multi-argument call is nested `Apply`. This single choice is what makes §6
(partial application) fall out for free instead of needing its own rule.

---

## 2. Value model

Values are the result of evaluating closed Core expressions: literals,
closures (`Lambda` + captured environment, §13), and fully-applied
constructor values (`Constructor(tag, [v1, ..., vn])`), plus mutable cells
as a distinct value kind (§12).

**Values are persistent.** No Core form mutates a `Constructor` value's
contents in place. The only mutable entity in the value model is the cell
introduced by `MutCell` — everything else, once constructed, is immutable
for its lifetime. "Updating" a constructed value (e.g. an array-like
structure) means producing a new `Constructor` value; nothing in the Core
supports in-place field mutation of a constructor value.

---

## 3. Type model

Types classify values: base types (`Int`, `Real`, `String`, `Bool`, `Unit`),
function types (`param → result`, corresponding to `Lambda`/`Apply`), and
data types declared via constructors (ADTs, records, tuples — all are
"data types with one or more constructors," products being the one-
constructor case).

Type parameters on a declared data type (`Result<T, E>`-shaped things) are
ordinary parametric polymorphism: a declaration introduces a type-level
name that gets instantiated per use-site by unification, the same way a
`Lambda`'s parameter gets instantiated by the argument at each `Apply`.
Nothing here requires the user to write type-level lambdas or type-level
application explicitly — instantiation is inferred, not spelled.

---

## 4. Kind model — and why it stays invisible

Type constructors with parameters (`Array`, `Optional`, `Result`, a
user's `Tree`) have an implicit **arity**, tracked internally by the type
checker so that `Array` applied to one type argument, or `Result` applied to
two, can be validated the same way function arity is validated for `Apply`.
This bookkeeping is what earlier drafts called "kinds."

**Kinds are a type-checker concept, not a Core form and not a surface
construct.** There is no `KindOf`, no kind signature a user writes, no
abstraction over kinds, and no higher-kinded parametrization (a function
generic over "any one-argument type constructor" is out of scope, per the
existing "small, deliberate generics" commitment). The checker infers a
constructor's arity from its declaration and rejects mismatched application;
none of that machinery is ever surfaced. This is a firm boundary, not a
temporary simplification — the moment kind syntax appears at the surface,
the language has quietly become a different, heavier one than intended.

---

## 5. Function application

`Apply(fn, arg)` is the single application form. A "multi-argument" call in
Surface Syntax is nested `Apply`s over a curried `Lambda` chain:

```
f(a, b, c)   desugars to   Apply(Apply(Apply(f, a), b), c)
```

---

## 6. Partial application — not a separate Core mechanism

Because every function is a curried chain of unary `Lambda`s (§1, §5),
"partial application" requires **no additional Core rule**. Stopping partway
through the chain — `Apply(f, a)` where `f` expects three arguments — simply
*is* a function value (the remaining `Lambda(b, Lambda(c, ...))`), by the
ordinary meaning of `Apply` on a `Lambda`. Under-application, explicit
argument holes, and "currying" are three different Surface Syntax spellings
of the same thing the Core already does for free. This is a genuine
simplification found during the semantic stress test, not an assumption
carried in from v0.2.

---

## 7. Pipeline application — pure desugaring, two cases

The Surface pipe connective has exactly two desugarings, disambiguated by
the syntactic shape of its right operand:

- **Bare callable** (an identifier or already-applied/held function value,
  containing no pipeline-value reference): `subject PIPE E` desugars to
  `Apply(E, subject)`.
- **Expression containing a pipeline-value reference** (§8): `subject PIPE E`
  desugars to `Apply(Lambda(fresh, E[reference := Var(fresh)]), subject)`.

There is no `Pipe` node in the Core. It is entirely a notational
convenience over `Apply` and, in the second case, `Lambda`.

---

## 8. Pipeline-value reference — formalized as lambda-binder introduction

Earlier drafts described this informally as scoping to "the smallest
enclosing pipeline-stage argument expression," which is intuitive but not
precise enough to answer nesting questions. The precise rule, stated in
terms of §7's desugaring:

> Each occurrence of the Surface pipe connective with an expression-shaped
> right operand introduces its own fresh binder over exactly that operand.
> A pipeline-value reference resolves to the **nearest lexically enclosing**
> such binder — ordinary lexical shadowing, nothing pipeline-specific.

Concretely, for nested pipelines, the desugaring in §7 is applied
recursively, innermost first: an inner pipe's right operand gets its own
fresh `Lambda` binder before the outer pipe's desugaring ever looks at it,
so a reference lexically inside an inner stage binds to the inner stage's
binder, never the outer one — exactly the behavior "ordinary variable
shadowing in nested lambdas" already gives, with zero special-casing. This
resolves every nesting case tested during review: the reference always
binds to the innermost stage boundary that contains it, full stop, because
that's what shadowing means for any bound variable.

One consequence worth stating plainly: if an outer stage's right operand
contains no reference to its own binder anywhere (because the only
reference present belongs to a nested inner stage), the outer `Lambda`
simply ignores its argument — legal, but a candidate for a compiler lint
("piped value unused in this stage"), not a Core-level error.

---

## 9. ADT construction

A data declaration with constructors introduces, for each constructor, a
Core-level curried `Lambda` chain of the constructor's declared arity,
whose body is `Constructor(tag, [param1, ..., paramN])`. This makes
constructors ordinary values from the moment they're declared — nothing
distinguishes `Circle` from any other function value in expression
position. `Circle(5)` is `Apply(Circle, Lit(5))`, no different in kind from
any other application.

**Consequence, not previously stated explicitly:** because a constructor
is exactly an ordinary value binding, constructor tags occupy the same
flat value namespace as every other binding in a module — this isn't a
new rule, it follows directly from the paragraph above, the same way two
ordinary bindings can't share a name. Two different ADTs declaring a
variant with the same tag collide exactly as two `≔` bindings of the same
name would. Worth stating once so it isn't rediscovered as a surprise —
e.g. the standard library's `Result<T, E>` (§15.2 already names
`Exception`'s payload variant `Failure`) needs an error-constructor name
that doesn't collide with it, not because of anything Result-specific,
but because this was always true.

---

## 9.1 `Array<T>` versus `List<T>` — not the same abstraction, resolved

Whether built-in sequences are structurally pattern-matchable turned out
to depend on which abstraction is meant, and conflating them was the risk.
Open head/rest destructuring (a `head, ...rest`-shaped pattern) against an
`Array` — a structure whose purpose is O(1) random access — either forces
materializing "the rest" as a new array on every destructure (an O(n)
operation disguised as a pattern match, producing O(n²) blowup under naive
recursive traversal) or requires a hidden, implementation-specific
representation to make it cheap. Both outcomes make runtime cost depend on
implementation accidents, which this design already rules out elsewhere.

**Resolved:**

- **`Array<T>`** supports only patterns whose cost is transparent at the
  type/shape level — length checks, fixed-arity destructuring, index
  access — never open head/rest splitting. It is consumed primarily via
  combinators (map/filter/fold/etc.), not recursive pattern matching.
- **`List<T>`** is added to the standard vocabulary as an entirely
  ordinary generic ADT — `Nil | Cons(T, List<T>)` — using nothing beyond
  what §9, §10, and §19 already specify for any user-defined recursive
  type. This costs **zero new Core surface** and is where open structural
  destructuring belongs (e.g. an explicit worklist in graph traversal).

This is deliberately the cheapest possible resolution: a distinction that
needed to exist is made using machinery that already existed for
unrelated reasons.

## 9.2 `Array<T>` — Core representation, typing, evaluation, errors

Unlike `List<T>` above, `Array<T>` is **not** free — it needs its own
Core representation and two new primitive `Expr` forms, decided here
alongside `CONCRETE_SYMBOLIC_GRAMMAR.md` §7.9's surface grammar:

- **Core representation**: `Expr::ArrayLiteral(Vec<Expr>)` and
  `Expr::Index { array: Expr, index: Expr }` — genuine Core primitives,
  the same category as `Expr::Constructor`/`Expr::BinaryOp` (§9, §20.2),
  not sugar over some other existing form. This mirrors exactly how
  arithmetic operators were added (§20.2): a closed pair of primitive
  node kinds, evaluated directly, with an explicit condition attached —
  being primitive Core forms does not make them permanent public-surface
  primitives forever; a later migration of `IndexExpr` to a prelude
  function remains open, same caveat §20.2 already states for operators.
- **Runtime value**: a new value kind holding a persistent, immutable
  sequence — never a value any existing `Value` case could be reused
  for, matching §2's "constructed values are immutable; 'updating'
  produces a new value" rule. `Array<T>`'s own homogeneity (§9's
  ordinary generic-type machinery: every element shares one Core type,
  the same way `List<T>`'s `Cons` field does) means no runtime tag
  checking beyond what any other ADT already needs.
- **Evaluation order**: `ArrayLiteral`'s elements evaluate strictly,
  left to right (§14's general rule, no exception carved out — the same
  rule `Constructor`'s argument list already follows). `IndexExpr`
  evaluates `array` before `index`, matching the established
  left-to-right convention for every other multi-subexpression form
  (§12's `MutRebind`, §20.2's `BinaryOp`).
- **Typing**: `Array<T>` is an ordinary one-parameter generic type
  (§19's existing generic-instantiation machinery — no special-casing
  beyond what `Cell<T>`/`Optional<T>` already receive).
  `ArrayLiteral`'s element type is unified across every element (a
  homogeneous-only collection, per §9.1's own framing and
  `DESIGN_VISION.md` §4's "homogeneous only" invariant) — an empty
  literal (`[]`) leaves that element type an ordinary unconstrained type
  variable, resolved from context exactly as `None`'s type parameter
  already is (§19.1's value-restriction machinery applies unchanged:
  `ArrayLiteral` is not listed as a syntactic value below, so a
  `let`-bound empty array does not spuriously generalize). `IndexExpr`
  requires `array : Array<T>` and `index : Int`, producing `T`.
- **Errors**: an out-of-bounds `IndexExpr` raises `Exception`'s existing
  `InvalidOperation(Str)` variant (§15.2) — the same "shared shape for
  every other internal runtime failure" role `InvalidOperation` already
  plays for e.g. a non-function `Apply` target, not a new dedicated
  `Exception` variant. This is a **runtime** condition, exactly like
  §15.3's division-by-zero — `IndexExpr`'s *type* (`T`) is valid
  regardless of whether `index` turns out in-bounds at evaluation time,
  so typechecking never inspects `index`'s value, only its type (`Int`).
- **Value restriction (§19.1)**: `Expr::ArrayLiteral` and `Expr::Index`
  are **not** syntactic values — an array literal is an allocation
  (`Apply`-shaped in spirit, the same reasoning already applied to
  `MutCell`/`Constructor`'s non-value siblings), and indexing is
  evaluation, not a value shape. Neither generalizes.
- **Not covered by this slice, deliberately**: `map`/`filter`/`fold`,
  `get`/`set`/`length`, and any other combinator — ordinary
  stdlib/intrinsic functions over `Array` values via ordinary
  `Application`, ADT/`TypeApplication` machinery already specified, not
  a Core or grammar concern. `IMPLEMENTATION_ARCHITECTURE.md` §11
  already anticipates some of these will eventually need a native
  (non-Core-expressible) runtime hook — a separate, later architectural
  decision, not required to land `ArrayLiteral`/`IndexExpr` themselves.

## 10. Pattern semantics — a distinct grammar category, not a variant of Expr

This needed to be settled explicitly rather than left implicit. **Patterns
are not expressions.** `Pattern` (§1) is a separate production from `Expr`,
used exclusively as the left-hand component of a `Match` arm. A
constructor tag is legitimately overloaded across the two categories — as
an `Expr`-building function (§9) on the right-hand side of bindings, and as
a `Pattern`-head (`PConstructor`) on the left-hand side of a match arm —
but a parser is never ambiguous about which grammar it's in, because
expression position and pattern position are syntactically distinct
contexts, never overlapping. This is the standard resolution in every
ML-family language and is adopted deliberately, not by default: it fixes
the parser's architecture as **two grammars sharing a name namespace for
constructor tags**, not one unified grammar with patterns embedded as an
expression subtype.

---

## 11. Binding semantics

`Let(name, value, body)` — ordinary non-recursive binding, `value`'s scope
does not include itself.

`LetRec(bindings, body)` — a **binding group**: every name in `bindings` is
in scope within every other binding's value expression, as well as in
`body`. This is the single form that resolves both self-recursion for a
single named function (a `LetRec` with one binding) and mutual recursion
across multiple functions defined together (a `LetRec` with several) —
previously flagged as unaddressed in v0.3, now closed with one form instead
of two special cases.

**`LetRec` is a recursive-*function*-binding mechanism, not a general
recursive-value mechanism, and this is intentional, not a temporary
limitation.** Obfusku has no way to construct genuinely self-referential
data at all — expected and correct for a strict, non-lazy, persistent-value
language, not a gap to close later. §19.1 depends on this framing directly:
because every `LetRec` binding is necessarily a `Lambda`, every `LetRec`
binding automatically qualifies for generalization with no extra rule
needed there either.

**Legality constraint (settled by strict evaluation, not by convention):**
every binding's value expression in a `LetRec` group must be syntactically
a `Lambda`. This is forced, not a style preference — `Constructor` args are
evaluated before construction (§14), so any non-`Lambda` recursive binding
(`x = Cons(1, x)`) would require evaluating a reference to `x` before `x`
exists, which is unconditionally broken under strict evaluation with no
laziness anywhere else in the design. `Lambda`, uniquely, never evaluates
its body at construction time, only captures a reference to its
environment — which is exactly what makes recursion through it safe.

**Initialization mechanism:** a `LetRec` group's environment slots are
allocated first (unfilled); each binding's `Lambda` expression is then
evaluated against a reference to that still-filling environment (safe,
since constructing a closure never inspects its captured environment); once
every slot is filled, `body` evaluates. No code path can observe a slot
before it is filled, because a slot is only ever read when its closure is
*applied*, never during group construction — this is proven by the
`Lambda`-only restriction, not merely typical of implementations.

---

## 12. Mutation

`MutCell(initial)` allocates a distinct value kind — a cell, not a plain
value — holding `initial`. `MutRebind(cell, newValue)` replaces the cell's
contents and evaluates to `Unit`; `MutRead(cell)` yields the cell's current
contents. Nothing else in the Core is mutable — this is the entire
mutation surface. Evaluation order for `MutRebind(cell, newValue)` follows
the same left-to-right principle as every other multi-subexpression form in
this document (§14): `cell` evaluates before `newValue` — not previously
stated explicitly, extended here from the general rule rather than left
implicit for this one form.

### 12.1 The type of a cell: `Cell<T>`, ordinary and invariant

A cell has a real, surface-visible type, `Cell<T>` — an ordinary
one-parameter generic type using exactly the machinery §19 already
specifies for `Array`, `Optional`, and any user-defined generic type,
nothing new. This is required for §12's explicit reference-taking form to
make sense at all: passing a cell as a value (to a function, into a data
structure) only means something if `Cell` is nameable in a signature.

Typing rules: `MutCell : T → Cell<T>`; `MutRead : Cell<T> → T`;
`MutRebind : Cell<T> → T → Unit` (the new value must match the cell's
element type exactly).

**Variance/aliasing:** this design has no subtyping relation anywhere, so
`Cell<T>` unifies invariantly the same way every other generic type
already does — `Cell<Int>` unifies only with `Cell<Int>`, never with
`Cell<Anything else>`. There is no separate variance rule to invent here;
this is confirmed explicitly so no reader assumes an exception was
intended for `Cell` specifically. The deeper soundness guarantee — that a
given cell's `T` can't be observed as two different types through two
different aliases — is not a new rule either: it's a direct consequence of
§19.1's generalization restriction (a cell's type parameter is fixed at
allocation and never regeneralized, so every alias of the same cell is
forced by ordinary unification to agree on `T`).

`Cell` has no user-visible constructors and is not a `data` declaration —
there is no `PConstructor` pattern for it; a `Cell`-typed match scrutinee
can only be bound (`PVar`) or ignored (`PWildcard`), never destructured.

**Identity and aliasing:** at the Core level, `Var(x)` always returns the
raw value `x` is bound to — a cell, if that's what's bound — with **no
implicit dereference ever happening inside `Var` itself.** What differs is
the *surface* desugaring, uniformly across every context (not only
closures — see §13's earlier version, which stated this rule for closure
capture alone and needed to generalize): an ordinary surface use of a
mut-bound name desugars to `MutRead(Var(name))`; an explicit
reference-taking surface form omits the read and passes the bare cell,
producing aliasing. Ordinary copying of a mut-bound name (`Let(y, Var(x),
body)` with no explicit reference-taking syntax) therefore reads a
snapshot of current contents — `y` is bound to an ordinary, thereafter-
immutable value, not aliased to `x`'s cell. This costs no new Core form,
only this desugaring-table statement.

---

## 13. Closure capture

**Amended during the runtime slice** (implementation-driven correction —
see the note at the end of this section for the concrete contradiction
that forced it; this replaces the section's original "snapshot at
creation time" wording, which was found to be unrealizable given the
already-frozen §12 desugaring rule).

A `Lambda` value is a pair of its code and a captured environment — a
mapping from its free variable names to their bindings at definition time.
Capture is **completely uniform**, with no Cell-specific case: for *every*
free variable, captured or not, the closure's environment holds exactly
whatever the enclosing environment currently binds that name to. For a
name bound to an ordinary (persistent) value, this is the value itself.
For a name bound to a `MutCell`, this is **the cell itself** — the same
cell object, not a copy of its contents — regardless of whether the
surface reference that produced this closure used default or explicit
capture syntax.

What *does* differ between default and explicit capture is not what gets
captured, but what the (already-frozen, §12) desugaring puts in the
closure's *body*: an ordinary surface reference desugars to
`MutRead(Var(name))`, so each occurrence re-reads the shared cell's
**current** contents at the moment that expression is evaluated — for a
reference inside a `Lambda` body, that moment is call time, not
creation time. A closure therefore **does** observe rebinds made to a
default-captured variable between its creation and any given call; there
is no snapshot, and no timing gap where the closure "misses" writes. The
explicit reference-taking surface form omits the `MutRead` and desugars to
a bare `Var(name)`, handing the caller the cell itself — needed for
`MutRebind`, and for passing the cell onward as a value.

**Required constraint** (unchanged in effect, restated for the corrected
mechanism): **a closure body performing `MutRebind` on a free variable
requires that variable to have been referenced via the explicit form
somewhere in that body; using only default (`MutRead`-wrapped) references
to a variable the body attempts to rebind is a static, checked error.**
Under the corrected semantics this is no longer a physical necessity —
the cell is reachable either way, since capture is uniform — it is a
**deliberate surface-level discipline**: writing `˚` is what makes
mutation-intent visible at the point of reference, and that visibility
requirement is enforced statically rather than left to follow implicitly
from how the variable happens to be used elsewhere in the body.

**Nesting does not propagate reference-capture status.** A closure nested
inside another closure that also wants to rebind an outer cell must
independently write an explicit reference to it somewhere in its own
body; otherwise `MutRebind` against it is rejected at its own boundary
under the constraint above. This is more verbose than automatic
propagation but consistent with the design's broader refusal to let
capability follow implicitly from usage rather than declaration.

**The contradiction that forced this amendment.** The original wording
said default capture "produces an ordinary, henceforth-immutable value in
the closure" via a `MutRead` performed *at closure-creation time*. Taken
literally, that means: evaluating `Lambda(x, MutRead(Var(counter)))`
(the already-frozen §12 desugaring of `λ(x) → counter` for a mut-bound
`counter`) would dereference `counter` while *constructing* the closure,
and store the resulting plain `Int` — not a cell — under `counter` in the
closure's environment. But the closure's *body* is exactly
`MutRead(Var(counter))`, unchanged, and under strict evaluation (§14) that
body is not touched until the closure is called. At call time,
`Var(counter)` would then return the plain `Int` stored at capture time,
and `MutRead` would be applied to it — a runtime type error, since
`MutRead` requires a `Cell<T>`, not a `T`. This is not an edge case: it
would fire on *every* default reference to a mut-bound free variable
inside *every* lambda, unconditionally, because §12's desugaring always
wraps such references in `MutRead`. Fixing it by restructuring the
desugaring (e.g. hoisting a snapshot-read out before the `Lambda`) was
ruled out as inventing new lowering machinery without spec authority for
it. The only resolution available from already-frozen rules, without
inventing anything, was to make capture itself uniform (Candidate B
above) and correct the section's timing claim to match — which is what
this revision does.

---

## 14. Evaluation order

Strict, call-by-value. `Apply(fn, arg)` evaluates `fn`, then `arg`, then
performs the application. `Constructor(tag, args)` evaluates `args`
left-to-right before constructing the value. `Match` evaluates the
scrutinee exactly once, before testing any arm. Deterministic: no
evaluation order is left to the implementation.

### 14.1 Guaranteed tail-call elimination — a semantic requirement, not an optimization

Given no general loop construct (§ Appendix) and no laziness anywhere,
recursion is the sole mechanism for open-ended iteration. Without a
guarantee, a program expressing ordinary iteration recursively could
stack-overflow depending on which conforming implementation runs it —
making "no loops" the hostile, ideology-driven outcome the design
explicitly set out to avoid, not an earned one. This guarantee is
therefore part of the language's observable contract, stated precisely
against this Core's grammar:

An `Apply` is in **tail position** — and therefore guaranteed constant
additional stack space across a recursive chain of such calls — when it
is: the `body` of a `Lambda`; the `body` (never the bound `value`) of a
`Let` or `LetRec`, transitively; or an arm's result (never the
`scrutinee`) of a `Match`, transitively. This must hold for **mutual**
tail recursion through a `LetRec` group, not only direct self-tail-calls —
the state-machine pattern (Appendix) depends on functions tail-calling
each other.

**Stated limit, not a hidden gap:** calls inside the dynamic extent of an
active `Catch` (§15) are **not** covered by this guarantee — a `Catch`
frame must remain observable to catch a possible `Raise`, so it cannot be
discarded the way an ordinary tail call's frame is. Wrapping a long
tail-recursive chain in one enclosing `Catch` is fine (the frame is paid
for once); placing a `Catch` *inside* the repeatedly-tail-called function,
around each iteration, defeats the guarantee. This boundary is deliberate
and must be documented as such wherever this guarantee is described to
users, not discovered by them empirically.

---

## 15. Error propagation — two genuinely different Core mechanisms

- **Expected failure** needs no special Core support at all. `Result` is
  an ordinary two-constructor data type (§9); "propagation" is ordinary
  function code (matching, or combinators built from matching) — nothing
  new in the Core.
- **Exceptional failure** needs an actual effect, because unwinding
  multiple stack frames isn't expressible as ordinary data-passing.
  `Raise(value)` abandons the current evaluation up to the nearest
  enclosing `Catch(body, handler)`, which evaluates `handler` applied to
  the raised value. This is intentionally the *only* place the Core has
  non-local control flow — everything else in this document evaluates
  in a straight line per §14.

### 15.1 Evaluation semantics of `Catch` (resolved by analogy with `Match`)

`Catch(body, handler)` evaluates `body`. If it completes without a
`Raise` reaching this frame, `handler` is **never evaluated at all** —
not evaluated-and-discarded. This is the same shape as `Match`: evaluate a
discriminant once, then evaluate exactly one of the possible branches. On
a caught `Raise(v)`, the result is `Apply(handler, v)` — ordinary
application, so `handler` is itself ordinary Core code and may `Raise`
again, propagating past *this* `Catch` (evaluation is now outside `body`)
to the next enclosing one. Re-raising therefore needs no dedicated form;
sequencing "act, then re-raise" inside a handler needs no new form either
— `Let(_, Apply(action, e), Raise(e))` already sequences via existing
`Let` semantics.

**Made fully explicit: the `Catch` frame is deactivated the instant
control passes from `body` to `handler` — before `handler` begins
running, not after it completes.** A `Raise` occurring during handler
evaluation is therefore never caught by the same `Catch`; resolution
searches from the next enclosing `Catch` outward, exactly as if this one
were no longer present. Once `handler` completes normally, this `Catch`
is fully exited and its result flows onward like any ordinary expression.

**Worked trace, to remove any ambiguity about nesting:**

```
Catch(
  body    = Catch(body = Raise(A), handler = Lambda(e, Raise(B))),
  handler = Lambda(e, ...)
)
```

`Raise(A)` executes with the inner `Catch` active → inner `Catch` catches
`A`, deactivates itself, evaluates `Apply(innerHandler, A)` →
`innerHandler` evaluates `Raise(B)`, with the inner `Catch` already
inactive → resolution searches outward and finds the outer `Catch` → outer
`Catch` catches `B` (not `A`) and evaluates its own handler applied to
`B`. The outer handler never sees `A` — this is the entire content of
"re-raise," stated as a trace rather than left to be inferred.

**Resolution is dynamic, not lexical — the one deliberate exception in
this Core.** `Raise` resolves to the nearest *dynamically* enclosing
`Catch`: the actual stack of active `Catch` frames at the moment `Raise`
executes, not the lexical nesting of `Catch` expressions in source text.
Every other binding form in this document (`Let`, `LetRec`, `Lambda`
capture) resolves lexically; this is the sole place dynamic extent
governs resolution, and it is stated here explicitly rather than left to
be inferred by analogy with more familiar languages. Because evaluation
order is already fixed (§14) and this resolution is a deterministic
function of that fixed order, unwinding is automatically deterministic —
no further commitment is needed beyond this statement.

**Uncaught `Raise`.** An uncaught `Raise` terminates the enclosing
program/module evaluation deterministically, carrying the raised value —
it must not silently swallow or continue. (Exact reporting/formatting is
an implementation concern; "terminates, does not continue" is the
semantic commitment.)

### 15.2 What `Raise` may carry — resolved: a closed, standard `Exception` type

Leaving `Raise`'s argument fully polymorphic (any type at all) was
considered and rejected: `Raise(42)`, `Raise("oops")`, and `Raise(true)`
would all type-check with no shared shape, so a single `Catch` downstream
could only meaningfully handle one of them — "catch several kinds of
failure" would require the programmer to invent an ad hoc unifying wrapper
anyway, at which point a closed error type has been reinvented informally
without being declared. A fixed, closed built-in `Exception` type with no
extensibility was also rejected as too restrictive for domain-specific
error information.

**Resolution:** `Raise` requires its argument to be of a single standard,
**closed** `Exception` type — an ordinary ADT, costing no new Core
mechanism — with exactly five built-in variants, enumerated here in full,
not illustratively:

- `DivisionByZero` — `Int` division/modulo by zero (§15.3).
- `IntegerOverflow` — an `Int` arithmetic result outside `i64`'s
  representable range (§20.2) — a genuinely different condition from
  `DivisionByZero`, not merged into it (`MIN_INT / -1`/`MIN_INT % -1` are
  the one case `÷`/`⌗` can raise this for).
- `NonExhaustiveMatch` — a non-exhaustive `Match` reached at runtime;
  should never occur per §17's static exhaustiveness check, but the
  variant exists defensively rather than being unreachable-and-undefined.
- `InvalidOperation(Str)` — a language-level operation misused in a way
  no other variant names (the message is the payload).
- `Failure(tag: Str, payload: Str)` — the one deliberately generic
  variant, parametric in spelling (not in type — both fields are
  concretely `Str`), for user-defined error shapes.

No sixth variant exists, and none is planned implicitly by a hedge like
"and similarly-shaped cases" — a new built-in failure condition requires
amending this list explicitly, the same way `IntegerOverflow` itself was
added here (P0-D) rather than left for a reader to infer. Because the
*variant set* of `Exception` is fixed (only `Failure`'s payload varies in
content, not in type), `Exception` remains an ordinary closed type for
exhaustiveness purposes (§17) — no mandatory wildcard is forced on a
`Match` over `Exception` the way one is forced for `Int` or `String`, and
a `Match`/`Catch` covering exactly these five arms is exhaustive. This
keeps the Result/exceptional-failure distinction sharp: nothing can be
raised without being deliberately
shaped as an `Exception` first.

### 15.3 Division by zero — the contradiction, resolved

`SEMANTIC_CORE_STRESS_TEST.md` found a genuine contradiction here: this
document's own examples listed "parse, lookup, divide" together as
`Result`-channel cases, while the closed-operator-ergonomics commitment
(arithmetic operators keep direct, unwrapped signatures — nobody wants
`a / b` to require unwrapping a `Result` at every use) makes `/` returning
`Result<Int, E>` untenable. **Resolved: arithmetic operators keep their
direct signatures (`/ : Int → Int → Int`), and division by zero raises**
(`Exception`'s `DivisionByZero` variant, §15.2) rather than returning a
`Result`. This fits the exceptional channel's own stated criterion better
on reflection than the Result channel's — division by zero is closer to a
programmer-error-shaped condition than to a genuinely anticipatable
"might be absent" case like lookup or parse. The earlier "parse, lookup,
divide" grouping was imprecise and is corrected here: "divide" does not
belong on that list.

**This applies to `Int` division only — `Real` division does not raise.**
This asymmetry wasn't stated before and needs to be, so it doesn't read as
a blanket rule: `Int` raises on zero divisor specifically because `Int`
has no representation for infinity or NaN and therefore no total answer to
give. `Real`, being IEEE-754-shaped, already has one — `Real` division
follows IEEE-754 totality (`1.0 / 0.0 = Infinity`, `0.0 / 0.0 = NaN`,
never a raise). Layering an exception in front of a domain that IEEE
already made total would be an inconsistency, not a safety improvement.

### 15.4 Ambient I/O — signatures and failure tags (resolved); provisioning and filesystem scope (deliberately not yet decided)

Obfusku's standard ambient environment — names available without any
explicit `⟲` — includes four I/O operations. **This subsection fixes their
observable contract: what a program author can rely on when calling them
and when writing a `Catch` handler around them, once an implementation
provides them.** As of this writing, `print` and `readLine` are
implemented in `crates/`; `readFile` and `writeFile` are specified here
but not yet wired (host wiring pending the filesystem scoping decision —
see `LANGUAGE_SPEC.md` §5 and `spec/adr/ADR-011-filesystem-scoping.md`).
This section specifies all four operations' contract normatively regardless
of implementation status. It deliberately does not decide
*whether every embedding of Obfusku must provide this environment*, or
*what exactly a filesystem operation's capability boundary is defined
relative to* — both questions depend on the module/artifact/distribution
model `LANGUAGE_SPEC.md` §5 already names as undecided, and are left
there rather than resolved by accident here.

**Signatures:**

```
print    : Str → Unit
readLine : Unit → Str
readFile : Str → Str
writeFile : Str → Str → Unit
```

`readLine`/`readFile`/`writeFile` reuse the exceptional channel exactly as
§15.2 already establishes — no new `Exception` variant is introduced for
any of them. A failure raises `Failure(tag, payload)` (§15.2's own
generic, user-extensible variant), with three tags fixed as part of this
environment's contract, since `tag` is otherwise an arbitrary `Str` a
program author has no way to predict from `Failure`'s shape alone:

- `"EOF"` — `readLine` found no more input to read.
- `"PermissionDenied"` — the operation was refused because it crossed
  some filesystem capability boundary. *What exactly that boundary is*
  (an entry-file-relative root? something else entirely?) is the open
  question this subsection defers, per its own opening paragraph — the
  tag's meaning is "a boundary was crossed," independent of where the
  boundary sits.
- `"IOError"` — any other I/O failure (the target doesn't exist, a disk
  error, and similarly-shaped conditions) — genuinely distinct from a
  boundary violation, not a catch-all merged with `"PermissionDenied"`.

**Deliberately not fixed here, and not to be inferred from the reference
implementation:**

- `readLine`'s exact timing — whether and how long a call blocks the
  calling program while waiting for input is a property of whatever
  external input source supplies it, not part of this language's evaluation
  semantics (§14 fixes evaluation *order*; it says nothing about a native
  call's real-world duration, and this subsection doesn't extend it to).
- The exact definition of `readFile`/`writeFile`'s filesystem capability
  boundary (what "the root" is, whether absolute paths or `..` are
  rejected, what a boundary even means for a program with no single
  on-disk entry file) — coupled to the still-open module/artifact model.
- Whether a given embedding of Obfusku is obligated to provide this
  entire standard environment, or may provide a partial or different
  one — also coupled to that same open model, since answering it requires
  first deciding what "an embedding of Obfusku" normatively is, which
  `spec/` has not yet done anywhere.

---

## 16. Desugaring rules (consolidated)

| Surface form | Desugars to |
|---|---|
| `if c then a else b` | `Match(c, [(PConstructor(True, []), a), (PConstructor(False, []), b)])` |
| `subject PIPE bareCallable` | `Apply(bareCallable, subject)` (§7) |
| `subject PIPE expr-with-reference` | `Apply(Lambda(fresh, expr'), subject)` (§7, §8) |
| under-applied / holed call | nothing to desugar — already a `Lambda` chain result of ordinary `Apply` (§6) |
| `mut` binding + reassignment | `Let(name, MutCell(init), body)` + `MutRebind`/`MutRead` at use sites (§12) |
| explicit mutable capture | captures the `MutCell` value itself rather than a `MutRead` snapshot (§13) |
| type-level `Base PIPE Wrapper` | ordinary type-level application, arity-checked via the invisible kind bookkeeping (§4) — not represented as a value-level `Apply` at all; a separate, non-Core, type-language construct |
| `a ∧ b` | `Match(a, [(PConstructor(True,[]), b), (PConstructor(False,[]), False)])` — short-circuiting falls out of `Match`'s own evaluation rule (§20.2) |
| `a ∨ b` | `Match(a, [(PConstructor(True,[]), True), (PConstructor(False,[]), b)])` — same reasoning |
| every other `BinOp`/`UnOp` (§20.2) | `BinaryOp`/`UnaryOp` primitive `Expr` forms — no desugaring, evaluated directly |
| `ArrayLiteral`/`IndexExpr` (§9.2) | `ArrayLiteral`/`Index` primitive `Expr` forms — no desugaring, evaluated directly |

`if` does not exist as a Core form. There is no `If` in §1's grammar — this
is deliberate, not an omission.

---

## 17. Exhaustiveness

A `Match` is exhaustive over a scrutinee of data type `D` if, for every
constructor declared for `D`, some arm's pattern either names that
constructor (recursively exhaustive in its subpatterns) or is `PWildcard`/
`PVar` (which covers all remaining constructors at that position). This is
checked structurally against the type declaration at compile time — never
a runtime fallback, never a silent no-op for an unhandled case. A
non-exhaustive `Match` is a compile-time error, full stop.

**Closed vs. open types — required for this rule to actually be
decidable, and previously unstated.** The rule above presumes `D` has a
finite, enumerable constructor set (`Bool`, any user ADT, `Exception`
per §15.2). For types whose `PLit` patterns can never enumerate the full
domain (`Int`, `Real`, `String`), listing literals alone can never be
exhaustive — for these **open** types, a `PWildcard`/`PVar` arm is
mandatory, not merely one sufficient way to close out the match. This
distinction must be part of the pattern checker's type-directed logic,
not an incidental compiler behavior.

---

## 18. Equality

Equality is not a single generic operator — it is a family of structural
comparisons, one per data type, generated automatically from that type's
constructor shape (two `Constructor` values are equal iff same tag and
pairwise-equal arguments; two primitives compare by value). This is
generated, not user-definable (consistent with §8 of the design draft:
closed, language-defined operator semantics). **`Lambda` values have no
equality** — there is no generated or definable comparison for function
values, since meaningful function equality is undecidable in general once
closures can be constructed dynamically (§6, §13).

**Consequence, not previously stated:** because generated equality
requires every field's type to itself support equality, **any data type
with a function-typed field anywhere in its structure has no generated
equality at all**, and using `==` on it is a compile-time type error, not
a runtime failure. This falls directly out of the rule above and needed
to be said as a consequence, not assumed obvious.

**`Real` — resolved, not left as raw IEEE-754.** Raw IEEE comparison
(`NaN ≠ NaN`) breaks reflexivity for any generated structural equality
touching a `Real` field, and for `PLit` matching on `Real` literals — an
equivalence relation that isn't reflexive is a bad foundation for pattern
matching in an otherwise disciplined type system. **Resolved:** `==` is
total, structural, language-defined value equality — `NaN == NaN` is
`true` under `==`, and this is the same equality `PLit` patterns use — made
fully explicit here for the case that motivated this rule in the first
place: whether `NaN` arises from a surface literal or, more commonly, as
the ordinary total result of an operation like `0.0 / 0.0` (§15.3), it
becomes an ordinary `Lit(NaN)` in the Core once produced, and is matched
and compared by the exact same rule as any other `Real` value — no
separate case. **Numeric ordering operators (`<`, `>`, `≤`, `≥`) are a separate operator
family and follow IEEE-754 semantics** where it applies (so a `NaN`
ordering comparison is `false`, as IEEE specifies) — this doesn't
reintroduce the reflexivity problem, since ordering was never claimed to
be an equivalence relation in the first place. Equality and ordering are
allowed to disagree about `NaN` because they answer different questions.

**`MutCell` — resolved: identity equality.** Two cells are equal under
`==` iff they are the same cell (aliasing), never by comparing current
contents. Structural comparison of contents was rejected: it would let
two independently-mutable, observably-different entities compare equal at
one instant and unequal the next with no operation visibly performed on
either side to a reader — the one case in an otherwise fully
persistent-value language where "equal" could silently stop meaning
anything stable. Comparing current contents remains possible, just not
implicit — `MutRead` both cells first, then compare the results with
ordinary structural equality.

**Cross-type comparison** is not defined at all — `==` is only
well-typed between two fully-instantiated values of the identical type,
consistent with the existing "no implicit conversions" commitment.

---

## 19. Generic instantiation

A parametric data or function declaration is, at the type level, a
type-indexed family. Instantiation happens by unification at each use
site — the checker solves for the type argument(s) from context, the same
way ordinary type inference solves for a `Lambda` parameter's type from its
use. **The user never writes an explicit type-level application** (no
`f<Int>`-shaped syntax at the surface) — this is a deliberate restriction,
not a gap, keeping generics "small and deliberate" per the existing
commitment and keeping kind bookkeeping (§4) fully behind the scenes.

### 19.1 Generalization and the value restriction — required for type safety

This was entirely unaddressed before this pass, and the gap is a real
soundness hole, not a stylistic one: combining unrestricted let-polymorphism
with mutable cells is the textbook ML unsoundness case. Without a
restriction, `Let(r, MutCell(Constructor(Nil, [])), body)` could generalize
`r`'s type to `∀T. Cell<T>`; a `MutRebind` elsewhere in `body` instantiates
`T = Int`; a later `MutRead` at `T = String` then reads an `Int` as a
`String` — a well-typed program that violates its own types at runtime.

**Resolution — the standard ML value restriction, kept to one rule:** a
`Let` or `LetRec` binding is eligible for generalization (universal
quantification over its free type variables) **if and only if its value
expression is a syntactic value** — `Lambda`, `Lit`, `Var`, or a
`Constructor` applied only, recursively, to syntactic values. Any other
expression form — critically, any `Apply`, which includes `MutCell(...)`
allocation, `MutRead`, `Raise`, and `Catch` — yields a monomorphic binding;
its type variables are resolved by ordinary unification against subsequent
usage within scope, never re-generalized.

**This single rule, applied uniformly, is also the entire mutability
answer** — no separate "mutable cells can't be polymorphic" rule is
needed. `MutCell(...)` is `Apply`-shaped regardless of what it allocates,
so every `mut` binding is monomorphic automatically, which is exactly what
prevents the unsound scenario above: the cell's element type is fixed once
by the first constraining use, and any later use requiring a different
type is a type error at that point.

**Ordinary immutable bindings are unaffected in the common case** — a
locally-defined helper function (`Let(id, Lambda(x, Var(x)), body)`) is a
syntactic value and generalizes normally, preserving the useful part of
local inference. **`LetRec` bindings are always eligible**, since §11
already forces every `LetRec` binding to be a `Lambda`, itself always a
syntactic value — one existing restriction quietly protects another.
Within a `LetRec` group, each binding's own name is treated monomorphically
while the group's bodies are being checked (standard non-polymorphic
recursion — avoiding undecidable polymorphic-recursion inference), and is
only generalized for use outside the group once checking completes.

---

## 20. Module boundaries

A module is a named collection of top-level `Let`/`LetRec` bindings and
data declarations. Export is metadata attached to a binding (visible
outside the module) rather than a Core expression form — it doesn't affect
evaluation, only name resolution across module boundaries. Import brings
another module's exported names into scope for resolution. This section is
intentionally minimal: the artifact model (what a compiled/packaged module
actually is) remains deferred per the design draft's open items, and
nothing here should be read as prejudging that.

**Module-result convention.** What executing a module produces —
"the result" of running a `.obk` program — is `run`'s own
execution-reporting contract, not a Core-level concept: `obfusku-runtime::
evaluate` evaluates every top-level binding in declaration order into one
shared environment, and `run` reports the value of the **last** binding
(named `RunResult` at the `obfusku-cli` boundary — see `ROADMAP.md`). This
says nothing about `exported` bindings specifically and is not a
`main`/entry-point convention; the broader artifact/distribution model
this section already defers (previous paragraph) is free to add its own
reporting convention on top without revisiting this one.

**Open question (deferred — see `LANGUAGE_SPEC.md` §5)**: once `Module`
bindings can belong to a `LetRec` group (`CONCRETE_SYMBOLIC_GRAMMAR.md`
§8.2), "the last binding" needs a precise meaning — the last *slot* in
declaration order regardless of grouping, or something that treats a whole
group as occupying one position? Left for whoever resolves the artifact
model together with the rest of §8.2's open items.

---

## 20.1 Recursive type declarations are unaffected by §11's `LetRec` restriction

Worth stating once so the two are never conflated: `type Tree =
Leaf | Node(Tree, Int, Tree)` is a structural/nominal declaration, never
evaluated — nothing analogous to §11's value-level cyclic-initialization
problem applies to a type referencing itself. The `Lambda`-only
restriction is about constructing *values* eagerly; type declarations
aren't constructed at all.

---

## 20.2 Primitive operators (arithmetic, comparison, equality, logical)

**Contract-extraction pass, no code written against it yet** (mirrors the
`FunctionDeclaration`/`LetRec` process). `CONCRETE_SYMBOLIC_GRAMMAR.md`
§7.1a/§13 already froze the full lexical grammar and precedence table for
`✚ ☠︎ ✱ ÷ ⌗ < > <= >= == != ∧ ⊻ ∨ ¬ −` — but nothing in this document gave
them a Core representation or typing rule, and `DESIGN_VISION.md` §8's
"closed, language-defined... not restricted to a single monomorphic type"
framing under-specifies exactly which types and how the checker resolves
them without inventing typeclasses. Resolved below.

**Core representation — new primitive forms, explicitly not a permanent
architectural commitment.** Two new `Expr` forms:

```
BinaryOp(op: ArithOp | CompareOp | EqOp | XorOp, lhs: Expr, rhs: Expr)
UnaryOp(op: NotOp | NegOp, operand: Expr)
```

covering `✚ ☠︎ ✱ ÷ ⌗` (arithmetic), `< > <= >=` (comparison), `== !=`
(equality), `⊻` (logical xor — not short-circuiting, see below), and
`¬ −` (unary). Both evaluate strictly, left-to-right, like every other
multi-subexpression form in this document (§14) — `lhs` before `rhs`.
**This is a primitive-for-now decision, not a claim that operators are
Core-level forever**: `IMPLEMENTATION_ARCHITECTURE.md` §15 favors a future
prelude-as-Obfusku-source model for most of the standard library. Module
import resolution (`ImportDeclaration`) is now implemented (see
`ROADMAP.md`), but operators have not been migrated onto it — they remain
primitive `Expr` forms. A later desugaring-only migration to prelude
functions would change nothing about surface syntax or observable
semantics, only where the implementation of `+` lives.

**`∧`/`∨` are short-circuiting and are NOT `BinaryOp` at all — they
desugar to `Match`, reusing §16's existing `if`-desugars-to-`Match`
mechanism instead of inventing a new conditional-evaluation rule:**

```
a ∧ b  desugars to  Match(a, [(PConstructor(True,[]), b), (PConstructor(False,[]), False)])
a ∨ b  desugars to  Match(a, [(PConstructor(True,[]), True), (PConstructor(False,[]), b)])
```

`Match` already evaluates its scrutinee exactly once and then exactly one
arm (§14/§17) — so short-circuiting falls out of machinery this document
already specifies precisely, with zero new evaluation-order rules needed.
This was the deciding reason to resolve `∧`/`∨` this way rather than as
`BinaryOp` variants with a bespoke "conditionally evaluate `rhs`" carve-out:
one exception to strict evaluation (this document already has one, `Catch`'s
TCO carve-out, §14.1) is tolerable; a second, differently-shaped one is
not, when an existing mechanism already produces the right behavior for
free. **`⊻` cannot be short-circuited by definition** — its result depends
on both operands regardless of either one's value — so it stays an
ordinary, always-both-operands-evaluated `BinaryOp`, unlike its siblings.

**Closed dispatch table — no typeclasses, no user extension, checked
structurally against the two operand types during inference:**

| Operator | Accepted operand types | Result |
|---|---|---|
| `✚` | `Int, Int` / `Real, Real` / `Str, Str` (concatenation) | same type as operands |
| `☠︎`, `✱`, `÷` | `Int, Int` / `Real, Real` | same type as operands |
| `⌗` | `Int, Int` only | `Int` |
| `<`, `>`, `<=`, `>=` | `Int, Int` / `Real, Real` | `Bool` |
| `==`, `!=` | any single type `T` for which structural equality is defined per §18 (both operands must be exactly `T`; a `Lambda`-typed operand — or any type with a function-typed field anywhere in its structure, per §18 — is a **static type error**, not a runtime failure) | `Bool` |
| `∧`, `∨`, `⊻` | `Bool, Bool` | `Bool` |
| `¬` | `Bool` | `Bool` |
| `−` (unary) | `Int` / `Real` | same type as operand |

This is the entire table — closed, not "and other types where the
meaning is unambiguous" (`DESIGN_VISION.md` §8's phrasing, which this
resolves rather than defers further): a future type wanting `✚` support
needs a deliberate amendment here, not an implicit extension inferred
from "seems unambiguous." `Str` concatenation is included as the one
unambiguous, non-numeric case the phrase could plausibly have meant;
nothing else was added speculatively.

**No implicit numeric coercion, stated explicitly rather than left as an
absence.** `1 ✚ 2.0` is a type error, full stop — `Int` and `Real` never
unify for operator purposes, consistent with §18's already-established
"no implicit conversions" commitment (there stated for cross-type
equality; extended here to arithmetic and comparison for the same
reason: an operator silently picking a conversion is exactly the kind of
accidental semantics this specification has repeatedly had to walk back
elsewhere in this document). Mixed `Int`/`Real` arithmetic requires an
explicit conversion function — not yet named or specified; a stdlib
question, not an operator-typing one.

**`⌗` (modulo) — truncating remainder, sign follows the dividend, raises
on a zero divisor exactly like `÷` (§15.3's reasoning applies identically:
`Int` has no total answer for a zero modulus either).** Frozen with
worked examples, not by reference to another language's behavior:

```
 7 ⌗  3 =  1
-7 ⌗  3 = -1
 7 ⌗ -3 =  1
-7 ⌗ -3 = -1
```

(equivalently: `a ⌗ b` has the same sign as `a`, and `(a ÷ b) * b + (a ⌗ b) = a`
under truncating integer division). Floor-modulo (sign follows the
divisor) was considered and rejected only for lack of a reason to prefer
it — truncating matches `÷`'s own already-frozen "Int is machine-integer-
shaped, no total answer past its natural range" framing (§15.3) more
directly than floor-mod's more "mathematically regular but non-native"
behavior would.

---

## Verdict on stability

Every finding from `SEMANTIC_CORE_STRESS_TEST.md` is resolved above: the
division/`Result` contradiction is corrected (§15.3); `LetRec`,
mutation-capture, `Catch`/`Raise`, TCO, `Array`/`List`, and exhaustiveness
are stated precisely rather than left implicit; `Real` and `MutCell`
equality are resolved with stated reasoning. A second, targeted pass then
closed the remaining holes needed to call this genuinely freeze-worthy: the
polymorphism/mutability value restriction (§19.1) — a real soundness gap,
not a style question — `Cell<T>`'s type and invariance (§12.1), explicit
`Catch`-frame deactivation with a worked nested trace (§15.1), the
`Real`-vs-`Int` division asymmetry (§15.3), and the Appendix's now-stale
"open questions" corrected to point at where they were actually resolved.
Nothing beyond what these two passes surfaced was introduced. This document
is the frozen semantic foundation — the next document to write against it
is an Abstract Grammar, not another semantics pass.

---

## Appendix: the ten-algorithm stress test

Before accepting "no general loop construct" as a real answer, ten small
algorithms were worked through analytically against
`ADT + Match + pipeline combinators + LetRec recursion`, per the
instruction to test this empirically rather than aesthetically.

| Algorithm | Verdict |
|---|---|
| sum, map, filter, count | Trivial — single stdlib combinator call each. Not a recursion case at all. |
| find | Trivial as a stdlib combinator over built-in collections — **built-in collections are consumed via combinators, not user-level pattern-matching recursion.** This division (combinators for built-in collections, match+recursion for user-defined recursive ADTs) is what actually makes "no loop construct" viable, and should be stated as a real design commitment, not left implicit. |
| Fibonacci | Natural — ordinary `LetRec`, `Match` on `Int` literals. |
| tree traversal | Natural — the canonical case `Match` + recursion is built for. |
| state machine | Natural — states as an ADT, a `step` function via `Match`; arguably one of the strongest fits found. |
| graph traversal (DFS, visited set) | **Workable, and no longer blocked.** Needs an explicit worklist threaded through recursive calls, or a `mut` cell for the visited set — this stress test originally surfaced open head/rest destructuring on built-in sequences as a missing prerequisite. **Resolved by §9.1**: the worklist should be a `List<T>` (an ordinary `Nil`/`Cons` ADT, fully pattern-matchable via existing machinery), not an `Array` (which deliberately does not support open destructuring, for cost-transparency reasons stated in §9.1). Slightly more ceremony than a `while`-loop version, not absurd, and no longer resting on an undecided question. |
| streaming input (read until a terminating condition) | Ordinary recursion calling an effectful read each step, tail-recursive by construction. **Resolved by §14.1**: guaranteed tail-call elimination is now a stated semantic requirement, covering this case directly (subject to the `Catch` carve-out noted there — the read loop itself must not be wrapped in a per-iteration `Catch`). |

**Conclusion**: the combinator/recursion model handles all 10 cases
naturally or acceptably. The two prerequisites this test identified as
necessary for "no loops" to be an earned outcome rather than an
ideological one — pattern-matchable structure for open-ended built-in-like
sequences, and guaranteed tail-call elimination — are both now resolved
elsewhere in this document (§9.1, §14.1) rather than left open. Nothing
here reopens either decision; this table is corrected to stop implying
otherwise.
