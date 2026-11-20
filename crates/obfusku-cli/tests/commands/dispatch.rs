//! How the command line is read: `--help`/`--version` from any position, unknown commands, mode flags valid only on their own command, `--json`, `--check`, and completions.

use crate::support::{TempTree, bin};

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
    assert!(out.status.success(), "{out:?}");
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
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("usage:"), "{stdout}");
}

#[test]
fn dash_dash_version_short_circuits_from_any_position() {
    let out = bin().arg("check").arg("--version").output().unwrap();
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains(env!("CARGO_PKG_VERSION")), "{stdout}");
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
fn check_json_reports_diagnostics_as_one_object() {
    let tree = TempTree::new();
    let bad = tree.write("bad.obk", "x ≔ 1 ✚ ◉\n❧\n");
    let out = bin().arg("check").arg("--json").arg(bad).output().unwrap();
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.starts_with("{\"ok\":false,\"diagnostics\":[{"),
        "{text}"
    );
    assert!(text.contains("\"line\":1"), "{text}");
    assert_eq!(text.trim_end().lines().count(), 1);

    let good = tree.write("good.obk", "x ≔ 1\n❧\n");
    let out = bin().arg("check").arg(good).arg("--json").output().unwrap();
    assert!(out.status.success(), "{out:?}");
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim_end(),
        "{\"ok\":true,\"diagnostics\":[]}"
    );
}

#[test]
fn json_is_rejected_on_commands_that_do_not_support_it() {
    let tree = TempTree::new();
    let file = tree.write("ok.obk", "x ≔ 1\n❧\n");
    let out = bin().arg("run").arg(file).arg("--json").output().unwrap();
    assert_eq!(out.status.code(), Some(2), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stderr).contains("only supported by 'check'"));
}

#[test]
fn check_json_marks_an_imported_modules_error_with_its_own_source_id() {
    let tree = TempTree::new();
    tree.write("lib.obk", "x ≔ 1 ✚ ◉\n❧\n");
    let main = tree.write("main.obk", "⟲ lib\ny ≔ 1\n❧\n");
    let out = bin().arg("check").arg("--json").arg(main).output().unwrap();
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(!text.contains("\"source\":0"), "{text}");
}

#[test]
fn completions_print_a_script_per_shell_and_reject_unknown_ones() {
    for shell in ["bash", "zsh", "fish", "powershell"] {
        let out = bin().args(["completions", shell]).output().unwrap();
        assert!(out.status.success(), "{shell}: {out:?}");
        assert!(String::from_utf8_lossy(&out.stdout).contains("obfusku"));
    }
    let out = bin().args(["completions", "tcsh"]).output().unwrap();
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn fmt_check_is_silent_and_exits_zero_when_formatted_and_one_when_not() {
    let tree = TempTree::new();
    let messy = tree.write("messy.obk", "x   ≔   1\n❧\n");
    let canonical =
        String::from_utf8(bin().arg("fmt").arg(&messy).output().unwrap().stdout).unwrap();
    let clean = tree.write("clean.obk", &canonical);

    let ok = bin()
        .arg("fmt")
        .arg("--check")
        .arg(&clean)
        .output()
        .unwrap();
    assert!(ok.status.success(), "{ok:?}");
    assert!(ok.stdout.is_empty());

    let bad = bin()
        .arg("fmt")
        .arg("--check")
        .arg(&messy)
        .output()
        .unwrap();
    assert_eq!(bad.status.code(), Some(1), "{bad:?}");
    assert!(bad.stdout.is_empty());
}
