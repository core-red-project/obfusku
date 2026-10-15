# ADR-027 — Numeric library and literals, text functions, `⌭` bytes, streams, clocks, randomness and file management

## Status
Accepted

## Context
Four gaps remained at the level of primitives: numeric literals could not spell `0xFF`, `1_000`, `1e9` or the `Int` minimum, and there was no `abs`, `pow`, bitwise, `sqrt`, rounding or trigonometry; `Str` had no case mapping, search, replacement, code-point access or grapheme view, and no `\u{…}` escape; bytes were an `Array<Int>` (a `Value` per byte); and I/O could not write without a newline, read to end of input, detect end of input, write binary to stdout, append, delete, rename, copy or stat, nor read a clock or a random number.

## Decision
Each point was decided against the spec's stance — symbolic names, no overloading beyond the generated operations, no implicit coercion, effects behind the `⌁` boundary, a filesystem root — and extended only where the stance did not already answer it.

**Literals** (`CONCRETE_SYMBOLIC_GRAMMAR.md` §5). `0x`/`0b`/`0o` integers, single `_` separators between digits, an exponent without a fractional part (`5e3` is a `Real`; `e` not followed by digits stays an identifier), and `\u{h…}` (one to six hex digits, a Unicode scalar value, else a lexical error). The `Int` minimum is written `−9223372036854775808`: the lexer reads 2^63 as `i64::MIN` and the parser accepts it only directly behind `−`. `.5` stays invalid: `.` is the field-access token.

**Numbers** — plain natives, not new operators, so the Core stays frozen and the typed-operator table of §20.2 is untouched. `Int`: `∥` abs, `⋆` power (exponent ≥ 0), `⊓ ⊔ ⊗ ∼` bitwise, `⋘ ⋙` shifts (count 0..=63; `⋘` is a bit shift and never raises, `⋙` sign-extends). Overflow and `∥` of the minimum raise `IntegerOverflow` as in `ADR-008`. `⊥ ⊤ : a → a → a` are minimum and maximum for any `Ord a`, by the generated order. `Real`: `√ ⌊ ⌃ ⌾ ℯ ℓ ∿ ∾ ∦ ⋇ ∠` (sqrt, floor, ceil, round-half-away, exp, ln, sin, cos, abs, pow, atan2) with IEEE-754 semantics — an out-of-domain argument yields `NaN`, nothing raises; `⍥` finite test; constants `ϖ` and `∞`. Real results stay `Real`; `↘` is the one way back to `Int`. `show` renders `∞`, `−∞`, `NaN`.

**Text** — `⇧ ⇩` case, `⊢ ⊣` starts/ends with, `⍸` first index (`Optional`), `⤳` replace all (non-empty needle), `⌯ ⌮` code point of the first scalar / scalar of a code point, `⍂` scalars, `⌶` grapheme clusters (`unicode-segmentation`). Lengths and positions remain Unicode scalar values (`⍴`, `⊏`, `⍸`); `⌶` is the opt-in grapheme view. There is no `Char` type: a one-scalar `Str` is a character, `⌯`/`⌮` convert. Adding a Core type for it would buy no operation the `Str` forms lack.

**Bytes** — `⌭` is a new, non-generic, variant-less type (`Value::Bytes`, an immutable `Rc<[u8]>`) with generated equality, ordering (lexicographic) and rendering (`⌭"6869"`). Its operations are named `⌭` + the glyph of the `Str`/`Array` operation they twin: `⌭#` length, `⌭⌷` slice, `⌭⍪` concat, `⌭⊠` repeat, `⌭⌕` element (`Optional`), `⌭⇋`/`⌭⇌` from/to `Array<Int>`, `⌭⌘` UTF-8 encode, `⌭⍕` strict decode (`Result`). `⌁↓⌭`/`⌁↑⌭` now read and write `⌭` (previously `Array<⟁>`; `ADR-024` is amended).

**I/O** — new direction marks `⊕` append, `⊖` remove, `⇄` move, `⧉` copy; new media `⎙` text stream, `⎚` byte stream, `⌖` terminal, `⧗` wall clock, `⧖` monotonic clock, `⚀` integer dice, `⚁` real dice. `⌁↑⎙` writes without a newline and flushes; `⌁↓⎙` reads to end of input; `⌁⍰⎙` tells whether input is exhausted (so `⌁↓` need not be driven to an `EOF` exception); `⌁↑⎚`/`⌁↓⎚` are the byte forms; `⌁⍰⌖` asks whether stdout is a terminal. `⌁↓⧗` is milliseconds since the Unix epoch, `⌁↓⧖` monotonic nanoseconds (differences only), `⌁↑⧗` sleeps. `⌁↓⚀ n` is uniform in `0..n` (rejection sampling, no modulo bias; `n ≤ 0` raises), `⌁↓⚁` uniform in `[0, 1)`; the generator is a per-thread xorshift64* seeded from the clock and process id — not cryptographic. File management is rooted exactly like `⌁↓⌬` (`ADR-011`, `ADR-019`): `⌁⊕⌬` append, `⌁⊖⌬` remove file, `⌁⊖⌂` remove tree (the root itself is refused), `⌁⇄⌬` and `⌁⧉⌬` check both paths (the check precedes the operation, so the root confines a program, not a second process that rewrites the tree concurrently), `⌁⍰⌭` size, `⌁⍰⧗` modification time.

**Decided against, as decisions rather than deferrals.** Subprocesses and signal handling: spawning an arbitrary program steps outside the filesystem-root capability that `ADR-011`/`ADR-019` define, and there is no confinement that keeps the boundary meaningful; a program that must orchestrate other tools is not expressible, by choice. A current-directory query: the root is the project directory, and an absolute path would invite escapes the sandbox forbids. Operators for bitwise/power: natives keep operator typing closed. Concurrency/async: unchanged (single-threaded, strict).

## Alternatives Considered
Type classes or overloading to make `abs`/`min`/`pow` single names: rejected again (`ADR-025`) — generated `Ord` already makes `⊥`/`⊤` polymorphic, and the numeric functions differ in domain, not just type. A `Char` type: rejected as above. A `Real`-returning clock: loses sub-microsecond precision at the epoch. A cryptographic generator and seeding control: out of scope for a non-secret primitive; revisit with a security use case. Reading stdin line-by-line only: cannot read binary or detect end without an exception.

## Consequences
Source using `5e10` as `Int 5` applied to `e10` changes meaning (it was never useful). `⌁↓⌭`/`⌁↑⌭` programs that handled `Array<⟁>` must convert with `⌭⇋`/`⌭⇌`. The CLI crate gains the `unicode-segmentation` dependency. The glyph choices are arbitrary where no convention exists (`⊥ ⊤` follow lattice min/max). Grapheme segmentation follows the crate's Unicode version.

## Related Specification
`spec/CONCRETE_SYMBOLIC_GRAMMAR.md` §5; `spec/SEMANTIC_CORE.md` §15.4, §20.4; `spec/GLYPH_SYSTEM_DESIGN.md` §10.2; `spec/adr/ADR-008-int-overflow-semantics.md`; `spec/adr/ADR-024-filesystem-bytes-rope-strings-and-flags.md`; `spec/adr/ADR-025-records-generated-operations-and-collections.md`.
