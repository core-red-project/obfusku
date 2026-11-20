# Cross-crate pending work (temporary)

Collected during the crate-by-crate Rust pass. Each item touches a crate other
than the one being cleaned, so it is deferred to a final sweep. Delete this file
when the list is empty.

## Done crates

`obfusku-cli`, `obfusku-core`, `obfusku-diagnostics`, `obfusku-fmt`, `obfusku-lsp`, `obfusku-runtime`, `obfusku-syntax`.

## Pending

### Use `Diagnostic::{error, warning, note, new}` instead of struct literals

Constructors exist in `obfusku-diagnostics`. Struct literals remain in:

| File | Sites |
|---|---|
| `obfusku-typecheck/src/lib.rs` | 7 |
| `obfusku-typecheck/src/infer.rs` | 4 |
| `obfusku-typecheck/src/adt.rs` | 1 |
| `obfusku-cli/src/project/modules.rs` | 7 (cli: low priority, already tidy) |
| `obfusku-cli/src/report/host_error.rs` | 3 |
| `obfusku-cli/src/project/manifest.rs`, `pipeline/run.rs`, `natives/capability.rs`, `report/render.rs` | 1 each |

### Use `Span::new` / `Span::point` instead of struct literals

`obfusku-typecheck/tests/typecheck.rs` (5);
`obfusku-runtime/tests/eval/support.rs` (1); `obfusku-cli/src/report/render.rs` (1);
`obfusku-core` tests (5).

### Adopt `obfusku-core` helpers

- `Literal::ty()`: `obfusku-typecheck/src/infer.rs` ~382 (expression literals)
  and ~1077 (pattern literals) both map literal to type by hand.
- `Expr::walk` / `children`: look for other hand-written traversals in
  typecheck (free variables, recursion checks) and runtime (`ir.rs`).
- `Type` helpers (`function`, `curried`, `arity`, `Display`): typecheck has its
  own `Type` (`describe`); decide whether the core `Type` and the checker's
  `Type` can share vocabulary.

### `obfusku-cli`

- `commands/programs.rs` tests (369 lines) could be split by theme.
- Three temp-dir helper types (`project::scratch::Scratch`,
  `tests/support` `TempObkDir`, `TempTree`) could be unified.
- `natives/{collections,text,numeric,bytes}.rs` are documented but not audited
  line by line for duplication.

### Markdown owned by the parallel agent

- `CLAUDE.md` "Where things go" still lists old cli paths
  (`src/{manifest,capability,deps}.rs`). Real layout:
  `project/{manifest,deps}.rs`, `natives/capability.rs`.
- Any doc that names `obfusku_cli::bundle` or other old module paths
  (now `project::bundle`, `report::render`, ...).

### `obfusku-runtime`

- Thread-local state (`variants` table, tag interner, recursion limit, call
  trace) is documented but still global per thread. Passing it explicitly would
  need an evaluator handle in `NativeFn` (`ADR-021` signature change in
  `obfusku-cli/src/natives`).
- `obfusku-cli` natives import `obfusku_runtime::{PVec, PMap}` (moved from
  `pvec::PVec`, `pmap::PMap`); the old module paths no longer exist.

### Built-in exceptions

`obfusku_core::exception::BUILTIN_EXCEPTIONS` is now the single table. Used by
`obfusku-syntax` (constructor synthesis) and `obfusku-runtime` (ordinals).
`obfusku-typecheck/src/adt.rs` (`AdtRegistry::build`) still lists the five
variants by hand and registers each payload type separately; derive the list
from the core table.

### `obfusku-syntax`

- Parser/desugar integration tests moved from `tests/{parser,desugar}.rs` to
  `tests/{parser,desugar}/`; `tests/lexer.rs` (117 lines) kept as is.
