# Obfusku

## Purpose
An esoteric programming language where symbols embody meaning and ritual execution defines semantics.

Obfusku investigates symbolic purity in programming language design. Rather than layering keywords on top of ASCII syntax, it grounds computation in immutable Core semantics: curried functions, algebraic data types, tail-call optimization, and sound Hindley-Milner type inference. Source code is tokenized with multi-byte Unicode preservation, parsed into surface AST, desugared into frozen Core AST, verified through strict Hindley-Milner inference, and executed on a lexical environment tree.

## Topology
`workspace` — Non-monolithic Cargo workspace consisting of 8 modular crates under `crates/`:

- `crates/obfusku-core`: Frozen Core AST, semantic types, literal and operator vocabulary (`spec/SEMANTIC_CORE.md`).
- `crates/obfusku-diagnostics`: Source spans, diagnostic levels (Error, Warning, Note), and `SourceMap` with line/column resolution.
- `crates/obfusku-syntax`: Multi-byte UTF-8 lexer, recursive descent parser, surface AST, and desugaring into Core AST.
- `crates/obfusku-typecheck`: Principal Hindley-Milner type inference (Algorithm W), ADT registry, type arity, and pattern exhaustiveness.
- `crates/obfusku-runtime`: Core AST evaluator, lexical environments (`Env`), closures, constant-stack tail-call optimization, exception unwinding, and native host bindings.
- `crates/obfusku-fmt`: Canonical source code formatter implementing strict operator precedence and semantic indentation rules.
- `crates/obfusku-lsp`: Language Server Protocol (LSP) daemon providing real-time diagnostics over JSON-RPC.
- `crates/obfusku-cli`: Command-line interface (`obfusku`), module resolution, bundled standard library (`stdlib.obk`), and interactive REPL (`ReplSession`).

## Commands
All workflows are orchestrated via the task runner (`Justfile`). Direct Cargo equivalents are supported across all targets:

```bash
just install       # Bootstrap toolchain components and install git hooks
just dev           # Start interactive REPL for local testing
just check         # The gate: format-check, lint, typecheck, test, deny. Must pass before any commit/push
just build         # Build all workspace crates (cargo build --workspace)
just test          # Run workspace test suite (cargo test --workspace)
just typecheck     # Compiler type checking across all targets
just lint          # Run Clippy static analysis with warnings denied
just format        # Format all workspace code with rustfmt
just format-check  # Verify formatting without modifying files
just deny          # Audit dependencies (cargo-deny: advisories, licenses, bans, sources)
just bench         # Startup and core-operation benchmarks (benches/)
just clean         # Clean build artifacts and caches
```

## Where things go
| What | Where |
|---|---|
| Language semantics, grammar, normative specs | `spec/` (`spec/adr/ADR-NNN-*.md` for architectural decisions) |
| Core AST and frozen semantic types | `crates/obfusku-core/` |
| Diagnostic reporting, spans, source mapping | `crates/obfusku-diagnostics/` |
| Lexer, parser, surface AST, desugaring | `crates/obfusku-syntax/` |
| Type inference, exhaustiveness, ADT registry | `crates/obfusku-typecheck/` |
| Evaluator, lexical environments, TCO, natives | `crates/obfusku-runtime/` |
| Source code formatter engine | `crates/obfusku-fmt/` |
| Language Server Protocol implementation | `crates/obfusku-lsp/` |
| CLI entrypoint, module resolution, REPL | `crates/obfusku-cli/` |
| Bundled standard library ambient prelude | `crates/obfusku-cli/src/stdlib.obk` |
| Manifest capabilities and dependencies | `crates/obfusku-cli/src/{manifest,capability,deps}.rs` (`ADR-029`) |
| Integration tests per crate | `crates/<crate>/tests/` |
| Executable canonical example suite | `examples/` |
| Benchmark fixtures | `benches/` |
| Documentation, architecture, roadmap | `docs/` (`docs/ROADMAP.md`, `ARCHITECTURE.md`) |
| Visual assets and banners | `docs/assets/` (`docs/assets/obfusku-banner.png`) |
| Git hooks and validation scripts | `.githooks/`, `scripts/` |
| Continuous Integration workflows | `.github/workflows/` (`ci.yml`, `release.yml`) |

Git hooks: `just install` points `core.hooksPath` at `.githooks/` (pre-commit enforces `just check`; commit-msg enforces Conventional Commits). Release notes are managed by `git cliff` (`cliff.toml`).

## Architecture & Data Flow
1. **Source Text** → `obfusku-syntax::lexer`: UTF-8 token stream with accurate byte spans.
2. **Tokens** → `obfusku-syntax::parser`: Surface syntax AST (`Expression`, `Declaration`, `Module`).
3. **Surface AST** → `obfusku-syntax::desugar`: Lowered into Core AST (`obfusku_core::ast::Module`). Synthesizes exception constructors and normalizes let/letrec groups.
4. **Core AST** → `obfusku-typecheck::check`: Type inference with ADT registry, checking exhaustiveness and returning typed bindings.
5. **Typed Core AST** → `obfusku-runtime::evaluate`: Evaluated in lexical environments with trampoline TCO and exception catching.

## Conventions
- **Language Standard**: Rust 2024 edition (`rust-version = "1.93"`), pinned in `rust-toolchain.toml`.
- **Code Style**: Format with `rustfmt` (`rustfmt.toml`). Maximum line width is 100 characters.
- **Linting**: Workspace-level `#![deny(warnings)]` via Clippy (`all` and `pedantic` warned). Allowances in `Cargo.toml` (`too_many_lines`, `float_cmp`, `match_same_arms`) are intentional compiler/spec design choices.
- **Error Handling**: Use `obfusku_diagnostics::Diagnostic` rather than panics or `unwrap()` in compiler and runtime pipeline stages.
- **Commits**: Follow Conventional Commits strictly (`feat:`, `fix:`, `docs:`, `style:`, `refactor:`, `test:`, `chore:`, `perf:`).
- **Changelog**: All notable changes must be documented in [CHANGELOG.md](CHANGELOG.md) under `[Unreleased]`.
- **Comments & Documentation**: Keep comments proportional, precise, and timeless. Cite formal specification sections (`spec/SEMANTIC_CORE.md`, `spec/CONCRETE_SYMBOLIC_GRAMMAR.md`, `spec/adr/`) rather than repeating entire specification text.

## Do not
- Do not bypass or weaken `just check` — it is the mandatory quality gate before commit or push.
- Do not use `unsafe` code anywhere in the repository (`unsafe_code = "forbid"` is enforced workspace-wide).
- Do not introduce `unwrap()`, `expect()`, or panics in user-facing compiler, typecheck, or runtime pipelines.
- Do not add comments explaining obvious code; keep code clean and self-explanatory.
- Do not invent non-standard syntax; all language behavior is governed strictly by `spec/`.
- Do not mention internal standard IDs or rule names in code or documentation.
