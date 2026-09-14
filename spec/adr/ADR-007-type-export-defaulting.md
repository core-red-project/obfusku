# ADR-007 — Types and their ADT variants are always exported; there is no opt-out

## Status
Accepted

## Context
The concrete grammar previously claimed the export modifier applies uniformly across value, function, and type declarations — a claim never actually implemented; writing the export modifier on a type declaration was always a parse error. This surfaced while fixing a cross-module ADT pattern-matching defect, which made clear that a type's visibility and its variants' availability for construction and pattern-matching are one fact, not two independently toggled layers.

## Decision
Every type declaration and every one of its ADT variants is exported unconditionally. The export modifier is never written on a type declaration — there is no third site for it, and no opt-out mechanism.

## Alternatives Considered
A genuine per-type opt-in/opt-out export mechanism was rejected as unneeded scope expansion with no demonstrated need, requiring a new namespace/qualified-name concept not otherwise present. Continuing to claim uniform modifier applicability across all three declaration kinds while leaving types unconditionally exported underneath was rejected as leaving the specification text contradicted by the implementation.

## Consequences
Any future need for private (non-exported) types requires a new, separately designed decision — it cannot be retrofitted as simply extending the export modifier to type declarations. Type metadata and value-level type schemes must always propagate together through the same prelude mechanism.

## Related Specification
`spec/CONCRETE_SYMBOLIC_GRAMMAR.md` §8.4.
