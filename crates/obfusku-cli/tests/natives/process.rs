//! Arguments, environment, stderr and exit codes (`ADR-022`, `ADR-023`).

use crate::support::{ARGS_PROGRAM, TempTree, bin};

#[test]
fn arguments_after_double_dash_reach_the_program_and_help_is_not_intercepted() {
    let tree = TempTree::new();
    let file = tree.write("args.obk", ARGS_PROGRAM);
    let out = bin()
        .arg("run")
        .arg(&file)
        .args(["--", "--help", "b"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    assert!(
        String::from_utf8(out.stdout)
            .unwrap()
            .starts_with("--help\n")
    );
}

#[test]
fn arguments_default_to_the_empty_list() {
    let tree = TempTree::new();
    let file = tree.write("args.obk", ARGS_PROGRAM);
    let out = bin().arg("run").arg(&file).output().unwrap();
    assert!(String::from_utf8(out.stdout).unwrap().starts_with("none\n"));
}

#[test]
fn environment_variable_lookup_returns_some_or_none() {
    let tree = TempTree::new();
    let src = "v \u{2254} \u{27E1} \u{2301}\u{2193}\u{2388}(\u{22}OBK_TEST_VAR\u{22}) {\n  \
        \u{29B0} \u{2192} \u{22}unset\u{22}\n  \u{27E2} \u{29EB}(s) \u{2192} s\n}\n\
        shown \u{2254} \u{2301}\u{2191}(v)\nresult \u{2254} 1\n\u{2767}\n";
    let file = tree.write("env.obk", src);
    let set = bin()
        .arg("run")
        .arg(&file)
        .env("OBK_TEST_VAR", "from-env")
        .output()
        .unwrap();
    assert!(
        String::from_utf8(set.stdout)
            .unwrap()
            .starts_with("from-env\n")
    );
    let unset = bin()
        .arg("run")
        .arg(&file)
        .env_remove("OBK_TEST_VAR")
        .output()
        .unwrap();
    assert!(
        String::from_utf8(unset.stdout)
            .unwrap()
            .starts_with("unset\n")
    );
}

#[test]
fn stderr_native_and_exit_code_are_observable_by_the_caller() {
    let tree = TempTree::new();
    let src = "\u{3BB}quit(n: \u{27C1}): \u{2205} \u{2192} \u{2301}\u{21AF}(n)\n\
        a \u{2254} \u{2301}\u{2191}(\u{22}out\u{22})\n\
        b \u{2254} \u{2301}\u{2191}\u{2349}(\u{22}err\u{22})\n\
        c \u{2254} quit(7)\n\u{2767}\n";
    let file = tree.write("exit.obk", src);
    let out = bin().arg("run").arg(&file).output().unwrap();
    assert_eq!(out.status.code(), Some(7), "{out:?}");
    assert_eq!(String::from_utf8(out.stdout).unwrap(), "out\n");
    assert_eq!(String::from_utf8(out.stderr).unwrap(), "err\n");
}

#[test]
fn exit_code_outside_0_to_255_raises_instead_of_exiting() {
    let tree = TempTree::new();
    let src = "\u{3BB}quit(n: \u{27C1}): \u{2205} \u{2192} \u{2301}\u{21AF}(n)\n\
        c \u{2254} quit(300)\n\u{2767}\n";
    let file = tree.write("badexit.obk", src);
    let out = bin().arg("run").arg(&file).output().unwrap();
    assert_eq!(out.status.code(), Some(1), "{out:?}");
}
