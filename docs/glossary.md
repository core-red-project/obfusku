# Glossary

<!-- Definitive domain terms, concepts, and semantic vocabulary of the Obfusku programming language. -->

**Purpose:** This document provides normative and canonical definitions for language-level concepts, architectural abstractions, and domain terminology across Obfusku specifications, compiler crates, and tooling.

---

## Language Semantics & Grammar

- **Symbolic Primacy**: The foundational design thesis that Unicode symbols embody intrinsic computational semantics rather than acting as cosmetic syntactic sugar over ASCII keywords.
- **Core AST**: The frozen, desugared intermediate representation over which formal semantics and evaluation rules are strictly defined (`spec/SEMANTIC_CORE.md`).
- **Surface AST**: The concrete parse tree reflecting source syntax written by programmers, supporting sugar forms like pipelines, hole expressions, and convenience connectives.
- **Seal (`❧`)**: The terminal module punctuation token that irrevocably closes the declaration sequence of a file or unit of compilation.
- **Curried Application (`▷`)**: All functions accept exactly one argument and return a function; multi-argument invocation is expressed as nested curried application.
- **Algorithm W**: Sound and complete principal type inference engine implementing parametric polymorphism with generic scheme generalization.
- **Value Restriction**: Generalization rule stating that only syntactic values (`Literal`, `Lambda`, `Reference`, `Constructor`) may generalize polymorphic schemes in let/letrec bindings, preserving type soundness under mutability.
- **Algebraic Data Type (ADT)**: First-class sum types defined with named constructor functions, supporting recursive definitions and parameterized type arguments.
- **Exhaustiveness Checking**: Static matrix verification proving that a pattern-matching expression covers every possible variant and literal shape before runtime.

## Execution & Runtime

- **Trampoline TCO**: A constant-stack execution runtime strategy where tail-recursive function applications yield thunks rather than consuming call frames, allowing unbounded recursion without stack overflow.
- **Lexical Environment (`Env`)**: Scoped frame trees mapping identifiers to evaluated values and garbage-collected cell references with uniform lexical capture.
- **Mutable Cell (`≔˚`)**: An explicitly designated mutable memory location governed by `Cell<T>` invariance, separating mutable references from immutable bindings.
- **Rebind (`⚙︎`)**: The in-place mutation operator modifying the contents of a mutable cell within lexical scope.
- **Dynamic-Extent Exception**: Stack-unwinding error mechanism built around the open `Exception` sum type, raised via `☄` and caught via `☊`.
- **Open Exception Variant (`☈`)**: Language keyword enabling modules to introduce new exception variants into the shared `Exception` type.
- **Ambient Prelude**: The standard library (`stdlib.obk`) loaded implicitly into the root environment of every module without explicit import syntax.

## Project & Tooling Architecture

- **Project Manifest (`obfusku.toml`)**: Declarative project configuration file (Format 2) specifying module entrypoints, dependencies, and capability sandbox grants.
- **Capability Sandboxing**: Security policy restricting program access to system resources (console, filesystem, environment, process exit) validated statically and enforced at native boundaries.
- **Source Artifact**: A certified, verified directory structure representing a valid and distributable Obfusku package.
- **Non-Monolithic Workspace**: Multi-crate Cargo organization separating language pipeline stages into independent compilation units (`core`, `diagnostics`, `syntax`, `typecheck`, `runtime`, `fmt`, `lsp`, `cli`).

---

*Last updated: 2026-11-23*
