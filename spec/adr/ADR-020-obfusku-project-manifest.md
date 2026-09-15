# ADR-020 — The Obfusku Project marker is `obfusku.toml`, a small, additively-versioned manifest naming the entry module

## Status
Accepted

## Context
`ADR-017` established that a Project marker must exist and must be able to record an entry module reference, but deliberately left its concrete file format undecided — inventing a format wasn't needed to fix the architecture. It is needed now: `obfusku run ./project` has no way to identify an entry module, and thus no way to execute, without a concrete, parseable marker. A marker also needs a story for how it evolves — a manifest format with no versioning or unknown-field policy becomes a compatibility hazard the moment a second field is ever added.

## Decision
The Project marker is a file named `obfusku.toml`, at the Project root — the directory containing it *is* the root `ADR-017`/`ADR-018`/`ADR-019` all resolve against; there is no separate "root" field, since the marker's own location already answers that question unambiguously.

**Fields, for format `1`:**

- `format` (required, integer). The manifest schema version. Fixed at `1` for everything this ADR defines. A binary that encounters a `format` value higher than the highest it supports fails with a clear, distinct error naming the unsupported version — it must never guess or silently ignore unrecognized structure at a version it doesn't know. A binary encountering a `format` value it *does* support proceeds normally regardless of that value being older than the binary's own maximum, per the additive-evolution rule below.
- `entry` (required, string). A bare module name — no `.obk` suffix, no path separators — resolved the same way `ADR-018` resolves a `⟲` import: a whole-Project-tree search by filename. `"main"` matches `main.obk` wherever it sits under the Project root; a value containing `/` is not a special "path" form, it simply will not match any module (module basenames never contain `/`). A manifest whose `entry` doesn't resolve to exactly one module (zero matches, or more than one under `ADR-018`'s ambiguity rule) is an invalid manifest (see below), not a runtime "file not found" surfaced later.
- `name` (optional, string). A human-readable project identity with no resolution semantics in this ADR — informational only. Reserved deliberately: a future external-dependency/identity model (explicitly out of scope for `ADR-017`/`ADR-018`) will need *some* project-level identity to resolve by, and this field exists so that need doesn't force a breaking manifest schema change later. Its absence is never an error.

**Unknown-field policy (additive evolution):** within a supported `format` value, a field this ADR doesn't define is ignored, not an error — a newer manifest read by an older-but-compatible binary must degrade gracefully rather than hard-fail on structure it doesn't yet understand. This is a `format`-scoped guarantee, not unconditional: a `format` bump is exactly the mechanism for introducing a change too significant to tolerate silently (a required-field addition, a semantic change to an existing field), and only a `format` value itself, not individual unknown fields, is the versioning signal.

**Invalid manifest handling:** malformed TOML syntax, a missing `format` or `entry`, an unsupported `format` value, or an `entry` that doesn't resolve to an existing module are all a single class of error — a clear, CLI-level diagnostic identifying the problem. An invalid manifest is never silently treated as "no manifest" (which would incorrectly fall back to `ADR-017`'s bare-file mode with a *different* file than the one actually being pointed at) — a directory containing a broken `obfusku.toml` is a Project with an error, not a markerless directory.

## Alternatives Considered
A schema-less or freeform key-value marker (no `format` field) was rejected: it would leave no mechanism to introduce a future breaking change without ambiguity about which manifests are affected. Treating unknown fields as errors unconditionally was rejected as making every future field addition a breaking change for every existing manifest, contrary to how this project's own ADRs (e.g. `ADR-006`'s "additive, not a replacement" framing) already treat compatible extension. Embedding dependency/versioning metadata now was rejected as exactly the scope `ADR-017`/`ADR-018` already excluded — this ADR reserves a slot (`name`) for future identity needs without deciding what uses it. A non-TOML format (JSON, a `.obk`-syntax manifest, a bespoke format) was not seriously weighed against TOML once TOML was the format actually proposed and accepted for this purpose; nothing about Obfusku's own design gives another format a specific advantage here.

## Consequences
The CLI's directory-invocation path (`ADR-017`'s "given a directory, its own marker names the entry module") must locate, parse, and validate `obfusku.toml` per this ADR before any module resolution begins. `ADR-017` is amended to replace its "concrete file format is a separate, later decision" language with a reference to this ADR. Every other Project-model decision (`ADR-018`, `ADR-019`) is unaffected — they already resolve against "the Project root" as an abstract concept this ADR now makes concrete.

## Related Specification
`spec/adr/ADR-017-obfusku-project-model.md`; `spec/adr/ADR-018-project-relative-module-resolution.md`; `spec/adr/ADR-019-project-root-filesystem-boundary.md`.
