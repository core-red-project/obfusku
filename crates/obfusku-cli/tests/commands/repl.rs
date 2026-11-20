//! `repl`: one evaluation per line, a bad line leaves the session intact, ambient stdlib and no filesystem.

use crate::support::bin;
use std::io::Write;
use std::process::Stdio;

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
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stdout.contains('5'), "stdout: {stdout}");
    assert!(stdout.contains('6'), "stdout: {stdout}");
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
    assert!(out.status.success(), "{out:?}");
}

#[test]
fn repl_has_ambient_stdlib_and_io_but_not_filesystem_natives() {
    // Post-incremental-REPL contract (`ReplSession` replaced the old
    // whole-buffer-rerun design, which excluded this ambient environment
    // specifically because re-evaluating history would replay effects —
    // that hazard no longer applies, since each line evaluates exactly
    // once): `print`/`readLine` and the ambient stdlib (`List`/`⁝`/
    // `⊘`/`⟐`) now collide as already-bound prelude names, exactly
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
                "\u{2301}\u{2191} \u{2254} 1\n\u{2301}\u{2193} \u{2254} 2\n\u{27D0} \u{2254} 3\n\u{233F} \u{2254} 4\n\
                 \u{2301}\u{2193}\u{232C} \u{2254} 5\n\u{2301}\u{2191}\u{232C} \u{2254} 6\n\
                 result \u{2254} \u{2301}\u{2193}\u{232C} \u{271A} \u{2301}\u{2191}\u{232C}\n\
                 :quit\n"
                    .as_bytes(),
            )
            .unwrap();
    }
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stdout.contains("11"), "stdout: {stdout}");
    let collisions = stderr
        .matches("already bound by the ambient prelude")
        .count();
    assert_eq!(
        collisions, 4,
        "expected exactly \u{2301}\u{2191}/\u{2301}\u{2193}/\u{27D0}/\u{233F} to collide, none of \u{2301}\u{2193}\u{232C}/\u{2301}\u{2191}\u{232C}: {stderr}"
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
                "a \u{2254} \u{2301}\u{2191}(\u{22}once\u{22})\nx \u{2254} 1\ny \u{2254} 2\nz \u{2254} 3\n:quit\n"
                    .as_bytes(),
            )
            .unwrap();
    }
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success(), "{out:?}");
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
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("42"), "stdout: {stdout}");
}

#[test]
fn collection_natives_are_available_in_the_repl_with_no_filesystem_root() {
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
            .write_all("result \u{2254} #([1, 2, 3])\n:quit\n".as_bytes())
            .unwrap();
    }
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains('3'), "{stdout}");
}

#[test]
fn numeric_conversion_is_available_in_the_repl() {
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
            .write_all("result \u{2254} \u{2197}(3)\n:quit\n".as_bytes())
            .unwrap();
    }
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("3.0") || stdout.contains('3'), "{stdout}");
}
