# ADR-012 — The standard library prelude is ambient, not explicitly imported

## Status
Accepted

## Context
The standard library's contents and naming were left open, but implementing the first real slice (the built-in list type and its combinators) required resolving whether a program must explicitly import the standard library or receive it automatically.

## Decision
The standard library is spliced ambiently into every module's top-level surface via the same prelude mechanism the import resolver already uses, with no explicit import required.

## Alternatives Considered
Requiring an explicit import for standard-library access was rejected as making the most basic operations require ceremony in every program, contradicting the stated lean toward ambient availability. Building a separate, bespoke ambient-injection mechanism distinct from the import resolver's prelude machinery was rejected as duplicating an already-proven mechanism for the same operation.

## Consequences
Any future standard-library addition follows this same ambient-splice mechanism by default; a genuinely opt-in module would need its own separate design decision. An ambient name colliding with a local one is the same static "already bound" error as any other duplicate, so each new standard-library name is a namespace commitment, not a low-cost addition.

## Related Specification
`spec/LANGUAGE_SPEC.md` §5 (standard-library contents remain open; the ambient-delivery mechanism is settled).
