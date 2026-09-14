# ADR-009 — Formatter operates on the surface AST, before desugaring, with no comment preservation

## Status
Accepted

## Context
A formatter must work on ill-typed or in-progress code, which means it cannot depend on the desugaring, typecheck, or runtime layers, and must operate before desugaring rather than after.

## Decision
The formatter operates strictly on the surface AST, with no dependency on the semantic crates, produces one fixed canonical style (not configurable), and does not preserve comments — the lexer discards comment trivia entirely, with no surface-AST slot to reattach it to.

## Alternatives Considered
Formatting from the desugared Core AST was rejected: it would fail or behave nonsensically on code that does not yet type-check, defeating the purpose of a formatter usable during editing. Retrofitting comment-trivia capture in the same pass was rejected as its own, separately-scoped multi-crate feature (lexer capture, surface-AST attachment, formatter interleaving), not undertaken speculatively alongside unrelated work.

## Consequences
Any future comment-preservation feature requires coordinated lexer, surface-AST, and formatter changes, not a formatter-local patch. The formatter's correctness bar is behavioral equivalence of the program before and after formatting, not merely re-parseability.

## Related Specification
`spec/IMPLEMENTATION_ARCHITECTURE.md` §13; `spec/CONCRETE_SYMBOLIC_GRAMMAR.md` §8.1.
