# ADR-005 — Function export mark position: `→⟳`, after the return-type arrow

## Status
Accepted

## Context
Three normative sources disagreed on where a `FunctionDeclaration`'s export mark attaches: an early glyph-vocabulary sketch placed it before the parameter list, the concrete grammar's rule text placed it after the declaration's arrow, and the concrete grammar's own worked examples placed it before the binding operator — a genuine three-way contradiction.

## Decision
Since `FunctionDeclaration` has no binding operator of its own (a recursive binding is always a lambda, never a mutable cell), the canonical export mark is `→⟳`, immediately after the return-type arrow. The earlier sketch and the inconsistent worked examples were amended in place to match.

## Alternatives Considered
Treating the disagreement as unresolvable without new sign-off was rejected — the specification's own reading-order and amendment conventions already supply a deterministic resolution for exactly this situation. Accepting two spellings for the same concept was rejected — the superseded form is rejected outright, not accepted as an alternate syntax.

## Consequences
Any future modifier on `FunctionDeclaration` should attach after the return-type arrow, following this established position, rather than reopening the placement question.

## Related Specification
`spec/CONCRETE_SYMBOLIC_GRAMMAR.md` §8.4, §18; `spec/GLYPH_SYSTEM_DESIGN.md` §4; `spec/LANGUAGE_SPEC.md` §3, §4.
