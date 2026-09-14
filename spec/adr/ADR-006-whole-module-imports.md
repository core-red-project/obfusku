# ADR-006 — Whole-module imports only: no selective imports, no search paths beyond the entry directory

## Status
Accepted

## Context
The module/artifact model was deliberately unspecified. Implementing an import declaration required real decisions before any code could be written: what a module is, how much of it an import brings in, and how collisions are handled.

## Decision
A module is a same-directory source file, resolved by the CLI, never by the syntax layer (no file I/O below the CLI boundary). Import always brings in a module's whole exported surface — there is no selective or aliased import syntax. An imported name colliding with a local name, or with another import, is the same static "already bound" error as an ordinary duplicate top-level name.

## Alternatives Considered
Selective or aliased imports were rejected for this slice as unnecessary surface-area expansion before the whole-module mechanism was proven — deferred, not ruled out. Module search paths beyond the same directory, or a package/versioning identity model, were rejected as coupled to the still-unresolved artifact/distribution model, which this decision deliberately does not presuppose.

## Consequences
Any future selective-import or re-export feature is additive to this whole-module baseline, not a replacement for it. Packaging, versioning, and search-path decisions remain blocked on the artifact/distribution model and must not be decided piecemeal through import-resolver changes. Encapsulation is enforced entirely by what the resolver chooses to hand on; a future visibility feature must preserve this property rather than adding a new privacy concept elsewhere.

## Related Specification
`spec/LANGUAGE_SPEC.md` §5; `spec/CONCRETE_SYMBOLIC_GRAMMAR.md` §14; `spec/DESIGN_VISION.md` §10.
