# ADR-015 — `Exception` is a built-in closed ADT; `Raise`/`Catch` resolve by dynamic extent

## Status
Accepted

## Context
The language needed an exceptional-failure mechanism distinct from
`Result<T, E>` (ordindary two-constructor ADT, `SEMANTIC_CORE.md` §15),
for genuine programmer-error conditions that require non-local unwinding
rather than ordinary data-passing. Two things needed deciding: how a
raised value is typed (open polymorphic vs. closed ADT), and how a handler
is scoped and resolved relative to where a value is raised (lexical vs.
dynamic extent).

## Decision
`Exception` is a real, built-in closed ADT — not a special-cased runtime
tag — registered through the same `AdtRegistry`/constructor-synthesis
mechanism a user `TypeDeclaration` receives, with a fixed variant set:
`DivisionByZero`, `IntegerOverflow`, `NonExhaustiveMatch`,
`InvalidOperation(Str)`, `Failure(tag: Str, payload: Str)`. No other
variant exists implicitly or by hedge.

`Catch`'s handler is parsed and lowered exactly like an ordinary `Lambda`,
including its capture-check rules, rather than through a separate
handler-binding form. Resolution is by dynamic extent: a `Raise` unwinds
to the nearest enclosing `Catch` on the call stack at the moment of the
raise, not by lexical scope. Runtime internal errors raise real `Exception`
values through this same channel, so a surface `Catch` handler observes
user-raised and runtime-raised failures uniformly.

## Alternatives Considered
- An open (non-closed) or user-extensible `Exception` type — rejected:
  the closed set matches `Match`'s existing exhaustiveness discipline and
  keeps `Catch` handlers statically checkable against a known variant set.
  A `Failure(Str, Str)` variant already provides the user-extensible slot
  for domain-specific error shapes without opening the type itself.
- A separate, dedicated handler-binding form distinct from `Lambda` —
  rejected: `Lambda`'s existing parsing, lowering, and capture-check rules
  already cover a handler's needs with no new mechanism.
- Lexical-scope resolution for `Catch` — rejected: does not match the
  unwind-to-nearest-enclosing-handler behavior the exceptional channel is
  meant to provide, and would require threading handler identity through
  closures that have nothing to do with the raise site.

## Consequences
`Exception`'s variant set is closed and enumerated; adding a new built-in
failure category is a `spec/`-level decision amending `SEMANTIC_CORE.md`
§15.2 explicitly, not an ad hoc runtime tag. Every internal runtime error
must raise through this same `Exception` channel rather than a separate,
parallel error path, so `Catch` remains a single, uniform mechanism for
every failure origin. The `Catch` frame is deactivated before its handler
begins running — a `Raise` inside the handler propagates outward past this
`Catch`, not back into it.

## Related Specification
`spec/SEMANTIC_CORE.md` §15, §15.1, §15.2, §15.3;
`spec/CONCRETE_SYMBOLIC_GRAMMAR.md` §7.8.
