# Obfusku Language Specification

**Status: normative entry point.** This document does not restate rules
already defined elsewhere in `spec/` — it establishes the epistemic
boundary this specification operates under, maps each design domain to
its single normative source, and states the rule for resolving any
apparent conflict. Duplicating content here would recreate exactly the
problem this document exists to prevent: two places claiming to say what
the language is, silently drifting apart.

---

## 1. The boundary

```
spec/      — the only normative source. What the language is.
crates/    — the implementation. Must conform to spec/, never the reverse.
```

If `spec/` says X and `crates/` does or says Y, the implementation is
wrong — it has not yet been brought into conformance. If `spec/` itself
turns out to be wrong, `spec/` is amended deliberately, as its own
decision, before any code changes to match it. `crates/` never gets a
vote in what the language currently is.

---

## 2. Domain map — each domain has exactly one normative source

| Domain | Normative document | Covers |
|---|---|---|
| Philosophy, identity, design laws | [`DESIGN_VISION.md`](DESIGN_VISION.md) | What Obfusku is for; the compositional principle (`subject, transformed by`) underlying the rest; paradigm, typing stance, evaluation model at the level of intent |
| Formal semantics | [`SEMANTIC_CORE.md`](SEMANTIC_CORE.md) | The core calculus: values, types, evaluation order, `LetRec`, mutation/`Cell`, `Catch`/`Raise`, exhaustiveness, equality, generalization — everything that determines what a program *means* |
| Surface structure, pre-glyph | [`ABSTRACT_GRAMMAR.md`](ABSTRACT_GRAMMAR.md) | What constructions exist and how they compose, in the abstract — `Declaration`, `Expression`, `Pattern`, `Type`, `Module` — before any symbol is chosen |
| Glyph vocabulary | [`GLYPH_SYSTEM_DESIGN.md`](GLYPH_SYSTEM_DESIGN.md) | Families, roots, modifier axes, which historical glyphs were kept/rejected/repurposed and why, typography constraints |
| Concrete syntax | [`CONCRETE_SYMBOLIC_GRAMMAR.md`](CONCRETE_SYMBOLIC_GRAMMAR.md) | Exact lexical grammar, precedence/associativity, whitespace rules, ambiguity resolution — how Obfusku is actually written |

Each document states its own status (frozen, established, candidate) at
its top — that status is authoritative for that document and is not
re-summarized here, since re-summarizing it is exactly the kind of
duplication that goes stale first.

---

## 3. Reading order

The documents are listed above in dependency order, and were produced in
that order for a reason: each one treats everything before it as fixed
and everything after it as not-yet-decided. Reading them in this order
(vision → semantics → abstract structure → glyphs → concrete syntax)
follows how the language was actually derived, not just how it's filed.

---

## 4. Amendment rule

`spec/` is not append-only or accretive by default. When a later document
finds a genuine contradiction in an earlier one — as opposed to a gap, an
underspecification, or a question the earlier document correctly deferred
— the earlier document is edited directly to resolve it, with the
resolution's reasoning kept in place (see, for example,
`SEMANTIC_CORE.md`'s §15.3 correction of its own earlier division/`Result`
example). This has already happened more than once during this
specification's own construction. It is expected to keep happening as
`spec/` is stress-tested further — that is a sign the process is working,
not a sign of instability.

---

## 5. What `spec/` does not yet cover

Named explicitly, so absence is never mistaken for an implied answer:

- **Standard library contents and naming — resolved.** `List`,
  `Optional`, and `Result` are implemented
  (`crates/obfusku-cli/src/stdlib.obk`) as ordinary generic ADTs with
  symbolic constructors (`GLYPH_SYSTEM_DESIGN.md` §10.3/§10.4). `Array`
  needed no wrapper-name decision — it is a Core primitive (§9.2), not
  a library ADT — and its combinator surface (`⊡`/`⊟`/`⊞`/`#`/`⊙`) is
  implemented as host natives (§10.5, `ADR-021`). `Cell` needed none
  either — a first-class `Type::Cell` variant, not an `AdtRegistry`
  entry. `Int↔Real` conversion (`↗`/`↘`, `SEMANTIC_CORE.md` §20.3) is
  also implemented. No canon stdlib surface remains undecided; further
  additions (a second collection type, more conversions, and so on)
  are new library-content questions, not gaps in what's named here.
- **Module/artifact/distribution model — resolved.**
  `ADR-017` defines an Obfusku Project (a directory rooted at a
  discovered project marker, with a bare `.obk` file remaining valid as
  a degenerate one-file Project); `ADR-018` generalizes module
  resolution to project-relative paths, superseding `ADR-006`'s
  same-directory-only restriction; `ADR-019` completes `ADR-011`'s
  filesystem scoping to the Project root, including symlink-escape
  containment; `ADR-020` fixes the marker's concrete format
  (`obfusku.toml`: `format`, `entry`, optional `name`, additive
  unknown-field tolerance within a supported `format`). `ADR-004`'s
  `RunResult` contract is confirmed unaffected. What remains genuinely
  open: any metadata the manifest might carry beyond `ADR-020`'s fields
  (dependency identity, versioning beyond `format`), external-Project
  dependency resolution, packaging/archive format for a distributed
  artifact, and `test`'s implementation — it needs a language-level
  testing construct that does not exist yet and is deliberately not
  invented as a CLI-level naming convention. `build` is
  implemented: it requires a real Project (no bare-file fallback,
  unlike `run`/`check`) and certifies the entry module's whole
  reachable import graph as a valid, distributable Source Artifact
  under `ADR-017`'s model — it resolves/parses/type-checks that graph
  exactly as `check` does, and writes nothing to disk, since no
  packaging/archive format exists yet to write.
- **Loop-vs-recursion idiom — resolved, non-normatively, in §6 below.**
  The tail-call guarantee itself (`SEMANTIC_CORE.md` §14.1) stays frozen
  and normative; §6 is style guidance for *which* recursive shape to
  reach for, not a change to what the language guarantees.
- **Toolchain, compiler architecture, parser implementation.** Explicitly
  out of scope for `spec/` by design — this specification defines the
  language, not how to build it.

---

## 6. Standard library iteration idiom

**Non-normative.** Nothing here changes any typing rule, evaluation
rule, or guarantee stated elsewhere in `spec/` — `SEMANTIC_CORE.md`
§14.1's tail-call guarantee applies identically to every shape named
below, since `List`'s combinators (`⟐`/`⌿`/`⌽`) are themselves ordinary
`LetRec`-recursive `.obk` source (`crates/obfusku-cli/src/stdlib.obk`),
not a faster or more primitive mechanism than writing the same
recursion by hand. This section exists only to say which already-legal
shape reads best for a given task, since the language now has three
different ways to traverse a collection and no stated guidance on
choosing between them.

**Prefer a `List` combinator (`⟐`/`⌿`/`⌽`) when the traversal is
exactly transform-each, keep-matching, or combine-to-one.** These are
the common shapes; naming the shape (`⟐(double, xs)`) is more legible
than re-deriving it via a hand-written `⟡`/`Cons`/`Nil` match every
time one is needed, and costs nothing in stack behavior — it *is* that
match, already written once and reused.

**Reach for direct recursion (`LetRec` + `⟡` pattern matching against
`Cons`/`Nil`, or against a user-defined recursive ADT) when the
traversal doesn't fit any single combinator shape** — multiple
accumulators tracked together, early termination before the structure
is exhausted, traversing two structures in lockstep, or building a
different structure than a straightforward map/filter/fold produces.
Forcing such a traversal through a composition of combinators (nested
`⟐`/`⌿`/`⌽` calls, throwaway intermediate lists) is not an error, but
direct recursion is usually the more honest expression of what the
code is actually doing — reads should not be a graph of function-call
shapes standing in for a hand-written traversal that would be shorter
and clearer.

**`Array` has no direct-recursion option at all** — `SEMANTIC_CORE.md`
§9.1 deliberately excludes `Array` from structural pattern matching, so
`⊡`/`⊟`/`⊞` (or, for a shape none of the three fit, an index-driven
helper function using `#` and `arr[i]` with a base case at
`i ≡ #(arr)`) are the *only* ways to consume one. This is not a gap
this section fills — it is `Array`'s own settled semantics
(`GLYPH_SYSTEM_DESIGN.md` §10.5), repeated here only so the choice
between `List` and `Array` includes it: **reach for `Array` when O(1)
positional access matters to the algorithm; reach for `List` when the
traversal is naturally inductive (`Cons`/`Nil`-shaped) and positional
access does not come up.** Neither is a general-purpose default the
other should be reached for out of habit.

**Nothing new is introduced by this section** — no `zip`, no `range`,
no general loop sugar, no additional combinator beyond `List`'s three
and `Array`'s five already-canon ones. A future gap found while
following this guidance (a shape common enough to deserve its own
combinator) is a separate, later stdlib-content decision, not an
implicit extension of this one.

---

## 7. For anyone implementing against this specification

**Implement the language defined by `spec/`.** If something needed to implement a parser
or evaluator is genuinely missing from `spec/` — not merely
under-explored, but actually absent — that is a defect in `spec/` to be
raised and fixed there, as the normative specification is the sole source of truth.
