# ADR-010 — I/O is host-provided; natives are arity-1 and reuse the existing failure mechanism

## Status
Accepted

## Context
Adding any I/O primitive required deciding how the runtime relates to the outside world, since the runtime is meant to stay embeddable and sandboxable (no direct system I/O inside it), and the CLI is meant to stay pure orchestration with no language logic of its own.

## Decision
I/O is host-provided: the runtime never performs language-level I/O directly; a native's actual implementation is supplied by whatever embeds the runtime. Native values are arity-1 and curried, structurally identical to closures — no second calling convention. A native's type scheme is authored by the host and seeded into the same prelude mechanism the standard library already uses. I/O failures reuse the existing tagged-failure mechanism with a fixed tag vocabulary, not a new exception variant per failure category.

## Alternatives Considered
Building I/O directly into the runtime was rejected as breaking embeddability and sandboxability. A dedicated multi-argument native calling convention was rejected as an unnecessary second mechanism when ordinary currying already composes multi-argument calls. A new exception variant per I/O failure category was rejected, since the existing tagged-failure form already generalizes this without growing the closed exception set unboundedly.

## Consequences
Every future native capability must be host-provided, arity-1/curried, and author its own type scheme through the shared prelude mechanism, reporting failures via the existing tagged form. Each native's host crate must independently test that its hand-written scheme agrees with its actual implementation, since nothing checks this automatically.

## Related Specification
`spec/SEMANTIC_CORE.md` §15.4; `spec/IMPLEMENTATION_ARCHITECTURE.md` §11, §12.
