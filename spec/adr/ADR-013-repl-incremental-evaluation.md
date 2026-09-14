# ADR-013 — REPL evaluates each line incrementally against committed state, not by re-running a growing buffer

## Status
Accepted

## Context
An earlier REPL architecture re-ran a whole accumulated buffer from scratch on every accepted line, chosen to avoid building a genuinely incremental evaluation path across crates. This implied a real defect: any effectful native bound on an earlier line would re-fire on every later submission, and it excluded the standard library and I/O from the REPL entirely.

## Decision
Each accepted line is parsed, type-checked, and evaluated exactly once, against everything previously committed in the session, seeded as an ordinary prelude via the same mechanism whole-module imports already use. Mutable-cell tracking is carried across lines so a cell committed on one line can be read or rebound by a later one.

## Alternatives Considered
Patching the whole-buffer-rerun model to suppress effect replay for specific natives was rejected as not fixing the underlying architectural cause and not solving cross-line mutable-cell state. Leaving the REPL permanently without standard-library or I/O access was rejected once revisited, in favor of fixing the underlying architecture directly.

## Consequences
The REPL is a genuinely stateful incremental session, not a single-shot pipeline invoked repeatedly; future REPL features build on this foundation. File I/O remains deliberately excluded from the REPL, since it has no single entry file to scope a filesystem capability against (see ADR-011) — a future REPL filesystem feature must resolve this by design, not by accident.

## Related Specification
`spec/IMPLEMENTATION_ARCHITECTURE.md` §11, §12, §20.
