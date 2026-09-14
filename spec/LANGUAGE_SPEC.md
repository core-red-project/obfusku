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

- **Standard library contents and naming.** `GLYPH_SYSTEM_DESIGN.md` §6.2
  establishes that generic wrapper types (`Array`, `Optional`, `Result`,
  `Cell`, `List`, `Exception`) are ordinary spelled names, not sigils, but
  the library's actual surface vocabulary is undecided.
- **Module/artifact/distribution model.** `DESIGN_VISION.md` §10 defers
  the CLI and what a compiled/packaged Obfusku artifact actually is;
  `ABSTRACT_GRAMMAR.md` §20 and `CONCRETE_SYMBOLIC_GRAMMAR.md` §14 define
  module *syntax* only, not packaging or distribution.
- **Loop-vs-recursion idiom and exact tail-call scope beyond what
  `SEMANTIC_CORE.md` §14.1 already guarantees** — the guarantee itself is
  frozen and normative; broader stdlib-level iteration idiom is not yet
  written.
- **Toolchain, compiler architecture, parser implementation.** Explicitly
  out of scope for `spec/` by design — this specification defines the
  language, not how to build it.

---

## 6. For anyone implementing against this specification

**Implement the language defined by `spec/`.** If something needed to implement a parser
or evaluator is genuinely missing from `spec/` — not merely
under-explored, but actually absent — that is a defect in `spec/` to be
raised and fixed there, as the normative specification is the sole source of truth.
