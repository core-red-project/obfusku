# Changelog

All notable changes to **Obfusku** are documented here.

This project follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [1.0.0] — 2026-09-14

Initial stable baseline release of Obfusku, completing the architectural transition from the legacy prototype to a clean, multi-crate workspace architecture backed by normative specifications, Hindley-Milner type inference, and constant-stack runtime execution.

### Language

- **Symbolic Unicode Surface Syntax**: Frozen concrete grammar featuring distinctive glyphs (`λ`, `≔`, `⟡`, `⟢`, `▷`, `☄`, `☊`, `❧`).
- **Core Type System**: Closed set of primitive types (`⟁` Int, `⧆` Real, `⌘` Str, `○` Bool, `∅` Unit) with no implicit numeric coercions.
- **Algebraic Data Types (ADTs)**: User-defined sum types with declared variant constructors and recursive type applications (`List ▷ t`).
- **Pattern Matching**: Exhaustive pattern matching with structural deconstruction, literal equality, boolean discriminants (`◉`/`◎`), and wildcards (`_`).
- **Exceptions**: Closed `Exception` ADT with dynamic-extent unwinding via raise (`☄`) and catch (`☊`) constructs.
- **State Semantics**: Explicit mutable cells (`≔˚`), in-place rebinding (`⚙︎`), explicit closure capture (`˚`), and scoped local bindings.
- **Module System**: File-scoped modular compilation, explicit imports (`⟲`), and symbol export markers (`≔⟳`, `→⟳`).

### Compiler

- **Modular Workspace Architecture**: Decoupled compiler frontend and backend into 7 dedicated workspace crates (`obfusku-core`, `obfusku-diagnostics`, `obfusku-syntax`, `obfusku-typecheck`, `obfusku-runtime`, `obfusku-fmt`, `obfusku-cli`).
- **Lexer & Parser**: Deterministic Unicode scanner and recursive descent parser with zero backtracking and precise byte spans.
- **Desugaring**: Lowering pipeline from surface syntax into desugared Core AST with explicit let-bindings, constructors, and mut-cells.
- **Hindley-Milner Type Inference**: Algorithm W implementation with polymorphic generalization, unification, and constructor scheme instantiations.
- **Static Exhaustiveness Checker**: Static pattern matching analysis detecting non-exhaustive matches and unreachable arms before code generation.

### Runtime

- **Lexical Environment Tree**: Strict lexical scoping with nested environment frames and garbage-collected cell references.
- **Tail-Call Optimization (TCO)**: Trampoline-based tail-call elimination ensuring constant stack depth for recursive functions.
- **Structured Error Handling**: Stack unwinding runtime converting unhandled exceptions or arithmetic faults into human-readable diagnostics.

### Standard Library

- **Ambient Prelude**: Bundled `stdlib.obk` automatically loaded into user programs without explicit imports.
- **Inductive Collections**: Fundamental `List<T>` ADT declaration with inductive `Nil` and `Cons` constructors.
- **Higher-Order Combinators**: Verified recursive implementations of `map`, `filter`, and `fold`.

### CLI

- **Interactive Tooling**: Unified command-line interface (`obfusku`) providing `run`, `check`, `fmt`, `inspect`, and `repl`.
- **Host I/O Capabilities**: Built-in native function bindings for console output (`print`), terminal input (`readLine`), and scoped filesystem access (`readFile`, `writeFile`).
- **Diagnostic Presentation**: Multi-line color-capable diagnostic renderer with source line highlights and precise error spans.

### Formatter

- **Deterministic Code Formatting**: Dedicated `obfusku-fmt` engine enforcing consistent layout, indentation, and operator precedence spacing for Obfusku source files.

### Tooling

- **Task Automation**: Canonical task runner (`Justfile`) providing standard DX commands (`build`, `test`, `lint`, `format`, `check`).
- **CI/CD Quality Gates**: Automated GitHub Actions workflows for continuous integration, workspace linting, cargo-deny auditing, and multi-platform validation.
- **Canonical Example Suite**: Verified executable examples in `examples/` covering language fundamentals, ADTs, modules, mutation, errors, and I/O.

---

[1.0.0]: https://github.com/core-red-project/obfusku/releases/tag/v1.0.0
