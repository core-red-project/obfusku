//! Values and everyday forms: literals, references, functions, pipelines, mutation, local bindings, and what `run` reports.

use crate::support::{as_int, run_err, run_ok};
use obfusku_runtime::Value;

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
    assert!(matches!(run_ok("x ≔ \"hello\"\n❧"), Value::Str(ref s) if &**s == "hello"));
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

#[test]
fn reference_to_an_earlier_binding_source_to_runtime_value() {
    let v = run_ok("x ≔ 5\ny ≔ x\n❧");
    assert_eq!(as_int(v), 5);
}

#[test]
fn identity_function_source_to_runtime_int() {
    // source -> AST -> Core -> typed Core -> runtime -> Int(5)
    let v = run_ok("f ≔ λ(x) → x\nresult ≔ f(5)\n❧");
    assert_eq!(as_int(v), 5);
}

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

#[test]
fn mutable_binding_mutread_mutrebind_and_closure_capture_end_to_end() {
    // bump explicitly captures counter (`˚counter`) and rebinds it; an
    // ordinary later reference must see the write via MutRead.
    let v = run_ok(
        "counter ≔˚ 0\nbump ≔ λ(step) → ˚counter ⚙︎ step\n_called ≔ bump(5)\nresult ≔ counter\n❧",
    );
    assert_eq!(as_int(v), 5);
}

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
