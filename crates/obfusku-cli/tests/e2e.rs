//! Whole-pipeline conformance tests: real `.obk` source text through the
//! actual lexer/parser/desugar/typecheck/runtime, never hand-built Core.

use obfusku_cli::run_source;
use obfusku_runtime::Value;

fn run_ok(source: &str) -> Value {
    // `.0.0` is deliberate, not an oversight: see `RunResult`'s
    // doc comment for why this crate refuses to let that convention hide
    // behind a bare `Value` at this call site. The second `.0` discards
    // `run_source`'s non-fatal warnings — this helper only cares about
    // the executed value; see `check_source_...warnings` tests for
    // warning-specific coverage.
    run_source(source)
        .unwrap_or_else(|diags| {
            panic!("expected {source:?} to run successfully, got diagnostics: {diags:?}")
        })
        .0
         .0
}

fn run_err(source: &str) -> Vec<obfusku_diagnostics::Diagnostic> {
    run_source(source).expect_err(&format!("expected {source:?} to fail"))
}

fn as_int(v: Value) -> i64 {
    match v {
        Value::Int(i) => i,
        other => panic!("expected Int, got {other:?}"),
    }
}

// ---------------------------------------------------------------------
// Literals
// ---------------------------------------------------------------------

#[test]
fn int_literal_source_to_runtime_value() {
    assert!(matches!(run_ok("x ≔ 5\n❧"), Value::Int(5)));
}

#[test]
#[allow(clippy::approx_constant)]
fn real_literal_source_to_runtime_value() {
    assert!(matches!(run_ok("x ≔ 3.14\n❧"), Value::Real(v) if v == 3.14));
}

#[test]
fn string_literal_source_to_runtime_value() {
    assert!(matches!(run_ok("x ≔ \"hello\"\n❧"), Value::Str(s) if &*s == "hello"));
}

#[test]
fn bool_true_glyph_source_to_runtime_value() {
    assert!(matches!(run_ok("x ≔ ◉\n❧"), Value::Bool(true)));
}

#[test]
fn bool_false_glyph_source_to_runtime_value() {
    assert!(matches!(run_ok("x ≔ ◎\n❧"), Value::Bool(false)));
}

#[test]
fn unit_literal_glyph_source_to_runtime_value() {
    assert!(matches!(run_ok("x ≔ ∅\n❧"), Value::Unit));
}

#[test]
fn unit_literal_passed_as_an_ordinary_function_argument_end_to_end() {
    let source = "\u{3bb}takesUnit(u: \u{2205}): \u{2205} \u{2192} u\nresult \u{2254} takesUnit(\u{2205})\n\u{2767}\n";
    assert!(matches!(run_ok(source), Value::Unit));
}

// ---------------------------------------------------------------------
// References
// ---------------------------------------------------------------------

#[test]
fn reference_to_an_earlier_binding_source_to_runtime_value() {
    let v = run_ok("x ≔ 5\ny ≔ x\n❧");
    assert_eq!(as_int(v), 5);
}

// ---------------------------------------------------------------------
// Functions
// ---------------------------------------------------------------------

#[test]
fn identity_function_source_to_runtime_int() {
    // source -> AST -> Core -> typed Core -> runtime -> Int(5)
    let v = run_ok("f ≔ λ(x) → x\nresult ≔ f(5)\n❧");
    assert_eq!(as_int(v), 5);
}

// ---------------------------------------------------------------------
// Pipelines
// ---------------------------------------------------------------------

#[test]
fn simple_pipeline_source_to_runtime_value() {
    // z ≔ xs ▷ f, proven at the parser level already
    // (`pipeline_with_a_bare_callable_stage`); here it must also
    // desugar, typecheck, and execute correctly.
    let v = run_ok("f ≔ λ(x) → x\nxs ≔ 5\nresult ≔ xs ▷ f\n❧");
    assert_eq!(as_int(v), 5);
}

#[test]
fn nested_pipeline_with_pipe_reference_scoping_end_to_end() {
    // The `◈` scoping example already proven independently at the
    // parser/desugar level (`nested_pipeline_with_pipe_reference_inside_parens`
    // in obfusku-syntax's own tests) — now proving it through the whole
    // semantic chain, not only its shape.
    let v = run_ok("inc ≔ λ(n) → n\ng ≔ λ(n) → n\nxs ≔ 5\nresult ≔ xs ▷ (◈ ▷ g)\n❧");
    assert_eq!(as_int(v), 5);
}

// ---------------------------------------------------------------------
// Mutation
// ---------------------------------------------------------------------

#[test]
fn mutable_binding_mutread_mutrebind_and_closure_capture_end_to_end() {
    // bump explicitly captures counter (`˚counter`) and rebinds it; an
    // ordinary later reference must see the write via MutRead.
    let v = run_ok(
        "counter ≔˚ 0\nbump ≔ λ(step) → ˚counter ⚙︎ step\n_called ≔ bump(5)\nresult ≔ counter\n❧",
    );
    assert_eq!(as_int(v), 5);
}

// ---------------------------------------------------------------------
// ADTs / pattern matching
// ---------------------------------------------------------------------

#[test]
fn adt_constructor_and_exhaustive_match_end_to_end() {
    let v = run_ok(
        r#"
        Option t ≔ { Some(t) ⟢ None }
        x ≔ Some(5)
        result ≔ ⟡ x {
          Some(n) → n
          ⟢ None → 0
        }
        ❧
        "#,
    );
    assert_eq!(as_int(v), 5);
}

#[test]
fn adt_none_case_of_the_same_match_end_to_end() {
    let v = run_ok(
        r#"
        Option t ≔ { Some(t) ⟢ None }
        x ≔ None
        result ≔ ⟡ x {
          Some(n) → n
          ⟢ None → 0
        }
        ❧
        "#,
    );
    assert_eq!(as_int(v), 0);
}

// ---------------------------------------------------------------------
// Optional / Result (established names from the earlier typecheck slice)
// ---------------------------------------------------------------------

#[test]
fn optional_end_to_end_no_special_runtime_or_typecheck_path() {
    let v = run_ok(
        r#"
        Optional t ≔ { Some(t) ⟢ None }
        x ≔ Some(42)
        result ≔ ⟡ x {
          Some(n) → n
          ⟢ None → 0
        }
        ❧
        "#,
    );
    assert_eq!(as_int(v), 42);
}

#[test]
fn result_end_to_end_no_special_runtime_or_typecheck_path() {
    // Err's arm returns `msg`, not a throwaway literal, so both arms
    // unify and pin `e = Int` — `Ok(7)` alone leaves `e` ambiguous.
    let v = run_ok(
        r#"
        Result t e ≔ { Ok(t) ⟢ Err(e) }
        r ≔ Ok(7)
        result ≔ ⟡ r {
          Ok(n) → n
          ⟢ Err(msg) → msg
        }
        ❧
        "#,
    );
    assert_eq!(as_int(v), 7);
}

// ---------------------------------------------------------------------
// Diagnostics: preserve source spans through the full pipeline
// ---------------------------------------------------------------------

#[test]
fn typecheck_error_span_points_at_the_offending_source_text() {
    let source = "result ≔ nope\n❧";
    let diags = run_err(source);
    assert!(!diags.is_empty());
    let d = &diags[0];
    let slice = &source[d.primary.start as usize..d.primary.end as usize];
    assert_eq!(
        slice, "nope",
        "diagnostic span should point at the unbound variable, got {slice:?} (message: {})",
        d.message
    );
    assert!(d.message.contains("unknown variable"), "{}", d.message);
}

#[test]
fn parse_error_span_points_at_the_offending_source_text() {
    // Missing '❧' entirely — the parser's own error, before desugaring
    // or typechecking ever run.
    let source = "x ≔ 5\n";
    let diags = run_err(source);
    assert!(!diags.is_empty());
    assert!(
        diags[0].message.contains('❧'),
        "expected the missing-seal message, got: {}",
        diags[0].message
    );
}

#[test]
fn typecheck_error_resolves_to_the_correct_line_and_column() {
    let source = "x ≔ 1\nresult ≔ nope\n❧";
    let diags = run_err(source);
    let mut map = obfusku_diagnostics::SourceMap::new();
    map.add_file(source);
    let loc = map.line_col(diags[0].primary.source, diags[0].primary.start);
    assert_eq!(loc.line, 2);
    assert_eq!(loc.column, 10);
}

#[test]
fn runtime_uncaught_exception_diagnostic_resolves_to_a_real_line_column() {
    let source = "x ≔ 1\ny ≔ 5 ÷ 0\nresult ≔ y\n❧";
    let diags = run_err(source);
    let mut map = obfusku_diagnostics::SourceMap::new();
    map.add_file(source);
    let rendered = map.render(&diags[0]);
    assert!(
        rendered.starts_with("error: uncaught exception"),
        "{rendered}"
    );
    let loc = map.line_col(diags[0].primary.source, diags[0].primary.start);
    assert_eq!(loc.line, 2);
}

#[test]
fn check_source_reports_the_same_typecheck_diagnostics_without_executing() {
    let source = "result ≔ nope\n❧";
    let diags = obfusku_cli::check_source(source).expect_err("expected a typecheck error");
    assert!(diags[0].message.contains("unknown variable"));
}

#[test]
fn duplicate_top_level_binding_names_are_a_static_error() {
    // A reused top-level name must be a static error, not silent
    // overwrite-in-place (which would let a closure retroactively see a
    // later redeclaration instead of the value in scope at creation).
    let source = "x ≔ 1\ncapture_x ≔ λ(ignored) → x\nx ≔ 2\nresult ≔ capture_x(◉)\n❧";
    let diags = run_err(source);
    assert!(
        diags[0].message.contains("already bound"),
        "{}",
        diags[0].message
    );
}

#[test]
fn mutual_recursion_between_two_top_level_functions_executes_end_to_end() {
    // Mutual recursion via two adjacent `FunctionDeclaration`s (§8.2) —
    // the only surface form that actually supports it.
    let source = "Nat ≔ { Zero ⟢ Succ(Nat) }
         λis_even(n: Nat): ○ → ⟡ n {
  Zero → ◉
  ⟢ Succ(k) → is_odd(k)
}
         λis_odd(n: Nat): ○ → ⟡ n {
  Zero → ◎
  ⟢ Succ(k) → is_even(k)
}
         three ≔ Succ(Succ(Succ(Zero)))
         result ≔ is_even(three)
❧";
    let v = run_ok(source);
    assert!(matches!(v, Value::Bool(false)), "{v:?}");
}

#[test]
fn directly_self_recursive_function_executes_end_to_end() {
    // Migrated to `FunctionDeclaration` — see the mutual-recursion test
    // above for why `depth ≔ λ(n) → ...` no longer works (nor should
    // it).
    let source = "Nat ≔ { Zero ⟢ Succ(Nat) }\nλdepth(n: Nat): ⟁ → ⟡ n {\n  Zero → 0\n  ⟢ Succ(k) → depth(k)\n}\nthree ≔ Succ(Succ(Succ(Zero)))\nresult ≔ depth(three)\n❧";
    let v = run_ok(source);
    assert_eq!(as_int(v), 0);
}

#[test]
fn value_declaration_can_no_longer_self_recurse() {
    // A bare Lambda-valued ValueDeclaration must not self-recurse.
    let source = "depth ≔ λ(n) → depth(n)\nresult ≔ 1\n❧";
    let diags = run_err(source);
    assert!(
        diags.iter().any(|d| d.message.contains("unknown variable")),
        "{diags:?}"
    );
}

// ---------------------------------------------------------------------
// Letrec-group inference: adversarial audit findings
// (`spec/SEMANTIC_CORE.md` §11/§19.1, `obfusku-typecheck/src/lib.rs`)
// ---------------------------------------------------------------------

#[test]
fn mutual_recursion_with_genuinely_different_types_in_each_leg_executes_end_to_end() {
    // f: Nat -> Bool and g: Nat -> Int call each other; a naive letrec
    // could force both directions to agree on one type, but needn't.
    let source = "Nat ≔ { Zero ⟢ Succ(Nat) }\n\
         intToBool ≔ λ(x) → ◉\n\
         boolToInt ≔ λ(b) → 0\n\
         λf(n: Nat): ○ → ⟡ n {\n  Zero → ◉\n  ⟢ Succ(k) → intToBool(g(k))\n}\n\
         λg(n: Nat): ⟁ → ⟡ n {\n  Zero → 0\n  ⟢ Succ(k) → boolToInt(f(k))\n}\n\
         three ≔ Succ(Succ(Succ(Zero)))\n\
         result ≔ f(three)\n❧";
    let v = run_ok(source);
    assert!(matches!(v, Value::Bool(true)), "{v:?}");
}

#[test]
fn incompatible_types_at_a_recursive_call_site_are_rejected() {
    // f's recursive call passes a Bool where its own annotation already
    // pins the slot to Int — must still be a hard type error.
    let source = "λf(x: ⟁): ⟁ → f(◉)\na ≔ f(5)\nresult ≔ a\n❧";
    let diags = run_err(source);
    assert!(diags[0].message.contains("argument type mismatch"));
}

#[test]
fn lambda_never_using_its_own_recursive_slot_stays_fully_polymorphic() {
    // An ordinary ValueDeclaration must still generalize to a
    // polymorphic identity usable at two different types.
    let source = "f ≔ λ(x) → x\na ≔ f(5)\nresult ≔ f(◉)\n❧";
    let v = run_ok(source);
    assert!(matches!(v, Value::Bool(true)), "{v:?}");
}

#[test]
fn earlier_polymorphic_binding_used_at_two_types_from_a_later_lambda_is_not_collapsed() {
    // No FunctionDeclaration here, so no LetRec group at all — basic
    // ordinary-Let-polymorphism regression; see
    // `polymorphism_survives_around_an_actual_letrec_group` for the
    // version that exercises a real LetRec group.
    let source = "id ≔ λ(x) → x\n\
         useTwice ≔ λ(ignored) → id(◉)\n\
         a ≔ id(5)\n\
         result ≔ useTwice(◉)\n❧";
    let v = run_ok(source);
    assert!(matches!(v, Value::Bool(true)), "{v:?}");
}

#[test]
fn polymorphism_survives_around_an_actual_letrec_group() {
    // `id`'s polymorphism must survive being referenced from inside a
    // LetRec group's member, not only from an ordinary ValueDeclaration.
    let source = "Nat ≔ { Zero ⟢ Succ(Nat) }\n\
         id ≔ λ(x) → x\n\
         a ≔ id(5)\n\
         λuseId(n: Nat): ○ → ⟡ n {\n  Zero → id(◉)\n  ⟢ Succ(k) → useId(k)\n}\n\
         three ≔ Succ(Succ(Succ(Zero)))\n\
         result ≔ useId(three)\n❧";
    let v = run_ok(source);
    assert!(matches!(v, Value::Bool(true)), "{v:?}");
}

#[test]
fn lambda_forward_referencing_a_non_lambda_binding_is_a_documented_limitation_not_a_silent_wrong_answer(
) {
    // `f ≔ λ(ignored) → laterVal` is an ordinary ValueDeclaration, never
    // part of any LetRec group — `laterVal` is simply an unbound
    // reference and must be rejected.
    let source = "f ≔ λ(ignored) → laterVal\nlaterVal ≔ 5\nresult ≔ f(◉)\n❧";
    let diags = run_err(source);
    assert!(
        diags
            .iter()
            .any(|d| d.message.contains("unknown variable") || d.message.contains("ambiguous")),
        "{diags:?}"
    );
}

// ---------------------------------------------------------------------
// LetRec × capture × mutation × TCO interaction (§11/§12/§13/§14.1) —
// each piece has its own tests elsewhere; these target the interaction.
// ---------------------------------------------------------------------

#[test]
fn letrec_member_explicitly_captures_and_rebinds_a_backward_referenced_cell() {
    // bump is self-recursive and explicitly captures counter to rebind
    // it; `⚙︎ 1` sets (doesn't increment), so it settles on 1, not 3.
    let source = "Nat ≔ { Zero ⟢ Succ(Nat) }\n\
         counter ≔˚ 0\n\
         λbump(n: Nat): ⟁ → ⟡ n {\n  Zero → counter\n  ⟢ Succ(k) → (λ(ignored) → bump(k))(˚counter ⚙︎ 1)\n}\n\
         three ≔ Succ(Succ(Succ(Zero)))\n\
         result ≔ bump(three)\n❧";
    let v = run_ok(source);
    assert_eq!(as_int(v), 1);
}

#[test]
fn one_letrec_member_rebinds_shared_cell_a_sibling_member_only_reads() {
    let source = "Nat ≔ { Zero ⟢ Succ(Nat) }\n\
         counter ≔˚ 0\n\
         λbump(n: Nat): ⟁ → ⟡ n {\n  Zero → peek(◉)\n  ⟢ Succ(k) → (λ(ignored) → bump(k))(˚counter ⚙︎ 1)\n}\n\
         λpeek(x: ○): ⟁ → counter\n\
         three ≔ Succ(Succ(Succ(Zero)))\n\
         result ≔ bump(three)\n❧";
    let v = run_ok(source);
    assert_eq!(as_int(v), 1);
}

#[test]
fn tco_survives_a_self_recursive_letrec_member_that_mutates_every_iteration() {
    // Not a stack-depth stress test (that's obfusku-runtime's job) — this
    // just confirms mutation-per-iteration doesn't break the trampoline
    // for a real, source-level FunctionDeclaration.
    let source = "Nat ≔ { Zero ⟢ Succ(Nat) }\n\
         counter ≔˚ 0\n\
         λloop_(n: Nat): ⟁ → ⟡ n {\n  Zero → counter\n  ⟢ Succ(k) → (λ(ignored) → loop_(k))(˚counter ⚙︎ 1)\n}\n\
         three ≔ Succ(Succ(Succ(Zero)))\n\
         result ≔ loop_(three)\n❧";
    let v = run_ok(source);
    assert_eq!(as_int(v), 1);
}

#[test]
fn recursive_function_declaration_body_nesting_a_plain_lambda_still_honors_outer_capture() {
    let source = "counter ≔˚ 0\n\
         λbump(step: ⟁): ⟁ → (λ(ignored) → counter)(˚counter ⚙︎ step)\n\
         _c ≔ bump(5)\n\
         result ≔ counter\n❧";
    let v = run_ok(source);
    assert_eq!(as_int(v), 5);
}

#[test]
fn recursive_letrec_member_rebinding_without_explicit_capture_is_still_statically_rejected() {
    // The §13 static check (explicit capture required to rebind) must
    // not be bypassed just because the enclosing binding is
    // self-recursive.
    let source = "Nat ≔ { Zero ⟢ Succ(Nat) }\n\
         counter ≔˚ 0\n\
         λloop_(n: Nat): ⟁ → ⟡ n {\n  Zero → counter\n  ⟢ Succ(k) → (λ(ignored) → loop_(k))(counter ⚙︎ 1)\n}\n\
         result ≔ 1\n❧";
    let diags = run_err(source);
    assert!(
        diags[0].message.contains("desugars to a read"),
        "{}",
        diags[0].message
    );
}

#[test]
fn cell_invariance_holds_in_a_program_that_also_recurses_over_a_different_cell() {
    let source = "Nat ≔ { Zero ⟢ Succ(Nat) }\n\
         intCounter ≔˚ 0\n\
         λtouch(n: Nat): ⟁ → ⟡ n {\n  Zero → intCounter\n  ⟢ Succ(k) → (λ(ignored) → touch(k))(˚intCounter ⚙︎ 1)\n}\n\
         boolFlag ≔˚ ◉\n\
         bad ≔ ˚boolFlag ⚙︎ intCounter\n\
         result ≔ 1\n❧";
    let diags = run_err(source);
    assert!(
        diags.iter().any(|d| d.message.contains("cell holds Bool")),
        "{diags:?}"
    );
}

#[test]
fn mutation_inside_a_self_recursive_letrec_member_is_visible_from_an_ordinary_read_after_it_returns(
) {
    // A Bool flag flips inside toggle's recursion; an ordinary read
    // after it returns must see the flip — the real cell, not a copy.
    let source = "Nat ≔ { Zero ⟢ Succ(Nat) }\n\
         flag ≔˚ ◎\n\
         λtoggle(n: Nat): ○ → ⟡ n {\n  Zero → flag\n  ⟢ Succ(k) → (λ(ignored) → toggle(k))(˚flag ⚙︎ ◉)\n}\n\
         one ≔ Succ(Zero)\n\
         _first ≔ toggle(one)\n\
         result ≔ flag\n❧";
    let v = run_ok(source);
    assert!(matches!(v, Value::Bool(true)), "{v:?}");
}

#[test]
fn both_letrec_group_members_rebind_the_same_shared_cell_with_correct_interleaving() {
    // a(2): Succ -> flag=True, call b(1); b(1): Succ -> flag=False, call
    // a(0); a(0): Zero -> return flag (False). Proves evaluation order
    // and shared identity hold across mutual rebinds from both sides.
    let source = "Nat ≔ { Zero ⟢ Succ(Nat) }\n\
         flag ≔˚ ◎\n\
         λa(n: Nat): ○ → ⟡ n {\n  Zero → flag\n  ⟢ Succ(k) → (λ(ignored) → b(k))(˚flag ⚙︎ ◉)\n}\n\
         λb(n: Nat): ○ → ⟡ n {\n  Zero → flag\n  ⟢ Succ(k) → (λ(ignored) → a(k))(˚flag ⚙︎ ◎)\n}\n\
         two ≔ Succ(Succ(Zero))\n\
         result ≔ a(two)\n❧";
    let v = run_ok(source);
    assert!(matches!(v, Value::Bool(false)), "{v:?}");
}

// ---------------------------------------------------------------------
// Arithmetic & primitive operators — end-to-end
// (`SEMANTIC_CORE.md` §20.2, `CONCRETE_SYMBOLIC_GRAMMAR.md` §7.1a/§13).
// One vertical pass: real `.obk` source through lexer → parser → desugar
// → Core → typecheck → runtime, closing out Arithmetic as a feature.
// ---------------------------------------------------------------------

#[test]
fn arithmetic_precedence_end_to_end() {
    // 1 ✚ 2 ✱ 3 == 7, not 9 — multiplicative binds tighter.
    let v = run_ok("result ≔ 1 ✚ 2 ✱ 3\n❧");
    assert_eq!(as_int(v), 7);
}

#[test]
fn unary_negation_and_not_end_to_end() {
    let v = run_ok("result ≔ −3 ✱ 2\n❧");
    assert_eq!(as_int(v), -6);
    let v2 = run_ok("result ≔ ¬◎\n❧");
    assert!(matches!(v2, Value::Bool(true)));
}

#[test]
fn int_and_real_arithmetic_end_to_end() {
    let v = run_ok("result ≔ (10 ☠︎ 4) ÷ 2\n❧");
    assert_eq!(as_int(v), 3);
    let v2 = run_ok("result ≔ 1.5 ✚ 2.5\n❧");
    assert!(matches!(v2, Value::Real(r) if r == 4.0));
}

#[test]
fn string_concatenation_end_to_end() {
    let v = run_ok("result ≔ \"foo\" ✚ \"bar\"\n❧");
    assert!(matches!(v, Value::Str(s) if &*s == "foobar"));
}

#[test]
fn comparison_and_equality_end_to_end() {
    let v = run_ok("result ≔ (1 < 2) ∧ (2 ≡ 2)\n❧");
    assert!(matches!(v, Value::Bool(true)));
}

#[test]
fn xor_end_to_end() {
    let v = run_ok("result ≔ ◉ ⊻ ◉\n❧");
    assert!(matches!(v, Value::Bool(false)));
}

#[test]
fn and_short_circuits_end_to_end_the_second_operand_never_runs() {
    // False ∧ (1 ÷ 0 == 1): if ∧ evaluated the right side, this would
    // raise DivisionByZero. It must not.
    let v = run_ok("result ≔ ◎ ∧ ((1 ÷ 0) ≡ 1)\n❧");
    assert!(matches!(v, Value::Bool(false)));
}

#[test]
fn and_does_not_short_circuit_when_the_left_side_is_true() {
    let diags = run_err("result ≔ ◉ ∧ ((1 ÷ 0) ≡ 1)\n❧");
    assert!(
        diags[0].message.contains("DivisionByZero"),
        "{}",
        diags[0].message
    );
}

#[test]
fn or_short_circuits_end_to_end_the_second_operand_never_runs() {
    let v = run_ok("result ≔ ◉ ∨ ((1 ÷ 0) ≡ 1)\n❧");
    assert!(matches!(v, Value::Bool(true)));
}

#[test]
fn or_does_not_short_circuit_when_the_left_side_is_false() {
    let diags = run_err("result ≔ ◎ ∨ ((1 ÷ 0) ≡ 1)\n❧");
    assert!(
        diags[0].message.contains("DivisionByZero"),
        "{}",
        diags[0].message
    );
}

#[test]
fn division_by_zero_end_to_end() {
    let diags = run_err("result ≔ 1 ÷ 0\n❧");
    assert!(
        diags[0].message.contains("DivisionByZero"),
        "{}",
        diags[0].message
    );
}

#[test]
fn negative_modulo_end_to_end() {
    let v = run_ok("result ≔ −7 ⌗ 3\n❧");
    assert_eq!(as_int(v), -1);
}

#[test]
fn nan_equals_nan_end_to_end() {
    // 0.0 ÷ 0.0 == 0.0 ÷ 0.0 — both sides NaN, must be True (§18 total
    // equality), even though NaN < NaN etc. would be False.
    let v = run_ok("result ≔ (0.0 ÷ 0.0) ≡ (0.0 ÷ 0.0)\n❧");
    assert!(matches!(v, Value::Bool(true)));
}

#[test]
fn mixed_int_real_arithmetic_is_rejected_end_to_end() {
    let diags = run_err("result ≔ 1 ✚ 2.0\n❧");
    assert!(!diags.is_empty());
}

#[test]
fn arithmetic_composes_with_functions_and_letrec_end_to_end() {
    // A self-recursive FunctionDeclaration summing 1..=n via Nat descent,
    // using ✚ in its own body — composition with LetRec/recursion.
    let source = "Nat ≔ { Zero ⟢ Succ(Nat) }\n\
         λtoInt(n: Nat): ⟁ → ⟡ n {\n  Zero → 0\n  ⟢ Succ(k) → 1 ✚ toInt(k)\n}\n\
         three ≔ Succ(Succ(Succ(Zero)))\n\
         result ≔ toInt(three)\n❧";
    let v = run_ok(source);
    assert_eq!(as_int(v), 3);
}

// ---------------------------------------------------------------------
// Raise / Catch — end-to-end (`SEMANTIC_CORE.md` §15/§15.1/§15.2).
// ---------------------------------------------------------------------

#[test]
fn raise_and_catch_round_trip_end_to_end() {
    let v = run_ok("result ≔ ☊ (☄ DivisionByZero) λ(e) → 42\n❧");
    assert_eq!(as_int(v), 42);
}

#[test]
fn uncaught_raise_end_to_end() {
    let diags = run_err("result ≔ ☄ DivisionByZero\n❧");
    assert!(!diags.is_empty());
}

#[test]
fn catch_intercepts_a_real_division_by_zero_from_the_arithmetic_operator() {
    // Not user-raised — this is runtime's own ÷ raise, and a surface
    // ☊ handler catches it exactly like a user-raised Exception.
    let v = run_ok("result ≔ ☊ (1 ÷ 0) λ(e) → 99\n❧");
    assert_eq!(as_int(v), 99);
}

#[test]
fn raise_is_rejected_when_the_operand_is_not_an_exception() {
    let diags = run_err("result ≔ ☄ 42\n❧");
    assert!(!diags.is_empty());
}

#[test]
fn catch_handler_receives_the_raised_exception_value() {
    // Match the caught Exception by tag inside the handler.
    let source = "result ≔ ☊ (☄ Failure(\"oops\", \"bad\")) λ(e) → ⟡ e {\n  Failure(tag, msg) → tag\n  ⟢ DivisionByZero → \"div0\"\n  ⟢ NonExhaustiveMatch → \"nem\"\n  ⟢ InvalidOperation(m) → m\n  ⟢ IntegerOverflow → \"overflow\"\n}\n❧";
    let v = run_ok(source);
    assert!(matches!(v, Value::Str(s) if &*s == "oops"));
}

// ---------------------------------------------------------------------
// FunctionDeclaration annotations, actually checked — `CONCRETE_SYMBOLIC_GRAMMAR.md`
// §8.2/§9, `SEMANTIC_CORE.md`'s inference (annotations unify into the
// LetRec slot *before* the body is inferred, so a mismatch is an
// ordinary type error, not silently discarded).
// ---------------------------------------------------------------------

#[test]
fn function_declaration_annotation_matching_the_body_is_accepted() {
    let v = run_ok("λf(x: ⟁): ⟁ → x\nresult ≔ f(5)\n❧");
    assert_eq!(as_int(v), 5);
}

#[test]
fn function_declaration_annotation_that_contradicts_the_body_is_rejected() {
    // Declared to return ⟁ (Int), but the body actually returns ○ (Bool)
    // — this must be a static error, not silently accepted (annotations
    // are no longer discarded once parsed).
    let diags = run_err("λf(x: ⟁): ⟁ → ◉\nresult ≔ f(5)\n❧");
    assert!(!diags.is_empty());
}

#[test]
fn function_declaration_annotation_wrong_parameter_type_is_rejected() {
    // Declared to take ⟁ (Int), called with ○ (Bool).
    let diags = run_err("λf(x: ⟁): ⟁ → x\nresult ≔ f(◉)\n❧");
    assert!(!diags.is_empty());
}

#[test]
fn function_type_and_type_application_annotations_work_end_to_end() {
    // A higher-order function declared with an explicit function-typed
    // parameter (parens required, per the return-type/body arrow
    // disambiguation) and Array's own TypeApplication annotation.
    let source = "Array t ≔ { Empty ⟢ Cons(t, Array ▷ t) }\n\
         λapplyTwice(f: (⟁ → ⟁), x: ⟁): ⟁ → f(f(x))\n\
         inc ≔ λ(n) → n\n\
         result ≔ applyTwice(inc, 5)\n❧";
    let v = run_ok(source);
    assert_eq!(as_int(v), 5);
}

// ---------------------------------------------------------------------
// RecordBody / TupleBody
// ---------------------------------------------------------------------

#[test]
fn record_construction_and_named_pattern_match_end_to_end() {
    let source = "Point ≔ { x: ⧆ ⟢ y: ⧆ }\n\
         p ≔ Point { y: 2.0 ⟢ x: 1.0 }\n\
         result ≔ ⟡ p {\n\
           Point { x: a ⟢ y: b } → a ✚ b\n\
         }\n❧";
    let v = run_ok(source);
    match v {
        Value::Real(r) => assert_eq!(r, 3.0),
        other => panic!("expected Real, got {other:?}"),
    }
}

#[test]
fn record_construction_via_positional_syntax_also_works() {
    // ABSTRACT_GRAMMAR.md §4: Record/Tuple share the same Core
    // Constructor machinery as ordinary sum variants, so a record's
    // single self-tagged variant is still constructible positionally.
    let source = "Point ≔ { x: ⧆ ⟢ y: ⧆ }\n\
         p ≔ Point(1.0, 2.0)\n\
         result ≔ ⟡ p {\n\
           Point(a, b) → a ✚ b\n\
         }\n❧";
    let v = run_ok(source);
    match v {
        Value::Real(r) => assert_eq!(r, 3.0),
        other => panic!("expected Real, got {other:?}"),
    }
}

#[test]
fn tuple_construction_and_positional_pattern_match_end_to_end() {
    let source = "Pair ≔ (⟁, ⌘)\n\
         p ≔ Pair(1, \"a\")\n\
         result ≔ ⟡ p {\n\
           Pair(n, s) → n\n\
         }\n❧";
    let v = run_ok(source);
    assert_eq!(as_int(v), 1);
}

#[test]
fn named_construction_of_an_unknown_record_type_is_a_static_error() {
    let diags = run_err("p ≔ Point { x: 1.0 }\n❧");
    assert!(!diags.is_empty());
}

#[test]
fn named_construction_with_a_missing_field_is_a_static_error() {
    let diags = run_err("Point ≔ { x: ⧆ ⟢ y: ⧆ }\np ≔ Point { x: 1.0 }\n❧");
    assert!(!diags.is_empty());
}

#[test]
fn named_construction_field_type_mismatch_is_rejected_by_typecheck() {
    // `x` is declared ⧆ (Real); passing an Int must be a static error —
    // records reuse ordinary Constructor typing, no separate rule needed.
    let diags = run_err("Point ≔ { x: ⧆ ⟢ y: ⧆ }\np ≔ Point { x: 1 ⟢ y: 2.0 }\n❧");
    assert!(!diags.is_empty());
}

// ---------------------------------------------------------------------
// LocalBinding (ABSTRACT_GRAMMAR.md §3.6, CONCRETE_SYMBOLIC_GRAMMAR.md
// §7.4) — real source through the whole pipeline.
// ---------------------------------------------------------------------

#[test]
fn local_value_binding_end_to_end() {
    let v = run_ok("result ≔ x ≔ 5\n x ✚ 1\n❧");
    assert_eq!(as_int(v), 6);
}

#[test]
fn nested_local_value_bindings_end_to_end() {
    let v = run_ok("result ≔ x ≔ 1\n y ≔ 2\n z ≔ 3\n x ✚ y ✚ z\n❧");
    assert_eq!(as_int(v), 6);
}

#[test]
fn local_binding_value_is_not_visible_within_its_own_expression() {
    // §11: `Let`'s own value is never in its own scope.
    let diags = run_err("result ≔ x ≔ x ✚ 1\n x\n❧");
    assert!(!diags.is_empty());
}

#[test]
fn local_function_declaration_self_recursion_end_to_end() {
    let source = "result ≔ λcountdown(n: ⟁): ⟁ → ⟡ n {\n\
         0 → 0\n\
         ⟢ m → countdown(m ☠︎ 1)\n\
       }\n countdown(1000)\n❧";
    let v = run_ok(source);
    assert_eq!(as_int(v), 0);
}

#[test]
fn adjacent_local_function_declarations_mutual_recursion_end_to_end() {
    let source = "result ≔ λisEven(n: ⟁): ○ → ⟡ n {\n\
         0 → ◉\n\
         ⟢ m → isOdd(m ☠︎ 1)\n\
       }\n \
       λisOdd(n: ⟁): ○ → ⟡ n {\n\
         0 → ◎\n\
         ⟢ m → isEven(m ☠︎ 1)\n\
       }\n isEven(10)\n❧";
    let v = run_ok(source);
    match v {
        Value::Bool(b) => assert!(b),
        other => panic!("expected Bool, got {other:?}"),
    }
}

#[test]
fn local_mutable_binding_end_to_end() {
    // A `LocalBinding` chain: each step is `ValueDeclaration
    // Expression`, so "rebind then read" needs a (throwaway) name for
    // the rebind's `Unit` result before the final `x` continuation —
    // there is no bare-statement-sequencing form in this grammar.
    let source = "result ≔ x ≔˚ 5\n discard ≔ x ⚙︎ 9\n x\n❧";
    let v = run_ok(source);
    assert_eq!(as_int(v), 9);
}

#[test]
fn local_mutable_binding_does_not_leak_past_its_own_scope_end_to_end() {
    // Same outer name, never mutable — this must still typecheck and
    // evaluate as a plain read, proving the local mutability from an
    // earlier, already-finished LocalBinding didn't leak into it.
    let source = "x ≔ 1\n\
         first ≔ (x ≔˚ 5\n discard ≔ x ⚙︎ 9\n x)\n\
         result ≔ x\n❧";
    let v = run_ok(source);
    assert_eq!(as_int(v), 1);
}

#[test]
fn local_binding_composed_with_a_pipeline_end_to_end() {
    // `1 ✚ v`, not `v ✚ 1`: unlike a `FunctionDeclaration` parameter
    // (whose declared type now reaches its body before inference, per
    // the P0-A fix), an anonymous `λ(v) → …` parameter has no
    // annotation syntax at all (a separate, still-open gap) — `v`
    // really is unconstrained until something pins it, so it still
    // needs a literal on ✚'s left.
    let source = "result ≔ 5 ▷ λ(v) → x ≔ 1 ✚ v\n 1 ✚ x\n❧";
    let v = run_ok(source);
    assert_eq!(as_int(v), 7);
}

// ---------------------------------------------------------------------
// List<T> (§9.1): an ordinary generic ADT, Nil | Cons(T, List<T>). A
// recursive field must use explicit TypeApplication ('List ▷ t'); a
// bare 'List' never implicitly infers the enclosing type's parameter.
// ---------------------------------------------------------------------

#[test]
fn list_adt_end_to_end_via_explicit_type_application() {
    let source = "List t ≔ { Nil ⟢ Cons(t, List ▷ t) }\n\
         xs ≔ Cons(1, Cons(2, Nil))\n\
         result ≔ ⟡ xs {\n\
           Nil → 0\n\
           ⟢ Cons(h, t) → h\n\
         }\n❧";
    let v = run_ok(source);
    assert_eq!(as_int(v), 1);
}

#[test]
fn list_open_structural_recursion_end_to_end() {
    // List supports open head/rest recursion via ordinary Match/
    // Constructor recursion. `h ✚ sum(t)` needs no literal-leads
    // workaround: `h` is pattern-bound against `xs`'s already-concrete
    // element type, and `sum`'s own declared return type is already
    // known at its recursive call site — neither is an unconstrained
    // Lambda parameter, so operand order here was never the issue P0-A
    // fixed elsewhere.
    let source = "List t ≔ { Nil ⟢ Cons(t, List ▷ t) }\n\
         λsum(xs: List ▷ ⟁): ⟁ → ⟡ xs {\n\
           Nil → 0\n\
           ⟢ Cons(h, t) → h ✚ sum(t)\n\
         }\n\
         result ≔ sum(Cons(1, Cons(2, Cons(3, Nil))))\n❧";
    let v = run_ok(source);
    assert_eq!(as_int(v), 6);
}

#[test]
fn bare_generic_tag_with_wrong_arity_is_a_static_arity_error() {
    // A bare 'List' (zero args) used where 'List' is declared with one
    // type parameter — must be a real type error, never silently
    // accepted as a zero-argument reference to the same type.
    let source = "List t ≔ { Nil ⟢ Cons(t, List) }\n\
         xs ≔ Cons(1, Nil)\n❧";
    let diags = run_err(source);
    assert!(diags
        .iter()
        .any(|d| d.message.contains("expects 1 type argument")));
}

// ---------------------------------------------------------------------
// Array<T> (SEMANTIC_CORE.md §9.2, CONCRETE_SYMBOLIC_GRAMMAR.md §7.9)
// ---------------------------------------------------------------------

#[test]
fn array_literal_and_index_end_to_end() {
    let v = run_ok("xs ≔ [10, 20, 30]\nresult ≔ xs[1]\n❧");
    assert_eq!(as_int(v), 20);
}

#[test]
fn empty_array_literal_end_to_end_context_pins_element_type() {
    // `[]`'s element type is pinned via `Match`'s "every arm shares one
    // type" rule against a concrete `Array<⟁>` sibling, without indexing
    // into (and evaluating) the empty array itself.
    let source = "xs ≔ []\nys ≔ [1, 2, 3]\nresult ≔ ⟡ ◉ {\n  ◉ → xs\n  ⟢ ◎ → ys\n}\n❧";
    let v = run_ok(source);
    match v {
        Value::Array(elems) => assert!(elems.is_empty()),
        other => panic!("expected an Array, got {other:?}"),
    }
}

#[test]
fn out_of_bounds_index_is_an_uncaught_exception_end_to_end() {
    let diags = run_err("xs ≔ [1, 2]\nresult ≔ xs[9]\n❧");
    assert!(!diags.is_empty());
}

#[test]
fn out_of_bounds_index_is_catchable_end_to_end() {
    let v = run_ok("xs ≔ [1, 2]\nresult ≔ ☊ xs[9] λ(e) → 0\n❧");
    assert_eq!(as_int(v), 0);
}

#[test]
fn array_element_type_mismatch_is_a_static_error() {
    let diags = run_err("xs ≔ [1, ◉, 3]\n❧");
    assert!(!diags.is_empty());
}

#[test]
fn array_index_must_be_int_static_error() {
    let diags = run_err("xs ≔ [1, 2]\nresult ≔ xs[\"nope\"]\n❧");
    assert!(!diags.is_empty());
}

#[test]
fn chained_indexing_of_nested_arrays_end_to_end() {
    let v = run_ok("xs ≔ [[1, 2], [3, 4]]\nresult ≔ xs[1][0]\n❧");
    assert_eq!(as_int(v), 3);
}

#[test]
fn array_equality_end_to_end() {
    let v = run_ok("a ≔ [1, 2, 3]\nb ≔ [1, 2, 3]\nresult ≔ ⟡ (a ≡ b) {\n  ◉ → 1\n  ⟢ ◎ → 0\n}\n❧");
    assert_eq!(as_int(v), 1);
}

// ---------------------------------------------------------------------
// `run`'s reported value (ROADMAP.md §1: operationally defined, not
// "module-result semantics") — frozen contract, not just incidental
// behavior. `⟳` must never affect which binding `run` reports.
// ---------------------------------------------------------------------

#[test]
fn run_reports_the_last_top_level_binding_regardless_of_name() {
    let v = run_ok("first \u{2254} 1\nsecond \u{2254} 2\nlast \u{2254} 3\n\u{2767}\n");
    assert_eq!(as_int(v), 3);
}

#[test]
fn an_unexported_last_binding_is_still_what_run_reports() {
    // `\u{27f3}` (\u{2254}\u{27f3}) exports a binding; the *last* binding
    // here is deliberately NOT exported, and `run` must still report it —
    // export status and "what run reports" are unrelated concepts.
    let v = run_ok("visible \u{2254}\u{27f3} 1\nhidden \u{2254} 2\n\u{2767}\n");
    assert_eq!(as_int(v), 2);
}

#[test]
fn an_exported_binding_that_is_not_last_is_not_what_run_reports() {
    // The inverse of the above: exporting a binding does not make it
    // `run`'s reported value if a later, unexported binding follows it.
    let v = run_ok("visible \u{2254}\u{27f3} 1\nlast \u{2254} 2\n\u{2767}\n");
    assert_eq!(as_int(v), 2);
}
