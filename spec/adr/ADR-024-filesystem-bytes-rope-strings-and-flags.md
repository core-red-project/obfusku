# ADR-024 — Directory, existence and byte natives; rope `Str`; argument classification

## Status
Accepted. Bytes are no longer `Array<⟁>`: `ADR-027` introduces the `⌭` type and `⌁↓⌭`/`⌁↑⌭` use it.

## Context
A command-line program needs to list and create directories, test for existence, handle non-text files, build output without quadratic cost, and classify its arguments. `Str` was an immutable flat `Rc<str>`, so `✚` in a loop was O(n²).

## Decision
Filesystem natives join the `⌁` family under the same Project-root boundary and `PermissionDenied`/`IOError` tags as `⌁↓⌬`/`⌁↑⌬` (`ADR-011`, `ADR-019`): `⌁↓⌂ : ⌘ → List<⌘>` (sorted entry names), `⌁↑⌂ : ⌘ → ∅` (create, with parents), `⌁⍰⌬ : ⌘ → ○` (exists, file or directory), `⌁⍰⌂ : ⌘ → ○` (is a directory), `⌁↓⌭ : ⌘ → Array<⟁>` and `⌁↑⌭ : ⌘ → Array<⟁> → ∅` (bytes; elements outside 0..=255 raise `InvalidOperation`). `⍰` is a new direction mark (query), `⌂` and `⌭` new medium marks. Delete, move, copy, append and stat are in `ADR-027`.

`Value::Str` holds a rope (`obfusku_runtime::Text`): `✚` is O(1), the flat string is built once on first read and cached. Equality, display and every native see a plain `str` through `Deref`.

`⚑ : List<⌘> → List<⌑>` classifies arguments, with `⌑ ≔ ➤(⌘, Optional ▷ ⌘) ⟢ ◇(⌘)` declared in the ambient stdlib: `--k=v`, `--k`, `-abc` (one flag per letter), `-` and everything after a bare `--` positional. A value in a separate word stays a positional; `-5` is a flag, so negative numbers go after `--`.

## Alternatives Considered
A mutable string builder value was rejected: it needs a second string type and a place for in-place mutation. Writing the flag parser in Obfusku was rejected: `Str` has no pattern form, and an ambient stdlib function would need a glyph name. `Array<Int>` for bytes is wasteful (a `Value` per byte) but needs no new value kind.

## Consequences
Reading a rope after a long append chain costs one O(total) flatten. A loop that appends and reads the result on every iteration still copies the whole string each time, so it stays O(n²). The argument type and constructors are glyph names (`⌑`, `➤`, `◇`), so they cannot shadow a program's own declarations. Directory listing is unbounded in size.

## Related Specification
`spec/GLYPH_SYSTEM_DESIGN.md` §10.2; `spec/adr/ADR-010-io-host-boundary.md`; `spec/adr/ADR-011-filesystem-scoping.md`; `spec/adr/ADR-019-project-root-filesystem-boundary.md`.
