# ADR-016 — Relational operators join the symbolic surface; `<`/`>` are kept as the family's own glyphs, not as an ASCII exemption

## Status
Accepted

## Context
ADR-014 recorded that `==`, `!=`, `<`, `<=`, `>`, `>=` were plain ASCII
because none of them had ever been assigned a dedicated glyph — a
settled, separate decision, not a fallback. That framing held only under
the assumption, live at the time, that ordinary English-spelled surface
elements (identifiers, stdlib names, host natives) were an acceptable
part of Obfusku's surface. That assumption is rejected: Obfusku is
glyph-first without exception for constructs that are not literal user
data (identifiers, string/number literals). Relational comparison is a
`BinOp` production exactly like arithmetic, which already has dedicated
glyphs (`✚ ☠︎ ✱ ÷ ⌗ −`) — there is no principled reason for one `BinOp`
family to be symbolic and its sibling to stay English-adjacent ASCII
punctuation.

## Decision
Comparison becomes a proper glyph family, built the same way as the
Binder family's modifier axis (`GLYPH_SYSTEM_DESIGN.md` §4): a strict
root per direction, augmented by a light "or-equal" postfix mark rather
than four unrelated tokens.

```
a < b      -- StrictLessThan   (kept: already the correct, minimal glyph)
a > b      -- StrictGreaterThan (kept: already the correct, minimal glyph)
a ≤ b      -- LessThanOrEqual  (= "<" + equality mark)
a ≥ b      -- GreaterThanOrEqual (= ">" + equality mark)
a ≡ b      -- Equal
a ≠ b      -- NotEqual
```

`<` and `>` are **not** an ASCII exemption reopened by this ADR — they
are retained as the family's own strict-inequality roots because they
are already, natively, the correct mathematical glyphs for "strictly
less/greater than," predating and outside the English lexicon (unlike
`map`, `print`, or `Cons`). `≤`/`≥` are `<`/`>` with the same equality
mark that distinguishes `≡` from ordinary binding (`≔`) and from
`Equal`'s own root, giving a learner one modifier ("or-equal") that
composes with either strict root — passing the inference test: someone
who has only seen `<` and `≡` can read `≤` correctly without being
told. `==`/`!=` are replaced outright by `≡`/`≠`, matching the
non-associative chaining rule already fixed in
`CONCRETE_SYMBOLIC_GRAMMAR.md` §9 row 5–6, which does not change.

Per `GLYPH_SYSTEM_DESIGN.md` §1 and ADR-014's own surviving rule
(glyph-purity), none of `≡ ≠ < > ≤ ≥` receive an ASCII alias.

## Alternatives Considered
Symbolizing all six operators with unrelated new glyphs was rejected:
`<`/`>` already are the correct minimal symbols, and replacing them
would manufacture a "found a glyph" problem `GLYPH_SYSTEM_DESIGN.md` §2
already warns against. Leaving `==`/`!=`/`<=`/`>=` as ASCII digraphs was
rejected as the one remaining inconsistency in an otherwise fully
symbolic operator surface.

## Consequences
`CONCRETE_SYMBOLIC_GRAMMAR.md` §9's `BinOp` production and its token
list are amended to replace `"=="`, `"!="`, `"<="`, `">="` with `≡`,
`≠`, `≤`, `≥`; `<`/`>` are unchanged. `obfusku-core`'s operator
vocabulary, the lexer, the type-checker's operator signatures, and
`obfusku-fmt`'s operator table all require a matching update wherever
they hold the old ASCII tokens. This ADR does not touch delimiters
(`( ) { } , :`), which `GLYPH_SYSTEM_DESIGN.md` §3/§10 already settled
on separate structural grounds and remain plain ASCII.

## Related Specification
`spec/GLYPH_SYSTEM_DESIGN.md` §4, §8 (as amended);
`spec/CONCRETE_SYMBOLIC_GRAMMAR.md` §9;
`spec/adr/ADR-014-glyph-purity.md` (comparator-specific finding reopened;
glyph-purity rule itself unchanged).
