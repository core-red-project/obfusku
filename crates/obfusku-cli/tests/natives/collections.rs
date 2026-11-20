//! `Array`, `Map` and `Set` natives (`ADR-021`, `ADR-025`), including limits that must raise instead of aborting.

use crate::support::{TempTree, bin, run_in, write_temp};

#[test]
fn array_map_transforms_every_element() {
    let f = write_temp(
        "xs ≔ [1, 2, 3]\n\
         result ≔ ⊡(λ(x: ⟁) → x ✱ 2)(xs)\n\u{2767}\n",
    );
    let out = bin().arg("run").arg(f.path()).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains('2') && stdout.contains('6'), "{stdout}");
}

#[test]
fn array_filter_keeps_only_matching_elements() {
    let f = write_temp(
        "xs ≔ [1, 2, 3, 4]\n\
         result ≔ ⊟(λ(x: ⟁) → x ⌗ 2 ≡ 0)(xs)\n\u{2767}\n",
    );
    let out = bin().arg("run").arg(f.path()).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(
        stdout.contains('2') && stdout.contains('4') && !stdout.contains('1'),
        "{stdout}"
    );
}

#[test]
fn array_fold_combines_left_to_right() {
    let f = write_temp(
        "xs ≔ [1, 2, 3, 4]\n\
         result ≔ ⊞(λ(a: ⟁) → λ(b: ⟁) → a ✚ b)(0)(xs)\n\u{2767}\n",
    );
    let out = bin().arg("run").arg(f.path()).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("10"), "{stdout}");
}

#[test]
fn array_length_reports_the_element_count() {
    let f = write_temp("result ≔ #([1, 2, 3, 4, 5])\n\u{2767}\n");
    let out = bin().arg("run").arg(f.path()).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains('5'), "{stdout}");
}

#[test]
fn array_set_is_persistent_and_does_not_mutate_the_original() {
    let f = write_temp(
        "xs ≔ [1, 2, 3]\n\
         updated ≔ ⊙(xs)(1)(99)\n\
         result ≔ xs[1] \u{271a} updated[1]\n\u{2767}\n",
    );
    let out = bin().arg("run").arg(f.path()).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    // xs[1] (still 2, untouched) + updated[1] (99) = 101 — if `⊙`
    // mutated `xs` in place instead of producing a new Array, this
    // would read 198 (99 + 99) instead.
    assert!(stdout.contains("101"), "{stdout}");
}

#[test]
fn array_set_out_of_bounds_is_a_clear_runtime_error_not_a_crash() {
    let f = write_temp("result ≔ ⊙([1, 2, 3])(10)(99)\n\u{2767}\n");
    let out = bin().arg("run").arg(f.path()).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.contains("out of bounds"), "{stderr}");
}

#[test]
fn a_huge_range_count_is_a_catchable_error_not_an_abort() {
    let tree = TempTree::new();
    let file = tree.write("big.obk", "r ≔ ⍳(9223372036854775807)\n❧\n");
    let out = bin().arg("run").arg(file).output().unwrap();
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stderr).contains("at most"));
}

#[test]
fn list_combinators_run_over_a_million_elements() {
    let tree = TempTree::new();
    let src = "λbuild(i: ⟁, acc: List ▷ ⟁): List ▷ ⟁ → ⟡ (i ≡ 0) { ◉ → acc ⟢ ◎ → build(i ☠︎ 1, ⁝(i, acc)) }\n\
        ys ≔ ⟐(λ(n: ⟁) → n ✚ 1, build(1000000, ⊘))\n\
        result ≔ ⌽(λ(a: ⟁, n: ⟁) → a ✚ n, 0, ⌿(λ(n: ⟁) → (n ⌗ 2) ≡ 0, ys))\n❧";
    let out = run_in(tree.root(), src);
    assert!(out.status.success(), "{out:?}");
    assert_eq!(String::from_utf8(out.stdout).unwrap(), "250000500000\n");
}
