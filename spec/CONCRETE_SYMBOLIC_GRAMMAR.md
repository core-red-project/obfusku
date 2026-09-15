# Obfusku — Concrete Symbolic Grammar

Status: **frozen.**

## 1. Status and relationship to previous documents

Builds on three frozen/established documents without revising any of them:
`SEMANTIC_CORE.md` (meaning), `ABSTRACT_GRAMMAR.md` (structure, with
placeholder tokens like `"="`, `"=>"`, `"match"`, `"type"`, `"import"`),
`GLYPH_SYSTEM_DESIGN.md` (families, roots, modifiers). This document's job
is narrow and specific: replace every remaining placeholder with an actual
token, and resolve every ambiguity that only becomes visible once real
tokens exist side by side. No semantics change here. Two small, load-bearing
findings surfaced while doing this — both are refinements of what those
documents left open, not contradictions of them, and are marked as such
inline rather than silently folded in.

**Finding 1**: `SEMANTIC_CORE.md` §2 ties value- and type-level application
to *one* connective (`Int ▷ Array`), but `ABSTRACT_GRAMMAR.md` §5 wrote
`TypeApplication ::= Type Type` (bare juxtaposition) as a placeholder. This
document resolves the tension in the Core's favor: `TypeApplication` uses
`▷`, not juxtaposition. This is filling in a placeholder correctly, not
overriding a decision — Abstract Grammar never committed to juxtaposition,
it just hadn't been asked yet.

**Finding 2**: `type` and `import`, still written as bare English words in
`ABSTRACT_GRAMMAR.md` §2.3/§6, would have been a live violation of "no
syntactic words" if carried through unexamined. Both are eliminated below
(§8, §12) using mechanisms the design already has — no new glyphs needed
for either.

---

## 2. Lexical model

Source text is UTF-8, required to be in Unicode Normalization Form C
(NFC). A source file that fails NFC validation is a lexical error, not
silently renormalized — consistent with "runtime behavior must not depend
on implementation accidents" (Core, throughout): silent renormalization
would make two byte-identical-looking files behave differently depending
on which one an editor happened to save.

Tokens are, in priority order at each position: comments (skipped), string
literals, numeric literals, the `❧` seal, multi-character glyphs (looked
up longest-match-first, mirroring the old lexer's proven approach),
single-character glyphs, identifiers, and structural ASCII punctuation
(`( ) { } , :`). Whitespace (spaces, tabs, newlines) separates tokens and
is otherwise **not significant** — see §15 for the precise argument that
this is sufficient without indentation sensitivity.

### 2.1 Comments

**Closed, not left as "comments (skipped)."** Two forms, both pure trivia
— they never produce a token, are recognized during the same skip pass as
whitespace, and never affect the span of any surrounding token (a span
measures only the characters that actually belong to that token; skipped
trivia in between simply isn't counted):

```
LineComment  ::= "//" (any char except newline)* newline
BlockComment ::= "⌈" (BlockComment | any char)* "⌉"
```

- **Line comments** (`//`) run to end of line — unremarkable, matching
  near-universal convention.
- **Block comments** (`⌈…⌉`) **nest.** This is a deliberate decision, not
  an oversight: `⌈` and `⌉` are a genuinely distinguishable open/close
  pair (unlike, say, `"..."`, where the same character opens and closes),
  so nesting is well-defined, and it's the property that actually makes
  block comments useful for their main purpose — commenting out a stretch
  of code that itself already contains a block comment. An unterminated
  block comment (unbalanced `⌈` with no matching `⌉` before end of input)
  is a lexical error.
- Comment-recognition only happens *between* tokens. A `//` or `⌈`
  occurring inside a `StringLiteral` (§5) is ordinary string content, not
  a comment — string scanning never invokes comment recognition.
- **Deferred, named explicitly rather than silently dropped**: whether
  comment text/position needs to be *preserved* (not just skipped) is a
  question for `obfusku-fmt`, which `IMPLEMENTATION_ARCHITECTURE.md` §13
  already notes will need "token trivia" to reproduce comments in
  formatted output. This section defines comments as lexically
  insignificant for parsing; a trivia-preserving lexing mode, if needed,
  is additive on top of this, not a revision to it.

---

## 3. Token classes

```
Token ::= Literal | Identifier | Glyph | Structural
Literal    ::= IntLiteral | RealLiteral | StringLiteral | BoolLiteral | UnitLiteral
Glyph       ::= one of the finalized glyph set (§ table below)
Structural  ::= "(" | ")" | "{" | "}" | "," | ":"
```

---

## 4. Identifier rules

**Case is meaningful and load-bearing, not stylistic** — a deliberate
addition beyond what `ABSTRACT_GRAMMAR.md` specified, because it gives a
second, independent way to distinguish a `Reference`/`PVar` from a
`Tag`/type name, on top of grammatical position. This matters because it
lets a reader disambiguate at a glance, not only a parser by context.

- An identifier beginning with a lowercase letter is a **value-level
  name** — usable as `Reference`, `PVar`, function/parameter name.
- An identifier beginning with an uppercase letter is a **type-level
  name** — a `Tag` (ADT variant, record, or primitive type reference) or
  a declared type name.
- Identifiers continue with letters, digits, or `_`. No identifier may
  consist solely of characters also assigned as glyphs — this prevents an
  identifier from ever being confusable with a structural token.
- The wildcard pattern (§10) uses plain ASCII `_` alone, which is
  reserved and cannot be used as an ordinary identifier — matching a
  near-universal convention (Haskell, Rust, OCaml) rather than inventing
  one.

---

## 5. Literal syntax

```
IntLiteral     ::= digit+
RealLiteral     ::= digit+ "." digit+ (("e"|"E") ("+"|"-")? digit+)?
StringLiteral   ::= '"' StringChar* '"'
StringChar      ::= (any char except '"' or '\')
                   | EscapeSequence
EscapeSequence  ::= "\" ('"' | "\" | "n" | "t" | "r" | "0")
BoolLiteral     ::= "◉" | "◎"
UnitLiteral     ::= "∅"
```

**`∅` is both `BaseType` (§6) and, in expression position, the one
`Unit` value** — the same glyph plays both roles, disambiguated purely
by which grammar position it occurs in, exactly like every other
base-type glyph never needed a separate value form until this one did.
Added so `readLine`'s `∅ → ⌘` signature (a native taking `Unit` as its
one argument, since this language's `Apply` has no zero-argument call
form) can actually be called from surface syntax — `λ() → …`-style
zero-arity declarations remain rejected (§7.3/§8.2); this only makes
`Unit` a value passable to an ordinary one-argument application.

**`StringLiteral` closed, not left as "any char except unescaped `\"`."**
That wording presupposed escape sequences without ever defining them —
fixed here with the actual production above, rather than left implicit.
Decisions made explicit, not left to an implementation's discretion:

- The accepted escapes are exactly `\"`, `\\`, `\n`, `\t`, `\r`, `\0` — a
  `\` followed by anything else is a lexical error, not a literal
  backslash.
- A raw, unescaped newline **is** permitted inside a `StringLiteral`
  (matching common convention elsewhere) — `\n` is a convenience
  alternative, not the only way to include one.
- **`\u{...}`-style Unicode escapes are explicitly deferred, not
  silently included or silently forbidden.** Adding them is more than
  closing this gap — it requires its own decision about invalid-codepoint
  and surrogate handling, which belongs to a future revision, named here
  so it isn't mistaken for an oversight.

`◉`/`◎` (True/False) are literals, not identifiers — reserved, never
usable as ordinary names (`GLYPH_SYSTEM_DESIGN.md` §6.1). `NaN`/`Infinity`
are not surface literal tokens; they arise only as ordinary `Real` values
from IEEE-total operations (`SEMANTIC_CORE.md` §15.3), consistent with
that section's finding that they need no special surface form, only
ordinary `Real`-value handling once produced.

---

## 6. Structural syntax — the two list shapes, stated as one rule

This is the single rule that keeps the "families, not vocabulary"
property intact across every construct with more than one item:

> **A flat, positional sequence of same-kind items** (call arguments,
> function parameters, tuple slots, a single ADT variant's own fields)
> is written **`( item , item , ... )`** — parens, comma-separated.
>
> **A set of named or tagged declarative alternatives** (ADT variant
> list, record fields, `Match` arms) is written **`{ item ⟢ item ⟢ ... }`**
> — braces, `⟢`-separated.

Every construct requiring a list uses one of these two shapes — no
construct invents its own bracket pair or separator. This is the
concrete payoff of `GLYPH_SYSTEM_DESIGN.md` §3's finding that the old
catalog's five bespoke block pairs were unnecessary: what replaced them
isn't "no punctuation," it's **exactly two, reused everywhere.**

**Amendment (`Array<T>` slice): a third shape, deliberately, not an
oversight being patched around.** `ArrayLiteral` (§7.9) is written
`[ item , item , ... ]` — square brackets, comma-separated — genuinely a
third bracket pair, not one of the two above. This was considered and
rejected as forcing `( item, item, ... )` (shape 1) onto `ArrayLiteral`
too: shape 1 already means "a flat, positional sequence of same-kind
items," which an array literal *is*, textually — but `(1, 2, 3)` is
already `TupleBody`'s own construction syntax (`PositionalConstruction`
against a tuple's self-tagged variant, §8.3/§3.8 of `ABSTRACT_GRAMMAR.md`),
and the two would be indistinguishable at the token level. Resolving
that by "the declared/expected type tells them apart" would require a
general bidirectional (expected-type-propagating) inference mode this
design does not otherwise have anywhere — every other literal's type is
determined bottom-up from its own shape, never from an ambient
expectation. Introducing that mechanism *only* to save a second bracket
pair was judged the worse trade: one new, narrow syntactic exception
(a third bracket pair, `[...]`, used for exactly one construct) versus a
new, general inference capability with its own scope for surprising
interaction elsewhere. `[...]` is otherwise unclaimed by this grammar
(no existing production uses `[` or `]`), so this costs no ambiguity
elsewhere — it just means §19's "exactly two shapes" readability test
now reads "two general-purpose shapes, plus one literal form for
`Array` specifically," restated at the point that bullet is stated.

---

## 7. Expression grammar

```
Expression ::= Literal
             | Reference
             | Application
             | PipeExpression
             | Lambda
             | LocalBinding
             | Conditional
             | Constructor
             | Match
             | MutationExpression
             | ExceptionExpression
             | "(" Expression ")"
```

### 7.1 Reference — corrected: not case-restricted

An identifier, used bare — **not** restricted to lowercase, contrary to
this section's earlier wording. That restriction was an imprecision, not
a deliberate rule: §7.6 requires an uppercase `Tag` to work as an
ordinary `Callable` ("`PositionalConstruction` is syntactically identical
to `Application`"), which is impossible if `Reference` excludes uppercase
identifiers by definition. §4's case convention (lowercase = value-level,
uppercase = type-level) is a *naming* convention distinguishing what a
name is likely to denote, not a grammar-level restriction on which
identifiers this production accepts. This is also what makes a bare
nullary constructor reference (`None`, no parens) parseable at all —
without this fix there would be no valid parse for it anywhere in this
grammar.

### 7.1a BinaryExpression / UnaryExpression
```
BinaryExpression ::= Expression BinOp Expression
UnaryExpression   ::= UnOp Expression
BinOp ::= "∨" | "⊻" | "∧" | "≡" | "≠" | "<" | ">" | "≤" | "≥"
        | "✚" | "☠︎" | "✱" | "÷" | "⌗"
UnOp   ::= "¬" | "−"
```
Per ADR-016, `≡`/`≠`/`≤`/`≥` replace the earlier `==`/`!=`/`<=`/`>=`
ASCII digraphs; `<`/`>` are unchanged, kept as the relational family's
own roots rather than retired (`GLYPH_SYSTEM_DESIGN.md` §8.1).

Precedence and associativity for these are stated in full in §13, not
here — kept as one authoritative table rather than split prose, per the
standing instruction not to hand-wave this.

**Typing, Core representation, and evaluation semantics for all of
these are resolved in `SEMANTIC_CORE.md` §20.2** (closed operand-type
dispatch table, no coercion, `∧`/`∨` short-circuit via `Match`-desugaring,
`⌗`'s truncating semantics, `≡`/`≠`'s same-type-only restriction per
§18) — this section only ever committed to the lexical/precedence
grammar, and §20.2 is what makes these implementable.

### 7.2 Application — zero-argument calls excluded, not silently accepted

```
Application ::= Callable "(" Argument ("," Argument)* ")"
Argument     ::= Expression | "•"
Callable     ::= Reference | Application  -- application chains via repeated "(...)"
```

**Resolved, not left as `Argument?`.** `SEMANTIC_CORE.md` §1/§5 defines
`Apply` as strictly single-argument — every multi-argument surface call
curries down to a chain of unary `Apply`s (§7 of this document). A
zero-argument call has no such chain to bottom out into: there is no Core
representation for "apply to nothing," and neither `SEMANTIC_CORE.md` nor
`ABSTRACT_GRAMMAR.md` defines a `Unit` value or literal that could stand
in for one. Rather than inventing a `Unit`-argument convention here —
which would be a real, consequential language-design decision made
silently, not a grammar detail — the first `Argument` is made mandatory.
`f()` is invalid at the concrete-syntax level: a parse error, not a
lowering-time or type-level one. If zero-argument calls are wanted later,
that requires `Unit` (or an equivalent) to be given a real Core and
surface representation first, as its own deliberate decision.

### 7.3 Lambda — zero-parameter lambdas excluded for the same reason

```
Lambda ::= "λ" "(" Parameter ("," Parameter)* ")" "→" Expression
Parameter ::= name (":" Type)?      -- annotation optional, inferred if absent
```

**Resolved, not left as `Parameter?`.** Identical reasoning to §7.2:
`SEMANTIC_CORE.md`'s `Lambda` is always single-parameter, multi-parameter
surface lambdas curry down to a chain of them, and a zero-parameter
lambda has no such chain to bottom out into without an undefined `Unit`
parameter. `λ() → …` is a parse error. Anonymous — no name follows `λ`
directly. Distinguished from `FunctionDeclaration` (§8.2) purely by that
absence, not by a different root, since both are the same Core `Lambda`
form (`GLYPH_SYSTEM_DESIGN.md` §4).

**Note left here for whoever revisits this**: excluding zero-arity forms
is a real, visible limitation, not a minor grammar tidy-up — there is
currently no way to write a niladic function (a pure side-effecting
action taking "no input") in Obfusku at all. That gap is now explicit,
not silently absent — resolving it is a future spec decision (most likely
requiring `Unit`), deliberately not made here.

### 7.4 LocalBinding
```
LocalBinding ::= ValueDeclaration Expression
               | FunctionDeclaration+ Expression
```
A binding followed by its continuation, both governed by §15's newline
rule for where each part ends.

### 7.5 Conditional — no dedicated tokens at all
```
Conditional ::= "⟡" Expression "{" "◉" "→" Expression "⟢" "◎" "→" Expression "}"
```
Not a separate production in practice — this is literally the `Match`
grammar (§11) applied to the two `Bool` literals. Included here only to
make the elimination concrete rather than asserted: there is no `if`/
`else` token anywhere in this grammar.

### 7.6 Constructor
```
Constructor ::= PositionalConstruction | NamedConstruction
PositionalConstruction ::= Tag "(" Expression? ("," Expression)* ")"
NamedConstruction        ::= Tag "{" FieldName ":" Expression ("⟢" FieldName ":" Expression)* "}"
```
`PositionalConstruction` is syntactically identical to `Application` with
an uppercase `Callable` — reinforcing, at the token level, that
constructors are ordinary functions (`SEMANTIC_CORE.md` §9), not a
special case the grammar treats differently.

### 7.7 `MutationExpression` — closed with real glyphs, not left as a placeholder

`ABSTRACT_GRAMMAR.md` §3.10 listed this category with illustrative,
explicitly-placeholder tokens (`"<-"`, the bare word `"ref"`). This
document never carried it forward into a real production — an oversight
this slice closes, using exactly what `GLYPH_SYSTEM_DESIGN.md` §4/§5
already decided rather than inventing anything new:

```
MutationExpression ::= ReadForm | RebindForm | ReferenceForm
ReadForm            ::= (an ordinary Reference to a mut-bound name — not a distinct token)
RebindForm           ::= name "⚙︎" Expression
ReferenceForm        ::= "˚" name
```

- **`ReadForm`** is what an ordinary, unmarked `Reference` to a name
  bound with `≔˚` (§8.1) desugars to — `MutRead(Var(name))`
  (`SEMANTIC_CORE.md` §12) — at every such use site.
- **`RebindForm`** (`x ⚙︎ expr`) desugars to `MutRebind(Var(x), expr)`.
  `⚙︎` is the same glyph `GLYPH_SYSTEM_DESIGN.md` §5 cherry-picked from
  the old catalog's `Assign` — a genuine reuse, not a new assignment.
- **`ReferenceForm`** (`˚name`) is the *general* explicit-handle form:
  it always lowers to bare `Var(name)` — the raw binding, cell included,
  with no implicit `MutRead`. It is not closure-capture-specific syntax;
  it is usable anywhere an `Expression` is valid (passing a cell to a
  function, storing one in a data structure), and closure capture
  (`SEMANTIC_CORE.md` §13) is simply one consumer of it — the same prefix
  mark `GLYPH_SYSTEM_DESIGN.md` §5 already specified for exactly this
  role, reused in expression position rather than invented fresh here.

Default and explicit closure capture share the same cell uniformly
(`SEMANTIC_CORE.md` §13); the distinction lives entirely in whether the
desugared body reads through `MutRead` (default: a fresh read of the live
cell at each evaluation) or hands back the bare cell (explicit: required
for `MutRebind`). See `SEMANTIC_CORE.md` §13 for the full capture
semantics.

---

### 7.8 `ExceptionExpression` — documented retroactively, already implemented

`ABSTRACT_GRAMMAR.md` §3.11 (its own numbering) specifies
`RaiseForm ::= "raise" Expression`, `CatchForm ::= "catch" Expression
"with" Lambda` using placeholder English-word tokens. This document never
carried those forward into real glyphs, even though `GLYPH_SYSTEM_DESIGN.md`
already settled them and the lexer/parser/desugar/typecheck/runtime slice
implementing `Raise`/`Catch` was built and shipped against those real
glyphs — a documentation gap, not an implementation one. Recorded here so
the concrete grammar actually says what ships:

```
ExceptionExpression ::= RaiseForm | CatchForm
RaiseForm             ::= "☄" Expression
CatchForm              ::= "☊" Expression Lambda
```

`RaiseForm`'s operand must type-check as `Exception` (`SEMANTIC_CORE.md`
§15.2) — not enforced by this production, exactly as `ABSTRACT_GRAMMAR.md`
§3.11 already stated for its own placeholder version. `CatchForm`'s
`handler` is parsed as an ordinary `Expression` at the grammar level;
`obfusku-syntax::desugar` is what additionally requires it to literally be
a single-parameter `Lambda` (needed to extract Core's `Expr::Catch`'s
`handler_param`/`handler_body` pair), a desugaring-time constraint, not a
parse-time one — matching how this document already treats other
desugaring-only restrictions elsewhere (e.g. `LetRec`'s "every member is a
`Lambda`" constraint, §11 of `SEMANTIC_CORE.md`).

### 7.9 `ArrayExpression` — `Array<T>` slice

`SEMANTIC_CORE.md` §9.1 resolved `Array<T>`'s *semantic* distinction from
`List<T>` (O(1)-random-access-only; no open head/rest destructuring;
consumed primarily via combinators) but left the concrete surface syntax
entirely unwritten — the one example using `Array` anywhere in this
document (§18's canonical-examples section) is illustrative prose, not a
grammar production, and its `(1, 2, 3, 4)` literal isn't distinguishable
from `TupleBody` construction at the token level. This section is that
missing production.

```
ArrayExpression ::= ArrayLiteral | IndexExpr
ArrayLiteral     ::= "[" (Expression ("," Expression)*)? "]"
IndexExpr         ::= Expression "[" Expression "]"
```

- **`ArrayLiteral`** is `[` `]` — a deliberate third bracket pair, not an
  instance of either of §6's two general-purpose shapes; see §6's own
  amendment for the full reasoning (in short: reusing shape 1's `( )`
  would be lexically identical to `TupleBody` construction, resolvable
  only by adding a general expected-type-propagating inference mode this
  design does not otherwise have — judged the worse trade against one
  narrow, single-purpose bracket pair). Empty (`[]`) is legal, unlike
  `Application`/`Lambda`'s "at least one" exclusions (§7.2/§7.3) — an
  array's *arity* is not part of its type, only its element type is,
  and an empty literal's element type is resolved from context exactly
  as `None`'s type parameter already is (no new inference mechanism
  needed for this specific case, since `Array`'s element type is an
  ordinary ADT-style type parameter, not special-cased).
- **`IndexExpr`** (`arr[i]`) is the *only* access form `Array` gets at
  the syntax level, matching §9.1's "consumed primarily via combinators,
  never open destructuring" framing — reading one element by position,
  not splitting the sequence. `SEMANTIC_CORE.md` (per this slice's
  amendment) specifies out-of-bounds indexing raises `Exception`'s
  existing `InvalidOperation` variant — no new `Exception` variant
  needed for this.
- **No index-assignment syntax exists, deliberately.** `Array<T>` is an
  ordinary immutable value type, exactly like every other value in this
  language (`SEMANTIC_CORE.md` §12: `Cell<T>` is the *only* mutable
  cell, not a rule with an implicit `Array` exception). "Updating" an
  array element is an ordinary value-returning combinator
  (`set(i, v): Array<T> → Int → T → Array<T>`, stdlib/intrinsic, not
  grammar) that produces a *new* array, the same "updating a constructed
  value produces a new value" rule `SEMANTIC_CORE.md` §2 already states
  for every value in the language. A genuinely mutable array, if ever
  needed, is `Cell<Array<T>>` — no new Core mechanism, the same
  composition any other `Cell<T>` already supports.
- **Combinators are ordinary functions, not new syntax — out of this
  grammar's scope by category, not by absence of a decision.** They
  operate on `Array` values via ordinary `Application`, resolved in
  `GLYPH_SYSTEM_DESIGN.md` §10.5 and implemented as host natives
  (`crates/obfusku-cli/src/array_native.rs`, `ADR-021`): `⊡`/`⊟`/`⊞`
  (map/filter/fold) and `#`/`⊙` (length/persistent set). `get` is
  deliberately not among them — `IndexExpr` (`arr[i]`) already is read
  access.

---

## 8. Declaration grammar

### 8.1 ValueDeclaration
```
ValueDeclaration ::= name (":" Type)? "≔" Mutability? Export? Expression
Mutability         ::= "˚"
Export              ::= "⟳"
```
The modifiers attach to `≔` itself, postfix, matching
`GLYPH_SYSTEM_DESIGN.md` §4 exactly — they are properties of the *binder*,
not of the name being bound.
```
x ≔ 5
x ≔˚ 0
x ≔⟳ 5
x ≔˚⟳ 0
```
Type annotation, when present, is required to appear only at module
boundaries (§9); locally it's always omitted and inferred
(`SEMANTIC_CORE.md` §3, design draft §4).

**A real ambiguity, discovered while implementing this, resolved here
rather than left implicit**: `Mutability` (`˚`) immediately following
`≔` collides with `˚` as the *first token* of a bare `ReferenceForm`
value (§7.7) — `x ≔ ˚y` is ambiguous between "immutable `x`, bound to
the expression `˚y`" and "mutable `x`" followed by an incomplete/invalid
continuation. **Resolved: `≔` immediately followed by `˚` is always the
`Mutability` modifier**, never the start of the value expression. A
binding whose value is a bare `ReferenceForm` at the top level must be
parenthesized — `x ≔ (˚y)` — the same disambiguation-by-parens pattern
already used for pipeline stages (§12). This does not restrict where
`ReferenceForm` can appear generally, only this one specific position
(directly, unparenthesized, right after `≔`).

### 8.2 FunctionDeclaration

**Arity — at least one parameter, required.** `SEMANTIC_CORE.md` §11:
every `LetRec` binding is necessarily a `Lambda`, and §7.2/§7.3 already
exclude zero-arity `Lambda`/`Application` for the same reason (no `Unit`
value or type exists to bottom a niladic form out into). Since
`FunctionDeclaration` desugars directly into that same `Lambda`-chain
machinery (see the translation rule below), the zero-arity exclusion
applies to it transitively.

**Annotations — mandatory on every parameter and on the return type**,
using a dedicated, non-reused parameter production (never `Lambda`'s own
optional-annotation `Parameter`):

```
FunctionDeclaration ::= "λ" name "(" FnParameter ("," FnParameter)* ")" ":" Type "→" Expression
FnParameter          ::= name ":" Type
```

Justification: `SEMANTIC_CORE.md` §3 / `DESIGN_VISION.md` §4's "explicit
at boundaries, inferred locally" is already the reason `ValueDeclaration`'s
own type annotation (§8.1) is required only at module boundaries — a named
function declaration is that same kind of boundary.

**Adjacency — trivia-transparent, declaration-kind-sensitive.**
`SEMANTIC_CORE.md` §11's `LetRec` grouping rule ("no extra syntax marks a
group as mutually recursive; adjacency is the signal") is defined
operationally here. Comments and whitespace are pure trivia (§2.1) and are
invisible to adjacency — a comment or blank line between two
`FunctionDeclaration`s does not break the group. Any other declaration
kind interrupting the run — including a `≔`-form `ValueDeclaration`, even
one whose value happens to be a bare `Lambda` — ends the group;
`ValueDeclaration` never participates in a recursive group
(`ABSTRACT_GRAMMAR.md` §2.2), regardless of what shape its value takes.

| Sequence | Same `LetRec` group? |
|---|---|
| `FunctionDeclaration` immediately followed by `FunctionDeclaration` | Yes |
| `FunctionDeclaration`, a `//` or `⌈…⌉` comment, `FunctionDeclaration` | Yes — comments are trivia (§2.1), invisible to adjacency |
| `FunctionDeclaration`, one or more blank lines, `FunctionDeclaration` | Yes — whitespace is trivia, §15's newline rule already treats it as insignificant |
| `FunctionDeclaration`, a `ValueDeclaration` (any value shape, including a bare `Lambda`), `FunctionDeclaration` | No — group ends at the `ValueDeclaration`; the second `FunctionDeclaration` starts a new group |
| `FunctionDeclaration`, a `TypeDeclaration`, `FunctionDeclaration` | No — same reasoning, any non-`FunctionDeclaration` declaration ends the run |
| `FunctionDeclaration`, then `❧`/end of declaration sequence | No further group; the run simply ends |

**Key property**: `LetRec` group membership is a pure function of the
sequence of declaration **kinds**, never of what any declaration's body
references — this is what makes group membership decidable without
inspecting bodies or doing dependency/SCC analysis.

```
λf(x: Int): Int → 1
λg(x: Int): Int → 2
```
`f` and `g` form one group even though neither references the other —
adjacency alone is sufficient, no reference required.

```
λf(x: Int): Int → g(x)
x ≔ 42
λg(x: Int): Int → x
```
`f` and `g` do **not** form one group, even though `f`'s body plainly
references `g` — the intervening `ValueDeclaration` ends the run, so `g`
is simply unbound inside `f`'s body, exactly as any other forward
reference to a later, non-adjacent declaration would be. This is the
concrete case that rules out reconstructing group membership from
reference/dependency analysis: a reference existing is neither necessary
(first example) nor sufficient (this example) for group membership.

**Normative translation, `FunctionDeclaration+` → `LetRec`.** A maximal
run of adjacent `FunctionDeclaration`s (per the table above) desugars to
one `LetRec` binding group over all of them, each becoming a `Lambda`
value inside it exactly as an anonymous `Lambda` would (curried down for
multi-parameter forms, per §7.3's existing rule) — a run of exactly one
`FunctionDeclaration` still forms a `LetRec` group of size one, which is
what makes direct self-recursion legal without requiring a second,
separate declaration to "pair" with. This is the only route to
self- or mutual recursion in the language; a `ValueDeclaration`, whatever
shape its value takes, never does.

Whether `obfusku-core::ast::Module` reifies this grouping as a distinct
structural type is an implementation representation choice, not a
semantic one. What is normative: the information needed to recover, for
any binding, which (if any) syntactic `LetRec` group it belonged to must
survive desugaring intact — recoverable from the Core representation
alone, never re-derived from binding shapes or reference patterns.

### 8.3 TypeDeclaration — no `type` keyword

Eliminated. A type declaration is recognizable purely from its left-hand
side being an **uppercase** name and its right-hand side having one of
three structurally distinctive shapes — no introductory word needed, the
same way `ValueDeclaration` and `TypeDeclaration` already share `≔` as
their connective by design.

```
TypeDeclaration ::= Tag TypeParameter* "≔" TypeBody
TypeParameter     ::= lowercase name
TypeBody           ::= SumBody | RecordBody | TupleBody
SumBody             ::= "{" Variant ("⟢" Variant)* "}"
Variant             ::= Tag ( "(" Type? ("," Type)* ")" )?
RecordBody          ::= "{" FieldName ":" Type ("⟢" FieldName ":" Type)* "}"
TupleBody           ::= "(" Type ("," Type)* ")"
```

```
Shape ≔ { Circle(Real) ⟢ Rectangle(Real, Real) }
Point ≔ { x: Real ⟢ y: Real }
Pair  ≔ (Int, String)
Box b ≔ { value: b }                 -- generic: type parameter 'b'
```

**Disambiguation from `ValueDeclaration`, made precise**: the parser
distinguishes the two productions by the left-hand identifier's case
(uppercase → `TypeDeclaration`, lowercase → `ValueDeclaration`) — this is
decidable from the very first token, before any lookahead into the
right-hand side is even needed.

### 8.4 Export
```
ExportedDeclaration ::= any Declaration with "⟳" immediately following its "≔" or "→"
```
Applies to `ValueDeclaration` and `FunctionDeclaration` — one modifier,
two sites, same meaning (`GLYPH_SYSTEM_DESIGN.md` §4). Invalid inside a
`LocalBinding` (§11 of that document) — enforced as a static check, not
a parse-time restriction, since the grammar shape is identical; only its
position (top-level `Module` vs. nested) makes it valid or invalid.

**`TypeDeclaration` is a deliberate exception to this modifier, not a
third site for it — 1.0.0 contract, resolved here, not left as a design
question**: `⟳` is never written on a `TypeDeclaration` at all (a
`TypeDeclaration ≔⟳ ...` is a parse error — `⟳` has no valid position in
this production, exactly the way it has none inside a `LocalBinding`).
Every `TypeDeclaration` and every one of its ADT variants is exported
unconditionally — there is no opt-out mechanism in 1.0.0, and none is
planned for it as a distinct axis alongside `⟳`. This is a real
asymmetry with `ValueDeclaration`/`FunctionDeclaration` (which default to
*not* exported), not an oversight: a type's *visibility* and its
variants' *availability for construction and pattern matching* are one
and the same automatic-export fact, not two independently-toggled
layers — a module importing `⟲ Shapes` that can see `Shape` at all can
always construct and match every one of `Shape`'s variants, with nothing
further to opt into. (An earlier draft of this section conflated the two
declaration-level exports and one type-level export as three uniform
sites for the same mark; on reflection, and once whole-module import
existed to actually test this against, that framing didn't hold: nothing
in this design calls for a type to be importable while some of its own
constructors are not, or vice versa, so no second export axis for types
was ever built, and this section now says so directly instead of via a
uniform claim the grammar itself never actually accepted.)

---

## 9. Type grammar

```
Type ::= BaseType | Tag | FunctionType | TypeApplication | "(" Type ")"
BaseType        ::= "⟁" | "⧆" | "⌘" | "○" | "∅"
FunctionType    ::= Type "→" Type              -- right-associative
TypeApplication ::= Type "▷" Type              -- left-associative, tighter than "→"
```

```
x: Array ▷ Int              -- Array<Int>
f: (Array ▷ Int) → Int      -- a function from Array<Int> to Int
g: Int → Int → Int          -- curried, right-associative: Int → (Int → Int)
```

No angle-bracket generic syntax exists anywhere (`SEMANTIC_CORE.md` §19's
restriction, carried through unchanged) — this is also exactly why
generic-type syntax cannot collide with comparison operators (§16.6):
there is no `<...>` token sequence in this grammar to collide with `<`/`>`
in the first place, and even setting that aside, position alone would
disambiguate (a parser always knows when it's reading a `Type`).

---

## 10. Pattern grammar

```
Pattern ::= PVar | PWildcard | PLit | PPositionalConstructor | PNamedConstructor
PVar                      ::= lowercase name
PWildcard                  ::= "_"
PLit                        ::= Literal
PPositionalConstructor       ::= Tag "(" Pattern? ("," Pattern)* ")"
PNamedConstructor            ::= Tag "{" FieldName ":" Pattern ("⟢" FieldName ":" Pattern)* "}"
```

`◇` (the old catalog's wildcard) is **not adopted** — a deliberate
correction to an oversight in `GLYPH_SYSTEM_DESIGN.md`, which finalized
`◈` for the pipeline reference without checking it against the old
wildcard glyph. `◇` and `◈` are both diamond-family shapes and would be
genuinely confusable at normal monospace size (`GLYPH_SYSTEM_DESIGN.md`
§1's typography requirement exists precisely to catch this). ASCII `_` is used instead —
zero collision risk, and matches convention widely enough to cost nothing
in learnability.

`Cell<T>` has no constructor and therefore no `PPositionalConstructor`/
`PNamedConstructor` form — only `PVar`/`PWildcard` are legal against a
`Cell`-typed scrutinee (`SEMANTIC_CORE.md` §12.1, unchanged, restated for
completeness since this document is meant to be usable without
cross-referencing every prior one).

---

## 11. `Match` grammar

```
Match     ::= "⟡" Expression "{" MatchArm ("⟢" MatchArm)* "}"
MatchArm ::= Pattern "→" Expression
```

Self-delimiting: the closing `}` — the *generic* list-closer from §6, not
a `Match`-specific glyph — is what marks the end. This is the concrete
form of `GLYPH_SYSTEM_DESIGN.md` §7's claim that no bespoke "end of match"
glyph is needed; it wasn't a hand-wave, this is what it cashes out to.

Exhaustiveness (`SEMANTIC_CORE.md` §17) is checked against this grammar's
`Pattern` set relative to the scrutinee's declared `TypeDeclaration` — a
closed-type scrutinee (any `TypeDeclaration`-defined `Tag`, or `Bool`) is
satisfied by covering every `Tag`; an open-type scrutinee (`Int`, `Real`,
`String`) requires a `PVar`/`PWildcard` arm present, mandatorily.

---

## 12. Pipeline grammar

```
PipeExpression ::= Expression "▷" Stage
Stage           ::= Callable
                   | "(" Expression ")"        -- may contain "◈" and/or "•"
```

**The bare-callable/expression-with-reference distinction is syntactic,
not semantic** — a refinement this document makes precise where
`SEMANTIC_CORE.md` §7/§8 described it abstractly. Classification depends
only on shape: a `Stage` is bare-callable if it's a `Reference` or an
`Application` (including one built from `•` holes); it's an
expression-stage if it's a parenthesized `Expression`, **whether or not
that expression happens to reference `◈` anywhere inside it.** This
matters for the nested case worked through in §16.1: it means the
classification never requires binding analysis during parsing, only
shape — a stage that turns out not to use `◈` at all is still legal (a
constant pipeline stage, Core §8's stated allowed-but-lint-worthy case),
just classified the same way syntactically regardless.

`◈` scoping follows directly from this: each `▷`'s `Stage` is its own
binding region; nested `▷`s inside a parenthesized `Stage` introduce their
own regions, shadowing the outer one for any `◈` lexically inside them —
ordinary nested-scope structure, no pipeline-specific mechanism (matching
`GLYPH_SYSTEM_DESIGN.md` §8's finding).

### 12.1 Partial application, and how it composes with `▷`

```
xs ▷ f(a, •, c)
```
`f(a, •, c)` is a `Callable` (an `Application` containing a hole) — a
bare-callable `Stage`, no parens required. It evaluates first to a
one-argument function; the pipe then applies the piped subject to it,
yielding `f(a, xs, c)` — exactly `SEMANTIC_CORE.md` §6's rule, with no
pipeline-specific handling needed, because holes and `▷` are two
independent mechanisms that happen to compose for free.

---

## 13. Precedence and associativity — the full table

Two categories don't fit a single precedence ladder honestly, and are
stated separately rather than forced in, per §9's and §12's own findings:

- **`→`** (Lambda body, `MatchArm` result, `FunctionType`) is a **fixed
  structural connective in exactly three productions**, never a
  general-purpose binary operator competing for a precedence slot. It has
  no entry below for that reason — comparing it against `✚` would be a
  category error, not an omission.
- **`▷`** (`PipeExpression`, `TypeApplication`) has a **restricted right
  operand grammar** (`Callable | "(" Expression ")"` for pipe;
  `Type` for type application), not a general `Expression`/`Type` slot at
  arbitrary precedence — so its "precedence" is really "how much of the
  left side it can absorb," not a comparison against arithmetic operators
  on its right. Left-associative in both uses.

Everything else genuinely is a conventional precedence ladder, stated in
full, **highest binding (tightest) to lowest**:

| Level | Operators | Associativity |
|---|---|---|
| 1 (tightest) | `Application` (juxtaposition via `(...)`), `Constructor` construction, `PipeReference` (`◈`), `Hole` (`•`), literals, references, parenthesized groups | n/a — primary/atomic |
| 2 | Unary `¬`, unary `−` (negation) | right-to-left (prefix) |
| 3 | `✱`, `÷`, `⌗` (multiply, divide, modulo) | left-to-right |
| 4 | `✚`, `☠︎` (add, subtract) | left-to-right |
| 5 | `<`, `>`, `≤`, `≥` (comparison) | non-associative — chaining (`a < b < c`) is a syntax error, not sugar for `a<b ∧ b<c` |
| 6 | `≡`, `≠` (equality) | non-associative, same reasoning |
| 7 | `∧` (logical and) | left-to-right |
| 8 | `⊻` (logical xor) | left-to-right |
| 9 (loosest) | `∨` (logical or) | left-to-right |

`◈` and `•` sit at level 1 because they are atomic tokens (primary
expressions), never infix operators themselves — they participate in
whatever expression contains them at that expression's own precedence,
the same way a literal or a reference does. This is stated explicitly
because §12's ambiguity work depended on it and shouldn't be left
implicit here.

**Pattern syntax has no precedence table** because it has no binary
operators at all — `Pattern` nesting is fully determined by
`PPositionalConstructor`/`PNamedConstructor`'s argument positions, which
are unambiguous by construction (§10). Stating "there is nothing to rank"
explicitly, rather than omitting the section, since the brief asked for
this category by name.

**Binding expressions** (`ValueDeclaration`, `FunctionDeclaration`,
`TypeDeclaration`) sit **outside** this table entirely — their
right-hand side is parsed as one full `Expression` (or `TypeBody`) at the
top of this table, not as an operand competing with `✚`/`▷`/etc. for a
slot within it.

---

## 14. Module grammar

```
Program            ::= Module+
Module              ::= Declaration* "❧"
Declaration          ::= ValueDeclaration | FunctionDeclaration
                        | TypeDeclaration | ImportDeclaration
ImportDeclaration   ::= "⟲" ModuleName
```

`import` is eliminated the same way `type` was (§8.3) — `⟲` alone is a
sufficient, unambiguous introducer; no English word is needed in front of
it. `⟲`/`⟳` (import/export) form a deliberate pair — counter-clockwise
inward, clockwise outward — completing a pairing `GLYPH_SYSTEM_DESIGN.md`
left half-finished (it finalized `⟳` for export but left `⟲` explicitly
open).

`❧` is **exactly** what `GLYPH_SYSTEM_DESIGN.md` §10.1 established:
mandatory, once per `Module`, closing the `Declaration*` sequence — not a
general block terminator, and it appears nowhere else in this grammar. No
other production in this document uses it.

---

## 15. Whitespace and newline rules

**Newlines are not significant.** No indentation sensitivity, no implicit
statement terminator. This is a real, load-bearing rule, not an evasion —
argued precisely, because "ordinary precedence" is exactly the kind of
prose this document was asked not to rely on:

A `Declaration`'s `Expression` extends across as many lines as needed for
as long as the next token is a valid **continuation** of the
in-progress expression (an infix operator, `▷`, or an immediately
following `(` continuing an `Application`). It stops — and the next
`Declaration` begins — the moment the next token matches one of a small,
closed set of **declaration-starting shapes**: an uppercase or lowercase
`name` immediately followed by `≔`, a bare `λ`, `⟲`, or the `❧` seal
itself. These two token classes (continuation tokens vs.
declaration-starting shapes) are disjoint by construction — `≔` is never
valid inside `Expression` grammar at all, so `name ≔` can never be
misread as continuing a prior expression, and no infix/continuation
token can ever be mistaken for the start of a new declaration. This
makes the split **fully decidable with one token of lookahead**, without
needing indentation or a terminator:

```
x ≔ a
    ✚ b            -- continuation: ✚ is a valid infix op → one Declaration, x ≔ (a ✚ b)

x ≔ a
y ≔ b               -- new Declaration: 'y ≔' matches a declaration-starting shape
```

Whitespace inside a token (an identifier, a multi-character glyph) is not
permitted; whitespace between tokens is otherwise purely cosmetic.

---

## 16. Ambiguity resolution — the challenge cases, worked

**16.1 Nested pipelines / nested `◈`**: `x ▷ (y ▷ (◈ ✚ 1))`. The inner
`◈` belongs to the inner `▷`'s `Stage` (`(◈ ✚ 1)`) by §12's shadowing
rule; the outer `Stage` (`(y ▷ (◈ ✚ 1))`) is classified as an
expression-stage purely by being parenthesized, regardless of whether an
unshadowed `◈` occurs in it — here it doesn't (the only `◈` present is
already claimed by the inner pipe), so the outer stage is a legal but
`x`-ignoring constant stage. No parse ambiguity; a lint-level concern at
most (Core §8), not a grammar one.

**16.2 Pipeline inside function application**: `g(x ▷ f)`. Argument
boundaries (commas, or the single-argument case here) are resolved before
any operator-precedence question — everything between `(` and `)` (or
between commas) is one `Expression`, and `x ▷ f` is a complete one. Not
ambiguous; argument parsing and expression-internal precedence operate at
different grammatical levels, not in competition.

**16.3 Application inside pipeline stages**: `x ▷ f(y)`. Parses without
ambiguity as a bare-callable `Stage` (an `Application`). Whether it
*type-checks* depends on `f`'s arity — if `f` takes exactly one argument,
`f(y)` is already fully applied and not a function `x` can be piped into,
a type error, not a parse error. Grammar and type-checking answer
different questions here; conflating them would be the actual mistake.

**16.4 Directly-nested `◈` shadowing**: `x ▷ (◈ ▷ (◈ ✚ 1))`. Well-defined
(inner `◈` always wins, §12), but flagged under §19 as a readability
concern, not prohibited — prohibiting it would require an arbitrary rule
where none is grammatically necessary, which the standing instruction
explicitly asks to avoid.

**16.5 Function literal adjacent to application**: `λ(x) → x(y)`. §7.2's
greedy-application rule means this is unambiguously "a lambda returning
`x(y)`" — there is no grammatical position where `x` and `(y)` could be
read as two separate things, because Obfusku has no bare
expression-sequencing at all (every `Expression` is exactly one thing).

**16.6 Generic type syntax adjacent to comparison operators**: cannot
arise — comparison uses plain ASCII (§18's canonical example), generics
use `▷` in `Type` position (§9), and there is no `<...>` bracket syntax
anywhere in this grammar for the two to collide over. Doubly resolved:
no shared token, and position would disambiguate even if there were.

**16.7 Constructor patterns adjacent to expressions**: `Circle(5)` as an
expression vs. `Circle(r)` as a pattern. Resolved by grammatical position
alone — left of a `MatchArm`'s `→` is `Pattern` grammar,
everywhere else is `Expression` grammar (`SEMANTIC_CORE.md` §10's "two
grammars, shared name namespace," made concrete).

**16.8 Multiline declarations**: covered fully in §15.

---

## 17. Invalid syntax — examples

```
x ≔ 5
                      -- ERROR: missing ❧, module not sealed (truncation-indistinguishable)

λ˚f(x: Int) → x        -- ERROR: mutability mark on a FunctionDeclaration root,
                        -- invalid per §11 of GLYPH_SYSTEM_DESIGN.md — LetRec bindings
                        -- are never Cells

xs ▷ filter(◈ ✚ •)     -- ERROR: "◈" and "•" in the same Stage — presence of "◈"
                        -- forces expression-stage interpretation; "•" has no meaning there

⟡ n {
  0 → "zero"
}                       -- ERROR: n : Int is an open type; no PVar/PWildcard arm present,
                        -- non-exhaustive (§11, §17 of SEMANTIC_CORE.md)
```

---

## 18. Canonical examples

```
Shape ≔ { Circle(Real) ⟢ Rectangle(Real, Real) }

λarea(s: Shape): Real →⟳
  ⟡ s {
    Circle(r)       → ⧆ ✱ r ✱ r
    Rectangle(w, h) → w ✱ h
  }
❧
```

```
xs: Array ▷ Int ≔ (1, 2, 3, 4)

total ≔ xs
  ▷ filter(◈ > 0)
  ▷ map(◈ ✱ 2)
  ▷ fold(0, ✚)
❧
```

```
counter ≔˚ 0

λincrement(): ∅ →⟳
  counter ⚙︎ counter ✚ 1
❧
```

```
⟲ shapes

λdescribe(n: Int): ⌘ →⟳
  ⟡ n {
    0 → "none"
    _ → "some"
  }
❧
```

---

## 19. Design invariants / readability tests

- Every list in the language uses one of exactly two general-purpose
  shapes (§6), plus one deliberate, narrow third — `ArrayLiteral`'s
  `[...]` (§6's amendment, §7.9) — reserved for exactly that one
  construct, not a precedent inviting further bespoke brackets.
- Every family established in `GLYPH_SYSTEM_DESIGN.md` composes exactly
  as documented there with no new exceptions introduced at the concrete
  level — `≔˚⟳`, `˚x`/`x ⚙︎`, `○`/`◉`/`◎` all read the same way they were
  designed to.
- Directly-nested `◈` shadowing (§16.4) is legal but discouraged
  stylistically — a future formatter/linter concern, not a grammar
  restriction, consistent with "don't add arbitrary punctuation to solve
  a non-ambiguity."
- `type`/`import` elimination (§8.3, §14) demonstrates the founding
  principle held even under pressure from familiar convention — the
  temptation to keep a bare keyword "just for these two cases" was
  available and declined.

---

## 20. Known remaining questions

1. **Exact codepoints for two new marks — resolved.** The mutability
   modifier is `˚` (U+02DA MODIFIER LETTER RING ABOVE) and the
   argument-hole mark is `•` (U+2022 BULLET), both fixed in
   `obfusku-syntax`'s lexer (`TokenKind::Mut`/`TokenKind::Hole`) and
   exercised throughout the test suite. Recorded here as closed rather
   than removed, so a reader tracing this document's own history sees
   where the open question was settled rather than finding it silently
   gone.
2. **Short standard names for generic wrapper types — resolved.**
   `List` and `Optional` are implemented in
   `crates/obfusku-cli/src/stdlib.obk` under their full spelled names
   (per `GLYPH_SYSTEM_DESIGN.md` §6.2's "ordinary spelled names, not
   sigils" finding), with symbolic constructors (`⊘`/`⁝`, `⦰`/`⧫` —
   §10.2/§10.3 of that document). `Result` is implemented the same way
   (`✓`/`✗`, §10.4). `Array`'s own type needed no wrapper-name decision
   at all — it is a Core primitive (§9.2), not a library ADT — and its
   combinator surface is resolved separately (§10.5). `Cell` needed no
   wrapper-name decision either — it is a first-class `Type::Cell`
   variant, not an `AdtRegistry` entry. No generic wrapper type in the
   canon stdlib has an undecided name.
3. **`Real` literal internationalization — closed, deferred by design,
   not technical debt.** `.` remains the sole, canonical decimal
   separator; `,` remains reserved as the generic structural list
   separator (§6: `Application` arguments, `ArrayLiteral` elements,
   `Constructor`/`TypeBody` fields — one mechanism, reused everywhere,
   per `GLYPH_SYSTEM_DESIGN.md` §3). A locale-variant decimal comma
   (`1,5`) was evaluated and rejected: `,` already carries this
   structural role throughout the grammar, so admitting it as a decimal
   separator too would make `f(1,5)` genuinely ambiguous between a
   two-argument call and a one-argument call with a `Real` literal —
   the same class of ambiguity `ADR-002` already rejected for
   `ArrayLiteral` vs. `TupleBody`, not a new one invented for this
   item. The inverted convention (`.` as thousands separator, `,` as
   decimal — `1.000,50`) does not escape this: it still needs `,` for
   the decimal position. No locale-dependent numeric syntax is
   introduced in 1.0. This is a settled design position, not a task
   awaiting future completion — there is nothing pending here to pick
   back up.

Nothing above blocks implementing a parser from this document as
written; each is a follow-on detail, not an ambiguity in the grammar
itself.
