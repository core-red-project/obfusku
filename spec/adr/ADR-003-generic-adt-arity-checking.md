# ADR-003 — Bare type references never infer type arguments; generic ADT arity is checked

## Status
Accepted

## Context
A bare named type reference always lowered to a zero-argument type application. A recursive generic ADT field written as a bare self-reference (e.g. a `List` field meant to mean `List<T>`) silently became a reference to the zero-arity type instead — a silent type-safety hole rather than a reported error.

## Decision
A bare type tag never implicitly infers type arguments. A recursive generic field must write the explicit type application. A type-arity check is enforced at ADT-registry construction time and at every declared-type annotation site, turning a wrong-arity reference into a reported static error.

## Alternatives Considered
Implicit arity inference for a bare self-reference inside its own recursive declaration was rejected: it would special-case "the type currently being declared" against every other named type reference, adding a narrow inference rule rather than closing a general gap, and would not cover wrong-arity references elsewhere.

## Consequences
Every generic ADT declaration that recurses on itself must spell the full type application explicitly. Any future generic-type feature must integrate with this arity-check mechanism rather than bypass it.

## Related Specification
`spec/SEMANTIC_CORE.md` §9.1; `spec/ABSTRACT_GRAMMAR.md` §2.3, §5.
