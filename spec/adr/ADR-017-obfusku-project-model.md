# ADR-017 — An Obfusku Project is a directory tree rooted at the nearest ancestor project marker; a bare `.obk` file remains valid with no marker

## Status
Accepted

## Context
`ADR-006` scoped module resolution to same-directory files, and `ADR-011` scoped filesystem access to the entry file's own directory — both explicitly deferred a broader "project"/"artifact" concept to a later decision, since inventing one wasn't required to ship a first slice of imports and file I/O. Two things forced that later decision now: the CLI cannot accept a directory (only a file) with no way to know which file inside it is the entry module, and neither module resolution nor filesystem access can generalize past "same directory as whatever file happens to be passed to `run`" without a boundary independent of that one file's location.

## Decision
An Obfusku Project is a directory containing a project marker — `obfusku.toml`, per `ADR-020` — recording at minimum an entry module reference, expressed as a project-relative module path per `ADR-018`. A Project's root is the nearest ancestor directory of a given `.obk` file that contains a marker, discovered by walking upward from that file. `obfusku run`/`check`/`fmt`/`inspect` accept either a single `.obk` file (unchanged, existing behavior) or a directory: given a directory, its own marker (required — a markerless directory has no defined entry module and is a clear CLI error, not a guess) names the entry module to resolve and run.

A `.obk` file with no marker anywhere in its ancestry runs exactly as it does today: root is that file's own directory (`ADR-011`'s original scoping), module search is same-directory only, in a degenerate one-file Project. This is mandatory backward compatibility, not an optional accommodation — every file in `examples/` has no marker and must keep working unchanged.

A Source Artifact (the unit of distribution) is a Project made portable: the same directory tree, containing its own marker, entry module, and every module reachable from it — with no distinction in logical structure between "a Project being developed" and "a Project received as a distributable unit." Packaging format (archive, directory copy, or anything else) is explicitly not decided here.

## Alternatives Considered
Requiring every Project's marker to live at a fixed, predetermined location (e.g., always the current working directory) was rejected: it would force a specific invocation convention on the user rather than letting `run`/`check` discover the boundary from whatever path they're actually given, which is what "point `obfusku` at a file or a directory and have it work" requires. Treating the entry file's own directory as the permanent, only root (status quo) was rejected as the exact limitation this ADR exists to lift. Requiring a marker unconditionally, with no bare-file fallback, was rejected as an unnecessary breaking migration on every existing single-file program, including this repository's own `examples/`.

## Consequences
`ADR-006`'s same-directory-only restriction is superseded by `ADR-018`, using the root this ADR defines. `ADR-011`'s filesystem scoping is completed by `ADR-019`, using the same root. `ADR-004`'s `RunResult` contract is unaffected: it remains defined purely in terms of the entry module's own last top-level binding, regardless of whether that module was reached directly or via a Project marker — this ADR does not touch it. `build`/`test` (currently stubbed in the CLI) become meaningful to design once this ADR exists, but their own semantics are not decided here: `build` conceptually validates a Project's whole reachable source graph as a valid Source Artifact under this model — it certifies distributability, it does not itself perform or verify a copy to another environment; `test` additionally requires a language-level testing construct that does not exist yet, and is not invented by this ADR. The marker's concrete file format is fixed by `ADR-020`; any metadata it might carry beyond what that ADR defines (dependency identity, versioning) remains a separate future decision.

## Related Specification
`spec/LANGUAGE_SPEC.md` §5 (module/artifact/distribution model, now substantially answered here); `spec/adr/ADR-004-runresult-contract.md`; `spec/adr/ADR-006-whole-module-imports.md`; `spec/adr/ADR-011-filesystem-scoping.md`; `spec/adr/ADR-018-project-relative-module-resolution.md`; `spec/adr/ADR-019-project-root-filesystem-boundary.md`; `spec/adr/ADR-020-obfusku-project-manifest.md`.
