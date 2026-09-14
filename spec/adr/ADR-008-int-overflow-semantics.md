# ADR-008 — `Int` is fixed-width; overflow raises a catchable exception, never panics

## Status
Accepted

## Context
The specification never committed `Int` to a width and said nothing about overflow. The inherited behavior was an accident of the underlying machine-word operators: panicking in one build profile, silently wrapping in another, and panicking unconditionally in both profiles for the minimum-value division/modulo edge case — a real crash risk, not merely a debug-mode inconsistency.

## Decision
`Int` remains fixed-width (arbitrary-precision integers are out of scope). Any arithmetic result outside that range raises an `IntegerOverflow` exception, deterministically, in every build profile — never a panic. Division-by-zero remains a separate, unmerged condition.

## Alternatives Considered
Arbitrary-precision integers were rejected as out of scope for this decision — a much larger semantic and performance commitment. Leaving the behavior profile-dependent was rejected as an unacceptable, silently inconsistent language semantic and a real crash surface in release builds.

## Consequences
Every arithmetic-producing operation on `Int` must route through checked arithmetic that raises `IntegerOverflow` rather than using unchecked machine operators. The exception set is treated as closed; any future arithmetic failure mode extends that same enumerated set. Introducing arbitrary-precision integers later would be a breaking semantic change to this decision, not an extension of it.

## Related Specification
`spec/SEMANTIC_CORE.md` §15.2, §15.3, §20.2.
