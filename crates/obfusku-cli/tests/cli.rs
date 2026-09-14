//! Real end-to-end tests against the compiled `obfusku` executable —
//! §12's command table, exercised as a user actually would (argv, exit
//! codes, stdout/stderr), not just through `obfusku_cli`'s library
//! functions (already covered by `tests/e2e.rs`).

use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_obfusku"))
}

/// A `.obk` file under the system temp directory, deleted when dropped.
/// No external crate needed for something this small.
struct TempObkFile(std::path::PathBuf);

impl TempObkFile {
    fn new(source: &str) -> Self {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("obfusku-cli-test-{}-{n}.obk", std::process::id()));
        std::fs::write(&path, source).expect("write temp .obk file");
        TempObkFile(path)
    }

    fn path(&self) -> &std::path::Path {
        &self.0
    }
}

impl Drop for TempObkFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn write_temp(source: &str) -> TempObkFile {
    TempObkFile::new(source)
}

/// A directory of named `.obk` files, for import-resolution tests —
/// `⟲ mathutil` must resolve to a real `mathutil.obk` sitting next to
/// the importing file, not an arbitrary temp filename.
struct TempObkDir(std::path::PathBuf);

impl TempObkDir {
    fn new() -> Self {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("obfusku-cli-test-dir-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        TempObkDir(dir)
    }

    fn write(&self, name: &str, source: &str) -> std::path::PathBuf {
        let path = self.0.join(format!("{name}.obk"));
        std::fs::write(&path, source).expect("write module file");
        path
    }
}

impl Drop for TempObkDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn version_prints_the_crate_version_and_exits_zero() {
    let out = bin().arg("version").output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains(env!("CARGO_PKG_VERSION")), "{stdout}");
}

#[test]
fn dash_dash_version_is_an_exact_alias_of_the_version_subcommand() {
    let via_flag = bin().arg("--version").output().unwrap();
    let via_short = bin().arg("-V").output().unwrap();
    let via_subcommand = bin().arg("version").output().unwrap();
    assert!(via_flag.status.success());
    assert!(via_short.status.success());
    assert_eq!(via_flag.stdout, via_subcommand.stdout);
    assert_eq!(via_short.stdout, via_subcommand.stdout);
}

#[test]
fn dash_dash_help_prints_usage_to_stdout_and_exits_zero() {
    // Distinct from the no-args/unknown-command cases: explicitly
    // requested help is success (stdout, exit 0), not a usage error
    // (stderr, exit 2).
    let out = bin().arg("--help").output().unwrap();
    assert!(out.status.success(), "{:?}", out);
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("usage:"), "{stdout}");
    assert!(out.stderr.is_empty(), "{:?}", out.stderr);

    let out_short = bin().arg("-h").output().unwrap();
    assert!(out_short.status.success());
    assert_eq!(stdout.as_bytes(), out_short.stdout);
}

#[test]
fn dash_dash_help_short_circuits_from_any_position_after_a_subcommand() {
    // `obfusku run --help` must show help, not try to run a file
    // literally named "--help" — the near-universal GNU convention.
    let out = bin().arg("run").arg("--help").output().unwrap();
    assert!(out.status.success(), "{:?}", out);
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("usage:"), "{stdout}");
}

#[test]
fn dash_dash_version_short_circuits_from_any_position() {
    let out = bin().arg("check").arg("--version").output().unwrap();
    assert!(out.status.success(), "{:?}", out);
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains(env!("CARGO_PKG_VERSION")), "{stdout}");
}

#[test]
fn inspect_accepts_the_mode_flag_before_or_after_the_path() {
    let f = write_temp("x \u{2254} 5\n\u{2767}\n");
    let after = bin()
        .arg("inspect")
        .arg(f.path())
        .arg("--ast")
        .output()
        .unwrap();
    let before = bin()
        .arg("inspect")
        .arg("--ast")
        .arg(f.path())
        .output()
        .unwrap();
    assert!(after.status.success(), "{:?}", after);
    assert!(before.status.success(), "{:?}", before);
    assert_eq!(after.stdout, before.stdout);
}

#[test]
fn inspect_rejects_an_unknown_flag_regardless_of_position() {
    let f = write_temp("x \u{2254} 5\n\u{2767}\n");
    let out = bin()
        .arg("inspect")
        .arg(f.path())
        .arg("--bogus")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.contains("unknown inspect flag"), "{stderr}");
}

#[test]
fn unknown_command_exits_two_and_prints_usage() {
    let out = bin().arg("bogus").output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.contains("unknown command: bogus"), "{stderr}");
    assert!(stderr.contains("usage:"), "{stderr}");
}

#[test]
fn no_command_exits_two_and_prints_usage() {
    let out = bin().output().unwrap();
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn run_a_well_typed_program_prints_the_result_and_exits_zero() {
    let f = write_temp("x \u{2254} 5\nresult \u{2254} x \u{271A} 1\n\u{2767}\n");
    let out = bin().arg("run").arg(f.path()).output().unwrap();
    assert!(out.status.success(), "{:?}", out);
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("Int(6)"), "{stdout}");
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
    assert_eq!(out.status.code(), Some(1), "{:?}", out);
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
    assert!(out.status.success(), "{:?}", out);
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("ok"), "{stdout}");
}

#[test]
fn check_an_ill_typed_program_exits_one() {
    let f = write_temp("result \u{2254} nope\n\u{2767}\n");
    let out = bin().arg("check").arg(f.path()).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
}

// ── P0-D: `Int` arithmetic overflow raises `IntegerOverflow`,
//    deterministically, never a process panic in any build profile. ──

#[test]
fn uncaught_integer_overflow_is_a_clean_diagnostic_not_a_crash() {
    let f = write_temp("result \u{2254} 9223372036854775807 \u{271a} 1\n\u{2767}\n");
    let out = bin().arg("run").arg(f.path()).output().unwrap();
    assert_eq!(out.status.code(), Some(1), "{:?}", out);
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.contains("IntegerOverflow"), "{stderr}");
}

#[test]
fn integer_overflow_is_catchable_by_the_existing_raise_catch_mechanism() {
    let f = write_temp(
        "result \u{2254} \u{260a} (9223372036854775807 \u{271a} 1) \u{3bb}(e) \u{2192} \u{27e1} e {\n  \
         IntegerOverflow \u{2192} 42\n  \u{27e2} DivisionByZero \u{2192} 0\n  \
         \u{27e2} NonExhaustiveMatch \u{2192} 0\n  \u{27e2} InvalidOperation(m) \u{2192} 0\n  \
         \u{27e2} Failure(t, m) \u{2192} 0\n}\n\u{2767}\n",
    );
    let out = bin().arg("run").arg(f.path()).output().unwrap();
    assert!(out.status.success(), "{:?}", out);
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("Int(42)"), "{stdout}");
}

// ── Category-1 audit finding #18: `obfusku-typecheck` already computed
//    unreachable-match-arm warnings; nothing in the CLI ever surfaced
//    them, on any command. ────────────────────────────────────────────

fn unreachable_arm_source() -> &'static str {
    "Box t \u{2254} { Full(t) \u{27e2} Empty }\n\
     x \u{2254} Full(5)\n\
     result \u{2254} \u{27e1} x {\n  \
     Full(n) \u{2192} n\n  \u{27e2} Full(m) \u{2192} m\n  \u{27e2} Empty \u{2192} 0\n}\n\u{2767}\n"
}

#[test]
fn check_surfaces_an_unreachable_arm_warning_but_still_reports_ok_and_exits_zero() {
    let f = write_temp(unreachable_arm_source());
    let out = bin().arg("check").arg(f.path()).output().unwrap();
    assert!(out.status.success(), "{:?}", out);
    let stdout = String::from_utf8(out.stdout).unwrap();
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stdout.contains("ok"), "{stdout}");
    assert!(stderr.contains("unreachable"), "{stderr}");
    assert!(stderr.contains("warning:"), "{stderr}");
}

#[test]
fn run_surfaces_an_unreachable_arm_warning_but_still_executes_and_exits_zero() {
    let f = write_temp(unreachable_arm_source());
    let out = bin().arg("run").arg(f.path()).output().unwrap();
    assert!(out.status.success(), "{:?}", out);
    let stdout = String::from_utf8(out.stdout).unwrap();
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stdout.contains("Int(5)"), "{stdout}");
    assert!(stderr.contains("unreachable"), "{stderr}");
}

#[test]
fn a_program_with_no_warnings_prints_nothing_to_stderr_on_check() {
    let f = write_temp("result \u{2254} 5\n\u{2767}\n");
    let out = bin().arg("check").arg(f.path()).output().unwrap();
    assert!(out.status.success(), "{:?}", out);
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.is_empty(), "{stderr}");
}

// ── P1-1: `Argument ::= Expression | "•"` (§7.2/§12.1) end-to-end. ─────

#[test]
fn trailing_hole_produces_the_same_result_as_ordinary_currying() {
    let f = write_temp(
        "\u{3bb}f(a: \u{27c1}, b: \u{27c1}, c: \u{27c1}): \u{27c1} \u{2192} a \u{271a} b \u{2731} c\n\
         partial \u{2254} f(1, \u{2022}, 3)\n\
         result \u{2254} partial(2)\n\u{2767}\n",
    );
    let out = bin().arg("run").arg(f.path()).output().unwrap();
    assert!(out.status.success(), "{:?}", out);
    let stdout = String::from_utf8(out.stdout).unwrap();
    // 1 + 2*3 = 7
    assert!(stdout.contains("Int(7)"), "{stdout}");
}

#[test]
fn two_holes_fill_positions_left_to_right_not_reversed() {
    // Non-commutative operator so a wrong hole order would give a
    // different, observably wrong answer: 10 - 1 - 100 = -91.
    let f = write_temp(
        "\u{3bb}sub3(a: \u{27c1}, b: \u{27c1}, c: \u{27c1}): \u{27c1} \u{2192} a \u{2620}\u{fe0e} b \u{2620}\u{fe0e} c\n\
         partial \u{2254} sub3(\u{2022}, \u{2022}, 100)\n\
         result \u{2254} partial(10)(1)\n\u{2767}\n",
    );
    let out = bin().arg("run").arg(f.path()).output().unwrap();
    assert!(out.status.success(), "{:?}", out);
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("Int(-91)"), "{stdout}");
}

#[test]
fn two_holes_on_an_arity_three_function_build_a_genuine_pending_chain() {
    // The critical case: `f(•, •)` leaves ONE parameter (c) still
    // unsupplied via ordinary currying — proving this builds a real
    // curried-application chain, not "one hole = one lambda" applied
    // ad hoc. 10 - 1 - 1 = 8.
    let f = write_temp(
        "\u{3bb}sub3(a: \u{27c1}, b: \u{27c1}, c: \u{27c1}): \u{27c1} \u{2192} a \u{2620}\u{fe0e} b \u{2620}\u{fe0e} c\n\
         partial \u{2254} sub3(\u{2022}, \u{2022})\n\
         result \u{2254} partial(10)(1)(1)\n\u{2767}\n",
    );
    let out = bin().arg("run").arg(f.path()).output().unwrap();
    assert!(out.status.success(), "{:?}", out);
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("Int(8)"), "{stdout}");
}

#[test]
fn a_hole_inside_a_nested_call_belongs_to_that_call_not_the_outer_one() {
    // outer = f(•, g(3, •)) — f's own hole and g's own hole must
    // resolve completely independently. outer(5) = 5 + g(3, 10) =
    // 5 + 30 = 35.
    let f = write_temp(
        "\u{3bb}g(x: \u{27c1}, y: \u{27c1}): \u{27c1} \u{2192} x \u{2731} y\n\
         \u{3bb}f(a: \u{27c1}, cb: \u{27c1} \u{2192} \u{27c1}): \u{27c1} \u{2192} a \u{271a} cb(10)\n\
         outer \u{2254} f(\u{2022}, g(3, \u{2022}))\n\
         result \u{2254} outer(5)\n\u{2767}\n",
    );
    let out = bin().arg("run").arg(f.path()).output().unwrap();
    assert!(out.status.success(), "{:?}", out);
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("Int(35)"), "{stdout}");
}

#[test]
fn a_pipe_ref_and_a_hole_in_the_same_stage_is_a_static_error() {
    let f = write_temp(
        "\u{3bb}pair(x: \u{27c1}, y: \u{27c1}): \u{27c1} \u{2192} x \u{271a} y\n\
         xs \u{2254} 5\n\
         result \u{2254} xs \u{25b7} (pair(\u{25c8}, \u{2022}))\n\u{2767}\n",
    );
    let out = bin().arg("check").arg(f.path()).output().unwrap();
    assert_eq!(out.status.code(), Some(1), "{:?}", out);
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        stderr.contains('\u{25c8}') && stderr.contains('\u{2022}'),
        "{stderr}"
    );
}

#[test]
fn too_many_arguments_without_any_holes_is_unaffected_by_this_slice() {
    let f = write_temp(
        "\u{3bb}f(a: \u{27c1}, b: \u{27c1}): \u{27c1} \u{2192} a \u{271a} b\n\
         result \u{2254} f(1, 2, 3)\n\u{2767}\n",
    );
    let out = bin().arg("check").arg(f.path()).output().unwrap();
    assert_eq!(out.status.code(), Some(1), "{:?}", out);
}

#[test]
fn fmt_roundtrips_a_hole_argument() {
    let f = write_temp(
        "\u{3bb}f(a: \u{27c1}, b: \u{27c1}): \u{27c1} \u{2192} a \u{271a} b\n\
         partial \u{2254} f(1, \u{2022})\n\
         result \u{2254} partial(2)\n\u{2767}\n",
    );
    let out = bin().arg("fmt").arg(f.path()).output().unwrap();
    assert!(out.status.success(), "{:?}", out);
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains('\u{2022}'), "{stdout}");
}

#[test]
fn fmt_prints_canonically_formatted_source_and_reparses() {
    let f = write_temp("x\u{2254}1\u{271A}2\n\u{2767}\n");
    let out = bin().arg("fmt").arg(f.path()).output().unwrap();
    assert!(out.status.success(), "{:?}", out);
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("\u{2254}"), "{stdout}");
    assert!(stdout.trim_end().ends_with('\u{2767}'), "{stdout}");
}

#[test]
fn fmt_roundtrips_the_unit_literal() {
    let f = write_temp("x\u{2254}\u{2205}\n\u{2767}\n");
    let out = bin().arg("fmt").arg(f.path()).output().unwrap();
    assert!(out.status.success(), "{:?}", out);
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains('\u{2205}'), "{stdout}");
}

#[test]
fn fmt_does_not_require_type_checking_to_succeed() {
    // §13: formatting ill-typed code must still work.
    let f = write_temp("result \u{2254} 1 \u{271A} \u{25C9}\n\u{2767}\n");
    let out = bin().arg("fmt").arg(f.path()).output().unwrap();
    assert!(out.status.success(), "{:?}", out);
}

#[test]
fn inspect_tokens_prints_a_token_list() {
    let f = write_temp("x \u{2254} 5\n\u{2767}\n");
    let out = bin()
        .arg("inspect")
        .arg(f.path())
        .arg("--tokens")
        .output()
        .unwrap();
    assert!(out.status.success(), "{:?}", out);
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("Ident"), "{stdout}");
}

#[test]
fn inspect_ast_prints_the_surface_module() {
    let f = write_temp("x \u{2254} 5\n\u{2767}\n");
    let out = bin()
        .arg("inspect")
        .arg(f.path())
        .arg("--ast")
        .output()
        .unwrap();
    assert!(out.status.success(), "{:?}", out);
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("ValueDeclaration"), "{stdout}");
}

#[test]
fn inspect_core_prints_the_desugared_module() {
    let f = write_temp("x \u{2254} 5\n\u{2767}\n");
    let out = bin()
        .arg("inspect")
        .arg(f.path())
        .arg("--core")
        .output()
        .unwrap();
    assert!(out.status.success(), "{:?}", out);
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(
        stdout.contains("BindingGroup") || stdout.contains("Let"),
        "{stdout}"
    );
}

#[test]
fn inspect_defaults_to_core_when_no_flag_given() {
    let f = write_temp("x \u{2254} 5\n\u{2767}\n");
    let out = bin().arg("inspect").arg(f.path()).output().unwrap();
    assert!(out.status.success(), "{:?}", out);
}

#[test]
fn inspect_unknown_flag_exits_two() {
    let f = write_temp("x \u{2254} 5\n\u{2767}\n");
    let out = bin()
        .arg("inspect")
        .arg(f.path())
        .arg("--bogus")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn build_and_test_are_explicitly_deferred_not_silently_missing() {
    for cmd in ["build", "test"] {
        let out = bin().arg(cmd).output().unwrap();
        assert_eq!(out.status.code(), Some(2));
        let stderr = String::from_utf8(out.stderr).unwrap();
        assert!(stderr.contains("not yet supported"), "{stderr}");
    }
}

#[test]
fn repl_evaluates_lines_and_a_bad_line_does_not_corrupt_the_session() {
    let mut child = bin()
        .arg("repl")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    {
        let stdin = child.stdin.as_mut().unwrap();
        stdin
            .write_all("x \u{2254} 5\nbogus\ny \u{2254} x \u{271A} 1\n:quit\n".as_bytes())
            .unwrap();
    }
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success(), "{:?}", out);
    let stdout = String::from_utf8(out.stdout).unwrap();
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stdout.contains("Int(5)"), "stdout: {stdout}");
    assert!(stdout.contains("Int(6)"), "stdout: {stdout}");
    assert!(
        !stderr.is_empty(),
        "expected the bad line to report an error"
    );
}

#[test]
fn repl_exits_cleanly_on_eof_without_quit() {
    let mut child = bin()
        .arg("repl")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    {
        let stdin = child.stdin.as_mut().unwrap();
        stdin.write_all("x \u{2254} 1\n".as_bytes()).unwrap();
    }
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success(), "{:?}", out);
}

#[test]
fn repl_has_ambient_stdlib_and_io_but_not_filesystem_natives() {
    // Post-incremental-REPL contract (`ReplSession` replaced the old
    // whole-buffer-rerun design, which excluded this ambient environment
    // specifically because re-evaluating history would replay effects —
    // that hazard no longer applies, since each line evaluates exactly
    // once): `print`/`readLine` and the ambient stdlib (`List`/`Cons`/
    // `Nil`/`map`) now collide as already-bound prelude names, exactly
    // like `run`/`check`. `readFile`/`writeFile` deliberately still
    // don't — the REPL has no single entry file to scope a filesystem
    // capability against (`LANGUAGE_SPEC.md` §5's still-open artifact
    // model), so those two remain ordinary unbound identifiers here.
    let mut child = bin()
        .arg("repl")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    {
        let stdin = child.stdin.as_mut().unwrap();
        stdin
            .write_all(
                "print \u{2254} 1\nreadLine \u{2254} 2\nmap \u{2254} 3\nfilter \u{2254} 4\n\
                 readFile \u{2254} 5\nwriteFile \u{2254} 6\n\
                 result \u{2254} readFile \u{271A} writeFile\n\
                 :quit\n"
                    .as_bytes(),
            )
            .unwrap();
    }
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success(), "{:?}", out);
    let stdout = String::from_utf8(out.stdout).unwrap();
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stdout.contains("Int(11)"), "stdout: {stdout}");
    let collisions = stderr
        .matches("already bound by the ambient prelude")
        .count();
    assert_eq!(
        collisions, 4,
        "expected exactly print/readLine/map/Cons to collide, none of readFile/writeFile: {stderr}"
    );
}

#[test]
fn repl_evaluates_each_line_exactly_once_no_effect_replay() {
    // The actual bug the incremental rewrite fixes: under the old
    // whole-buffer-rerun design, a native's effect from an earlier line
    // fired again on every later submission. `print` is the simplest
    // observable proof — it must appear on stdout exactly once, not
    // once per line submitted afterward.
    let mut child = bin()
        .arg("repl")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    {
        let stdin = child.stdin.as_mut().unwrap();
        stdin
            .write_all(
                "a \u{2254} print(\u{22}once\u{22})\nx \u{2254} 1\ny \u{2254} 2\nz \u{2254} 3\n:quit\n"
                    .as_bytes(),
            )
            .unwrap();
    }
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success(), "{:?}", out);
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert_eq!(stdout.matches("once").count(), 1, "stdout: {stdout}");
}

#[test]
fn repl_shares_mutable_state_correctly_across_lines() {
    // A `Cell` committed by one line must remain the *same* cell for a
    // later line to mutate/read through — proving the incremental
    // design carries live values forward, not re-derived copies.
    let mut child = bin()
        .arg("repl")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    {
        let stdin = child.stdin.as_mut().unwrap();
        stdin
            .write_all(
                "c \u{2254}\u{02da} 1\nbumped \u{2254} c \u{2699}\u{fe0e} c \u{271A} 41\nresult \u{2254} c\n:quit\n"
                    .as_bytes(),
            )
            .unwrap();
    }
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success(), "{:?}", out);
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("42"), "stdout: {stdout}");
}

// ── Imports / modules (§20, ⟲ ModuleName) ───────────────────────────

#[test]
fn a_local_generic_function_reused_at_two_types_type_checks_with_the_ambient_stdlib_prelude() {
    // P0-B's original repro, through the real binary: `stdlib::load()`
    // seeds several of its own polymorphic schemes (`map`/`filter`/
    // `fold`/`Nil`/`Cons`) from a completely separate `Checker` before
    // this file is ever type-checked — a local generic function must
    // still generalize and be reused at two different types despite
    // that ambient prelude being present.
    let f = write_temp(
        "\u{3bb}id(x: t): t \u{2192} x\n\
         a \u{2254} id(5)\n\
         b \u{2254} id(\u{22}hi\u{22})\n\
         result \u{2254} b\n\u{2767}\n",
    );
    let out = bin().arg("run").arg(f.path()).output().unwrap();
    assert!(out.status.success(), "{:?}", out);
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("Str(\"hi\")"), "{stdout}");
}

#[test]
fn an_unused_local_generic_function_checks_cleanly_with_the_ambient_stdlib_prelude() {
    let f = write_temp("\u{3bb}id(x: t): t \u{2192} x\nresult \u{2254} 5\n\u{2767}\n");
    let out = bin().arg("check").arg(f.path()).output().unwrap();
    assert!(out.status.success(), "{:?}", out);
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("ok"), "{stdout}");
}

#[test]
fn a_local_generic_function_still_generalizes_alongside_an_imported_generic_value() {
    // P0-B regression, real end-to-end: an imported module's own
    // generic value and the importer's own, unrelated generic function
    // are typechecked by two genuinely different `Checker` instances
    // (`modules::resolve_module_uncached` recursively calls
    // `check_with_prelude` per imported file) — the importer's local
    // `id2` must still generalize correctly regardless of whatever
    // `TypeVarId` numbering the imported module's own checker happened
    // to use for its own generic value.
    let dir = TempObkDir::new();
    dir.write(
        "Lib",
        "idLib \u{2254}\u{27F3} \u{3bb}(x) \u{2192} x\n\u{2767}\n",
    );
    let main = dir.write(
        "main",
        "\u{27F2}Lib\n\
         \u{3bb}id2(x: t): t \u{2192} x\n\
         a \u{2254} id2(5)\n\
         b \u{2254} id2(\u{22}hi\u{22})\n\
         result \u{2254} b\n\u{2767}\n",
    );
    let out = bin().arg("run").arg(&main).output().unwrap();
    assert!(out.status.success(), "{:?}", out);
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("Str(\"hi\")"), "{stdout}");
}

#[test]
fn run_resolves_a_whole_module_import_and_hides_private_helpers() {
    let dir = TempObkDir::new();
    dir.write(
        "mathutil",
        "helper \u{2254} \u{3bb}(n) \u{2192} 1 \u{271A} n\n\
         publicAdd \u{2254}\u{27F3} \u{3bb}(n) \u{2192} helper(n)\n\u{2767}\n",
    );
    let main = dir.write(
        "main",
        "\u{27F2}mathutil\nresult \u{2254} publicAdd(5)\n\u{2767}\n",
    );
    let out = bin().arg("run").arg(&main).output().unwrap();
    assert!(out.status.success(), "{:?}", out);
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("Int(6)"), "{stdout}");
}

#[test]
fn importing_a_module_does_not_expose_its_private_bindings() {
    let dir = TempObkDir::new();
    dir.write(
        "mathutil",
        "helper \u{2254} \u{3bb}(n) \u{2192} 1 \u{271A} n\n\
         publicAdd \u{2254}\u{27F3} \u{3bb}(n) \u{2192} helper(n)\n\u{2767}\n",
    );
    let main = dir.write(
        "main",
        "\u{27F2}mathutil\nresult \u{2254} helper(5)\n\u{2767}\n",
    );
    let out = bin().arg("check").arg(&main).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.contains("unknown variable 'helper'"), "{stderr}");
}

#[test]
fn a_local_binding_reusing_an_imported_name_is_a_static_error() {
    let dir = TempObkDir::new();
    dir.write("mathutil", "publicAdd \u{2254}\u{27F3} 5\n\u{2767}\n");
    let main = dir.write("main", "\u{27F2}mathutil\npublicAdd \u{2254} 1\n\u{2767}\n");
    let out = bin().arg("check").arg(&main).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        stderr.contains("already bound by the ambient prelude"),
        "{stderr}"
    );
}

#[test]
fn two_imports_exporting_the_same_name_is_a_static_error() {
    let dir = TempObkDir::new();
    dir.write("m1", "shared \u{2254}\u{27F3} 1\n\u{2767}\n");
    dir.write("m2", "shared \u{2254}\u{27F3} 2\n\u{2767}\n");
    let main = dir.write(
        "main",
        "\u{27F2}m1\n\u{27F2}m2\nresult \u{2254} shared\n\u{2767}\n",
    );
    let out = bin().arg("check").arg(&main).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        stderr.contains("exported by more than one imported module"),
        "{stderr}"
    );
}

#[test]
fn a_circular_import_is_a_static_error_not_a_hang() {
    let dir = TempObkDir::new();
    let a = dir.write("a", "\u{27F2}b\nx \u{2254} 1\n\u{2767}\n");
    dir.write("b", "\u{27F2}a\ny \u{2254} 2\n\u{2767}\n");
    let out = bin().arg("check").arg(&a).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.contains("circular import"), "{stderr}");
}

#[test]
fn importing_a_nonexistent_module_is_a_clear_error_not_a_panic() {
    let dir = TempObkDir::new();
    let main = dir.write("main", "\u{27F2}doesnotexist\nx \u{2254} 1\n\u{2767}\n");
    let out = bin().arg("check").arg(&main).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        stderr.contains("could not read imported module"),
        "{stderr}"
    );
}

// ── Cross-module ADT pattern matching (real bug found during 1.0
//    product-closure review, not a policy question): `Pattern::
//    Constructor`/`Expr::Constructor` both resolve a tag through the
//    `Checker`'s own `AdtRegistry`, previously built only from the
//    *current* file's own `TypeDeclaration`s — an imported (or ambient
//    stdlib) constructor could always be *called* (an ordinary `Apply`
//    against an already-resolved `Scheme`) but never *matched against*.
//    Fixed by also merging imported/stdlib `TypeDeclaration`s into the
//    consuming module's own registry
//    (`obfusku_typecheck::check_with_prelude_and_types`). ─────────────

#[test]
fn pattern_matching_against_a_locally_declared_adt_is_unaffected() {
    // Baseline: the case that always worked, confirming the fix didn't
    // disturb it.
    let src = "Shape \u{2254} { Circle(\u{27C1}) \u{27E2} Square(\u{27C1}) }\n\
               result \u{2254} \u{27E1} Circle(2) {\n \
               Circle(r) \u{2192} r\n \u{27E2} Square(s) \u{2192} s\n}\n\u{2767}\n";
    let out = bin()
        .arg("run")
        .arg(write_temp(src).path())
        .output()
        .unwrap();
    assert!(out.status.success(), "{:?}", out);
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("Int(2)"), "{stdout}");
}

#[test]
fn an_imported_constructor_can_still_be_constructed_as_a_plain_value() {
    let dir = TempObkDir::new();
    dir.write(
        "shapes",
        "Shape \u{2254} { Circle(\u{27C1}) \u{27E2} Square(\u{27C1}) }\n\u{2767}\n",
    );
    let main = dir.write(
        "main",
        "\u{27F2}shapes\nresult \u{2254} Circle(5)\n\u{2767}\n",
    );
    let out = bin().arg("run").arg(&main).output().unwrap();
    assert!(out.status.success(), "{:?}", out);
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("tag: \"Circle\""), "{stdout}");
}

#[test]
fn pattern_matching_against_an_imported_adts_constructors_works() {
    // The exact bug: this used to fail with "unknown constructor
    // 'Circle' in pattern" even though `Circle` imports and constructs
    // fine — matching against it needs the ADT's *type* metadata, not
    // just the constructor function's `Scheme`.
    let dir = TempObkDir::new();
    dir.write(
        "shapes",
        "Shape \u{2254} { Circle(\u{27C1}) \u{27E2} Square(\u{27C1}) }\n\u{2767}\n",
    );
    let main = dir.write(
        "main",
        "\u{27F2}shapes\n\
         result \u{2254} \u{27E1} Circle(7) {\n \
         Circle(r) \u{2192} r\n \u{27E2} Square(s) \u{2192} s\n}\n\u{2767}\n",
    );
    let out = bin().arg("run").arg(&main).output().unwrap();
    assert!(out.status.success(), "{:?}", out);
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("Int(7)"), "{stdout}");
}

#[test]
fn exhaustiveness_checking_works_against_an_imported_adt() {
    // A missing arm for an *imported* ADT's variant must be caught,
    // proving exhaustiveness (not just single-tag resolution) has the
    // imported ADT's full variant set, not only whichever tag the
    // pattern happened to name.
    let dir = TempObkDir::new();
    dir.write(
        "shapes",
        "Shape \u{2254} { Circle(\u{27C1}) \u{27E2} Square(\u{27C1}) }\n\u{2767}\n",
    );
    let main = dir.write(
        "main",
        "\u{27F2}shapes\n\
         result \u{2254} \u{27E1} Circle(7) {\n \
         Circle(r) \u{2192} r\n}\n\u{2767}\n",
    );
    let out = bin().arg("check").arg(&main).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.contains("non-exhaustive"), "{stderr}");
    assert!(stderr.contains("Square"), "{stderr}");
}

#[test]
fn an_imported_generic_adt_pattern_matches_correctly_at_two_different_instantiations() {
    // Mirrors P0-B's own concern one level down: a generic ADT's
    // `VariantInfo.type_params` are bare `String` names
    // (`CoreType::Param`), never a `Checker`-relative `TypeVarId`, so
    // merging registry data built by the exporting module's own
    // `Checker` carries none of `Scheme`'s cross-`Checker` identity
    // hazard — confirmed empirically here by reusing the same imported
    // generic ADT at `Int` and at `Str` in the *same* importing module.
    let dir = TempObkDir::new();
    dir.write(
        "boxes",
        "Box t \u{2254} { Empty \u{27E2} Full(t) }\n\u{2767}\n",
    );
    let main = dir.write(
        "main",
        "\u{27F2}boxes\n\
         unwrapOr \u{2254} \u{3bb}(b, default) \u{2192} \u{27E1} b {\n \
         Empty \u{2192} default\n \u{27E2} Full(x) \u{2192} x\n}\n\
         a \u{2254} unwrapOr(Full(1), 0)\n\
         b \u{2254} unwrapOr(Full(\u{22}hi\u{22}), \u{22}\u{22})\n\
         result \u{2254} b\n\u{2767}\n",
    );
    let out = bin().arg("run").arg(&main).output().unwrap();
    assert!(out.status.success(), "{:?}", out);
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("Str(\"hi\")"), "{stdout}");
}

#[test]
fn stdlib_list_pattern_matching_works_without_any_local_declaration() {
    // The exact case named as mandatory: `Nil`/`Cons` come from the
    // *ambient* stdlib prelude (no `⟲`, no local `List` declaration at
    // all) — previously this failed with "unknown constructor 'Nil' in
    // pattern" despite `List<T>` being recorded as "Implemented,
    // verified end-to-end" in ROADMAP.md. That verification, in
    // hindsight, only ever exercised a *locally re-declared* `List`
    // (see `crates/obfusku-cli/tests/e2e.rs`'s own `list_adt_end_to_end_
    // via_explicit_type_application`), never the real ambient path a
    // user program actually takes — this is the test that closes that
    // coverage gap.
    let src = "xs \u{2254} Cons(1, Cons(2, Nil))\n\
               result \u{2254} \u{27E1} xs {\n \
               Nil \u{2192} 0\n \u{27E2} Cons(h, t) \u{2192} h\n}\n\u{2767}\n";
    let out = bin()
        .arg("run")
        .arg(write_temp(src).path())
        .output()
        .unwrap();
    assert!(out.status.success(), "{:?}", out);
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("Int(1)"), "{stdout}");
}

#[test]
fn a_local_type_redeclaring_an_imported_types_name_is_a_static_error() {
    let dir = TempObkDir::new();
    dir.write("shapes", "Shape \u{2254} { Circle(\u{27C1}) }\n\u{2767}\n");
    let main = dir.write(
        "main",
        "\u{27F2}shapes\nShape \u{2254} { Square(\u{27C1}) }\n\u{2767}\n",
    );
    let out = bin().arg("check").arg(&main).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        stderr.contains("'Shape' is declared by more than one type"),
        "{stderr}"
    );
}

#[test]
fn two_imports_each_declaring_their_own_unrelated_adt_do_not_collide() {
    let dir = TempObkDir::new();
    dir.write("shapes", "Shape \u{2254} { Circle(\u{27C1}) }\n\u{2767}\n");
    dir.write(
        "colors",
        "Color \u{2254} { Red \u{27E2} Green \u{27E2} Blue }\n\u{2767}\n",
    );
    let main = dir.write(
        "main",
        "\u{27F2}shapes\n\u{27F2}colors\n\
         c \u{2254} Green\n\
         result \u{2254} \u{27E1} c {\n \
         Red \u{2192} \u{22}r\u{22}\n \u{27E2} Green \u{2192} \u{22}g\u{22}\n \u{27E2} Blue \u{2192} \u{22}b\u{22}\n}\n\u{2767}\n",
    );
    let out = bin().arg("run").arg(&main).output().unwrap();
    assert!(out.status.success(), "{:?}", out);
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("Str(\"g\")"), "{stdout}");
}

#[test]
fn record_shaped_adt_self_tag_does_not_falsely_collide_with_itself() {
    // Regression for a bug introduced and caught while building this
    // very matrix: a record/tuple `TypeDeclaration` deliberately
    // self-tags (its one synthesized variant's tag equals its own
    // type's tag) — the collision check must treat ADT names and
    // constructor tags as separate namespaces, or every record/tuple
    // type would spuriously collide with itself.
    let src = "Point \u{2254} { x: \u{27C1} \u{27E2} y: \u{27C1} }\n\
               p \u{2254} Point { x: 1 \u{27E2} y: 2 }\n\
               result \u{2254} \u{27E1} p {\n \
               Point { x: a \u{27E2} y: b } \u{2192} a \u{271A} b\n}\n\u{2767}\n";
    let out = bin()
        .arg("run")
        .arg(write_temp(src).path())
        .output()
        .unwrap();
    assert!(out.status.success(), "{:?}", out);
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("Int(3)"), "{stdout}");
}

#[test]
fn an_exported_self_recursive_function_is_importable_and_callable() {
    // P1-2b: the case FunctionDeclaration export exists to make
    // possible at all — a self-recursive function (only expressible via
    // FunctionDeclaration/LetRec, never ValueDeclaration) exported from
    // one module, imported and called from another, correct result.
    let dir = TempObkDir::new();
    dir.write(
        "mathutil",
        "λfactorial(n: ⟁): ⟁ →⟳\n\
         \u{27E1} n {\n\
         0 \u{2192} 1\n\
         \u{27E2} _ \u{2192} n \u{2731} factorial(n \u{2620}\u{FE0E} 1)\n\
         }\n\u{2767}\n",
    );
    let main = dir.write(
        "main",
        "\u{27F2}mathutil\nresult \u{2254} factorial(5)\n\u{2767}\n",
    );
    let out = bin().arg("run").arg(&main).output().unwrap();
    assert!(out.status.success(), "{:?}", out);
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("Int(120)"), "{stdout}");
}

#[test]
fn an_exported_generic_function_is_importable_and_reusable_at_two_types() {
    // P1-2b, second half: proves the FunctionDeclaration -> LetRec ->
    // Scheme -> module Prelude -> instantiate path (hardened by the
    // P0-B cross-Checker TypeVarId fix) handles a genuinely exported
    // generic function correctly, not just a monomorphic one.
    let dir = TempObkDir::new();
    dir.write("identity", "λid(x: t): t \u{2192}\u{27F3} x\n\u{2767}\n");
    let main = dir.write(
        "main",
        "\u{27F2}identity\na \u{2254} id(5)\nb \u{2254} id(\"hi\")\nresult \u{2254} b\n\u{2767}\n",
    );
    let out = bin().arg("run").arg(&main).output().unwrap();
    assert!(out.status.success(), "{:?}", out);
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("Str(\"hi\")"), "{stdout}");
}

#[test]
fn export_immediately_after_lambda_stays_invalid_for_function_declarations() {
    // The superseded candidate '\u{03bb}\u{27f3}f(...)' stays rejected —
    // '\u{27f3}' after the return-type arrow is the only accepted form.
    let src = "\u{03bb}\u{27F3}f(x: \u{27C1}): \u{27C1} \u{2192} x\n\u{2767}\n";
    let out = bin()
        .arg("check")
        .arg(write_temp(src).path())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.contains("function name"), "{stderr}");
}

#[test]
fn value_declaration_annotation_rejects_a_genuinely_incompatible_value() {
    // P1-3a's critical regression: the annotation must be a real
    // constraint, not documentation the checker silently ignores.
    let src = "x: \u{27C1} \u{2254} \"hi\"\n\u{2767}\n";
    let out = bin()
        .arg("check")
        .arg(write_temp(src).path())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn value_declaration_annotation_does_not_disturb_valid_inference() {
    let src = "x: \u{27C1} \u{2254} 5\nresult \u{2254} x \u{271A} 1\n\u{2767}\n";
    let out = bin()
        .arg("run")
        .arg(write_temp(src).path())
        .output()
        .unwrap();
    assert!(out.status.success(), "{:?}", out);
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("Int(6)"), "{stdout}");
}

#[test]
fn value_declaration_annotation_composes_with_mutability_and_export_across_a_module_boundary() {
    // Annotation + Mutability + Export together, exercised through a
    // real import: the element type is what's annotated, not Cell<T>
    // (desugar wraps the annotation to match the MutCell-wrapped value).
    let dir = TempObkDir::new();
    dir.write(
        "counter",
        "x: \u{27C1} \u{2254}\u{02da}\u{27F3} 0\n\u{2767}\n",
    );
    let main = dir.write("main", "\u{27F2}counter\nresult \u{2254} 1\n\u{2767}\n");
    let out = bin().arg("check").arg(&main).output().unwrap();
    assert!(out.status.success(), "{:?}", out);
}

#[test]
fn type_annotation_on_a_local_binding_is_a_clear_static_error_not_a_confusing_parse_failure() {
    let src = "result \u{2254}\n y: \u{27C1} \u{2254} 5\n y\n\u{2767}\n";
    let out = bin()
        .arg("check")
        .arg(write_temp(src).path())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.contains("not valid on a local binding"), "{stderr}");
}

#[test]
fn lambda_parameter_annotation_rejects_a_genuinely_incompatible_body() {
    let src = "f \u{2254} \u{3bb}(x: \u{27C1}) \u{2192} x \u{271A} \"x\"\nresult \u{2254} f(5)\n\u{2767}\n";
    let out = bin()
        .arg("check")
        .arg(write_temp(src).path())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn lambda_parameter_annotation_does_not_disturb_valid_inference() {
    let src =
        "f \u{2254} \u{3bb}(x: \u{27C1}) \u{2192} x \u{271A} 1\nresult \u{2254} f(5)\n\u{2767}\n";
    let out = bin()
        .arg("run")
        .arg(write_temp(src).path())
        .output()
        .unwrap();
    assert!(out.status.success(), "{:?}", out);
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("Int(6)"), "{stdout}");
}

#[test]
fn unannotated_lambda_parameter_still_generalizes_across_two_types() {
    let src = "id \u{2254} \u{3bb}(x) \u{2192} x\na \u{2254} id(5)\nb \u{2254} id(\"hi\")\nresult \u{2254} b\n\u{2767}\n";
    let out = bin()
        .arg("run")
        .arg(write_temp(src).path())
        .output()
        .unwrap();
    assert!(out.status.success(), "{:?}", out);
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("Str(\"hi\")"), "{stdout}");
}

#[test]
fn shared_type_variable_across_one_parameter_list_forces_both_arguments_equal() {
    // The critical case from the P0 matrix: it's not enough to show
    // `f ≔ λ(x: t, y: t) → x` parses — an application that respects
    // "x and y share a type" must succeed, and one that violates it
    // must fail, or the shared-fresh-var mechanism could be a no-op.
    let src = "f \u{2254} \u{3bb}(x: t, y: t) \u{2192} x\nresult \u{2254} f(1, 2)\n\u{2767}\n";
    let out = bin()
        .arg("run")
        .arg(write_temp(src).path())
        .output()
        .unwrap();
    assert!(out.status.success(), "{:?}", out);
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("Int(1)"), "{stdout}");

    let bad_src =
        "f \u{2254} \u{3bb}(x: t, y: t) \u{2192} x\nresult \u{2254} f(1, \"hi\")\n\u{2767}\n";
    let bad_out = bin()
        .arg("check")
        .arg(write_temp(bad_src).path())
        .output()
        .unwrap();
    assert_eq!(bad_out.status.code(), Some(1));
}

#[test]
fn two_independent_top_level_lambdas_reusing_a_type_variable_spelling_never_unify() {
    // Post-fix audit case: `lambda_param_vars` is a checker-lifetime
    // cache keyed by the *renamed* name, not the original spelling —
    // this is the test that would fail if desugar's per-parameter-list
    // renaming were ever bypassed or reused across unrelated top-level
    // bindings. `f` and `g` are two separate top-level `ValueDeclaration`s
    // (not nested lambdas), each independently spelling its parameter's
    // annotation `t` — applying them at genuinely incompatible types in
    // the same module/Checker must succeed, or `t` is leaking identity
    // across signatures that share nothing but a spelling.
    let src = "f \u{2254} \u{3bb}(x: t) \u{2192} x\ng \u{2254} \u{3bb}(y: t) \u{2192} y\na \u{2254} f(5)\nresult \u{2254} g(\"hi\")\n\u{2767}\n";
    let out = bin()
        .arg("run")
        .arg(write_temp(src).path())
        .output()
        .unwrap();
    assert!(out.status.success(), "{:?}", out);
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("Str(\"hi\")"), "{stdout}");
}

#[test]
fn same_spelled_type_variable_in_two_separately_written_lambdas_stays_independent() {
    // `λ(x: t) → λ(y: t) → …` — two SEPARATE Lambda literals, not one
    // parameter list; despite reusing the spelling `t`, x and y must be
    // independently typeable.
    let src = "f \u{2254} \u{3bb}(x: t) \u{2192} \u{3bb}(y: t) \u{2192} x\ninner \u{2254} f(1)\nresult \u{2254} inner(\"hi\")\n\u{2767}\n";
    let out = bin()
        .arg("run")
        .arg(write_temp(src).path())
        .output()
        .unwrap();
    assert!(out.status.success(), "{:?}", out);
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("Int(1)"), "{stdout}");
}

#[test]
fn lambda_parameter_annotation_composes_with_export_and_import() {
    let dir = TempObkDir::new();
    dir.write(
        "util",
        "makeAdder \u{2254}\u{27F3} \u{3bb}(n: \u{27C1}) \u{2192} \u{3bb}(x: \u{27C1}) \u{2192} x \u{271A} n\n\u{2767}\n",
    );
    let main = dir.write(
        "main",
        "\u{27F2}util\nadd5 \u{2254} makeAdder(5)\nresult \u{2254} add5(10)\n\u{2767}\n",
    );
    let out = bin().arg("run").arg(&main).output().unwrap();
    assert!(out.status.success(), "{:?}", out);
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("Int(15)"), "{stdout}");
}

#[test]
fn fmt_roundtrips_a_lambda_parameter_annotation() {
    let src = "result \u{2254} \u{3bb}(x: \u{27C1}, y: t) \u{2192} x\n\u{2767}\n";
    let f = write_temp(src);
    let out = bin().arg("fmt").arg(f.path()).output().unwrap();
    assert!(out.status.success(), "{:?}", out);
    let formatted = String::from_utf8(out.stdout).unwrap();
    assert!(formatted.contains("x: \u{27C1}"), "{formatted}");
    assert!(formatted.contains("y: t"), "{formatted}");
}

#[test]
fn non_nfc_source_file_is_a_clean_lexical_error_not_a_silent_pass() {
    // A real file on disk with an NFD-decomposed "é" in a string literal
    // — CONCRETE_SYMBOLIC_GRAMMAR.md §2 requires this rejected as a
    // lexical error, not silently renormalized.
    let src = "x \u{2254} \"caf\u{65}\u{301}\"\nresult \u{2254} x\n\u{2767}\n";
    let out = bin()
        .arg("check")
        .arg(write_temp(src).path())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.contains("Normalization Form C"), "{stderr}");
}

#[test]
fn an_integer_directly_followed_by_an_exponent_suffix_is_rejected_end_to_end() {
    // CONCRETE_SYMBOLIC_GRAMMAR.md §5: `RealLiteral`'s exponent is only
    // reachable after the mandatory fractional part; '5e10' is not a
    // valid literal of either kind and must not silently become a Real
    // through a real program's actual pipeline.
    let src = "x \u{2254} 5e10\nresult \u{2254} x\n\u{2767}\n";
    let out = bin()
        .arg("check")
        .arg(write_temp(src).path())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn a_dotted_real_literal_with_an_exponent_is_unaffected() {
    let src = "result \u{2254} 5.0e10\n\u{2767}\n";
    let out = bin()
        .arg("run")
        .arg(write_temp(src).path())
        .output()
        .unwrap();
    assert!(out.status.success(), "{:?}", out);
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("Real(50000000000.0)"), "{stdout}");
}

// ── Stdlib (`List<T>`, `map`/`filter`/`fold`) as an implicit prelude —
//    ambiently available with no `⟲`, ordinary `.obk` source (see
//    `obfusku_cli::stdlib`), zero new Core surface. ──────────────────

#[test]
fn map_is_ambiently_available_and_recurses_over_a_multi_element_list() {
    let f = write_temp(
        "\u{3bb}addOne(n: \u{27c1}): \u{27c1} \u{2192} 1 \u{271a} n\n\
         \u{3bb}addInts(acc: \u{27c1}, n: \u{27c1}): \u{27c1} \u{2192} acc \u{271a} n\n\
         xs \u{2254} Cons(1, Cons(2, Cons(3, Nil)))\n\
         mapped \u{2254} map(addOne, xs)\n\
         result \u{2254} fold(addInts, 0, mapped)\n\u{2767}\n",
    );
    let out = bin().arg("run").arg(f.path()).output().unwrap();
    assert!(out.status.success(), "{:?}", out);
    let stdout = String::from_utf8(out.stdout).unwrap();
    // (1+1) + (2+1) + (3+1) = 9
    assert!(stdout.contains("Int(9)"), "{stdout}");
}

#[test]
fn filter_is_ambiently_available_and_keeps_only_matching_elements() {
    let f = write_temp(
        "\u{3bb}isEven(n: \u{27c1}): \u{25cb} \u{2192} \u{27e1} n \u{2317} 2 {\n  \
         0 \u{2192} \u{25c9}\n  \u{27e2} _ \u{2192} \u{25ce}\n}\n\
         \u{3bb}addInts(acc: \u{27c1}, n: \u{27c1}): \u{27c1} \u{2192} acc \u{271a} n\n\
         xs \u{2254} Cons(1, Cons(2, Cons(3, Cons(4, Nil))))\n\
         filtered \u{2254} filter(isEven, xs)\n\
         result \u{2254} fold(addInts, 0, filtered)\n\u{2767}\n",
    );
    let out = bin().arg("run").arg(f.path()).output().unwrap();
    assert!(out.status.success(), "{:?}", out);
    let stdout = String::from_utf8(out.stdout).unwrap();
    // 2 + 4 = 6
    assert!(stdout.contains("Int(6)"), "{stdout}");
}

#[test]
fn fold_over_nil_returns_the_seed_unchanged() {
    let f = write_temp(
        "\u{3bb}addInts(acc: \u{27c1}, n: \u{27c1}): \u{27c1} \u{2192} acc \u{271a} n\n\
         result \u{2254} fold(addInts, 0, Nil)\n\u{2767}\n",
    );
    let out = bin().arg("run").arg(f.path()).output().unwrap();
    assert!(out.status.success(), "{:?}", out);
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("Int(0)"), "{stdout}");
}

#[test]
fn filter_over_nil_is_nil_and_folds_to_the_seed() {
    let f = write_temp(
        "\u{3bb}isEven(n: \u{27c1}): \u{25cb} \u{2192} \u{27e1} n \u{2317} 2 {\n  \
         0 \u{2192} \u{25c9}\n  \u{27e2} _ \u{2192} \u{25ce}\n}\n\
         \u{3bb}addInts(acc: \u{27c1}, n: \u{27c1}): \u{27c1} \u{2192} acc \u{271a} n\n\
         result \u{2254} fold(addInts, 0, filter(isEven, Nil))\n\u{2767}\n",
    );
    let out = bin().arg("run").arg(f.path()).output().unwrap();
    assert!(out.status.success(), "{:?}", out);
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("Int(0)"), "{stdout}");
}

#[test]
fn fold_over_a_singleton_list_applies_the_combining_function_exactly_once() {
    let f = write_temp(
        "\u{3bb}addInts(acc: \u{27c1}, n: \u{27c1}): \u{27c1} \u{2192} acc \u{271a} n\n\
         result \u{2254} fold(addInts, 100, Cons(7, Nil))\n\u{2767}\n",
    );
    let out = bin().arg("run").arg(f.path()).output().unwrap();
    assert!(out.status.success(), "{:?}", out);
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("Int(107)"), "{stdout}");
}

#[test]
fn fold_accumulates_strictly_left_to_right() {
    // fold(f, "seed-", [a, b]) == f(f("seed-", a), b) == "seed-ab" —
    // a right-to-left (or reordered) fold would produce a different
    // string, so this pins down the accumulator's evaluation order.
    let f = write_temp(
        "\u{3bb}concatStr(acc: \u{2318}, s: \u{2318}): \u{2318} \u{2192} acc \u{271a} s\n\
         result \u{2254} fold(concatStr, \u{22}seed-\u{22}, Cons(\u{22}a\u{22}, Cons(\u{22}b\u{22}, Nil)))\n\u{2767}\n",
    );
    let out = bin().arg("run").arg(f.path()).output().unwrap();
    assert!(out.status.success(), "{:?}", out);
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("seed-ab"), "{stdout}");
}

#[test]
fn map_is_genuinely_polymorphic_reusable_at_int_and_str_in_the_same_module() {
    // `map`/`addInts`/`concatStr` are each used once at `Int` and once
    // at `Str`, within one module — proof the stdlib's `map` really is
    // `∀t,u. (t → u) → List<t> → List<u>`, not implicitly pinned to
    // whichever element type happened to type-check it first.
    let f = write_temp(
        "\u{3bb}addOne(n: \u{27c1}): \u{27c1} \u{2192} 1 \u{271a} n\n\
         \u{3bb}idStr(s: \u{2318}): \u{2318} \u{2192} s\n\
         \u{3bb}addInts(acc: \u{27c1}, n: \u{27c1}): \u{27c1} \u{2192} acc \u{271a} n\n\
         \u{3bb}concatStr(acc: \u{2318}, s: \u{2318}): \u{2318} \u{2192} acc \u{271a} s\n\
         xsInt \u{2254} Cons(1, Cons(2, Nil))\n\
         xsStr \u{2254} Cons(\u{22}a\u{22}, Cons(\u{22}b\u{22}, Nil))\n\
         mappedInt \u{2254} map(addOne, xsInt)\n\
         mappedStr \u{2254} map(idStr, xsStr)\n\
         sumInt \u{2254} fold(addInts, 0, mappedInt)\n\
         concatResult \u{2254} fold(concatStr, \u{22}\u{22}, mappedStr)\n\
         result \u{2254} sumInt\n\u{2767}\n",
    );
    let out = bin().arg("check").arg(f.path()).output().unwrap();
    assert!(out.status.success(), "{:?}", out);
    let out = bin().arg("run").arg(f.path()).output().unwrap();
    assert!(out.status.success(), "{:?}", out);
    let stdout = String::from_utf8(out.stdout).unwrap();
    // (1+1) + (2+1) = 5
    assert!(stdout.contains("Int(5)"), "{stdout}");
}

#[test]
fn a_local_binding_reusing_a_stdlib_name_is_a_static_error() {
    let f = write_temp("map \u{2254} 5\nresult \u{2254} map\n\u{2767}\n");
    let out = bin().arg("check").arg(f.path()).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        stderr.contains("already bound by the ambient prelude"),
        "{stderr}"
    );
}

// ── I/O: `readFile`/`writeFile`, scoped to the entry file's own
//    directory (see `obfusku_cli::natives`'s own doc comment). ────────

#[test]
fn write_then_read_a_file_within_the_entry_directory() {
    let dir = TempObkDir::new();
    let main = dir.write(
        "main",
        "unused \u{2254} writeFile(\u{22}data.txt\u{22})(\u{22}hello from obfusku\u{22})\n\
         result \u{2254} readFile(\u{22}data.txt\u{22})\n\u{2767}\n",
    );
    let out = bin().arg("run").arg(&main).output().unwrap();
    assert!(out.status.success(), "{:?}", out);
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("hello from obfusku"), "{stdout}");
    assert_eq!(
        std::fs::read_to_string(dir.0.join("data.txt")).unwrap(),
        "hello from obfusku"
    );
}

#[test]
fn read_file_with_a_parent_traversal_path_is_permission_denied() {
    let dir = TempObkDir::new();
    let main = dir.write(
        "main",
        "result \u{2254} readFile(\u{22}../secret.txt\u{22})\n\u{2767}\n",
    );
    let out = bin().arg("run").arg(&main).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.contains("PermissionDenied"), "{stderr}");
}

#[test]
fn read_file_with_an_absolute_path_is_permission_denied() {
    let dir = TempObkDir::new();
    let main = dir.write(
        "main",
        "result \u{2254} readFile(\u{22}/etc/passwd\u{22})\n\u{2767}\n",
    );
    let out = bin().arg("run").arg(&main).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.contains("PermissionDenied"), "{stderr}");
}

#[test]
fn a_denied_filesystem_access_is_catchable_by_the_existing_mechanism() {
    let dir = TempObkDir::new();
    let main = dir.write(
        "main",
        "result \u{2254} \u{260a} (readFile(\u{22}/etc/passwd\u{22})) \u{3bb}(e) \u{2192} \u{27e1} e {\n  \
         Failure(tag, msg) \u{2192} tag\n  \u{27e2} DivisionByZero \u{2192} \u{22}div0\u{22}\n  \
         \u{27e2} NonExhaustiveMatch \u{2192} \u{22}nem\u{22}\n  \
         \u{27e2} InvalidOperation(m) \u{2192} m\n  \u{27e2} IntegerOverflow \u{2192} \u{22}overflow\u{22}\n}\n\u{2767}\n",
    );
    let out = bin().arg("run").arg(&main).output().unwrap();
    assert!(out.status.success(), "{:?}", out);
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("PermissionDenied"), "{stdout}");
}

#[test]
fn read_file_in_a_subdirectory_of_the_entry_directory_is_permitted() {
    let dir = TempObkDir::new();
    std::fs::create_dir_all(dir.0.join("sub")).unwrap();
    std::fs::write(dir.0.join("sub").join("nested.txt"), "nested content").unwrap();
    let main = dir.write(
        "main",
        "result \u{2254} readFile(\u{22}sub/nested.txt\u{22})\n\u{2767}\n",
    );
    let out = bin().arg("run").arg(&main).output().unwrap();
    assert!(out.status.success(), "{:?}", out);
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("nested content"), "{stdout}");
}

#[test]
fn read_file_that_does_not_exist_is_an_io_error_not_a_crash() {
    let dir = TempObkDir::new();
    let main = dir.write(
        "main",
        "result \u{2254} readFile(\u{22}does-not-exist.txt\u{22})\n\u{2767}\n",
    );
    let out = bin().arg("run").arg(&main).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.contains("IOError"), "{stderr}");
}

#[test]
#[cfg(unix)]
fn a_real_os_permission_error_reads_as_permission_denied_end_to_end() {
    // Distinct from the lexical-scoping denials above (a path that never
    // reaches the filesystem at all): a real file *inside* the allowed
    // root, with its own read permission revoked via `chmod`, so the
    // failure genuinely originates from the OS. Environment-dependent:
    // skipped, not falsely asserted, when running as root (which ignores
    // Unix permission bits) — a skip here means no evidence was
    // collected in that environment, not that the behavior was verified.
    use std::os::unix::fs::PermissionsExt;

    let dir = TempObkDir::new();
    let target = dir.0.join("no-read.txt");
    std::fs::write(&target, "secret").unwrap();
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o000)).unwrap();
    if std::fs::read_to_string(&target).is_ok() {
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o644)).unwrap();
        eprintln!(
            "SKIPPED: a_real_os_permission_error_reads_as_permission_denied_end_to_end \
             requires non-root execution — running as root ignores Unix permission bits, \
             so this environment cannot produce the permission boundary this test exists \
             to exercise. This is not positive evidence that permission-error \
             classification is correct here; it means no such evidence was collected."
        );
        return;
    }

    let main = dir.write(
        "main",
        "result \u{2254} readFile(\u{22}no-read.txt\u{22})\n\u{2767}\n",
    );
    let out = bin().arg("run").arg(&main).output().unwrap();
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.contains("PermissionDenied"), "{stderr}");
}

#[test]
fn a_local_binding_reusing_the_read_file_name_is_a_static_error() {
    let dir = TempObkDir::new();
    let main = dir.write(
        "main",
        "readFile \u{2254} 5\nresult \u{2254} readFile\n\u{2767}\n",
    );
    let out = bin().arg("check").arg(&main).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        stderr.contains("already bound by the ambient prelude"),
        "{stderr}"
    );
}

#[test]
fn read_line_reads_a_real_line_from_piped_stdin() {
    let f = write_temp("result \u{2254} readLine(\u{2205})\n\u{2767}\n");
    let mut child = bin()
        .arg("run")
        .arg(f.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"hello from stdin\n")
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success(), "{:?}", out);
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("hello from stdin"), "{stdout}");
}

#[test]
fn read_line_at_real_eof_raises_a_failure_catchable_by_the_existing_mechanism() {
    let f = write_temp(
        "result \u{2254} \u{260a} (readLine(\u{2205})) \u{3bb}(e) \u{2192} \u{27e1} e {\n  \
         Failure(tag, msg) \u{2192} tag\n  \u{27e2} DivisionByZero \u{2192} \u{22}div0\u{22}\n  \
         \u{27e2} NonExhaustiveMatch \u{2192} \u{22}nem\u{22}\n  \
         \u{27e2} InvalidOperation(m) \u{2192} m\n  \u{27e2} IntegerOverflow \u{2192} \u{22}overflow\u{22}\n}\n\u{2767}\n",
    );
    let mut child = bin()
        .arg("run")
        .arg(f.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // Drop stdin immediately, without writing anything, to force a real
    // EOF on the child's stdin.
    drop(child.stdin.take());
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success(), "{:?}", out);
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("EOF"), "{stdout}");
}

#[test]
fn read_line_uncaught_eof_is_an_uncaught_exception_not_a_crash() {
    let f = write_temp("result \u{2254} readLine(\u{2205})\n\u{2767}\n");
    let mut child = bin()
        .arg("run")
        .arg(f.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    drop(child.stdin.take());
    let out = child.wait_with_output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.contains("Failure"), "{stderr}");
}

// ── I/O: `print` as the first `Value::Native` (host-provided capability,
//    runtime never touches `std::io` itself — see `obfusku_cli::natives`
//    and `obfusku_runtime::value::NativeFn`'s own doc comments). ───────

#[test]
fn print_writes_to_real_stdout_through_the_compiled_binary() {
    let f = write_temp("result \u{2254} print(\u{22}hello from native print\u{22})\n\u{2767}\n");
    let out = bin().arg("run").arg(f.path()).output().unwrap();
    assert!(out.status.success(), "{:?}", out);
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("hello from native print"), "{stdout}");
    // print : Str -> Unit
    assert!(stdout.contains("Unit"), "{stdout}");
}

#[test]
fn print_composes_with_an_ordinary_function_and_stdlib_map() {
    // print called from inside a user-defined function, and from a
    // stdlib `map` callback — proves a native value is an ordinary
    // first-class function at every call site, not special-cased.
    let f = write_temp(
        "\u{3bb}announce(n: \u{27c1}): \u{2205} \u{2192} print(\u{22}got a number\u{22})\n\
         xs \u{2254} Cons(1, Cons(2, Nil))\n\
         result \u{2254} map(announce, xs)\n\u{2767}\n",
    );
    let out = bin().arg("run").arg(f.path()).output().unwrap();
    assert!(out.status.success(), "{:?}", out);
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert_eq!(stdout.matches("got a number").count(), 2, "{stdout}");
}

#[test]
fn calling_print_with_a_non_str_argument_is_a_static_type_error_not_a_runtime_crash() {
    let f = write_temp("result \u{2254} print(5)\n\u{2767}\n");
    let out = bin().arg("check").arg(f.path()).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(!stderr.is_empty(), "{stderr}");
}

#[test]
fn a_local_binding_reusing_the_print_name_is_a_static_error() {
    let f = write_temp("print \u{2254} 5\nresult \u{2254} print\n\u{2767}\n");
    let out = bin().arg("check").arg(f.path()).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        stderr.contains("already bound by the ambient prelude"),
        "{stderr}"
    );
}

#[test]
fn a_diagnostic_originating_inside_an_imported_file_renders_without_panicking() {
    // The exact bug found while building this slice: rendering a
    // diagnostic whose span belongs to an *imported* file's text against
    // the importer's own (shorter/differently-laid-out) text panicked on
    // a non-UTF-8-boundary slice. Both files must share one `SourceMap`.
    let dir = TempObkDir::new();
    dir.write("broken", "x \u{2254} nope\n\u{2767}\n");
    let main = dir.write("main", "\u{27F2}broken\nresult \u{2254} 1\n\u{2767}\n");
    let out = bin().arg("check").arg(&main).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.contains("unknown variable 'nope'"), "{stderr}");
}
