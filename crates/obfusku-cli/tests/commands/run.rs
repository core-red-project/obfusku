//! `run` and `check`: exit codes, rendered diagnostics, warnings, call-chain traces, and Project directories as arguments.

use crate::support::{TempTree, bin, run_in, write_temp};

fn unreachable_arm_source() -> &'static str {
    "Box t \u{2254} { Full(t) \u{27e2} Empty }\n\
     x \u{2254} Full(5)\n\
     result \u{2254} \u{27e1} x {\n  \
     Full(n) \u{2192} n\n  \u{27e2} Full(m) \u{2192} m\n  \u{27e2} Empty \u{2192} 0\n}\n\u{2767}\n"
}

#[test]
fn run_a_well_typed_program_prints_the_result_and_exits_zero() {
    let f = write_temp("x \u{2254} 5\nresult \u{2254} x \u{271A} 1\n\u{2767}\n");
    let out = bin().arg("run").arg(f.path()).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains('6'), "{stdout}");
}

#[test]
fn run_an_ill_typed_program_exits_one_and_reports_line_and_column() {
    let f = write_temp("x \u{2254} 1\nresult \u{2254} nope\n\u{2767}\n");
    let out = bin().arg("run").arg(f.path()).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.contains("unknown variable"), "{stderr}");
    assert!(
        stderr.contains("(2:"),
        "expected a line:col location, got: {stderr}"
    );
}

#[test]
fn duplicate_top_level_binding_reports_line_col_for_both_occurrences_never_a_byte_offset() {
    // Category-1 audit finding #26: the first occurrence used to be
    // described as "first bound at byte N" — a raw byte offset baked
    // into the message text, inconsistent with every other diagnostic
    // location. Three blank lines before the duplicate force a
    // genuinely non-zero, multi-line location, so this can't pass by
    // coincidence the way a same-line duplicate might.
    let f = write_temp("x \u{2254} 1\n\n\n\nx \u{2254} 2\n\u{2767}\n");
    let out = bin().arg("check").arg(f.path()).output().unwrap();
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(!stderr.contains("byte"), "{stderr}");
    assert!(
        stderr.contains("(5:1)"),
        "expected the duplicate at (5:1), got: {stderr}"
    );
    assert!(
        stderr.contains("(1:1)"),
        "expected the first occurrence noted at (1:1), got: {stderr}"
    );
}

#[test]
fn run_missing_file_exits_two() {
    let out = bin().arg("run").arg("/no/such/file.obk").output().unwrap();
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn run_missing_argument_exits_two() {
    let out = bin().arg("run").output().unwrap();
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn check_a_well_typed_program_prints_ok_and_never_executes() {
    // Division by zero would raise at runtime if this were executed —
    // `check` must not evaluate it, only type-check.
    let f = write_temp("result \u{2254} 1 \u{00F7} 0\n\u{2767}\n");
    let out = bin().arg("check").arg(f.path()).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("ok"), "{stdout}");
}

#[test]
fn check_an_ill_typed_program_exits_one() {
    let f = write_temp("result \u{2254} nope\n\u{2767}\n");
    let out = bin().arg("check").arg(f.path()).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn check_surfaces_an_unreachable_arm_warning_but_still_reports_ok_and_exits_zero() {
    let f = write_temp(unreachable_arm_source());
    let out = bin().arg("check").arg(f.path()).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stdout.contains("ok"), "{stdout}");
    assert!(stderr.contains("unreachable"), "{stderr}");
    assert!(stderr.contains("warning:"), "{stderr}");
}

#[test]
fn two_arms_of_one_constructor_with_different_nested_patterns_are_not_unreachable() {
    let f = write_temp(
        "S \u{2254} { A(\u{27c1}) \u{27e2} B(\u{27c1}) }\n\
         \u{3bb}f(xs: List \u{25b7} S): \u{27c1} \u{2192} \u{27e1} xs {\n  \
         \u{2298} \u{2192} 0\n  \u{27e2} \u{205d}(A(n), t) \u{2192} n\n  \
         \u{27e2} \u{205d}(B(n), t) \u{2192} n\n}\nresult \u{2254} f(\u{2298})\n\u{2767}\n",
    );
    let out = bin().arg("check").arg(f.path()).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(!stderr.contains("unreachable"), "{stderr}");
}

#[test]
fn run_surfaces_an_unreachable_arm_warning_but_still_executes_and_exits_zero() {
    let f = write_temp(unreachable_arm_source());
    let out = bin().arg("run").arg(f.path()).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stdout.contains('5'), "{stdout}");
    assert!(stderr.contains("unreachable"), "{stderr}");
}

#[test]
fn a_program_with_no_warnings_prints_nothing_to_stderr_on_check() {
    let f = write_temp("result \u{2254} 5\n\u{2767}\n");
    let out = bin().arg("check").arg(f.path()).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.is_empty(), "{stderr}");
}

#[test]
fn check_accepts_a_project_directory() {
    let tree = TempTree::new();
    tree.manifest("proj", "main");
    tree.write("proj/main.obk", "result \u{2254} 1\n\u{2767}\n");
    let out = bin()
        .arg("check")
        .arg(tree.root().join("proj"))
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
}

#[test]
fn an_uncaught_exception_reports_the_call_chain() {
    let tree = TempTree::new();
    let src = "λdiv(a: ⟁, b: ⟁): ⟁ → a ÷ b\nλmid(n: ⟁): ⟁ → div(10, n) ✚ 1\n\
        λtop(n: ⟁): ⟁ → mid(n) ✚ 1\nresult ≔ top(0)\n❧";
    let out = run_in(tree.root(), src);
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8(out.stderr).unwrap();
    let div = stderr.find("called 'div'").expect(&stderr);
    let mid = stderr.find("called 'mid'").expect(&stderr);
    let top = stderr.find("called 'top'").expect(&stderr);
    assert!(div < mid && mid < top, "innermost first: {stderr}");
}

#[test]
fn a_caught_exception_leaves_no_stale_trace() {
    let tree = TempTree::new();
    let src = "λdiv(a: ⟁, b: ⟁): ⟁ → a ÷ b\n\
        safe ≔ ☊ div(1, 0) { _ → 0 }\nfail ≔ ☄ Failure(\"x\", \"y\")\nresult ≔ safe\n❧";
    let out = run_in(tree.root(), src);
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(!stderr.contains("called 'div'"), "{stderr}");
}
