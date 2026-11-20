//! `build`: certifies a valid Project and writes nothing; a bare file or a failing Project gets its own exit code.

use crate::support::{TempTree, bin};

#[test]
fn build_with_no_path_argument_reports_the_ordinary_missing_argument_message() {
    let out = bin().arg("build").output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.contains("expected a file path"), "{stderr}");
}

#[test]
fn build_succeeds_on_a_valid_project_and_writes_nothing_to_disk() {
    let tree = TempTree::new();
    tree.manifest("proj", "main");
    tree.write("proj/main.obk", "result \u{2254} 1 \u{271a} 2\n\u{2767}\n");
    let before: std::collections::BTreeSet<_> = std::fs::read_dir(tree.root().join("proj"))
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();

    let out = bin()
        .arg("build")
        .arg(tree.root().join("proj"))
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("valid Source Artifact"), "{stdout}");

    let after: std::collections::BTreeSet<_> = std::fs::read_dir(tree.root().join("proj"))
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(
        before, after,
        "build must not write any new file to the Project"
    );
}

#[test]
fn build_on_a_bare_file_with_no_manifest_is_a_distinct_usage_error() {
    let tree = TempTree::new();
    let main = tree.write("main.obk", "result \u{2254} 1\n\u{2767}\n");
    let out = bin().arg("build").arg(&main).output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        stderr.contains("not part of an Obfusku Project"),
        "{stderr}"
    );
}

#[test]
fn build_on_a_directory_with_no_manifest_is_the_same_distinct_usage_error() {
    let tree = TempTree::new();
    tree.write("proj/main.obk", "result \u{2254} 1\n\u{2767}\n");
    let out = bin()
        .arg("build")
        .arg(tree.root().join("proj"))
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn build_on_a_project_that_fails_typecheck_is_exit_one_not_two() {
    let tree = TempTree::new();
    tree.manifest("proj", "main");
    tree.write(
        "proj/main.obk",
        "result \u{2254} 1 \u{271a} \u{22}nope\u{22}\n\u{2767}\n",
    );
    let out = bin()
        .arg("build")
        .arg(tree.root().join("proj"))
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn build_validates_the_whole_reachable_import_graph() {
    let tree = TempTree::new();
    tree.manifest("proj", "main");
    tree.write(
        "proj/main.obk",
        "\u{27F2}helper\nresult \u{2254} bumped\n\u{2767}\n",
    );
    // helper's own body is ill-typed — build must catch this even
    // though main.obk itself is fine, since helper is reachable.
    tree.write(
        "proj/helper.obk",
        "bumped \u{2254}\u{27f3} 1 \u{271a} \u{22}nope\u{22}\n\u{2767}\n",
    );
    let out = bin()
        .arg("build")
        .arg(tree.root().join("proj"))
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
}
