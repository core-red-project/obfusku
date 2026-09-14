# ADR-014 — No ASCII aliases for operators that have a dedicated glyph

## Status
Accepted

## Context
An earlier draft of the glyph design proposed carrying forward a legacy convention of pairing symbolic operators with typable ASCII aliases, with the exact alias table deferred indefinitely and never written.

## Decision
Obfusku is glyph-pure: no operator that has a dedicated symbolic glyph receives an ASCII alias. The comparison operators are plain ASCII not as a fallback but because they were never assigned a symbolic glyph in the first place — a separate, already-settled decision.

## Alternatives Considered
Implementing the deferred ASCII-alias table as originally sketched was rejected: no document had actually settled a concrete alias table waiting to be recognized, so implementing one now would be inventing new surface syntax rather than closing an existing gap.

## Consequences
Any future proposal to add ASCII-typeable alternatives for existing symbolic operators must explicitly reopen and reverse this decision, not be treated as an incremental addition. New operators introduced later must not receive ASCII aliases either, to stay consistent with this identity commitment.

## Related Specification
`spec/GLYPH_SYSTEM_DESIGN.md` §1.
