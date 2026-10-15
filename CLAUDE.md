# Obfusku Assistant Guide

## Project Purpose
Obfusku is a symbolic, ritualistic esoteric programming language (Esolang) where symbols carry semantic primacy. Programs are parsed into a surface AST, desugared into a typed Core AST, checked via a Hindley-Milner type inference engine with user-defined algebraic data types (ADTs), and evaluated using an environment-frame tree with tail-call optimization (TCO).

## Repository Topology
This repository is organized as a **Non-Monolithic Cargo workspace** consisting of seven modular crates under `crates/`:

- `crates/obfusku-core`: Frozen Core AST, type vocabulary, and literal/operator definitions (`spec/SEMANTIC_CORE.md`).
- `crates/obfusku-diagnostics`: Spans, diagnostic levels (Error, Warning, Note), and `SourceMap` with line/column resolution.
- `crates/obfusku-syntax`: Lexer (`tokenize`), recursive descent parser (`parse`), surface AST, and desugaring (`desugar`) into Core AST.
- `crates/obfusku-typecheck`: Hindley-Milner type inference, unification, ADT registry, type arity validation, and pattern exhaustiveness checking.
- `crates/obfusku-runtime`: Core AST evaluator, lexical environments (`Env`), closures, tail-call optimization, exception unwinding (`Raise`/`Catch`), and native function host bindings.
- `crates/obfusku-fmt`: Canonical source code formatter implementing strict operator precedence and indentation rules.
- `crates/obfusku-cli`: Command-line interface (`obfusku`), module resolution (`resolve_imports`), bundled standard library (`stdlib.obk`), and interactive REPL (`ReplSession`).

## Standard Command Surface
Use `just` to run common workflow tasks:

```bash
just install     # Bootstrap toolchain components and install the git hooks
just dev         # Start interactive REPL for local testing
just build       # Build all workspace crates
just test        # Run workspace test suite
just typecheck   # Run compiler type checking across all targets
just lint        # Run Clippy static analysis with warnings denied
just format      # Format all workspace code with rustfmt
just format-check # Verify formatting without modifying files
just deny        # Audit dependencies (advisories, licenses, sources); DENY_OFFLINE=1 skips the advisory fetch
just check       # Run the complete quality gate (format-check, lint, typecheck, test, deny); never modifies files
just bench       # Startup and core-operation timings (not part of check)
just clean       # Clean build artifacts and caches
```

Direct Cargo equivalents are also available (e.g. `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`).

## Where Things Go
| What | Where |
|---|---|
| Language semantics, grammar, decisions | `spec/` (`spec/adr/ADR-NNN-*.md` for each decision) |
| Planning and open work | `docs/ROADMAP.md`; shipped changes in `CHANGELOG.md` |
| Images and other assets used by docs | `docs/assets/` |
| Compiler, runtime, CLI, LSP code | `crates/<crate>/src/`; integration tests in `crates/<crate>/tests/` |
| Bundled standard library | `crates/obfusku-cli/src/stdlib.obk` |
| Manifest, capabilities, dependencies | `crates/obfusku-cli/src/{manifest,capability,deps}.rs` (`ADR-029`) |
| Example programs | `examples/` |
| Benchmark fixtures | `benches/` |
| Git hooks, commit-message checks | `.githooks/`, `scripts/` |
| CI and release workflows | `.github/workflows/` |

Git hooks: `just install` points `core.hooksPath` at `.githooks/` (pre-commit runs `just check`; commit-msg enforces Conventional Commits). Release notes come from `git cliff` (`cliff.toml`).

## Architecture & Data Flow
1. **Source Text** → `obfusku-syntax::lexer`: UTF-8 token stream with accurate spans.
2. **Tokens** → `obfusku-syntax::parser`: Surface syntax AST (`Expression`, `Declaration`, `Module`).
3. **Surface AST** → `obfusku-syntax::desugar`: Lowered into Core AST (`obfusku_core::ast::Module`). Synthesizes exception constructors and normalizes let/letrec groups.
4. **Core AST** → `obfusku-typecheck::check`: Type inference with ADT registry, checking exhaustiveness and returning typed bindings.
5. **Typed Core AST** → `obfusku-runtime::evaluate`: Evaluated in lexical environments with TCO and exception catching.

## Coding Conventions
- **Language Standard**: Rust 2021 edition, toolchain pinned in `rust-toolchain.toml`.
- **Code Style**: Format with `rustfmt` (`rustfmt.toml`). Maximum line width is 100 characters.
- **Linting**: All code must pass `clippy` with `#![deny(warnings)]`. Avoid unneeded returns, handle complex types cleanly, and annotate intentional exceptions with targeted `#[allow(...)]`.
- **Error Handling**: Use `obfusku_diagnostics::Diagnostic` rather than panics in language pipeline stages.
- **Comments & Documentation**: Keep comments proportional, precise, and timeless. Cite formal specification sections (`spec/SEMANTIC_CORE.md`, `spec/CONCRETE_SYMBOLIC_GRAMMAR.md`, `spec/adr/`) rather than repeating entire specification text.

## Commit Conventions
Follow Conventional Commits:
- `feat: ...` for new features or capabilities
- `fix: ...` for bug fixes
- `docs: ...` for documentation updates
- `refactor: ...` for code refactoring without behavior change
- `test: ...` for test additions or improvements
- `chore: ...` for tooling, CI, or dependency maintenance
