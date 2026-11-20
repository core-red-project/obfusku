# Architecture

## Overview
Obfusku is an esoteric programming language with symbolic semantic primacy, curried functions, algebraic data types, constant-stack tail-call optimization, and sound Hindley-Milner type inference. Programs compile from UTF-8 surface syntax through a desugared frozen Core AST, verified by type inference and pattern exhaustiveness checking, and execute on a lexical environment tree with trampoline evaluation.

## Components
| Component | Responsibility | Depends on |
|---|---|---|
| `obfusku-core` | Semantic Core AST, primitive type definitions, frozen literal and operator vocabulary | None |
| `obfusku-diagnostics` | UTF-8 source span management, `SourceMap`, error/warning/note diagnostic reporting | None |
| `obfusku-syntax` | Multi-byte Unicode lexer, recursive descent parser, surface AST, Core desugaring | `obfusku-core`, `obfusku-diagnostics` |
| `obfusku-typecheck` | Principal Hindley-Milner type inference (Algorithm W), ADT registry, pattern exhaustiveness | `obfusku-core`, `obfusku-diagnostics` |
| `obfusku-runtime` | Evaluator, lexical environments, closures, trampoline TCO, exceptions, host bindings | `obfusku-core`, `obfusku-diagnostics` |
| `obfusku-fmt` | Canonical source code formatter with strict operator precedence and indentation | `obfusku-core`, `obfusku-syntax`, `obfusku-diagnostics` |
| `obfusku-lsp` | Language Server Protocol (LSP) daemon providing real-time diagnostics over JSON-RPC | `obfusku-syntax`, `obfusku-typecheck`, `obfusku-diagnostics` |
| `obfusku-cli` | Command-line interface (`obfusku`), module resolution, bundled stdlib, interactive REPL | All workspace crates |

## Data flow
1. **Source Text**: Input read from `.obk` source files or interactive REPL input.
2. **Tokenization (`obfusku-syntax::lexer`)**: Lexical analysis preserving multi-byte Unicode glyphs and byte spans.
3. **Parsing (`obfusku-syntax::parser`)**: Recursive descent surface syntax parsing into an untyped surface AST.
4. **Desugaring (`obfusku-syntax::desugar`)**: Lowering surface AST into normalized Core AST (`obfusku-core::ast::Module`).
5. **Type Checking (`obfusku-typecheck`)**: Principal type inference via Algorithm W, ADT validation, and matrix pattern exhaustiveness.
6. **Execution (`obfusku-runtime`)**: Trampoline-based evaluation over nested lexical environment frames with tail-call elimination.

## Decisions
| Decision | Why | Alternatives rejected |
|---|---|---|
| Frozen Symbolic Core (`spec/SEMANTIC_CORE.md`) | Ensures glyphs have invariant semantic meaning without syntactic drift | ASCII keyword syntax sugar |
| Trampoline TCO | Guarantees constant stack depth on unbounded recursion | Unchecked call stack growth, bytecode interpreter |
| Algorithm W Type Inference | Provides sound, principal types with polymorphic generalization under value restriction | Dynamic typing, local-only bidirectional typing |
| Multi-Crate Workspace | Isolates compiler stages and enables targeted standalone binaries (`obfusku`, `obfusku-lsp`) | Monolithic single-crate compiler |
| Persistent Trie Collections | Immutable O(log n) collections matching language purity semantics | Mutable linear vectors |

## Constraints
- **Zero Unsafe Code**: The entire workspace forbids `unsafe` code (`unsafe_code = "forbid"`).
- **Constant Stack Depth**: Recursive tail calls must execute in O(1) stack space via trampoline.
- **Strict Determinism**: Formatter and evaluation semantics must yield byte-for-byte deterministic output across platforms.
- **Toolchain Baseline**: Rust 2024 edition, MSRV 1.93.
