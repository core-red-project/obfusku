# ADR-023 — `Str` combinators and parsing are pure host natives; the error stream and process exit join the `⌁` family

## Status
Accepted

## Context
`Str` has no pattern form (`SEMANTIC_CORE.md` §9.1), so splitting, trimming, parsing and rendering cannot be written in Obfusku. Without them, and without a way to write to the error stream or choose an exit code, a program cannot behave as a command-line tool.

## Decision
`obfusku-cli` registers pure natives, curried and host-provided (`ADR-010`, `ADR-021`): `⫽` split, `⫴` join, `⌫` trim, `∋` contains, `⍴` length (characters), `⊏` slice (characters, clamped), `↦`/`⇝` strict parse to `Result<Int|Real, Str>`. Rendering is `⍕ : t → Str` for any value (`ADR-025`). Parsing never raises; a non-finite `Real` is an `✗`. An empty separator or a negative slice bound raises `InvalidOperation`.

The `⌁` family gains `⌁↑⍉ : ⌘ → ∅` (emit to the error stream; `⍉` is a new medium mark) and `⌁↯ : ⟁ → a` (terminate with an exit code in 0..=255, anything else raises `InvalidOperation`; `↯` is a new direction mark). Both are available wherever `⌁↑`/`⌁↓` are, including the REPL, where `⌁↯` ends the session.

## Alternatives Considered
Words (`split`, `trim`) were rejected by `ADR-014`. An `Exit` exception unwinding to the top was rejected: a `Catch` would be able to swallow it, and a program could not tell "exit" from "failure". Returning the exit code from the last binding was rejected as implicit and indistinguishable from `run`'s value-printing convention.

## Consequences
`⌁↯` skips destructors, so it flushes stdout itself; there is no cleanup hook. `⌁↯`'s result type is unconstrained, so a top-level `x ≔ ⌁↯(1)` is rejected as ambiguous; use it where a type is expected or in a function annotated `∅`.

## Related Specification
`spec/GLYPH_SYSTEM_DESIGN.md` §10.2; `spec/adr/ADR-010-io-host-boundary.md`; `spec/adr/ADR-014-glyph-purity.md`; `spec/adr/ADR-021-array-combinators-are-pure-natives.md`.
