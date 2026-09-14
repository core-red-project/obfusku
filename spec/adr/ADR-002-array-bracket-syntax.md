# ADR-002 — Array literals use a dedicated bracket pair, not reused parentheses

## Status
Accepted

## Context
The concrete grammar originally recognized exactly two list/bracket shapes. Introducing `Array<T>` literals required deciding whether to reuse the existing `( ... )` production (already used for tuple construction) or introduce a new bracket pair.

## Decision
`[ ... ]` is a genuine third bracket family (`ArrayLiteral`), added as an explicit amendment rather than an implicit extension of the prior two-shape rule.

## Alternatives Considered
Reusing `( ... )` for array literals was rejected: it is lexically identical to tuple construction, and disambiguating the two would require a general expected-type-propagating inference mode the typechecker does not otherwise have — out of proportion for this feature.

## Consequences
The grammar now has three structurally distinct enclosure families — tuples, records/blocks, and arrays. Any future collection-like literal must fit one of these three or justify a fourth bracket pair with equivalent rigor.

## Related Specification
`spec/CONCRETE_SYMBOLIC_GRAMMAR.md` §6, §7.9; `spec/SEMANTIC_CORE.md` §9.1, §9.2.
