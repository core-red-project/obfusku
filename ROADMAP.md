# Obfusku — Roadmap

**Status: project tracking, not normative.** This document is not part of
`spec/` and carries no authority over language semantics — `spec/LANGUAGE_SPEC.md`
§1 is explicit that only `spec/` says what the language *is*. This is a
snapshot of what has been built against what `spec/` currently commits to.
Expected to go stale; re-run periodically. Decision rationale lives in
`spec/adr/`, not here — this file only tracks status.

**Legend**:
- **Implemented** — built, tested, exercised through the real pipeline (or, for runtime-only concerns, through `obfusku-runtime`'s own isolated tests).
- **Partially implemented** — a real path exists but a documented boundary is missing.
- **Specified but missing** — `spec/` defines it; nothing in `crates/` reaches it yet.
- **Spec gap** — `spec/` doesn't yet answer the question.
- **Intentionally deferred** — `spec/LANGUAGE_SPEC.md` §5 names this as out of scope on purpose.

---

## Implemented / 1.0 contract

### Core semantics
- Var/Lit/Lambda/Apply, strict L2R evaluation — → `spec/SEMANTIC_CORE.md` §1
- Let / LetRec (self + mutual recursion) — → `spec/SEMANTIC_CORE.md` §11
- MutCell/MutRead/MutRebind, `Cell<T>` invariance, uniform capture — → `spec/SEMANTIC_CORE.md` §12, §12.1, §13
- Closures, lexical capture, shadowing — → `spec/SEMANTIC_CORE.md` §13
- TCO + `Catch` carve-out — → `spec/SEMANTIC_CORE.md` §14.1
- ADTs / constructors as ordinary functions — → `spec/SEMANTIC_CORE.md` §9
- Match/Pattern, full exhaustiveness (incl. record patterns) — → `spec/SEMANTIC_CORE.md` §10, §17
- Value restriction / letrec-group-aware generalization — → `spec/SEMANTIC_CORE.md` §19.1
- `Optional`/`Result` as ordinary user ADTs — → `spec/SEMANTIC_CORE.md` §9
- `Raise`/`Catch`, dynamic-extent resolution, built-in closed `Exception` ADT — → `spec/SEMANTIC_CORE.md` §15, §15.1, §15.2, §15.3 · `spec/CONCRETE_SYMBOLIC_GRAMMAR.md` §7.8 · `spec/adr/ADR-015-exception-closed-adt-and-catch-semantics.md`
- Arithmetic/comparison/equality/logical primitives, closed-table typing, total equality — → `spec/SEMANTIC_CORE.md` §18, §20.2
- Generic top-level function / local-`LetRec` signatures — → `spec/SEMANTIC_CORE.md` §19
- `List<T>` (incl. ambient-stdlib pattern matching) — → `spec/SEMANTIC_CORE.md` §9.1
- `Array<T>` (`ArrayLiteral`/`IndexExpr`), bracket syntax — → `spec/SEMANTIC_CORE.md` §9.1, §9.2 · `spec/adr/ADR-002-array-bracket-syntax.md`
- `run`'s reported value — → `spec/adr/ADR-004-runresult-contract.md`
- Typechecker parameter-type propagation and cross-`Checker` scheme identity — → `spec/adr/ADR-001-typechecker-parameter-propagation-and-scheme-identity.md`
- Generic ADT arity checking — → `spec/adr/ADR-003-generic-adt-arity-checking.md`
- `Int` overflow semantics (fixed-width, raises, never panics) — → `spec/adr/ADR-008-int-overflow-semantics.md`

### Surface grammar
- Literals, references, application, pipeline, nested scoping — → `spec/ABSTRACT_GRAMMAR.md` §3.3–3.4
- Argument-hole placeholder — → `spec/CONCRETE_SYMBOLIC_GRAMMAR.md` §7.2, §12.1, §13, §16.1
- `ValueDeclaration` + mutability/export modifiers + type annotation — → `spec/CONCRETE_SYMBOLIC_GRAMMAR.md` §8.1
- `FunctionDeclaration`, incl. function-type/type-application annotations — → `spec/CONCRETE_SYMBOLIC_GRAMMAR.md` §8.2, §9
- Anonymous lambda parameter type annotation — → `spec/CONCRETE_SYMBOLIC_GRAMMAR.md` §7.3
- Function export mark placement — → `spec/CONCRETE_SYMBOLIC_GRAMMAR.md` §8.4 · `spec/adr/ADR-005-export-mark-placement.md`
- ADT `TypeDeclaration`, sum/record/tuple bodies — → `spec/ABSTRACT_GRAMMAR.md` §2.3, §4 · `spec/CONCRETE_SYMBOLIC_GRAMMAR.md` §8.3
- `LocalBinding` (incl. local `FunctionDeclaration+` groups) — → `spec/SEMANTIC_CORE.md` §11 · `spec/CONCRETE_SYMBOLIC_GRAMMAR.md` §7.4
- Whole-module imports, cross-module ADT pattern matching — → `spec/CONCRETE_SYMBOLIC_GRAMMAR.md` §14 · `spec/adr/ADR-006-whole-module-imports.md`
- Type/ADT-variant export defaulting — → `spec/CONCRETE_SYMBOLIC_GRAMMAR.md` §8.4 · `spec/adr/ADR-007-type-export-defaulting.md`
- General `Type`/`TypeApplication`/`FunctionType` grammar — → `spec/CONCRETE_SYMBOLIC_GRAMMAR.md` §9
- NFC source normalization enforcement — → `spec/CONCRETE_SYMBOLIC_GRAMMAR.md` §2
- Glyph purity (no ASCII operator aliases) — → `spec/GLYPH_SYSTEM_DESIGN.md` §1 · `spec/adr/ADR-014-glyph-purity.md`
- No general loop construct, no `return`/early-exit form (by design) — → `spec/GLYPH_SYSTEM_DESIGN.md` §7.2

### Type system
- Hindley-Milner inference, unification, occurs check — → `spec/SEMANTIC_CORE.md` §3, §19
- Polymorphic instantiation/generalization, letrec-group-correct — → `spec/SEMANTIC_CORE.md` §19.1
- `Cell<T>` invariance — → `spec/SEMANTIC_CORE.md` §12.1
- ADT/constructor typing, exhaustiveness — → `spec/SEMANTIC_CORE.md` §17
- Duplicate top-level name rejection (static, not lexical shadowing) — → `spec/CONCRETE_SYMBOLIC_GRAMMAR.md` §8.2
- Type annotations checked against inferred types — → `spec/SEMANTIC_CORE.md` §1
- `Raise`/`Catch` typing rules — → `spec/SEMANTIC_CORE.md` §15.1

### Runtime
- Tree-walking evaluator, TCO trampoline (stack-safe to ~100k) — → `spec/SEMANTIC_CORE.md` §14.1
- Values: Int/Real/Str/Bool/Unit/Closure/Cell/Adt/List/Array — → `spec/SEMANTIC_CORE.md` §2, §9.1, §9.2
- `print`/`readLine` (ambient I/O, host boundary contract) — → `spec/SEMANTIC_CORE.md` §15.4 · `spec/adr/ADR-010-io-host-boundary.md`
- Unit value literal in expression position — → `spec/CONCRETE_SYMBOLIC_GRAMMAR.md` §5
- Standard library as ambient prelude (`List<T>`/`map`/`filter`/`fold`) — → `spec/adr/ADR-012-ambient-stdlib.md`

### Diagnostics
- Span/SourceMap, spans preserved parse→desugar→typecheck→runtime — → `spec/IMPLEMENTATION_ARCHITECTURE.md` §14
- Line/column resolution for rendering — → `spec/IMPLEMENTATION_ARCHITECTURE.md` §14

### Toolchain
- `run`/`check` pipeline as a library — → `spec/IMPLEMENTATION_ARCHITECTURE.md` §12
- CLI dispatch (`run`/`check`/`fmt`/`repl`/`inspect`/`version`) — → `spec/IMPLEMENTATION_ARCHITECTURE.md` §12
- REPL: per-line incremental evaluation, ambient stdlib + I/O, no effect replay — → `spec/adr/ADR-013-repl-incremental-evaluation.md`
- Formatter: surface-AST, pre-desugar, behavior-preserving round-trip — → `spec/IMPLEMENTATION_ARCHITECTURE.md` §13 · `spec/adr/ADR-009-formatter-architecture.md`

---

## Pending

- **Filesystem access (`readFile`/`writeFile`) — signatures specified, host wiring not yet built, entry-directory scoping policy blocked on the module/artifact model.** → `spec/SEMANTIC_CORE.md` §15.4 · `spec/adr/ADR-011-filesystem-scoping.md` · `spec/LANGUAGE_SPEC.md` §5

## Deferred (intentional)

- Module/artifact/distribution model (packaging, versioning, search paths beyond the entry directory) — → `spec/LANGUAGE_SPEC.md` §5
- Standard library contents/naming beyond `List`/`map`/`filter`/`fold` — → `spec/LANGUAGE_SPEC.md` §5
- LSP / editor tooling — never scoped in `spec/`
- `Array<T>` combinators (`map`/`filter`/`get`/`set`/`length`) — → `spec/CONCRETE_SYMBOLIC_GRAMMAR.md` §7.9 · `spec/SEMANTIC_CORE.md` §9.2
- Selective/aliased imports, re-exporting an imported name — → `spec/adr/ADR-006-whole-module-imports.md`
- Formatter comment/trivia preservation — → `spec/adr/ADR-009-formatter-architecture.md`
- REPL filesystem natives — no single entry file to scope against — → `spec/adr/ADR-013-repl-incremental-evaluation.md`

## Future / evaluated and declined

- Hand-rolled CLI argument parsing kept in place of adopting a dedicated parsing crate.
- Structured (`--format=json`) diagnostic output for `check`/`inspect` — no concrete consumer yet.
- Line-editing library for the REPL — a real nicety, declined to keep a minimal dependency footprint.
- Arbitrary-precision integers — would reverse `spec/adr/ADR-008-int-overflow-semantics.md`; not planned.
