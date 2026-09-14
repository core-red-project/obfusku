# Obfusku — Glyph System Design

Status: **established** — built on the frozen `SEMANTIC_CORE.md` and the
established `ABSTRACT_GRAMMAR.md`. The vocabulary here is the one
`CONCRETE_SYMBOLIC_GRAMMAR.md` (frozen) assigns concrete tokens to.

The old catalog (`src/symbols/meaning.rs`, `src/lexer/symbols.rs`) is
inventoried below as historical material only. It is not a vocabulary to
preserve. Every glyph in this document earns its place by a stated semantic
relationship to the Abstract Grammar — never by appearance alone.

---

## 0. Design principles

1. **Families and modifiers, not a dictionary.** A learner should acquire
   a small set of roots and a small set of modifier axes, then be able to
   read compositions of them they've never seen.
2. **Not every semantic concept needs a glyph.** If a construct is sugar
   over another (per `SEMANTIC_CORE.md` §16's desugaring table), it should
   normally *reuse* the family it desugars into rather than mint its own —
   the same discipline that made `if` sugar over `Match` at the semantic
   level should show up at the glyph level too. Tested per construct below,
   not assumed.
3. **The inference test, applied concretely, not just claimed.** For every
   family proposed, at least one worked example shows a *new*, previously
   unseen composition and argues whether a learner who knows the root and
   modifiers could plausibly read it. If a family can't clear this, it's
   rejected or reworked.
4. **Peers are not always modifiers of each other.** Root+modifier
   composition fits things that are genuinely variations on one concept
   (mutability of a binding). It does not fit things that are independent
   peers of equal standing (`Int` and `String` are not variants of one
   another) — those need a coherent *matched set*, not a forced
   root/modifier relationship. Both patterns are legitimate; using the
   wrong one for a given case is itself a design mistake.
5. **Historical identity is a point in a glyph's favor, not a veto on
   removing it, and not a reason to keep it if it never had real semantic
   grounding to begin with** (rule 5/6 from the brief).

---

## 1. Typography and practical constraints (real, not decorative)

- **No Unicode combining diacritics as modifiers.** Combining marks
  normalize inconsistently (NFC/NFD), behave unpredictably in terminals
  and editors, and can silently break copy/paste and equality checks.
  Every modifier proposed below is a **standalone codepoint**, positioned
  adjacent to its root, never combined into one grapheme cluster.
- **Modifiers must stay visually light.** A mark that appears on every
  mutable binding in a program cannot carry the visual weight of a
  standalone glyph, or the page turns to noise (rule 8 — no modifier
  stacking that reads as clutter).
- **Distinguishability at typical monospace sizes and common terminal
  fonts is a hard requirement**, not an aesthetic preference — a family
  whose members are only distinguishable when zoomed in has failed before
  it starts. Where this document proposes visually close variants (the
  primitive-type set, §5), it says explicitly why they stay distinguishable
  at normal size.
- **No ASCII fallbacks for symbolic operators — Obfusku is glyph-pure.**
  Every operator this document and `CONCRETE_SYMBOLIC_GRAMMAR.md` assign a
  dedicated glyph to (`✚ ☠︎ ✱ ÷ ⌗ − ¬ ∧ ∨ ⊻`, …) has no ASCII spelling; this
  is a permanent design choice, not a gap awaiting a table. This is
  unrelated to `<`/`>`/`<=`/`>=`/`==`/`!=` being plain ASCII (§8) — those
  were never assigned a symbolic glyph to begin with, so they are not a
  fallback for anything; the rule here concerns only operators that *do*
  have a dedicated glyph.

---

## 2. What the old catalog actually contains (authoritative source)

Pulled directly from `src/symbols/meaning.rs` and `src/lexer/symbols.rs`,
not from `LANGUAGE_SPEC.md`/grimoires, which the earlier audit already
found to have drifted from the real implementation. One more instance of
that same drift turned up here, worth naming as evidence for principle 5:
the file's own doc comments claim `Continue` is `⊕` and describe `Xor` as
`⊕` too, while the actual `register()` calls bind `Continue` to `↺` and
`Xor` to `⊻` — drift *inside a single file*, between its comments and its
own registrations. This is exactly the failure mode of accumulating
glyphs one at a time without a system holding them together, and it's
part of why this pass doesn't treat the old catalog as authoritative
vocabulary even where it's internally consistent.

The catalog has ~90 entries across type sigils, arithmetic/comparison/
logical operators (each with an ASCII alias), control-flow block pairs,
I/O, exceptions, modules, stack ops, and delimiters — organized by
*implementation category* (how the VM dispatches on them), not by any
learner-facing semantic family. That's the structural problem this
redesign exists to fix, independent of whether any individual glyph is
good.

---

## 3. Enclosure and grouping — the biggest structural finding

The old catalog has **five separate paired block delimiters**: `λ…Λ`,
`⟨…⟫` (with `⟩` for else in between), `⊂…⊃`, `⟡…⟣`, and an effectively
unclosed `☄…☊`. Each pair is a bespoke, unrelated glyph pair memorized
independently — the single clearest instance of the "vocabulary, not
family" problem named in the brief.

**This entire pattern is a symptom of a statement-oriented mental model**
(blocks that must be explicitly opened and closed) that the frozen Core
doesn't have. `ABSTRACT_GRAMMAR.md` is fully expression-oriented: a
`Lambda`'s body is just "the expression that follows," not a block
requiring a terminator. Under that grammar, most of these closing glyphs
have nothing to close *against* — there's no block, only a value.

**Resolution:**
- **No paired closing glyph for `Lambda`, `Conditional`, or single-body
  constructs.** A function's body ends where the expression ends,
  determined by ordinary precedence — the same way `a + b * c`'s parse
  needs no explicit "end of addition" marker. `Λ` is dropped entirely, not
  replaced.
- **One generic grouping/list mechanism**, reused everywhere the grammar
  needs a *delimited list of items* rather than a single trailing
  expression — `Match` arms, ADT variant lists, record fields, argument
  lists. This needs exactly two things: a light separator mark between
  items, and ordinary parenthesis-like enclosure where grouping must be
  explicit (the same role ordinary parens already play in expressions).
  No construct-specific enclosure pair is needed beyond this one, generic,
  reused shape.

**Inference test:** a learner who knows "commas separate list items,
parens group when needed" already knows how to read a `Match`'s arm list,
a variant declaration's field list, and a function call's argument list —
one mechanism, not four bespoke ones. This directly satisfies principle 2:
the old design gave `Match`, loops, and conditionals each their own
closing glyph because nothing forced the question of whether they actually
needed one; the expression-oriented Core answers that question for free.

---

## 4. Binder family

**Root: `≔`, cherry-picked from the old catalog's `Bind` ("creates
immutable binding").** Strong candidate on its own terms, independent of
history: it already reads as "defined as" in existing mathematical
convention, which is exactly `Let`'s meaning (Core §11) — not an arbitrary
shape, a borrowed one with real prior meaning.

**Modifier axis: mutability and export, as two independent light postfix
marks — not two unrelated replacement glyphs.**

```
name ≔ value              -- ordinary immutable ValueDeclaration
name ≔˚ value              -- mutable (Cell-producing) binding
name ≔⟳ value              -- exported binding
name ≔˚⟳ value             -- mutable AND exported
```

- **Mutability mark**: a new, small ring-like postfix mark (illustrated
  as `˚`, a standalone spacing character, not a combining diacritic —
  exact codepoint is Concrete Grammar work). Genuinely new, because
  nothing in the old catalog was designed to function as a light modifier
  at all — everything there is a standalone token. This is exactly the
  case principle 5/§9 anticipates: new glyphs only where the catalog
  can't supply the *role*, not just the meaning.
- **Export mark**: cherry-picked from `⟳` (`Export`) — but repositioned
  from a standalone statement-level glyph to a light postfix modifier.
  The semantic fit (outward circulation = made available elsewhere) is
  real and worth keeping; the *role* it plays changes to fit the family
  structure.

**`FunctionDeclaration` gets its own root, deliberately not a `≔`
variant** — `Abstract_Grammar.md` §2.2 makes `FunctionDeclaration` a
separate production from `ValueDeclaration` specifically so the `LetRec`
`Lambda`-only restriction is structurally enforced, and the glyph system
should preserve that visible distinction rather than paper over it with
one shared binder root.

**Root: `λ`, cherry-picked — the strongest possible historical/cultural
candidate in the entire catalog.** Directly named in the Core's own
grammar (`Lambda`), universally recognized outside Obfusku too. Kept
without its old closing partner `Λ`, per §3.

**Reserved/invalid combination:** the mutability mark cannot attach to
`λ` — a function slot in a `LetRec` group is never a `Cell` (Core §11's
restriction is exactly that recursive bindings are `Lambda`s, never
mutable cells), so `λ˚` is not a legal combination and is rejected at the
grammar level.

The export mark applies to a top-level function the same as any other
module-boundary binding. Its exact position is `λf(...): T →⟳ body` —
immediately after the return-type arrow, per `CONCRETE_SYMBOLIC_GRAMMAR.md`
§8.4 (the canonical position; see ADR-005) — never immediately after `λ`
and never before the function's name.

**Inference test:** a learner who has only ever seen `x ≔˚ 0` (a mutable
binding) and separately `f ≔⟳ …` (an exported ordinary binding) should be
able to read `y ≔˚⟳ []` correctly as "a mutable, exported binding" without
having seen that exact combination before — the two marks are independent
and their meanings don't interact.

---

## 5. Mutation: reading and rebinding a cell

- **Explicit reference-taking**, required wherever `SEMANTIC_CORE.md` §12
  needs a real handle rather than a snapshot (closures that mutate a
  captured cell, passing a cell by identity): a light prefix mark on the
  name, illustrated as a small superscript ring matching the mutability
  postfix mark's shape (`˚x`) — deliberately drawn from the *same* small
  mark used in the Binder family's mutability modifier, reused in prefix
  position rather than invented fresh, because it's the same underlying
  concept (this cell's mutable identity) appearing at a different
  grammatical position. This is principle 4 in reverse: here, two
  positions genuinely are the same concept, so they should share a mark.
- **Rebind**: cherry-picked `⚙︎` (old `Assign` — "evaluates and assigns to
  variable") for `MutRebind`. Good fit on inspection: the old catalog
  already separated `Assign` from `Bind`, which maps precisely onto the
  Core's own separation of ordinary binding (`Let`) from cell mutation
  (`MutRebind`) — a case where the old catalog's category boundary was
  already correct, just not connected to a coherent family narrative.

```
˚x ⚙︎ x ✚ 1     -- rebind x (must have been captured/declared with ˚) to x+1
```

---

## 6. Type family

### 6.1 Primitive base types — a matched set, not a root+modifier family

`Int`, `Real`, `String`, `Bool`, `Unit` are peers, not variations of one
underlying concept (principle 4) — forcing them into a root+modifier
scheme would be artificial. What they need is mutual, at-a-glance
distinguishability and, ideally, real semantic grounding rather than
imported-for-shape borrowing.

- **`Int` — keep `⟁`.** Reasonably distinct, no collision, no
  demonstrably better alternative; kept on its own footing, not because
  it's old.
- **`String` — keep `⌘`.** Same reasoning.
- **`Real` — keep `⧆`.** Visually distinct enough from `⟁` at normal size
  (different stroke count and silhouette) to survive the distinguishability
  requirement in §1.
- **`Bool` — reject `☍`, replace with a real family.** `☍` (astrological
  opposition) has no stated semantic connection to true/false anywhere in
  the old documentation — it is exactly the "here's a glyph I found"
  pattern the brief names directly. The old catalog's **`True`/`False`
  pair (`◉`/`◎`) is genuinely good** — same base circle, differentiated by
  fill state, a real minimal family. **Promote that logic to cover `Bool`
  itself**: use the unfilled circle `○` as the type, so the three form one
  coherent, inferable set: `○` (the type), `◉` (filled = true), `◎`
  (ring-only = false). This is the single strongest new family in this
  document, because it was already half-built in the old catalog and just
  needed to be completed rather than invented from nothing.
- **`Unit` — repurpose `∅`.** The old catalog spent `∅` on universal null,
  which the frozen Core rejected outright (`Optional<T>` instead — design
  draft §4). That frees a glyph with strong pre-existing mathematical
  grounding (empty set = "nothing of informational content") for exactly
  the role `Unit` needs. Not a new assignment on a whim — a freed, well-fit
  glyph finding its correct home.

**Inference test:** `○`/`◉`/`◎` — a learner seeing `◎` for the first time,
having already learned `◉` = true and `○` = the Bool type, can infer "the
other one" correctly without being told, purely from the shared circle and
the fill-state axis. This is the clearest possible demonstration of the
whole document's stated goal.

### 6.2 Generic/wrapper types — no dedicated sigils at all

`Array<T>`, `List<T>`, `Optional<T>`, `Result<T,E>`, `Cell<T>`, and
`Exception` are all `TypeReference`s (Abstract Grammar §5) — ordinary
named, generic types, not members of the closed primitive set. **The old
catalog's `⌬` (Array) and `⌖` (Map) are rejected outright**, not for being
poor shapes, but for a category error one level up: giving bespoke sigils
to *some* library-defined generic types and not others was arbitrary by
construction, no matter how well-chosen any individual glyph was. The
fix is systemic — these are written as ordinary type names (short,
spelled identifiers, or a small set of standard abbreviations decided at
the Concrete Grammar stage), combined via ordinary `TypeApplication`
juxtaposition (Abstract Grammar §5), the same mechanism any user-defined
generic type uses. No new sigil vocabulary is needed here at all — this
section's finding is an elimination, not an addition.

---

## 7. Flow family: `Match`, and what doesn't need its own glyph

**Root: a match-introducer glyph plus a light arm-separator**, replacing
the old catalog's three-part `⟡…⟢…⟣` (start/arm/end). Per §3, no closing
glyph is needed — arms are a delimited list (the generic grouping
mechanism), not a block needing a bespoke terminator. Candidate: keep the
old `⟡` as the match-introducer root (a real, recognizable shape with no
collision), keep `⟢` repurposed as the generic list-separator role from
§3 rather than a match-specific "arm" glyph — i.e., `⟢` graduates from a
one-construct glyph into the shared enclosure family's separator, used
identically for arm lists, variant lists, and field lists. Drop `⟣`
entirely (§3's finding: no closing glyph needed).

**Arm results and function types share one arrow, deliberately — not a
collision.** `→` (freed: its old role, "directs result to target
variable," is now handled entirely by the Binder family, §4) is proposed
for both a `Match` arm's result *and* a `FunctionType`'s arrow
(Abstract Grammar §5). This mirrors `SEMANTIC_CORE.md` §2's own finding
that value- and type-level application are genuinely the same operation
in different registers — arm-result and function-type arrows are both
"leads to," just in different grammatical positions, and Design Law 6
(equivalent concepts, related syntax) argues for sharing rather than
differentiating them.

### 7.1 `Conditional` gets no glyph of its own — the clearest NOTA case

`if`/`else` (Abstract Grammar §3.7) desugars entirely to a two-arm
exhaustive `Match` on `Bool` (Core §16) — and unlike `Pipe` (§8), this
surface form doesn't change reading order or add any compositional power
over what `Match` already provides; it's a pure rename. Per principle 2,
it gets **no dedicated glyphs at all** — a conditional is written using
exactly the `Match` family (§7's root and arm syntax) against the literal
`◉`/`◎` values from §6.1. This is a direct elimination of the old
catalog's `⟨`/`⟩`/`⟫` trio, not a replacement for it.

### 7.2 `Return` and `Call` — eliminated, not replaced

Neither has a place in an expression-oriented Core. `SEMANTIC_CORE.md` has
no `Return` form at all — a `Lambda`'s value *is* its body's value; there
is no mid-body early exit outside the exceptional channel (`Raise`,
already covered, §9). The old `⤶` (`Return`) is dropped with nothing to
replace it. Similarly, `Application` (Abstract Grammar §3.3) is ordinary
juxtaposition/parenthesized calling — nothing requires a prefix
"now-calling" marker before it, so `⤷` (`Call`) is dropped too. Both
eliminations follow directly from applying principle 2 rather than from
any flaw in the original shapes.

---

## 8. Pipeline family

**A real, load-bearing collision was found and resolved, not glossed
over.** Every document in this project so far used `▷` as a placeholder
for pipe/application — but the old catalog already assigns `▷` to
`GreaterThan`, with `◁` for `LessThan`. Reusing `▷` for both would be
exactly the "forces unrelated concepts into the same family" failure the
brief warns against.

**Resolution, which improves both assignments rather than just avoiding
the clash:** numeric comparison doesn't need an exotic glyph at all —
`<`/`>`/`<=`/`>=` are already universally legible with zero learning cost,
and this is a case where *not* reaching for a special Unicode form is the
right call (principle 2's logic applied one level further: not every
concept needs a *symbolic* glyph, some are better served by staying
plain). That frees `▷` — which already visually reads as a forward/play
triangle — for **`Pipe`**, where "flowing forward" is exactly the right
metaphor and a far better semantic fit than it ever was for comparison.

```
xs ▷ filter(◈ ○> 0) ▷ map(◈ ✱ 2)
```

(`○>` above is illustrative ASCII-mixed shorthand for whichever
comparison glyph is finalized in Concrete Grammar — not a proposal in
itself.)

- **`◈` (pipeline-value reference)** — adopted as-is; it was already used
  as a working stand-in throughout this project's design documents, isn't
  claimed by the old catalog, and its light diamond-outline shape suits a
  mark that appears frequently inline (§1's weight requirement).
- **`•` (argument hole)** — a plain bullet, minimal weight, universally
  available, not claimed by the old catalog. Deliberately distinct in
  shape from `◈`, since `SEMANTIC_CORE.md` §8 is explicit that these are
  formally unrelated mechanisms (a hole never opens a lambda scope, a
  pipeline reference always does) — the glyphs should not look like
  variants of one root, because they aren't.

**Reserved/invalid combination:** a `StageExpression` cannot mix `•` and
`◈` referring to the same argument position — presence of `◈` anywhere in
a stage forces the expression-with-reference interpretation (Core §7)
unconditionally; `•` inside such a stage would be ambiguous about which
mechanism governs that position, and is rejected rather than given an
implicit resolution rule.

---

## 9. Exception family

**Kept largely as history had it — this is the strongest pre-existing
semantic reasoning anywhere in the old catalog.** `☄` (`TryStart`) and `☊`
(`CatchBlock`) were justified in `grimoires/02_Philosophy_and_Symbol_
Primacy.md` as "comet and ascending node — disruption and containment,"
and that reasoning holds up on independent inspection, not just as
retroactive narrative: a comet is a genuine visual metaphor for an
unpredictable, disruptive event, and an ascending node (where a chaotic
orbit crosses back into a stable reference plane) is a strong metaphor for
"catching" that disruption back into ordinary control flow. Adopted for
`Raise`/`Catch` (Core §15) without change. `⚠` (`Throw`) is dropped as
redundant now that `☄` covers the raising side of the family directly —
one root per concept, not two overlapping ones.

`FinallyBlock` (`☋`) has no place in the frozen Core at all — there is no
`finally` construct anywhere in `SEMANTIC_CORE.md`, and inventing one now
would violate "do not add features" from the standing instructions across
this whole project. Dropped, not replaced.

---

## 10. Delimiters, literals, comments — mostly unremarkable, kept plain

Ordinary brackets/parens/commas stay ASCII, per §3's generic grouping
mechanism — there's no semantic reason to spend a Unicode glyph on
"argument separator." Block comments (`⌈…⌉`) are kept as-is: a
genuinely well-formed pair (visually related open/close shapes, no
collision, no semantic ambiguity) with nothing to fix.

---

## 10.1 `❧` — evaluated as a candidate seal glyph, not restored by default

Explicitly re-examined against §3 rather than reinstated on historical
grounds. The old framing — "end program," mandatory, no exceptions — was
proposed as a candidate for a *general* structural seal: one glyph closing
function bodies, `Match`, module regions, alike. That general version does
**not** survive: §3 already established that `Lambda` bodies, `Match`
arms, and `Conditional` need no closing glyph at all, because each has its
own natural termination signal (an expression's value, or a delimited
list's own structure). Reintroducing `❧` at that level would just be `Λ`
or `⟣` renamed — undoing §3's elimination, not extending it.

A narrower role does survive, and for a structural reason rather than
identity. `Module ::= Declaration*` (Abstract Grammar §1) has no stated
termination mechanism at all — unlike a `Lambda` body or a `Match` arm
list, a module's declaration sequence has no expression *value* to fall
back on for determining its own boundary, so a terminal marker here isn't
redundant with anything else in the grammar. **`❧` is adopted as a
mandatory, once-per-module seal**, closing `Module`'s declaration sequence
specifically — nowhere else. Its function is a completeness assertion, not
a block delimiter: a truncated or accidentally-cut-off source file becomes
a definite parse error instead of an ambiguous "maybe there were supposed
to be more declarations." This is also why it must stay mandatory rather
than optional (matching the old rule's severity, now for a stated reason
rather than ritual insistence) — an optional integrity marker isn't one;
if it's sometimes present and sometimes not, truncation and a deliberately
short module become indistinguishable.

`❧` correctly takes no modifier and joins no family — not a violation of
the family-first principle, just its proper limit. It has no peers or
variants (a "mutable seal" or "exported seal" is meaningless), so forcing
it into a family would be the same mistake in reverse: manufacturing
structure where none exists.

This reveals a small, genuine gap in `ABSTRACT_GRAMMAR.md` §1, not
previously noticed — `Module`'s production is amended minimally to state
the boundary explicitly:

```
Module ::= Declaration* Seal
```

No other production changes; this is not a reopening of the grammar, only
the precise consequence of adopting `❧` in this role.

---

## 11. Reserved/invalid combinations — consolidated

- Mutability modifier (`˚`) on `FunctionDeclaration`'s `λ` — invalid (§4).
- `•` and `◈` referring to the same argument position within one stage —
  invalid (§8).
- Export modifier on a `LocalBinding` — invalid; Core §20 only defines
  export at module-boundary declarations, and the grammar should reject
  it locally rather than silently ignore it.
- Any glyph from the closed `BaseType` set (§6.1) used where a
  `TypeReference` is grammatically required, or vice versa — these are
  different productions (Abstract Grammar §5) and shouldn't be
  interchangeable at the surface either.

---

## 12. Summary: rejected, kept, and new

| Glyph | Old meaning | Verdict | Why |
|---|---|---|---|
| `λ` | FunctionStart | **Kept** | Strongest historical/cultural fit in the catalog |
| `Λ` | FunctionEnd | **Dropped** | No block to close (§3) |
| `⟨⟩⟫` | If/Else/EndIf | **Dropped** | `Conditional` is pure sugar over `Match` (§7.1) |
| `⊂⊃` | Loop start/end | **Dropped** | No general loop construct exists in the Core |
| `⟡⟢⟣` | Match start/arm/end | **Partially kept** | `⟡` root kept; `⟢` graduates to generic separator; `⟣` dropped (§3, §7) |
| `☄☊` | Try/Catch | **Kept** | Genuine semantic grounding, not just aesthetic (§9) |
| `⚠` | Throw | **Dropped** | Redundant with `☄` |
| `☋` | Finally | **Dropped** | No such construct in the frozen Core |
| `⤶⤷` | Return/Call | **Dropped** | No place in an expression-oriented Core (§7.2) |
| `≔` | Bind | **Kept, as family root** | Strong pre-existing "defined as" meaning (§4) |
| `⚙︎` | Assign | **Kept, repurposed** | Correct category boundary, now connected to `MutRebind` (§5) |
| `⟳`/`⟲` | Export/Import | **`⟳` kept as modifier; `⟲` finalized for import** | Repositioned from statement to light postfix mark (§4); see `CONCRETE_SYMBOLIC_GRAMMAR.md` §14 |
| `⟁⧆⌘` | Int/Real/String | **Kept** | No collision, no better alternative, judged on their own merits |
| `☍` | Bool | **Rejected** | No stated semantic grounding — the exact "found a glyph" pattern being avoided |
| `◉◎` | True/False | **Kept, promoted** | Genuinely good existing minimal family, extended to cover `Bool` itself (§6.1) |
| `∅` | Null | **Repurposed → Unit** | Freed by rejecting universal null; strong pre-existing "nothing" grounding fits `Unit` better |
| `⌬⌖` | Array/Map | **Rejected outright** | Category error: bespoke sigils for some generic types and not others (§6.2) |
| `▷◁` | Greater/Less-than | **Reassigned** | `▷` moves to `Pipe`; comparison uses plain ASCII instead (§8) |
| `→` | Arrow (assignment target) | **Kept, repurposed** | Freed by the Binder family; reused for `Match`-arm/function-type arrow (§7) |
| `◈`, `•` | — | **New** | No prior catalog role exists for a light inline reference/hole mark (§8) |
| `˚` (mutability), postfix `⟳` role | — | **New / repositioned** | No modifier-role precedent in the old catalog at all (§4) |
| `○` (Bool type) | — | **New, minimal extension** | Completes an already-good existing pair rather than inventing from nothing (§6.1) |

---

## Verdict

This is a candidate glyph system, not a locked one — several exact
codepoints (the mutability mark, the argument-hole mark's final shape,
short names for generic wrapper types) are explicitly left to a Concrete
Symbolic Grammar pass. What's fixed here is the *architecture*: fewer
roots, real modifier axes, one generic enclosure mechanism instead of
five bespoke ones, and — per the added instruction this round — a
demonstrated willingness to eliminate a glyph entirely (`Return`, `Call`,
`Conditional`'s trio, `Finally`) rather than assign one out of habit. The
next document downstream should be the Concrete Symbolic Grammar: exact
codepoints and rendering — not another semantics or grammar pass. (ASCII
fallbacks, mentioned here in an earlier draft, are not part of that
handoff — see §1: Obfusku is glyph-pure, by design, not by omission.)
