# Changelog

All notable changes to **Obfusku** are documented here.

This project follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

## [1.1.0] — 2026-10-15

This release turns Obfusku from a minimal core into a language that can host real tools: a numeric and text library, bytes, records, persistent collections, a host I/O family, standalone executables, a resolved runtime and call traces. Each decision has an ADR (`ADR-022` … `ADR-028`).

### Breaking

- A `Match` over `Exception` needs a wildcard or variable arm, because programs can add variants (`ADR-026`).
- `⍘` (Real → Str) is removed; use `⍕` (`ADR-025`).
- `⌁↓⌭` and `⌁↑⌭` read and write `⌭` instead of `Array ▷ ⟁` (`ADR-027`).
- `5e3` is a `Real` literal (`ADR-027`).
- `⟐ ⌿ ⌽` are host natives, no longer functions declared in the standard library; a program that declared its own `⟐`, `⌿` or `⌽` now shadows them instead of failing (`ADR-028`).
- Release assets: the duplicated `obfusku-cli-*` binaries are no longer published; `obfusku-<os>-<arch>` and `obfusku-lsp-<os>-<arch>` are the names `install.sh` and `install.ps1` use.

### Added

#### Language

- Record field access `p.x` and persistent update `p.{x: v}` (`ADR-025`).
- Generated ordering (`<`, `>`, `≤`, `≥`, `⍋`) and rendering `⍕ : t → Str` for every type without function parts, with the constraints carried through generic functions (`ADR-025`).
- `☈` declares user exception variants; `Exception` is open (`ADR-026`).
- Numeric literals `0x…`, `0b…`, `0o…`, digit separators `1_000`, exponent without fraction `5e3`, string escape `\u{…}`, and `−9223372036854775808` (accepted only behind `−`) (`ADR-027`).
- `⌭`, the byte-string type: non-generic, no variants, ordered and renderable as `⌭"6869"` (`ADR-027`).
- Real values render `∞`, `−∞` and `NaN` (`ADR-027`).
- A module may declare a name the standard library or a native already uses; it shadows the ambient one in that module only (`ADR-028`).

#### Collections

- Persistent `Array` (a trie) with `⊕ ⍪ ⌷ ⍳ ⊠ ⍋ ⇋ ⇌` (`ADR-025`).
- Ordered persistent `Map` (`⍬ ⊎ ⊖ ⌕ ⊛ ⌸ ⌹`) and `Set` (`⍭ ⊌ ⊝ ∈ ⊚ ⌻`) (`ADR-025`).
- List combinators `⟐` (map), `⌿` (filter) and `⌽` (fold) as iterative natives: a million elements run in constant stack (`ADR-028`).

#### Numbers

- `Int` natives `∥ ⋆ ⊓ ⊔ ⊗ ∼ ⋘ ⋙` and ordered `⊥ ⊤` (`ADR-027`).
- `Real` natives `√ ⌊ ⌃ ⌾ ℯ ℓ ∿ ∾ ∦ ⋇ ∠ ⍥` and constants `ϖ ∞` (`ADR-027`).

#### Text

- `Str` is an immutable rope: `✚` is O(1) (`ADR-024`).
- `Str` natives `⫽ ⫴ ⌫ ∋ ⍴ ⊏ ↦ ⇝` (`ADR-023`).
- `Str` natives `⇧ ⇩ ⊢ ⊣ ⍸ ⤳ ⌯ ⌮ ⍂ ⌶`, grapheme-aware where the operation is about characters a reader sees (`ADR-027`).

#### Bytes

- `⌭# ⌭⌷ ⌭⍪ ⌭⊠ ⌭⌕ ⌭⇋ ⌭⇌ ⌭⌘ ⌭⍕` (`ADR-027`).
- `⌁↓⌭` and `⌁↑⌭` read and write bytes under the project root (`ADR-024`, `ADR-027`).

#### Host I/O (`⌁` family)

- Program arguments `⌁↓⌥`, environment variables `⌁↓⎈`, `obfusku run <target> -- <args>` (`ADR-022`).
- Error stream `⌁↑⍉` and process exit `⌁↯` (`ADR-023`).
- Directories and existence `⌁↓⌂ ⌁↑⌂ ⌁⍰⌬ ⌁⍰⌂` and `⚑` argument classification with the ambient `Arg` type (`ADR-024`).
- Text and byte streams `⌁↑⎙ ⌁↓⎙ ⌁⍰⎙ ⌁↑⎚ ⌁↓⎚` and terminal query `⌁⍰⌖` (`ADR-027`).
- Wall clock `⌁↓⧗`, monotonic clock `⌁↓⧖` and `⌁↑⧗` (sleep) (`ADR-027`).
- Integer dice `⌁↓⚀` (rejection sampling, no modulo bias) and real dice `⌁↓⚁`; a per-thread xorshift64*, not cryptographic (`ADR-027`).
- File management under the project root: `⌁⊕⌬` append, `⌁⊖⌬` remove, `⌁⊖⌂` remove tree (the root itself is refused), `⌁⇄⌬` move, `⌁⧉⌬` copy, `⌁⍰⌭` size, `⌁⍰⧗` modification time (`ADR-027`).

#### Tests (`ADR-030`)

- `obfusku test [project]` runs the files matched by `[test] include` (default `tests/**/*.obk`), each as its own `obfusku run`, and reports passes and failures (exit 1 on any failure, 2 when nothing matches).

#### Manifest and dependencies (`ADR-029`)

- `obfusku.toml` `format = 2`: `[project]` identity (`name`, `version`, `description`, `license`, `authors`, minimum `obfusku`), `[entry]` by module or explicit path, `[bundle] output`, reserved `[test]`, and unknown keys rejected with a suggestion. Format 1 is unchanged.
- `[capabilities]` declares what a program may use (`console`, `fs`, `env`, `args`, `clock`, `random`, `exit`); a denied native is a static error in the entry module and `PermissionDenied` elsewhere.
- `[dependencies]` from local paths or git (`github`, `gitlab`, `git` with `tag`, `rev` or `branch`), pinned in `obfusku.lock` by `obfusku fetch [--update]`; `run` never uses the network. `bundle` embeds dependencies.
- `obfusku bundle` names the executable from `[bundle] output`, then `project.name`.

#### Runtime and diagnostics

- An uncaught exception lists the innermost twelve function activations as notes (`called 'f' from here (line:column)`); a curried declaration keeps its name in every link and an anonymous function appears as `λ` (`ADR-028`).
- A native that calls back into the program (`⟐`, `⌿`, `⌽`, array combinators) counts as sixteen evaluation levels toward the recursion limit.

#### CLI

- `obfusku bundle <file|project> [-o out]` writes a standalone executable: the interpreter followed by the program's sources; modules load from memory and nothing is extracted (`ADR-022`).
- `obfusku completions <bash|zsh|fish|powershell>`.
- `obfusku check --json` prints one JSON object with `ok` and each diagnostic's `severity`, `message`, `source`, `line` and `column`.
- `-q`, `--quiet` suppresses warnings.
- `obfusku fmt --check` prints nothing and exits 1 when the file is not in canonical form; `--write` rewrites the file.

#### Tooling

- `just check` is non-mutating and adds `deny`; new recipes `format-check`, `deny`, `bench`, `package` and `changelog`.
- Git hooks in `.githooks/` (`just install` enables them): `pre-commit` runs `just check`, `commit-msg` enforces Conventional Commits with `scripts/check-commit-message.sh`.
- `cliff.toml` for release notes, `.editorconfig`, `benches/fixture.obk`.
- CI validates Conventional Commits on pull requests and runs on Linux, macOS and Windows. Release verifies the tag against every manifest and that the commit is on the default branch, re-runs `just check`, writes notes with `git-cliff` and publishes `SHA256SUMS`.
- Every workflow action is pinned by commit SHA on pinned runners.
- `install.sh` and `install.ps1` verify `SHA256SUMS` before installing.
- Examples: `records.obk`, `collections.obk`, `exceptions.obk`, `numbers_text_bytes.obk`.
- ADRs `ADR-022` to `ADR-028`.

### Changed

- **Runtime.** Variables resolve to `(depth, slot)` pairs and top-level names to shared cells before evaluation; constructor tags are interned; the depth limit and call trace are threaded explicitly instead of through thread-local state (`ADR-028`). Release timings on this machine:

  | Workload | Before | After |
  |---|---|---|
  | 3 M-iteration tail loop | 0.98 s | 0.40 s |
  | 300 k list pipeline | 0.87 s | 0.19 s |
  | 1 M record updates | 0.64 s | 0.38 s |
  | `fib(30)` | 0.52 s | 0.24 s |

  The `fib(30)` figure was not measured against a rebuilt old evaluator and is the least reliable.
- `Array` bulk construction is O(n); `⌷` copies only the requested range; `⍪` no longer clones the tail per element; sorting same-constructor ADTs skips the declaration-order lookup.
- Core AST bodies (`Lambda`, `Let`, `LetRec`, `Catch` handler, match arm) are `Rc<Expr>`; evaluation no longer deep-clones them.
- A duplicate top-level name in hand-written Core no longer overwrites what an earlier closure already resolved (the type checker rejects it in source programs).
- The formatter prints string literals that would not stay NFC with `\u{…}` escapes, so format → parse round-trips.
- The language server runs the same pipeline as `check` instead of a copy of it.
- `run` and the REPL print the canonical rendering (`⍕`) instead of Rust debug output.
- Workspace is on Rust edition 2024, `rust-version = "1.93"`, resolver 3, with clippy `pedantic` as warnings denied in CI; the allowed lints are listed with reasons in `Cargo.toml`.
- `deny.toml` follows the canonical template (`yanked = "deny"`, `wildcards = "deny"`, `Unicode-3.0` allowed).
- `ROADMAP.md` moved to `docs/ROADMAP.md`, the banner to `docs/assets/`; specifications describe the language as a general-purpose one rather than a deliberately minimal one.
- The release workflow runs the whole test suite per platform; the release profile uses LTO, one codegen unit and `strip` (binaries 2.3 → 1.6 MB and 3.2 → 2.0 MB).
- `just deny` accepts `DENY_OFFLINE=1` to reuse the cached advisory database; `just package` runs `scripts/package.sh`.
- New dependencies: `unicode-segmentation` (CLI) and `unicode-normalization` (formatter).

### Security

- A `git` or `github`/`gitlab` dependency whose location began with `-` was handed to `git` as an option, so a manifest (including a transitive dependency's) could run a command during `obfusku fetch` (`--upload-pack=…`). The manifest now rejects such locations and `git` is invoked with `--`.

### Fixed

- `obfusku test` waits forever on a test that never ends; each file now has a timeout (`[test] timeout`, default 60 s) and is killed, reported as `FAIL … timed out`.
- Language-server diagnostics used `char` columns; LSP positions are UTF-16 code units, so a line with an emoji put the underline one column early.
- The language server analyzed once per queued edit and died on a panic; queued edits collapse to one analysis per document, analysis runs on a 256 MiB stack, and a panic becomes a diagnostic instead of a dead server.
- The language server reported `could not read '<path>'` for a buffer whose file is not on disk yet; it is checked against its directory.
- Source nested deeper than 2048 expressions, patterns or types aborted the process with a stack overflow; it is a diagnostic now, as is a chain of more than 100 000 operators or calls in one declaration.
- Two parallel `obfusku fetch` runs sharing a cache failed when the second finished storing the same commit.
- The recursion-limit error pointed at `1:1`, and its trace repeated the same call twelve times; it points at the innermost call and repeated frames collapse.
- `obfusku fmt` discarded every comment. Comments between declarations stay between them, a trailing comment stays on its line, and one inside a declaration moves just above it; formatting is idempotent.
- `obfusku fmt` printed `Optional ▷ (List ▷ ⟁)` without its parentheses, which re-parses as a different type, and printed nested `⟡` arms and local bindings flat. Nested arms are indented and a run of local bindings prints as a parenthesized block.
- A `Match` with two arms of the same constructor but different nested patterns (`⁝(A(n), t)` and `⁝(B(n), t)`) was warned as unreachable; only an arm that covers every value of its constructor makes a later one unreachable.
- `⍳`, `⊠` and `⌭⊠` abort the process on a huge count; counts above 2^28 now raise `InvalidOperation`.
- Freeing a list of tens of millions of elements overflowed the stack; the spine is released iteratively.
- The language server resolved `file:///C:/…` URIs to an invalid path on Windows, and left a file's diagnostics in the editor after it closed.
- An error inside an imported module was drawn at its own line and column on the importing file.
- `install.sh` silently skipped the language server on machines with `wget` but no `curl`.
- Recursion through a native callback aborted the process with a stack overflow before the recursion limit applied; it now raises the catchable `InvalidOperation("recursion limit exceeded")`.
- `obfusku bundle` on an executable whose payload trailer was corrupt could panic; it returns an error.
- `install.ps1` reported a checksum mismatch for `obfusku-lsp` as a failed download.
- A second program evaluated on the same thread no longer inherits the first one's record field layout.
- The REPL no longer dumps a closure's AST.
- Argument classification uses glyph names (`⌑`, `➤`, `◇`) instead of ambient `Arg*` names that blocked user declarations.
- Imported modules see the standard library and the natives; previously only the entry module did.
- Records declared in one module can be constructed and matched by field name in another.
- Bare relative filenames (`obfusku run main.obk`) resolve imports from the current directory.
- Deep non-tail recursion raises a catchable `InvalidOperation` instead of overflowing the stack; the interpreter runs on a 1 GiB stack.
- `print` on a closed pipe raises `Failure("IOError", …)` instead of panicking.
- Uncaught runtime errors report the source location of the raise.
- Module search skips hidden directories and `target`, and does not follow symlinks.

### Known limits

- `⌁⇄⌬` and `⌁⧉⌬` check both paths before acting; the root confines a program, not another process rewriting the tree concurrently.
- `just bench`, `just changelog` and the new workflows have not run in GitHub Actions or on a machine with `hyperfine` and `git-cliff`.
- A language-level `test` construct does not exist; `obfusku test` runs whole files (`ADR-030`).
- Pedantic cast lints are allowed workspace-wide; numeric conversions are not audited site by site.

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

[Unreleased]: https://github.com/core-red-project/obfusku/compare/v1.1.0...HEAD
[1.1.0]: https://github.com/core-red-project/obfusku/compare/v1.0.0...v1.1.0
[1.0.0]: https://github.com/core-red-project/obfusku/releases/tag/v1.0.0
