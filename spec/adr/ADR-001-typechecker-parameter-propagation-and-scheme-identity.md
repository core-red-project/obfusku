# ADR-001 — Typechecker parameter-type propagation and cross-Checker scheme identity

## Status
Accepted

## Context
Two independent typechecker correctness defects existed prior to a completeness audit. First, a `Lambda` parameter's type from a declared `FunctionDeclaration`/`LetRec` signature was never propagated into that parameter's own scope before its body was inferred, so any operator dispatch requiring an already-resolved operand type failed for ordinary two-argument annotated functions. Second, `TypeVarId` is a bare integer, meaningful only relative to the `Checker` instance that minted it; merging a `Scheme` produced by one `Checker` (stdlib, natives, or an imported module's own separate typecheck run) directly into another `Checker`'s environment could silently corrupt generalization when variable numbering collided across the two.

## Decision
Parameter types declared on a `FunctionDeclaration`/`LetRec` signature are peeled off and seeded into each nested `Lambda`'s own scope before its body is inferred, applied uniformly at both the module-level and local-`LetRec` sites. Separately, every externally-produced `Scheme` (from stdlib, natives, or an imported module) is normalized — its quantified variables rebound to the receiving `Checker`'s own fresh ones — at the single shared boundary where prelude environments are constructed, before insertion into the local `TypeEnv`. Generalization itself is left untouched; the fix is a boundary discipline on foreign input, not a change to how free-variable analysis works.

## Alternatives Considered
Patching generalization's environment scan to detect or avoid identifier collisions directly was rejected: it would entangle the correctness of a core algorithm with a foreign-input hygiene problem, and would not automatically cover any future source of prelude-shaped input. Giving `TypeVarId` global uniqueness via a shared counter across all `Checker` instances was also rejected, since it would require threading shared mutable counter state through every construction site for the same net effect as a single boundary-normalization function.

## Consequences
Every prelude-producing path must funnel through the same scheme-normalization boundary; a future additional source of externally-produced schemes must be wired through that same function rather than merging ad hoc. `TypeVarId`'s per-`Checker`-relative numbering remains true by design — any code that compares raw `TypeVarId`s across `Checker` instances outside of `Var`/`instantiate` is unsafe.

## Related Specification
`spec/SEMANTIC_CORE.md` §19 (generic instantiation), §19.1 (generalization and the value restriction). This decision is otherwise an implementation-only invariant with no direct normative spec section.
