//! Finding the Project: bare files stay unchanged, the nearest manifest wins, and a moved Project still runs (`ADR-017`).

use crate::support::{TempTree, bin};
use std::sync::atomic::{AtomicU32, Ordering};

fn copy_dir(from: &std::path::Path, to: &std::path::Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap().flatten() {
        let path = entry.path();
        let dest = to.join(entry.file_name());
        if path.is_dir() {
            copy_dir(&path, &dest);
        } else {
            std::fs::copy(&path, &dest).unwrap();
        }
    }
}

#[test]
fn bare_file_with_no_manifest_anywhere_runs_exactly_as_before() {
    let tree = TempTree::new();
    let main = tree.write("main.obk", "result \u{2254} 1 \u{271a} 2\n\u{2767}\n");
    let out = bin().arg("run").arg(&main).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    assert!(String::from_utf8(out.stdout).unwrap().contains('3'));
}

#[test]
fn bare_file_check_with_no_manifest_anywhere_works_unchanged() {
    let tree = TempTree::new();
    let main = tree.write("main.obk", "result \u{2254} 1\n\u{2767}\n");
    let out = bin().arg("check").arg(&main).output().unwrap();
    assert!(out.status.success(), "{out:?}");
}

#[test]
fn running_a_project_directory_discovers_its_manifest_and_entry() {
    let tree = TempTree::new();
    tree.manifest("proj", "main");
    tree.write("proj/main.obk", "result \u{2254} 41 \u{271a} 1\n\u{2767}\n");
    let out = bin()
        .arg("run")
        .arg(tree.root().join("proj"))
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    assert!(String::from_utf8(out.stdout).unwrap().contains("42"));
}

#[test]
fn running_a_file_inside_a_project_uses_the_projects_root_for_resolution() {
    let tree = TempTree::new();
    tree.manifest("proj", "main");
    tree.write(
        "proj/main.obk",
        "\u{27F2}helper\nresult \u{2254} bumped\n\u{2767}\n",
    );
    tree.write("proj/helper.obk", "bumped \u{2254}\u{27f3} 5\n\u{2767}\n");
    let out = bin()
        .arg("run")
        .arg(tree.root().join("proj/main.obk"))
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    assert!(String::from_utf8(out.stdout).unwrap().contains('5'));
}

#[test]
fn nearest_ancestor_manifest_wins_over_an_outer_one() {
    // workspace/obfusku.toml (entry: outer, which does not even exist)
    // workspace/nested/obfusku.toml (entry: main)
    // workspace/nested/src/main.obk
    // Running the nested project directory must resolve against its
    // own manifest, never walk past it to the outer one.
    let tree = TempTree::new();
    tree.write("workspace/obfusku.toml", "format = 1\nentry = \"outer\"\n");
    tree.manifest("workspace/nested", "main");
    tree.write(
        "workspace/nested/src/main.obk",
        "result \u{2254} 7\n\u{2767}\n",
    );
    let out = bin()
        .arg("run")
        .arg(tree.root().join("workspace/nested"))
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    assert!(String::from_utf8(out.stdout).unwrap().contains('7'));
}

#[test]
fn a_file_belongs_to_the_nearest_ancestor_project_not_an_outer_one() {
    // The exact scenario named as mandatory: main.obk under
    // workspace/nested/src must resolve imports/filesystem against
    // workspace/nested (its own manifest), not workspace (the outer
    // one) — verified by an import that only exists in the nested
    // project's own tree.
    let tree = TempTree::new();
    tree.write("workspace/obfusku.toml", "format = 1\nentry = \"main\"\n");
    tree.manifest("workspace/nested", "main");
    tree.write(
        "workspace/nested/src/main.obk",
        "\u{27F2}onlyInNested\nresult \u{2254} v\n\u{2767}\n",
    );
    tree.write(
        "workspace/nested/onlyInNested.obk",
        "v \u{2254}\u{27f3} 9\n\u{2767}\n",
    );
    let out = bin()
        .arg("run")
        .arg(tree.root().join("workspace/nested/src/main.obk"))
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    assert!(String::from_utf8(out.stdout).unwrap().contains('9'));
}

#[test]
fn invoking_the_entry_file_directly_behaves_identically_to_invoking_the_project_directory() {
    // The property named as mandatory: `obfusku run ./proj/src/main.obk`
    // must resolve imports and filesystem access against the *same*
    // Project root `obfusku run ./proj` would use — not a weaker,
    // same-directory-only fallback just because a file path was given
    // instead of a directory. Exercises both an import (a subdirectory
    // away from the entry file) and a `readFile` (a subdirectory away
    // in the *other* direction) so a divergence in either boundary
    // would be caught, not just one.
    let tree = TempTree::new();
    tree.manifest("proj", "main");
    tree.write(
        "proj/src/main.obk",
        "\u{27F2}helper\nfileContents \u{2254} \u{2301}\u{2193}\u{232C}(\u{22}data/note.txt\u{22})\n\
         result \u{2254} bumped\n\u{2767}\n",
    );
    tree.write("proj/data/note.txt", "irrelevant to the result, only read");
    tree.write("proj/helper.obk", "bumped \u{2254}\u{27f3} 5\n\u{2767}\n");

    let via_directory = bin()
        .arg("run")
        .arg(tree.root().join("proj"))
        .output()
        .unwrap();
    let via_file = bin()
        .arg("run")
        .arg(tree.root().join("proj/src/main.obk"))
        .output()
        .unwrap();

    assert!(via_directory.status.success(), "{via_directory:?}");
    assert!(via_file.status.success(), "{via_file:?}");
    assert_eq!(
        String::from_utf8(via_directory.stdout).unwrap(),
        String::from_utf8(via_file.stdout).unwrap(),
        "file-invocation must produce byte-identical output to directory-invocation"
    );
}

#[test]
fn a_project_copied_to_a_new_location_still_runs() {
    let tree = TempTree::new();
    tree.manifest("proj", "main");
    tree.write(
        "proj/main.obk",
        "\u{27F2}helper\nresult \u{2254} v\n\u{2767}\n",
    );
    tree.write("proj/helper.obk", "v \u{2254}\u{27f3} 100\n\u{2767}\n");

    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let copy_dest = std::env::temp_dir().join(format!(
        "obfusku-project-test-copy-{}-{n}",
        std::process::id()
    ));
    copy_dir(&tree.root().join("proj"), &copy_dest);

    let out = bin().arg("run").arg(&copy_dest).output().unwrap();
    let _ = std::fs::remove_dir_all(&copy_dest);
    assert!(out.status.success(), "{out:?}");
    assert!(String::from_utf8(out.stdout).unwrap().contains("100"));
}
