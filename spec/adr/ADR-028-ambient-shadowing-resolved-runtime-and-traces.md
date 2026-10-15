# ADR-028 — Ambient names are shadowable; resolved runtime IR; call traces; native list combinators

## Status
Accepted

## Context
Every ambient name added to the standard library or the host natives was a name a program could no longer declare (`λ⟐` and `⌁↑ ≔ 5` were errors). The evaluator looked variables up by string through a chain of hash maps and allocated a tag string per constructed value. An uncaught exception named only the raise site, and `⟐`/`⌿` were recursive source functions that exhausted the stack on a million elements.

## Decision
**Shadowing.** A module-level declaration may reuse an *ambient value name* — a stdlib or native name — and replaces it for that module only. Not shadowable: a name an import re-binds (still `already bound`, `ADR-006`), a constructor tag (tied to its type), and ambient *type* names (`List`, `Optional`, `Result`, `Array`, `Map`, `Set`, `Exception`, `⌭`, `⌑`): types are nominal by name, so replacing one would silently retarget every native whose signature mentions it. These are the language's reserved type vocabulary. The type checker takes the shadowable set from its caller (`check_shadowing`); the REPL keeps the strict rule because its prelude mixes ambient names with the session's own declarations.

**Resolved runtime.** `obfusku-runtime` compiles each top-level binding from Core to an IR (`ir.rs`) in which a local is a `(frame depth, slot)` pair, a top-level or prelude name a shared cell, and a constructor tag an interned `Rc<str>`. Frames are slot vectors; a `Match` arm allocates one only when its pattern binds something. A name is resolved when its binding is compiled, so a later binding of the same name no longer reaches into an earlier closure. The Core AST, the type checker and every observable result are unchanged. Depth and trace state are passed explicitly instead of through thread-locals; the recursion limit is checked every 16 levels.

**Traces.** The evaluator keeps the live non-tail function activations. At an uncaught raise the innermost twelve become `Note` diagnostics (`called 'f' from here`) after the error, innermost first; a curried declaration carries its declared name on every link, an anonymous function appears as `λ`. A raise that is caught leaves no trace.

**List combinators.** `⟐ ⌿ ⌽` are host natives (like the `Array` ones, `ADR-021`) iterating in constant stack space instead of recursive source.

## Alternatives Considered
Making ambient types shadowable by namespacing them per module was rejected as a type-identity redesign for no program the reserved vocabulary blocks in practice. A bytecode VM was not attempted: the measured costs were lookups and allocation, which the resolved IR removes; instruction dispatch was not the bottleneck. A mutable per-call stack of frames for traces was rejected in favor of the activation list because a tail call must replace, not extend, its caller.

## Consequences
Measured on the release build against the previous evaluator: a 3 M-iteration tail loop 0.98 s → 0.40 s, a 300 k-element list pipeline 0.87 s → 0.19 s, a million record updates 0.64 s → 0.38 s, `fib(30)` 0.52 s → 0.24 s; `⟐`/`⌿`/`⌽` over one million elements 0.8 s with no stack growth. The `Value` type is still 40 bytes; shrinking it is the next measurable lever. A shadowed ambient name makes the standard function inaccessible by that name in that module. Traces are capped at twelve frames and record call sites, not argument values.

## Related Specification
`spec/adr/ADR-012-ambient-stdlib.md`; `spec/SEMANTIC_CORE.md` §13, §14.1, §20; `spec/adr/ADR-021-array-combinators-are-pure-natives.md`.
