//! `test`: each file in its own process, timeouts, and the default test directory (`ADR-030`).

use crate::support::{TempTree, bin};

#[test]
fn test_runs_each_file_in_isolation_and_reports_pass_and_fail() {
    let tree = TempTree::new();
    tree.write(
        "p/obfusku.toml",
        "format = 2\nentry = \"main\"\n[test]\ninclude = [\"t/**/*.obk\"]\n",
    );
    tree.write("p/main.obk", "r ≔ 1\n❧\n");
    tree.write("p/t/good.obk", "x ≔ 1 ✚ 1\n❧\n");
    tree.write(
        "p/t/bad.obk",
        "x ≔ ⍴(☄ Failure(\"assertion\", \"nope\"))\n❧\n",
    );
    tree.write(
        "p/other/ignored.obk",
        "x ≔ ⍴(☄ Failure(\"assertion\", \"never\"))\n❧\n",
    );
    let out = bin()
        .arg("test")
        .arg(tree.root().join("p"))
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("ok    t/good.obk"), "{stdout}");
    assert!(stdout.contains("FAIL  t/bad.obk"), "{stdout}");
    assert!(stdout.contains("nope"), "{stdout}");
    assert!(!stdout.contains("ignored.obk"), "{stdout}");
    assert!(stdout.contains("1 passed, 1 failed"), "{stdout}");
}

#[test]
fn test_with_all_passing_exits_zero_and_defaults_to_tests_directory() {
    let tree = TempTree::new();
    tree.write("p/obfusku.toml", "format = 1\nentry = \"main\"\n");
    tree.write("p/main.obk", "r ≔ 1\n❧\n");
    tree.write("p/tests/a.obk", "x ≔ 1\n❧\n");
    let out = bin()
        .arg("test")
        .arg(tree.root().join("p"))
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
}

#[test]
fn test_kills_a_file_that_outlives_the_timeout() {
    let tree = TempTree::new();
    tree.write(
        "p/obfusku.toml",
        "format = 2\nentry = \"main\"\n[test]\ntimeout = 1\n",
    );
    tree.write("p/main.obk", "r ≔ 1\n❧\n");
    tree.write(
        "p/tests/spin.obk",
        "λspin(n: ⟁): ⟁ →⟳ spin(n ✚ 1)\nr ≔ spin(0)\n❧\n",
    );
    let started = std::time::Instant::now();
    let out = bin()
        .arg("test")
        .arg(tree.root().join("p"))
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(started.elapsed().as_secs() < 30);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("timed out after 1 s"), "{stdout}");
}

#[test]
fn test_with_no_matching_files_is_a_usage_error() {
    let tree = TempTree::new();
    tree.write("p/obfusku.toml", "format = 1\nentry = \"main\"\n");
    tree.write("p/main.obk", "r ≔ 1\n❧\n");
    let out = bin()
        .arg("test")
        .arg(tree.root().join("p"))
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2), "{out:?}");
}
