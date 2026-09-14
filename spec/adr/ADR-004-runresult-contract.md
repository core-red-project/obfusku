# ADR-004 — `RunResult`: the value `run` reports is a closed CLI contract, not a language concept

## Status
Accepted

## Context
What value a module "results in" when run was originally left as a placeholder pending a future artifact/build model. Tracing its actual footprint showed it is confined entirely to one CLI command's display behavior, with no dependency from the typechecker, runtime, or module resolution on any notion of "the module's result" distinct from "its last top-level binding."

## Decision
The value `run` reports is a closed, permanent contract: the module's last top-level binding's value. This is `run`'s own execution-reporting convention, not a language-level semantic. Export/visibility (`⟳`) remains a separate concern from what `run` prints; no entry-point declaration is added to the grammar; `run` does not return the whole binding set.

## Alternatives Considered
Giving the export modifier a second responsibility — marking which binding `run` reports — was rejected as conflating two orthogonal concerns. Adding a `main`-style entry-point declaration was rejected as out of proportion for a CLI display convention, and as presupposing parts of the still-open module/artifact model.

## Consequences
A future artifact/build model may define its own reporting convention independently, without needing to migrate this one. Any change to what `run` reports is a considered contract change, not an adjustment to a placeholder.

## Related Specification
Not a `spec/` concept — lives entirely in `crates/obfusku-cli`. Related: `spec/LANGUAGE_SPEC.md` §5 (module/artifact model, still open; this contract is explicitly not blocked on it).
