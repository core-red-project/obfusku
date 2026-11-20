//! `print` and `readLine` through the real binary: stdout, piped stdin and EOF.

use crate::support::{bin, write_temp};
use std::io::Write;
use std::process::Stdio;

#[test]
fn read_line_reads_a_real_line_from_piped_stdin() {
    let f = write_temp("result \u{2254} ⌁↓(\u{2205})\n\u{2767}\n");
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
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("hello from stdin"), "{stdout}");
}

#[test]
fn read_line_at_real_eof_raises_a_failure_catchable_by_the_existing_mechanism() {
    let f = write_temp(
        "result \u{2254} \u{260a} (⌁↓(\u{2205})) \u{3bb}(e) \u{2192} \u{27e1} e {\n  \
         Failure(tag, msg) \u{2192} tag\n  \u{27e2} DivisionByZero \u{2192} \u{22}div0\u{22}\n  \
         \u{27e2} NonExhaustiveMatch \u{2192} \u{22}nem\u{22}\n  \
         \u{27e2} InvalidOperation(m) \u{2192} m\n  \u{27e2} IntegerOverflow \u{2192} \u{22}overflow\u{22}\n  \
         \u{27e2} _ \u{2192} \u{22}other\u{22}\n}\n\u{2767}\n",
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
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("EOF"), "{stdout}");
}

#[test]
fn read_line_uncaught_eof_is_an_uncaught_exception_not_a_crash() {
    let f = write_temp("result \u{2254} ⌁↓(\u{2205})\n\u{2767}\n");
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

#[test]
fn print_writes_to_real_stdout_through_the_compiled_binary() {
    let f = write_temp("result \u{2254} ⌁↑(\u{22}hello from native ⌁↑\u{22})\n\u{2767}\n");
    let out = bin().arg("run").arg(f.path()).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("hello from native ⌁↑"), "{stdout}");
    // ⌁↑ : Str -> Unit, rendered as `∅`.
    assert!(stdout.contains("∅"), "{stdout}");
}

#[test]
fn print_composes_with_an_ordinary_function_and_stdlib_map() {
    // ⌁↑ called from inside a user-defined function, and from a
    // stdlib `\u{27D0}` callback — proves a native value is an ordinary
    // first-class function at every call site, not special-cased.
    let f = write_temp(
        "\u{3bb}announce(n: \u{27c1}): \u{2205} \u{2192} ⌁↑(\u{22}got a number\u{22})\n\
         xs \u{2254} \u{205D}(1, \u{205D}(2, ⊘))\n\
         result \u{2254} \u{27D0}(announce, xs)\n\u{2767}\n",
    );
    let out = bin().arg("run").arg(f.path()).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert_eq!(stdout.matches("got a number").count(), 2, "{stdout}");
}

#[test]
fn calling_print_with_a_non_str_argument_is_a_static_type_error_not_a_runtime_crash() {
    let f = write_temp("result \u{2254} ⌁↑(5)\n\u{2767}\n");
    let out = bin().arg("check").arg(f.path()).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(!stderr.is_empty(), "{stderr}");
}

#[test]
fn a_local_binding_may_shadow_the_print_name() {
    let f = write_temp("⌁↑ \u{2254} 5\nresult \u{2254} ⌁↑\n\u{2767}\n");
    let out = bin().arg("check").arg(f.path()).output().unwrap();
    assert!(out.status.success(), "{out:?}");
}
