# ADR-026 — `Exception` is open: programs declare typed variants

## Status
Accepted (supersedes the "closed `Exception`" decision of ADR-015)

## Context
`Exception` had five fixed variants and `Failure(tag: Str, payload: Str)`. A failure carrying structured data had to be flattened into strings, and no library could define an error a caller could match by constructor.

## Decision
`☈ Name(T, …)` is a top-level declaration adding one variant to `Exception`. The variant is an ordinary constructor: raised with `☄`, matched by pattern, rendered by `⍕`, exported to importers with the rest of the module's types. Desugaring lowers it to a Core `TypeDecl` named `Exception`, which the checker merges into the built-in type; variant tags must not collide with any other constructor.

`Exception` is an **open** type: a module may add variants another module never sees, so a `Match` over it is exhaustive only with a wildcard or variable arm. The five built-in variants and their meanings are unchanged. An uncaught exception is reported with its canonical rendering, e.g. `NotFound("k")`.

## Alternatives Considered
`Raise` of any type was rejected in ADR-015 for the reason it still holds: a handler could not tell what it was catching. A per-module exception type with its own `Catch` was rejected: it multiplies the unwinding mechanism for no gain over variants of one type. Keeping `Exception` closed and adding a `Custom(Any)` variant was rejected: it needs dynamic typing.

## Consequences
Every handler that lists variants needs a trailing wildcard. Because imports are non-transitive, an exception raised by a module's dependency can reach code that has never imported it; the wildcard is what makes that sound. Exception declarations are not generic.

## Related Specification
`spec/SEMANTIC_CORE.md` §15.2, §17; `spec/CONCRETE_SYMBOLIC_GRAMMAR.md` §8; `spec/adr/ADR-015-exception-closed-adt-and-catch-semantics.md`.
