# ADR-022 — Program arguments and environment are host natives in the `⌁` family; `bundle` ships interpreter and sources as one executable

## Status
Accepted

## Context
A program had no way to receive command-line arguments or read environment variables, so no Obfusku program could be a conventional CLI. `obfusku build` only certifies a Project (`ADR-017`) and emits nothing runnable on its own.

## Decision
Two natives extend `GLYPH_SYSTEM_DESIGN.md` §10.2's host-effect family with new medium marks on the read direction: `⌁↓⌥ : ∅ → List<⌘>` (program arguments) and `⌁↓⎈ : ⌘ → Optional<⌘>` (environment variable). Both are host-provided, arity-1, and need no filesystem root (`ADR-010`). `obfusku run <target> -- <args…>` passes everything after `--` to the program; the CLI's own `--help`/`--version` scan stops at `--`. `check`, the REPL and the LSP see the same types with empty arguments.

`obfusku bundle <target> [-o out]` type-checks the program, then writes a copy of the running interpreter with the Project's `.obk` files, manifest and entry path appended behind a length-and-magic trailer. A bundled executable owns its whole command line (no subcommands, no `--`), resolves imports from its embedded sources, and roots `⌁↓⌬`/`⌁↑⌬` at the caller's working directory instead of the Project root (`ADR-019` is otherwise unchanged). It prints no final value.

## Alternatives Considered
A single `⌁↓⎈` returning the whole environment as a list was rejected: it forces every program to search, and exposes more than it needs. A runtime-level argv value was rejected by `ADR-010`: the runtime stays free of host state. Compiling to native code was rejected as out of scope; embedding keeps the interpreter the only execution path.

## Consequences
A program can read any environment variable the invoking user can; environment access is not sandboxed, unlike the filesystem. A bundle is as large as the interpreter and is platform-specific; it is produced by running `bundle` with the interpreter for the target platform. A bundle's modules are resolved from memory; nothing is written to disk.

## Related Specification
`spec/GLYPH_SYSTEM_DESIGN.md` §10.2; `spec/adr/ADR-010-io-host-boundary.md`; `spec/adr/ADR-017-obfusku-project-model.md`; `spec/adr/ADR-019-project-root-filesystem-boundary.md`.
