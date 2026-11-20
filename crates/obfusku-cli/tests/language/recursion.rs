//! Recursion and the interactions that make it hard: letrec groups with capture, mutation and tail calls, and mutual recursion.

use crate::support::{as_int, run_err, run_ok};
use obfusku_runtime::Value;

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
fn mutation_inside_a_self_recursive_letrec_member_is_visible_from_an_ordinary_read_after_it_returns()
 {
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
