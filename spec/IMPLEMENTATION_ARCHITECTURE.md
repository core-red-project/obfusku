# Obfusku — Implementation Architecture

Status: **established.** This is the current Rust workspace layout, not a
proposal — the crate boundaries and dependency graph below match
`crates/` as built.

---

## 1. Purpose and architectural principles

This document describes the Rust workspace that implements Obfusku,
derived from `spec/` — not from `src/`.

```
spec → architecture → implementation
```

never

```
old src → new architecture → language
```

Every crate boundary proposed below is justified by an actual dependency
or compilation-isolation reason, tested explicitly with "does anything
need X without needing Y" — not by "it's a different responsibility."
Where a plausible-sounding boundary failed that test, it's rejected below
and the rejection is shown, not just the surviving design.

**The old implementation is a migration source, not an architectural
dependency.**

---

## 2. Relationship between `spec/` and the implementation

```
spec/      — normative. The implementation conforms to this, full stop.
crates/    — the implementation, designed from spec/ alone.
```

The specification is authoritative; the implementation is subordinate.
If a difference exists between them, spec/ wins and crates/ must change.
See `spec/LANGUAGE_SPEC.md` §1.


---

## 3. Repository tree

```
obfusku/
├── Cargo.toml                — workspace manifest, members = crates/*
├── Cargo.lock
├── crates/
│   ├── obfusku-diagnostics/
│   ├── obfusku-core/
│   ├── obfusku-typecheck/
│   ├── obfusku-syntax/
│   ├── obfusku-runtime/
│   ├── obfusku-fmt/
│   └── obfusku-cli/
└── spec/
```


---

## 4. Cargo workspace design

```toml
[workspace]
members = ["crates/*"]
resolver = "2"
```

Each crate under `crates/` is a normal library crate except
`obfusku-cli`, which is the workspace's sole binary. Standard, boring
Rust project layout throughout — no custom build orchestration, no
`xtask` pattern, no workspace-level proc-macros — per the standing
instruction to prefer convention over cleverness, and because nothing
proposed here needs anything more elaborate.

---

## 5. Crate responsibility matrix

| Crate | Owns | Depends on |
|---|---|---|
| `obfusku-diagnostics` | `Span`, `SourceMap`, `Diagnostic`, severity levels | *(none)* |
| `obfusku-core` | Core AST types (`SEMANTIC_CORE.md` §1's grammar), `Type` vocabulary, kind bookkeeping | `obfusku-diagnostics` |
| `obfusku-typecheck` | Inference, unification, the value restriction (§19.1), exhaustiveness (§17), equality-generation validity (§18) | `obfusku-core`, `obfusku-diagnostics` |
| `obfusku-syntax` | Lexer, parser, surface AST (`ABSTRACT_GRAMMAR.md` shapes), desugaring to Core AST, mutation-capture static checks (§13) | `obfusku-core`, `obfusku-diagnostics` |
| `obfusku-runtime` | Value representation, evaluator (§8 below), `Cell`, `Raise`/`Catch` unwinding, the TCO trampoline | `obfusku-core`, `obfusku-diagnostics` |
| `obfusku-fmt` | Formatter, consuming surface AST + token trivia | `obfusku-syntax`, `obfusku-diagnostics` |
| `obfusku-cli` | Argument parsing, pipeline orchestration per subcommand, diagnostic rendering | all of the above |

**For every crate, "why is this a crate," stated as a dependency or
compilation-isolation fact, not a label:**

- **`obfusku-diagnostics`**: small and foundational on purpose — nearly
  everything depends on it, so isolating it keeps that shared surface
  minimal and rarely-changing, which means it almost never forces a
  rebuild of everything above it.
- **`obfusku-core`**: the one thing every other crate needs a stable view
  of. Keeping it dependency-light (only `diagnostics`) is what lets
  `obfusku-syntax` and `obfusku-runtime` be siblings rather than a chain
  (§6).
- **`obfusku-typecheck` split from `obfusku-core`, not merged into it**:
  tested directly — does `obfusku-runtime` need the type-checking
  *algorithm* (unification, generalization) to execute an already-checked
  program? No, only the AST/type *types*. Bundling them would force
  `obfusku-runtime` to compile the entire inference engine for no reason
  every time it changes. Splitting them is what makes test question #2
  (§20) true architecturally, not just procedurally.
- **`obfusku-syntax`**: the concrete boundary for "can this be tested
  without the CLI" and "can an LSP reuse the frontend" (test questions #1
  and #4, §20) — a real, explicitly-requested reuse case, not a
  hypothetical one.
- **`obfusku-runtime`**: isolated specifically so the execution *strategy*
  (§8) can change later without touching `obfusku-typecheck` or
  `obfusku-syntax` — neither depends on it, and it doesn't depend on
  either of them.
- **`obfusku-fmt`**: minimal dependency footprint (`syntax` +
  `diagnostics` only) is the whole point — a future LSP's format-on-save
  should not need to pull in type-checking or execution to format a file
  that might not even type-check yet.
- **`obfusku-cli`**: the only crate allowed to see the whole graph, and
  the only reason it's a separate crate from everything else is that it's
  the binary — Rust requires that boundary regardless.

**Where I deliberately did *not* split further**, and why:

- **Lexer and parser stay one crate** (inside `obfusku-syntax`, as
  modules, not separate crates). Tested: does anything need tokens
  without ever wanting a parse tree? Only a hypothetical
  syntax-highlighter, and that's served fine by a `pub mod lexer` inside
  `obfusku-syntax` — a full crate boundary buys nothing here, since the
  parser recompiles with every lexer change anyway (they're inseparably
  coupled), so splitting them wouldn't even improve incremental
  compilation.
- **Surface AST types stay inside `obfusku-syntax`, not their own
  crate.** Nothing outside `obfusku-syntax` and `obfusku-fmt` needs
  surface AST without also needing the parser that produces it — unlike
  Core AST (§6), which genuinely has independent consumers.
- **No standalone `obfusku-stdlib` crate yet** — `spec/LANGUAGE_SPEC.md`
  §5 states stdlib content is undecided; inventing a crate boundary for
  an undesigned thing would be a premature commitment. Noted as deferred
  in §15 and §20, not solved here.

---

## 6. Dependency graph and dependency direction

```
                    obfusku-diagnostics
                     ↑        ↑       ↑
              obfusku-core    │       │
               ↑      ↑       │       │
   obfusku-typecheck  │       │       │
               ↑      obfusku-syntax  │
               │       ↑    ↑         │
               │       │  obfusku-fmt │
               │       │              │
               │   obfusku-runtime ───┘
               │       ↑
               └───────┴──── obfusku-cli
```

The load-bearing fact this diagram exists to show: **`obfusku-syntax` and
`obfusku-runtime` are siblings, not a chain.** Both depend on
`obfusku-core`; neither depends on the other. By the time a program is
executing, it has already been fully desugared into Core AST — the
runtime has no reason to know anything about surface syntax at all. This
wasn't the obvious first guess (a naive pipeline diagram would draw
syntax → typecheck → runtime as one straight line, implying runtime
depends on syntax) — it only became clear by asking the dependency
question directly rather than following the pipeline's *execution* order,
which is not the same as its *dependency* order.

`obfusku-cli` is the only crate with in-edges from nowhere and out-edges
to everywhere — exactly the "CLI at the edge" property the brief asked
for, verified structurally rather than just stated.

---

## 7. Frontend architecture (`obfusku-syntax`)

Lexer → recursive-descent parser with Pratt-style precedence climbing for
`CONCRETE_SYMBOLIC_GRAMMAR.md` §13's table → surface AST (spans attached
to every node) → desugaring pass producing Core AST.

The desugaring pass also performs the required static error for
`MutRebind` on a mut-bound free variable a closure did not explicitly
`˚`-capture (§12/§13 of `SEMANTIC_CORE.md`) — a genuine correctness gate,
not a lint. This runs here, over *surface* AST, rather than in
`obfusku-typecheck`: by the time a `Lambda` reaches Core AST, whether a
given reference used default or explicit capture syntax is already
resolved into which Core form it lowered to (`MutRead(Var(name))` vs. a
bare `Var(name)`) — the distinction the check needs is a surface-syntax
fact, not something the type checker (Core-AST-only, §8) has left to
reconstruct.

Recommending hand-written recursive descent over a parser-generator or
combinator library: `CONCRETE_SYMBOLIC_GRAMMAR.md` §16's ambiguity
resolutions (declaration-boundary lookahead, pipe-stage shape
classification, case-based `Tag`/name disambiguation) are bespoke,
spec-specific rules, not generic grammar patterns — a hand-written parser
lets each one map to one identifiable function citing the spec section it
implements, which matters more here for auditability than a
grammar-generator's implicit conflict resolution would help with
generality. This is a leaning, not a hard architectural boundary — it
doesn't affect crate structure.

**Spans survive desugaring; they are propagated, never fabricated.** A
surface `Conditional` desugaring to a Core `Match` (per
`CONCRETE_SYMBOLIC_GRAMMAR.md` §7.5) produces a `Match` node whose span is
the original `if` expression's full span — so a type error against the
desugared form still points at what the user actually wrote. This is the
concrete mechanism behind "the parser should not need to know about the
CLI, the CLI should not need to reconstruct parser locations": spans are
data attached to AST nodes from the moment of parsing onward, not
something reconstructed downstream.

---

## 8. Semantic/type-checking architecture (`obfusku-typecheck`)

Operates over Core AST (`obfusku-core` types) only — never sees surface
syntax. Implements, each traceable to its spec section:

- Unification-based inference with the value restriction (§19.1) —
  generalization eligibility is a syntactic check on the Core AST node
  shape (`Lambda`/`Lit`/`Var`/value-only `Constructor`), not a heuristic.
- Kind bookkeeping (§4) — internal to the checker, never surfaced as a
  public type or diagnostic-visible concept, matching the Core's own
  requirement that kinds stay invisible.
- Exhaustiveness (§17), including the closed/open type distinction,
  checked against `TypeDeclaration` structure.

Public surface: one entry point taking a Core `Module` and returning
either a typed program or a list of `Diagnostic`s. Internals (the
unification algorithm's data structures) are private — satisfying test
question #7 (§20): the checker's internals can evolve freely as long as
that one signature holds.

---

## 9. Core representation (`obfusku-core`)

Data and type vocabulary only — **no parsing logic, no execution logic,
no glyphs.** This mirrors `SEMANTIC_CORE.md`'s own "zero concrete glyphs"
property deliberately (§9's architectural invariant, §19): a contributor
should be able to read this crate with no knowledge of Obfusku's surface
syntax and still understand the language's semantics, the same way the
spec document itself is readable that way.

Where Rust's type system can directly enforce a spec rule, it should —
this is cheaper and more reliable than a runtime check. Concretely: a
`LetRecBinding` type that can only hold a `Lambda`, not any `Expr`, makes
`SEMANTIC_CORE.md` §11's restriction a compile-time fact about the AST
representation rather than a check some other pass has to remember to
run. Every public type in this crate is expected to doc-comment the exact
spec section it realizes.

---

## 10. Compilation/execution architecture

**Recommendation: a Core-AST tree-walking evaluator with an explicit
trampoline for tail positions — not bytecode + a VM, at least not as the
initial design.** Argued from spec requirements, not from what the old
implementation had:

- `SEMANTIC_CORE.md` §1 already defines the Core AST with a direct,
  per-node evaluation rule for every form. Evaluating that AST directly
  is the shortest, most auditable path from spec to conformance — each
  evaluation rule in the document becomes one match arm in the
  interpreter. A bytecode lowering pass would add a second translation
  layer between spec and execution, which is exactly where the old
  implementation's real bugs lived (unenforced arity, unreliable
  `finally` — both plausibly artifacts of the compiler's lowering not
  perfectly preserving source-level semantics, not of the VM itself being
  a bad idea in general).
- The TCO guarantee (`SEMANTIC_CORE.md` §14.1) does not require bytecode
  to satisfy. A tree-walking evaluator can guarantee it directly: the
  Rust-level `eval` function represents tail positions (`Lambda` body,
  `Let`/`LetRec` body, `Match` arm result — exactly §14.1's list) as an
  explicit loop rather than a recursive Rust call, giving O(1) Rust stack
  growth for tail-recursive Obfusku programs. The §14.1 `Catch` carve-out
  maps directly onto this mechanism: inside an active `Catch`'s dynamic
  extent, the evaluator uses an ordinary (non-trampolined) recursive call
  so the `Catch` frame stays on the real Rust call stack — ordinarily
  needed for the exception unwinding it exists to support anyway. This
  isn't a workaround; it's a direct realization of the Core's own stated
  boundary, evidence the evaluator design isn't ad hoc.
- This is **not** a rejection of bytecode + VM as inherently wrong — the
  old VM is legitimate evidence that the approach can work for this
  language shape. It's a statement that a typed Core AST — which the old
  implementation never had — changes the calculus: with a stable
  intermediate representation already in hand, a tree-walker over it is
  simpler and no less capable of meeting every frozen guarantee.

**The decision is deliberately revisable without an architectural
rewrite**: `obfusku-runtime`'s public API is one narrow entry point
(`evaluate(program: &core::Module) -> Result<Value, RuntimeError>`) —
whole-module only. An earlier draft of this section anticipated a second,
incremental variant specifically for the REPL; that was never built, and
§12's own row now states why (resolved by deviation instead — the
shipped `repl` re-runs its whole growing buffer per accepted line through
this same whole-module entry point, needing no second API at all). Nothing
outside this crate observes whether `evaluate` is backed by a tree-walker
or, later, a bytecode VM. If performance ever becomes a stated requirement
(it isn't one now — nothing in `spec/` asks for it), swapping the internal
strategy touches only this crate.

---

## 11. Runtime/VM boundary (`obfusku-runtime`)

Owns: the `Value` representation (distinct from `obfusku-core`'s `Type`
vocabulary — values only exist during execution, types don't), `Cell`
(§12.1's mutable cell, with its identity/aliasing semantics), the
`Raise`/`Catch` unwinding mechanism (dynamic-extent resolution, §15.1),
and the TCO trampoline (§10 above).

**Explicit invariant**: `obfusku-runtime` assumes its input `Module` has
already passed `obfusku-typecheck`. It does not depend on
`obfusku-typecheck` and does not re-derive types — this is a stated
precondition, not something the runtime enforces itself. Calling
`obfusku-runtime` directly on an untyped or ill-typed program is a
caller error, not a runtime-detected one. This keeps the sibling
relationship with `obfusku-syntax` (§6) real rather than nominal.

---

## 12. CLI boundary (`obfusku-cli`)

Pure orchestration and presentation — no language logic of its own.

| Command | Pipeline |
|---|---|
| `run` | `syntax::parse_and_desugar` → `typecheck::check` → `runtime::evaluate` → print. The printed value currently uses Rust's derived `Debug` representation of `runtime::Value`; this is an implementation convention, not a stable display format or a compatibility contract. |
| `check` | `syntax::parse_and_desugar` → `typecheck::check` → report diagnostics, no execution |
| `fmt` | `syntax::parse` (surface only, desugaring not needed) → `fmt::format` |
| `repl` | **settled by deviation from an earlier plan, not built as originally sketched here (§20 item 4)**: rather than a persistent `runtime::Environment` behind a new incremental-evaluation entry point, each accepted line is appended to a growing buffer, invisibly sealed, and the whole buffer is re-run from scratch through the same whole-module `syntax::parse_and_desugar` → `typecheck::check` → `runtime::evaluate` pipeline every other command already uses — a line that fails to parse/type-check is reported and dropped without being committed, so one bad line can't corrupt the session. No incremental API was added to `obfusku-runtime` or `obfusku-typecheck`. **Current limitation, a direct consequence of that same deviation, not a separate decision**: the REPL does not currently provide the ambient stdlib/I/O-native environment `run`/`check` seed via `stdlib::load`/`natives::load` (§15). Evaluation is eager (§14), and this pipeline re-evaluates the *entire* accumulated buffer on every submission — a native with an observable effect (`print`, `readLine`, `writeFile`, …) bound into a previously-committed line would therefore re-fire that effect on every later submission, not just once. Stdlib's own combinators are pure, so re-evaluating them is harmless and their absence here is a separate, unrelated scoping choice (§15); I/O natives are not, and providing them under the current whole-buffer-rerun model would be observably wrong, not merely untested. This is a statement about the current pipeline, not a permanent guarantee that the REPL will never have I/O — an evaluation model that avoids replaying committed effects would resolve it, and is unbuilt future work, not a defect in what's shipped now. Like `run`, its printed value uses `Value`'s derived `Debug` representation — an implementation convention, not a stable display format. |
| `inspect` | exposes intermediate representations (`--tokens`, `--ast`, `--core`) via debug-printable types already public in `obfusku-syntax`/`obfusku-core` |
| `build`, `test` | **explicitly unresolved** — both depend on the artifact/module distribution model `spec/LANGUAGE_SPEC.md` §5 already flags as undecided; not invented here |
| `version` | crate version metadata only |

No crate other than `obfusku-cli` may depend on it — this is what makes
"can the CLI be replaced without rewriting the language implementation"
(test #6) true by construction rather than by discipline.

---

## 13. Formatter architecture (`obfusku-fmt`)

Consumes surface AST plus token trivia (comments, original layout intent)
from `obfusku-syntax` — deliberately *before* desugaring, since
desugaring discards exactly the surface-level shape (whether something
was written as `if` or spelled-out `Match`, pipe-chain layout) that
canonical formatting needs to preserve or normalize. No dependency on
`obfusku-core`, `obfusku-typecheck`, or `obfusku-runtime` — formatting a
file that doesn't type-check must still work, since `fmt` is frequently
run on in-progress, currently-broken code.

---

## 14. Diagnostics/source-span architecture (`obfusku-diagnostics`)

`Span` (byte-range + `SourceId`), `SourceMap` (byte offset → line/column,
for rendering), `Diagnostic` (severity, primary span, message, optional
secondary spans/notes). No dependency on any other crate — every other
crate depends on this one, not the reverse, keeping the shared vocabulary
stable and rarely-changing.

---

## 15. Standard-library boundary

Deliberately underdesigned here, matching `spec/LANGUAGE_SPEC.md` §5's
own admission that stdlib naming/content is undecided. One architectural
note worth recording now rather than losing: given `spec/`'s own
ten-algorithm test found most collection operations (`map`/`filter`/
`fold`) are ordinary combinators, not language primitives, **most of the
standard library plausibly doesn't need to be Rust code at all** — it can
be Obfusku source, loaded as an implicit prelude module by
`obfusku-runtime`/`obfusku-cli`, going through the exact same
parse/typecheck/evaluate pipeline as user code. Only genuinely primitive
operations (raw `Array` indexing, I/O, `Cell` primitives) need native
Rust "intrinsic" hooks inside `obfusku-runtime`. Not decided here —
flagged as a real, favorable-looking option for whoever designs the
stdlib boundary next.

---

## 16. Test architecture

One tier in practice, standard Rust convention (per "prefer stable,
conventional structure over cleverness"), not the two originally
sketched here — no top-level `tests/` directory was ever built, and
whole-pipeline conformance is proven from inside the workspace instead
of a separate fixture-file tree:

- **Crate-local tests** (`crates/*/tests/`, and `#[cfg(test)]` unit tests
  inline): each crate proves its own public API against its own slice of
  `spec/` — `obfusku-syntax`'s tests cite `CONCRETE_SYMBOLIC_GRAMMAR.md`
  sections, `obfusku-typecheck`'s cite `SEMANTIC_CORE.md` sections, etc.
- **Whole-pipeline (parse → typecheck → evaluate) conformance** lives in
  `crates/obfusku-cli/tests/` (`e2e.rs` against `obfusku_cli`'s library
  functions; `cli.rs` against the real compiled binary via
  `std::process::Command`) rather than a separate top-level fixture
  tree — real `.obk` source strings inline in each test, not fixture
  files, asserting the specific runtime value/diagnostic produced, not
  just "this succeeds/fails."

**Invalid-program cases** (`CONCRETE_SYMBOLIC_GRAMMAR.md` §17's examples
and their kind) are ordinary tests in the same crate-local/whole-pipeline
tiers above, asserting the *specific* diagnostic produced, not just
"this fails to compile" —
matching the standing project-wide discipline of never leaving behavior
merely "an error occurs" without specifying which one.

Not adopting benchmarks at this stage — nothing in `spec/` states a
performance requirement to benchmark against, and adding a `benches/`
directory speculatively would be exactly the "tooling because it's
fashionable" the brief warns against. Revisit if and when a real
performance question exists.

---

## 17. Developer experience conventions

- **Crate naming**: `obfusku-<responsibility>`, kebab-case, matching §5's
  matrix exactly.
- **Type naming**: Rust types in `obfusku-core`/`obfusku-syntax` should
  match spec vocabulary directly (`Expr`, `Pattern`, `LetRec`, `MutCell`
  as literal names) — a contributor grepping a spec term should land on
  its Rust realization.
- **Documentation discipline**: every public type/function in
  `obfusku-core`, `obfusku-typecheck`, `obfusku-syntax`, and
  `obfusku-runtime` doc-comments the exact `spec/` section it implements.
  This is the primary documentation mechanism, more valuable here than
  generic prose.
- **Error types**: each crate defines its own strongly-typed error enum
  (`SyntaxError`, `TypeError`, `RuntimeError`), convertible into
  `obfusku_diagnostics::Diagnostic` for rendering — never stringly-typed
  internally, since a future LSP needs structured errors, not formatted
  text.
- **Test naming**: `<spec_section>_<behavior>`, e.g.
  `semantic_core_11_letrec_requires_lambda_bindings`.
- **Fixtures**: no separate `tests/fixtures/` tier — settled by
  deviation (§16), test source lives inline as string literals in each
  test. The `.obk` file extension is settled in practice: every
  file-based test, `obfusku-cli::modules`' import resolution
  (`<ModuleName>.obk`), and every real fixture the CLI reads all use it,
  though `CONCRETE_SYMBOLIC_GRAMMAR.md` itself still never names one
  explicitly.
- **Feature flags**: none proposed; add only when a real, stated need
  arises (e.g., an optional `serde` feature for AST serialization, if a
  future tool needs it — not now).
- **Generated code**: none planned. No proc-macros, no `build.rs` codegen.
- **Dependency policy**: standard library first; add a well-established
  external crate only for a concrete, named need (e.g., `rustc-hash` for
  the symbol table, an established pattern in the old code worth keeping
  on its own merits, not because it's already there).

---

## 18. Migration/reuse policy

Every piece of legacy material — algorithm, test case, error case,
low-level utility, historical example — passes one gate before entering
the new implementation:

```
Does it conform to the current spec?
```

Not "is it close," not "it mostly works," not "it would take effort to
redo." If a legacy test case exercises behavior `spec/` explicitly
changed (e.g., the old exhaustiveness-optional matching, or `finally`
semantics — both gone per `SEMANTIC_CORE.md`), it does not migrate, full
stop, regardless of how well-written it is. Reused material is
re-justified at the point of reuse (in the PR/commit that reuses it), not
grandfathered in by having existed.

---

## 19. Architectural invariants

1. Dependency direction is exactly: `diagnostics` ← `core` ←
   `{typecheck, syntax, runtime}` ← (`fmt` depends on `syntax` only) ←
   `cli`. No cycles.
2. `obfusku-core` contains no glyphs, no parsing logic, no execution
   logic — data and typing rules only.
3. `obfusku-runtime` never depends on `obfusku-syntax` or
   `obfusku-typecheck`.
4. `obfusku-fmt` never depends on `obfusku-core`, `obfusku-typecheck`, or
   `obfusku-runtime`.
5. Only `obfusku-cli` may depend on every other crate; no crate may
   depend on `obfusku-cli`.
6. The workspace crates form a clean DAG with zero external or legacy dependencies.
7. Every public type in `obfusku-core`/`obfusku-syntax`/
   `obfusku-typecheck`/`obfusku-runtime` doc-comments the exact spec
   section it realizes.
8. Spans are propagated through desugaring, never fabricated downstream.
9. **The normative specifications and ADRs are the sole architectural authority.**
10. No execution-strategy commitment (tree-walk vs. future VM) is
    observable outside `obfusku-runtime`'s public API.

---

## 20. Open architectural questions

1. **Source file extension** — still not named anywhere in
   `CONCRETE_SYMBOLIC_GRAMMAR.md` itself, but settled in practice as
   `.obk` (see §17's "Fixtures" note) — used unconditionally throughout
   the shipped CLI and module resolution; revisit only if a reason to
   change it ever surfaces.
2. **Standard library boundary and naming** — §15's "mostly Obfusku
   source, not Rust" idea is a lead, not a decision.
3. **`build`/`test` CLI commands** — blocked on the artifact/module
   distribution model, itself blocked on decisions `spec/LANGUAGE_SPEC.md`
   §5 already names as pending.
4. **REPL's incremental-evaluation API shape** on `obfusku-runtime` —
   originally sketched as a requirement in §11/§12, not designed in
   detail; resolved by deviation instead, now stated directly in §12's
   own row — the shipped `repl` command re-runs its whole growing buffer
   per line through the ordinary whole-module pipeline rather than adding
   an incremental API to `obfusku-runtime`.

None of these block executing §3's repository restructuring or beginning
crate scaffolding — each is a follow-on decision, not a foundational gap.

---

## Final architecture test — answered explicitly

1. **Parser tested without the CLI?** Yes — `obfusku-syntax` has no
   dependency on `obfusku-cli` at all; crate-local tests exercise it
   directly.
2. **Type checker tested without the runtime?** Yes —
   `obfusku-typecheck` depends only on `obfusku-core`/`diagnostics`; this
   is exactly why it was split out from `obfusku-core` rather than merged
   (§5).
3. **Formatter consume syntax without depending on execution?** Yes —
   `obfusku-fmt` depends only on `obfusku-syntax`/`diagnostics` (§13).
4. **Future LSP tooling reuse the frontend?** Yes — depend on
   `obfusku-syntax` (plus `obfusku-typecheck` for type-aware features)
   directly, with no `obfusku-cli`/`obfusku-runtime` pulled in.
5. **Runtime execute programs without knowing CLI concerns?** Yes —
   `obfusku-runtime` has zero CLI dependency (§11).
6. **CLI replaced without rewriting the language implementation?** Yes —
   it's pure orchestration over public APIs; nothing depends on it (§12).
7. **Compiler internals evolve without changing the language's public
   API?** Yes — each crate exposes one narrow public entry point; internal
   algorithms (unification, evaluation strategy) are private (§10, §11).
8. **Old implementation deleted without changing the new architecture?**
   Yes — historical implementations were decoupled and deleted; the current architecture stands entirely on its own.
9. **Tests prove behavior directly against `spec/`?** Yes — the
   spec-section-citing test-naming and doc-comment conventions, plus
   whole-pipeline conformance tests against the real compiled binary
   (§16, §17).
10. **Contributor understands where a new feature belongs?** Yes, and
    concretely: adding a new built-in operator touches `obfusku-core`
    (if a new AST/type node is needed), `obfusku-syntax` (lexer/parser/
    desugaring), `obfusku-typecheck` (typing rule), and
    `obfusku-runtime` (evaluation rule) — traceable by asking "which
    `spec/` document defines this," since each crate maps to a specific
    subset of `spec/` by design, not by convention that could drift.

All ten pass. No boundary needs revisiting before this becomes the basis
for the next, code-writing step.
