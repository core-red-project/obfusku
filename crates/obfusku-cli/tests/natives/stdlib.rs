//! The ambient stdlib seen from a program: `List` combinators and `Result`, with no import (`ADR-012`).

use crate::support::{bin, write_temp};

#[test]
fn map_is_ambiently_available_and_recurses_over_a_multi_element_list() {
    let f = write_temp(
        "\u{3bb}addOne(n: \u{27c1}): \u{27c1} \u{2192} 1 \u{271a} n\n\
         \u{3bb}addInts(acc: \u{27c1}, n: \u{27c1}): \u{27c1} \u{2192} acc \u{271a} n\n\
         xs \u{2254} \u{205D}(1, \u{205D}(2, \u{205D}(3, ⊘)))\n\
         mapped \u{2254} \u{27D0}(addOne, xs)\n\
         result \u{2254} \u{233D}(addInts, 0, mapped)\n\u{2767}\n",
    );
    let out = bin().arg("run").arg(f.path()).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    // (1+1) + (2+1) + (3+1) = 9
    assert!(stdout.contains('9'), "{stdout}");
}

#[test]
fn filter_is_ambiently_available_and_keeps_only_matching_elements() {
    let f = write_temp(
        "\u{3bb}isEven(n: \u{27c1}): \u{25cb} \u{2192} \u{27e1} n \u{2317} 2 {\n  \
         0 \u{2192} \u{25c9}\n  \u{27e2} _ \u{2192} \u{25ce}\n}\n\
         \u{3bb}addInts(acc: \u{27c1}, n: \u{27c1}): \u{27c1} \u{2192} acc \u{271a} n\n\
         xs \u{2254} \u{205D}(1, \u{205D}(2, \u{205D}(3, \u{205D}(4, ⊘))))\n\
         filtered \u{2254} \u{233F}(isEven, xs)\n\
         result \u{2254} \u{233D}(addInts, 0, filtered)\n\u{2767}\n",
    );
    let out = bin().arg("run").arg(f.path()).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    // 2 + 4 = 6
    assert!(stdout.contains('6'), "{stdout}");
}

#[test]
fn fold_over_nil_returns_the_seed_unchanged() {
    let f = write_temp(
        "\u{3bb}addInts(acc: \u{27c1}, n: \u{27c1}): \u{27c1} \u{2192} acc \u{271a} n\n\
         result \u{2254} \u{233D}(addInts, 0, ⊘)\n\u{2767}\n",
    );
    let out = bin().arg("run").arg(f.path()).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains('0'), "{stdout}");
}

#[test]
fn filter_over_nil_is_nil_and_folds_to_the_seed() {
    let f = write_temp(
        "\u{3bb}isEven(n: \u{27c1}): \u{25cb} \u{2192} \u{27e1} n \u{2317} 2 {\n  \
         0 \u{2192} \u{25c9}\n  \u{27e2} _ \u{2192} \u{25ce}\n}\n\
         \u{3bb}addInts(acc: \u{27c1}, n: \u{27c1}): \u{27c1} \u{2192} acc \u{271a} n\n\
         result \u{2254} \u{233D}(addInts, 0, \u{233F}(isEven, ⊘))\n\u{2767}\n",
    );
    let out = bin().arg("run").arg(f.path()).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains('0'), "{stdout}");
}

#[test]
fn fold_over_a_singleton_list_applies_the_combining_function_exactly_once() {
    let f = write_temp(
        "\u{3bb}addInts(acc: \u{27c1}, n: \u{27c1}): \u{27c1} \u{2192} acc \u{271a} n\n\
         result \u{2254} \u{233D}(addInts, 100, \u{205D}(7, ⊘))\n\u{2767}\n",
    );
    let out = bin().arg("run").arg(f.path()).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("107"), "{stdout}");
}

#[test]
fn fold_accumulates_strictly_left_to_right() {
    // \u{233D}(f, "seed-", [a, b]) == f(f("seed-", a), b) == "seed-ab" —
    // a right-to-left (or reordered) \u{233D} would produce a different
    // string, so this pins down the accumulator's evaluation order.
    let f = write_temp(
        "\u{3bb}concatStr(acc: \u{2318}, s: \u{2318}): \u{2318} \u{2192} acc \u{271a} s\n\
         result \u{2254} \u{233D}(concatStr, \u{22}seed-\u{22}, \u{205D}(\u{22}a\u{22}, \u{205D}(\u{22}b\u{22}, ⊘)))\n\u{2767}\n",
    );
    let out = bin().arg("run").arg(f.path()).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("seed-ab"), "{stdout}");
}

#[test]
fn map_is_genuinely_polymorphic_reusable_at_int_and_str_in_the_same_module() {
    // `\u{27D0}`/`addInts`/`concatStr` are each used once at `Int` and once
    // at `Str`, within one module — proof the stdlib's `\u{27D0}` really is
    // `∀t,u. (t → u) → List<t> → List<u>`, not implicitly pinned to
    // whichever element type happened to type-check it first.
    let f = write_temp(
        "\u{3bb}addOne(n: \u{27c1}): \u{27c1} \u{2192} 1 \u{271a} n\n\
         \u{3bb}idStr(s: \u{2318}): \u{2318} \u{2192} s\n\
         \u{3bb}addInts(acc: \u{27c1}, n: \u{27c1}): \u{27c1} \u{2192} acc \u{271a} n\n\
         \u{3bb}concatStr(acc: \u{2318}, s: \u{2318}): \u{2318} \u{2192} acc \u{271a} s\n\
         xsInt \u{2254} \u{205D}(1, \u{205D}(2, ⊘))\n\
         xsStr \u{2254} \u{205D}(\u{22}a\u{22}, \u{205D}(\u{22}b\u{22}, ⊘))\n\
         mappedInt \u{2254} \u{27D0}(addOne, xsInt)\n\
         mappedStr \u{2254} \u{27D0}(idStr, xsStr)\n\
         sumInt \u{2254} \u{233D}(addInts, 0, mappedInt)\n\
         concatResult \u{2254} \u{233D}(concatStr, \u{22}\u{22}, mappedStr)\n\
         result \u{2254} sumInt\n\u{2767}\n",
    );
    let out = bin().arg("check").arg(f.path()).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    let out = bin().arg("run").arg(f.path()).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    // (1+1) + (2+1) = 5
    assert!(stdout.contains('5'), "{stdout}");
}

#[test]
fn a_local_binding_may_shadow_a_stdlib_name_without_breaking_the_stdlib() {
    // `ADR-028`: ambient names are replaceable per module; the stdlib's
    // own `⌿` still calls the real `⟐`-independent code.
    let src = "\u{27D0} \u{2254} 5\n\
        evens \u{2254} \u{233F}(\u{3BB}(n: \u{27C1}) \u{2192} (n \u{2317} 2) \u{2261} 0, ⁝(1, ⁝(2, ⊘)))\n\
        result \u{2254} \u{27D0}\n\u{2767}\n";
    let f = write_temp(src);
    let out = bin().arg("run").arg(f.path()).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    assert!(String::from_utf8(out.stdout).unwrap().contains('5'));
}

#[test]
fn result_ok_is_ambiently_available_and_pattern_matches() {
    let f = write_temp(
        "x ≔ ✓(42)\n\
         result ≔ ⟡ x {\n  ✓(n) → n\n  ⟢ ✗(e) → 0 ☠︎ e\n}\n\u{2767}\n",
    );
    let out = bin().arg("run").arg(f.path()).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("42"), "{stdout}");
}

#[test]
fn result_err_is_ambiently_available_and_pattern_matches() {
    let f = write_temp(
        "x : Result ▷ ⟁ ▷ ⌘ ≔ ✗(\"oops\")\n\
         result ≔ ⟡ x {\n  ✓(n) → \"got value\"\n  ⟢ ✗(e) → e\n}\n\u{2767}\n",
    );
    let out = bin().arg("run").arg(f.path()).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("oops"), "{stdout}");
}

#[test]
fn result_is_genuinely_polymorphic_in_both_type_parameters_independently() {
    // Ok and Err carry unrelated types in this one program (Int/Str vs
    // Str/Int) — proof Result<T,E> really is ∀t,e, not pinned to
    // whichever pair happened to type-check it first.
    let f = write_temp(
        "a ≔ ✓(5)\n\
         b ≔ \u{27E1} a {\n  ✓(n) → n\n  \u{27E2} ✗(e) → 0 ☠︎ e\n}\n\
         c ≔ ✗(\"bad\")\n\
         d ≔ \u{27E1} c {\n  ✓(s) → s\n  \u{27E2} ✗(e) → e\n}\n\
         result ≔ b\n\u{2767}\n",
    );
    let out = bin().arg("check").arg(f.path()).output().unwrap();
    assert!(out.status.success(), "{out:?}");
}

#[test]
fn a_local_type_reusing_results_constructor_tags_is_a_static_error() {
    let f = write_temp("Result t e ≔ { ✓(t) ⟢ ✗(e) }\nresult ≔ 1\n\u{2767}\n");
    let out = bin().arg("check").arg(f.path()).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        stderr.contains("already bound by the ambient prelude"),
        "{stderr}"
    );
}
