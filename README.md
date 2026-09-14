# Obfusku

![Banner](obfusku-banner.png)

![Version](https://img.shields.io/badge/version-0.1.0-blue)
![License](https://img.shields.io/badge/License-MIT-green)
[![CI](https://github.com/core-red-project/obfusku/workflows/CI/badge.svg)](https://github.com/core-red-project/obfusku/actions)

<p align="center">
  <strong>Symbolic Primacy ✦ Strict Hindley-Milner ✦ Tail-Call Optimized</strong><br>
  <em>An esoteric programming language where symbols embody meaning and ritual execution defines semantics.</em>
</p>

<p align="center">
  <a href="#about">About</a> ✦
  <a href="#features">Features</a> ✦
  <a href="#installation">Installation</a> ✦
  <a href="#usage">Usage</a> ✦
  <a href="#architecture">Architecture</a> ✦
  <a href="#contributing">Contributing</a>
</p>

---

## About

**Obfusku** is an esoteric programming language where symbols carry semantic primacy rather than serving as syntactic shortcuts.

Obfusku investigates symbolic purity in programming language design. Rather than layering keywords on top of ASCII syntax, it grounds computation in immutable Core semantics: curried functions, algebraic data types, tail-call optimization, and sound Hindley-Milner type inference.

Source code is tokenized with multi-byte Unicode preservation, parsed into surface AST, desugared into frozen Core AST, verified through strict Hindley-Milner inference, and executed on a lexical environment tree.

### Philosophy

> *"Symbols do not merely represent operations—they embody semantics."*

This is a Core Red Project, part of the Sxnnyside Project ecosystem.

## Features

- **Symbol Primacy & Concrete Grammar**: Computation is expressed through a frozen symbolic alphabet (`λ`, `≔`, `⟡`, `⟢`, `▷`, `☄`, `☊`, `❧`) where glyphs embody semantic primacy rather than acting as ASCII cosmetic sugar.
- **Principal Hindley-Milner Type Inference**: Sound, complete Algorithm W inference engine with parametric polymorphism, polymorphic generalization under value restriction, and explicit monomorphic annotations when desired.
- **First-Class Algebraic Data Types**: Custom sum types with named constructor functions, recursive self-referential types (`List ▷ t`), and parameterized generic variants.
- **Static Pattern Exhaustiveness**: High-assurance pattern matching compiler that statically verifies matrix coverage over constructors, literals, booleans (`◉`/`◎`), and wildcards (`_`), rejecting non-exhaustive branches before runtime.
- **Constant-Stack Tail-Call Optimization (TCO)**: Trampoline-based evaluation runtime eliminating call stack growth on self and mutual tail calls, enabling unbounded recursive computations.
- **Scoped Mutability & Explicit Reference Capture**: Immutable-by-default architecture with explicit mutable cells (`≔˚`), in-place rebind operations (`⚙︎`), and strict lexical closure capture sigils (`˚`).
- **Dynamic-Extent Exception Architecture**: Structured, unwinding error model built around a closed `Exception` ADT, featuring clean `☄` (raise) and `☊` (catch) expression semantics.
- **Ambient Functional Prelude**: Zero-dependency standard library (`stdlib.obk`) providing inductive collections (`List<T>`) and verified higher-order combinators (`map`, `filter`, `fold`) out of the box.
- **Sandboxed Capability I/O**: Host interface providing console streaming (`print`), terminal input (`readLine`), and path-validated filesystem primitives (`readFile`, `writeFile`) scoped securely to entry directory roots.
- **Deterministic Formatter**: Dedicated `obfusku-fmt` engine that canonizes source layout, operator precedence spacing, and semantic indentation with strict roundtrip fidelity.
- **Multi-Crate Workspace Architecture**: Strict separation of concerns across 7 decoupled workspace crates (`core`, `diagnostics`, `syntax`, `typecheck`, `runtime`, `fmt`, `cli`) forming an acyclic dependency graph.

## Installation

### Prerequisites

- Rust (1.93.0 or later)
- just (optional task runner)

### From Source

```bash
git clone https://github.com/core-red-project/obfusku.git
cd obfusku

cargo build --release -p obfusku-cli
```

## Usage

```bash
# Run an Obfusku program
cargo run -p obfusku-cli -- run spell.obk

# Type-check without running
cargo run -p obfusku-cli -- check spell.obk

# Format source code
cargo run -p obfusku-cli -- fmt spell.obk

# Interactive REPL
cargo run -p obfusku-cli -- repl
```

## Architecture

```
obfusku/
├── crates/
│   ├── obfusku-core/         # Core AST & frozen semantic types
│   ├── obfusku-diagnostics/  # Spans, diagnostics, SourceMap
│   ├── obfusku-syntax/       # Lexer, parser, surface AST, desugaring
│   ├── obfusku-typecheck/    # Hindley-Milner type inference & exhaustiveness
│   ├── obfusku-runtime/      # Evaluator, lexical environments, TCO
│   ├── obfusku-fmt/          # Canonical source code formatter
│   └── obfusku-cli/          # CLI interface, stdlib, REPL
├── examples/                 # Executable canonical example suite
└── spec/                     # Formal normative language specifications & ADRs
```

## Contributing

Contributions are accepted. See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

Before contributing, read the [Code of Conduct](CODE_OF_CONDUCT.md).

## License

This project is licensed under the MIT License — see the [LICENSE](LICENSE) file for details.

---

<p align="center">
  <strong>Obfusku</strong> — A Core Red Project<br>
  <em>&copy; 2026 Sxnnyside Project</em>
</p>
