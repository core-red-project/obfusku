# ADR-019 — The filesystem capability boundary is the Project root; symlink escapes are rejected by resolved location, not lexical shape alone

## Status
Accepted

## Context
`ADR-011` scoped `readFile`/`writeFile` to the entry file's own directory and stated this was "explicitly incomplete: what counts as 'the entry file' for a packaged or distributed artifact... is coupled to the unresolved module/artifact/distribution model." `ADR-017` resolves that model. Separately, auditing `resolve_within_root` (`crates/obfusku-cli/src/natives.rs`) while designing this ADR found it is purely lexical — it never consults the filesystem — so a symlink inside an allowed root pointing outside it is not caught: `project/data -> /outside/` lets `readFile("data/secret")` reach outside the root through a path that is lexically well-formed.

## Decision
The filesystem boundary is the Project root `ADR-017` defines (falling back to the entry file's own directory in bare-file mode, unchanged from `ADR-011`'s original scoping — no regression for any program with no marker). `resolve_within_root`'s existing lexical containment check (rejecting absolute paths and any `..` that climbs above the root) is kept, and is joined by a second, independent check: the resolved path's real, symlink-followed location (via filesystem canonicalization) must also stay within the canonicalized root. A path that passes the lexical check but resolves, through a symlink, outside the root is rejected with the same `"PermissionDenied"` tag `ADR-011`/`SEMANTIC_CORE.md` §15.4 already define — this is a refinement of when that tag applies, not a new failure category.

## Alternatives Considered
Relying on the lexical check alone (status quo) was rejected once the symlink-escape gap was found — a boundary that a project's own filesystem contents (not even an adversarial input string) can silently defeat isn't the boundary `ADR-011` intended. Refusing to follow symlinks at all (treating every symlink as an error) was rejected as unnecessarily strict against legitimate in-project symlinks that stay within the root — the containment check only needs to reject escapes, not symlinks generally.

## Consequences
`resolve_within_root` (or its caller) must canonicalize the resolved candidate path and verify it remains under the canonicalized root before the file operation proceeds, in addition to its existing lexical check. This is a behavior change (a previously-permitted symlink-escape read/write now fails), scoped narrowly enough that it should not be treated as a compatibility concern — no shipped program relies on escaping its own sandbox. `ADR-011`'s Decision and Alternatives Considered are otherwise unchanged; this ADR completes its Consequences' explicitly-acknowledged gap rather than revising the rest of it.

## Related Specification
`spec/adr/ADR-011-filesystem-scoping.md`; `spec/adr/ADR-017-obfusku-project-model.md`; `spec/SEMANTIC_CORE.md` §15.4.
