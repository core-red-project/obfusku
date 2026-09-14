# ADR-011 — Filesystem paths are bare strings, scoped lexically to the entry file's directory

## Status
Accepted

## Context
Extending I/O to file access required a capability decision I/O primitives without side-effects on the filesystem never needed: what a path is as a value, and whether and how access is restricted.

## Decision
A path is a bare string value, not an opaque validated-path type — this language's ADT tags have no sealing boundary, so a wrapper type would only look like a capability without containing one. Access is scoped lexically to the entry file's own directory, generalizing the same precedent already established for import resolution. The allowed root is ordinary host-side state, invisible to program source; there is no capability-as-first-class-value mechanism.

## Alternatives Considered
An opaque, sealed capability type wrapping a validated path was rejected: nothing prevents program source from reconstructing or forging any ADT tag, so the wrapper would provide no real containment. Exposing a first-class capability value that programs could narrow or pass around was rejected as requiring new runtime surface for no demonstrated need. Unrestricted access, or access scoped to the current working directory, was rejected as inconsistent with the existing import-resolution precedent and a larger, unreviewed security surface.

## Consequences
This decision is explicitly incomplete: what counts as "the entry file" for a packaged or distributed artifact, and whether every embedding must provide this environment, is coupled to the unresolved module/artifact/distribution model and is not answered here. Any future capability (network, process spawn) that needs scoping should default to this same lexical, entry-file-relative pattern unless a new decision explicitly supersedes it.

## Related Specification
`spec/SEMANTIC_CORE.md` §15.4 (signatures and tags only — access-policy portion blocked on `spec/LANGUAGE_SPEC.md` §5).
