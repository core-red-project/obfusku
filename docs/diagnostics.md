# Diagnostic Codes

<!-- Comprehensive reference catalog of diagnostic codes, compiler error categories, and remediation advice. -->

**Purpose:** This document catalogs all diagnostic codes emitted across Obfusku compilation, type inference, runtime evaluation, and project management pipelines.

---

## Diagnostic Categories

Obfusku reports diagnostics with structured severity levels (`error`, `warning`, `note`) categorized by functional subsystem:

| Prefix | Subsystem | Description |
|---|---|---|
| `LEX-` | Lexer & Tokenizer | Unicode validation, tokenization, delimiters, and glyph encoding |
| `SYN-` | Surface Parser | Concrete symbolic grammar, declarations, and AST structure |
| `TYP-` | Type Inference | Hindley-Milner Algorithm W, unification, arity, and exhaustiveness |
| `RT-`  | Runtime Evaluator | Dynamic evaluation faults, stack traces, and arithmetic boundaries |
| `PRJ-` | Project & Modules | Manifest parsing, module resolution, and dependency graph validation |
| `CAP-` | Capability Sandbox | Sandboxed host I/O permissions and filesystem boundaries |

---

## Lexical Diagnostics (`LEX-`)

- **`LEX-0001`**: Source text is not in Unicode Normalization Form C (NFC). Obfusku strictly mandates NFC source files (`spec/CONCRETE_SYMBOLIC_GRAMMAR.md` §2).
- **`LEX-0002`**: Unexpected character or unrecognized glyph in source text.
- **`LEX-0003`**: Unterminated string literal (`"…"`).
- **`LEX-0004`**: Invalid Unicode escape sequence (`\u{…}`) in string literal.
- **`LEX-0005`**: Unterminated block comment (`⌈ … ⌉`).
- **`LEX-0006`**: Malformed numeric literal (invalid radix prefix `0x`, `0b`, `0o`, or trailing exponent).
- **`LEX-0007`**: Invalid character literal or empty byte string literal (`⌭""`).
- **`LEX-0008`**: Unterminated raw string or missing delimiter.

---

## Syntax Diagnostics (`SYN-`)

- **`SYN-0001`**: Unexpected token encountered during recursive descent parsing.
- **`SYN-0002`**: Missing terminal module seal (`❧`). Every module must be sealed at declaration end.
- **`SYN-0003`**: Malformed function declaration or invalid parameter list syntax.
- **`SYN-0004`**: Malformed type declaration; constructors must follow variant specification rules.
- **`SYN-0005`**: Invalid pattern syntax in `Match` expression.
- **`SYN-0006`**: Unbalanced parentheses, brackets, or braces (`( )`, `[ ]`, `{ }`).
- **`SYN-0007`**: Duplicate field name in record declaration or update syntax.
- **`SYN-0008`**: Expected expression on the right-hand side of declaration binding (`≔`).
- **`SYN-0009`**: Invalid constructor name in variant pattern (constructors must begin with uppercase or glyph).
- **`SYN-0010`**: Malformed exception declaration syntax (`☈`).
- **`SYN-0011`**: Invalid import declaration syntax or malformed module reference.
- **`SYN-0012`**: Hole expression (`•`) placed outside a valid partial application context.

---

## Type Diagnostics (`TYP-`)

- **`TYP-0001`**: Type mismatch: expected type `T1`, found incompatible type `T2`.
- **`TYP-0002`**: Occurs check failure: type variable occurs within the type equation, resulting in an infinite type.
- **`TYP-0003`**: Unbound identifier or variable in expression scope.
- **`TYP-0004`**: Constructor arity mismatch: constructor expects `N` arguments, received `M`.
- **`TYP-0005`**: Non-exhaustive pattern matching: matrix coverage algorithm identified uncovered cases.
- **`TYP-0006`**: Unreachable match arm detected during exhaustiveness checking.
- **`TYP-0007`**: Open exception match requires a wildcard (`_`) or catch-all variable arm (`ADR-026`).
- **`TYP-0008`**: Value restriction violation: non-syntactic value expression cannot generalize polymorphic type variables.
- **`TYP-0009`**: Unknown record field: attempted to access or update field `f` not present on record type.
- **`TYP-0010`**: Record update type mismatch: update expression type disagrees with declared field type.
- **`TYP-0011`**: Non-orderable type: relational comparison (`<`, `≤`, `>`, `≥`) requires an ordered type without function components.
- **`TYP-0012`**: Non-renderable type: string rendering (`⍕`) requires a type without function components.
- **`TYP-0013`**: Type constructor arity error: expected `N` type arguments for generic type `G`, received `M`.
- **`TYP-0014`**: MutCell element type mismatch on rebind operation (`⚙︎`).
- **`TYP-0015`**: Attempted mutation of an immutable binding; mutability requires an explicit `Cell<T>`.
- **`TYP-0016`**: Generic type parameter shadowing or conflicting scheme instantiation.

---

## Runtime Diagnostics (`RT-`)

- **`RT-0001`**: Unhandled exception unwound past root execution frame; diagnostic includes innermost call trace.
- **`RT-0002`**: Integer arithmetic overflow during evaluation of fixed-width 64-bit integer operations.
- **`RT-0003`**: Integer division by zero (`÷`).
- **`RT-0004`**: Array index out of bounds on direct indexing operation (`⌷`).
- **`RT-0005`**: Invalid UTF-8 sequence during byte-string to text decoding native (`⌭→Str`).
- **`RT-0006`**: Empty collection access on non-empty collection prerequisite native.
- **`RT-0007`**: Host process termination via exit native (`⌁↯`).
- **`RT-0008`**: Invalid numeric conversion or NaN magnitude boundary violation.
- **`RT-0009`**: Recursive stack depth exceeded in non-tail-call context (trampoline failure).
- **`RT-0010`**: Deadlock or unresolvable resource contention in native runtime thread pool.

---

## Project & Capability Diagnostics (`PRJ-` / `CAP-`)

- **`PRJ-0001`**: Missing project manifest `obfusku.toml` in project directory root.
- **`PRJ-0002`**: Unsupported manifest format version; currently supported format versions: 1, 2.
- **`PRJ-0003`**: Missing entry module declared in manifest `[entry] module`.
- **`PRJ-0004`**: Ambiguous module resolution: multiple files match module identifier under project root.
- **`PRJ-0005`**: Unlocked git dependency in manifest requires `obfusku fetch` to pin in `obfusku.lock`.
- **`PRJ-0006`**: Circular project dependency graph detected.
- **`PRJ-0007`**: Target interpreter version mismatch between manifest constraint and running binary.
- **`PRJ-0008`**: Test suite failure: test program exited with non-zero status or unhandled diagnostic.
- **`CAP-0001`**: Permission denied: program attempted to use console output without `capabilities.console` grant.
- **`CAP-0002`**: Permission denied: program attempted to read/write filesystem without `capabilities.fs` grant.
- **`CAP-0003`**: Filesystem boundary escape: attempted path resolves outside entry directory or project root.
- **`CAP-0004`**: Permission denied: program attempted to read CLI arguments without `capabilities.args` grant.
- **`CAP-0005`**: Permission denied: program attempted to access environment variables without `capabilities.env` grant.

---

*Last updated: 2026-11-23*
