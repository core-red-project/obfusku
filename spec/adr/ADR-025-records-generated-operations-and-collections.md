# ADR-025 — Record access and update, generated ordering and rendering with checked constraints, persistent collections

## Status
Accepted

## Context
Records were construction sugar with no way to read or change a field except by matching. `<` accepted only `Int` and `Real`, there was no way to print a value, and `Array` could not be built to a computed size or grown. `Map` and `Set` did not exist. A list of values could not be sorted. Each of these made ordinary programs either impossible or quadratic.

## Decision
**Records.** `Field(base, name)` and `Update(base, fields)` join the Core (`SEMANTIC_CORE.md` §9.3). `VariantDecl` carries `field_names`, so record declarations are known across module boundaries, and the desugarer receives the declarations of already-resolved modules to reorder a named construction or pattern. The checker resolves a field against `base`'s type, or against the single record declaring that name when `base` is still unknown.

**Generated operations.** Ordering and rendering join equality (`SEMANTIC_CORE.md` §18, §18.1): total, structural, language-defined, and a static error on any type with a function-typed part. `<`, `>`, `≤`, `≥` accept any such type; `⍕ : t → Str` renders any such value. They are enforced by *obligations*: each use records `(type, Eq | Ord | Show)`; an obligation on a variable a binding generalizes becomes a constraint on that binding's `Scheme`, and instantiating the scheme re-creates it at the call site. Native schemes can carry constraints (`⍋`, `⊎`, `⍕`). Obligations that are still unresolved when the module ends cannot hold a function type and are dropped.

**Collections.** `Array` is a persistent 32-way trie with a tail buffer (`obfusku-runtime::pvec`): O(log₃₂ n) get/set/push. `Map` and `Set` are persistent AVL trees (`pmap`) over the generated order, with `Set` a map to `Unit`. Natives: `⊕ ⍪ ⌷ ⍳ ⊠ ⍋ ⇋ ⇌` (array), `⍬ ⊎ ⊖ ⌕ ⊛ ⌸ ⌹` (map), `⍭ ⊌ ⊝ ∈ ⊚ ⌻` (set) — `GLYPH_SYSTEM_DESIGN.md` §10.7.

**Ambient scope.** Imported modules are checked and evaluated against the same ambient environment as the entry module (stdlib and natives); previously they saw none of it. The language server runs the same pipeline as `check`.

## Alternatives Considered
Typeclasses with user instances were rejected for now: they need instance resolution, coherence and dictionary passing for a gain the generated operations already deliver for every type a program declares; the design leaves room to add instances later behind the same constraint mechanism. Type-directed field resolution by field name alone was rejected as ambiguous across records. A mutable array was rejected: `Cell<Array>` composes the same effect without a second mutation rule.

## Consequences
`≡` is now checked through the same obligation mechanism, so a generic `λeq(a: t, b: t)` is accepted and checked per call. Existing programs are unaffected except where they relied on `<` rejecting a type. A `Map` key or `⍋` element must have an order; a record with a function field cannot be a key, rendered, or compared. Rendering and ordering of a long `List` are iterative, but an ADT nested deeper than the interpreter's recursion limit can still exhaust it. Record update cannot change a field's type.

## Related Specification
`spec/SEMANTIC_CORE.md` §9.3, §18, §18.1, §20.2; `spec/GLYPH_SYSTEM_DESIGN.md` §10.7; `spec/CONCRETE_SYMBOLIC_GRAMMAR.md` §7.8a.
