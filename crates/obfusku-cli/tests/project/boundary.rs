//! The Project-root filesystem boundary, including symlink escapes (`ADR-019`).

use crate::support::{TempTree, bin};

#[test]
fn write_then_read_within_the_project_root_works() {
    let tree = TempTree::new();
    tree.manifest("proj", "main");
    tree.write(
        "proj/main.obk",
        "unused \u{2254} \u{2301}\u{2191}\u{232C}(\u{22}out.txt\u{22})(\u{22}hello\u{22})\n\
         result \u{2254} \u{2301}\u{2193}\u{232C}(\u{22}out.txt\u{22})\n\u{2767}\n",
    );
    let out = bin()
        .arg("run")
        .arg(tree.root().join("proj"))
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    assert!(String::from_utf8(out.stdout).unwrap().contains("hello"));
}

#[test]
fn read_from_a_nested_subdirectory_stays_within_the_project_root() {
    let tree = TempTree::new();
    tree.manifest("proj", "main");
    tree.write("proj/data/note.txt", "nested data");
    tree.write(
        "proj/src/main.obk",
        "result \u{2254} \u{2301}\u{2193}\u{232C}(\u{22}data/note.txt\u{22})\n\u{2767}\n",
    );
    let out = bin()
        .arg("run")
        .arg(tree.root().join("proj"))
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    assert!(
        String::from_utf8(out.stdout)
            .unwrap()
            .contains("nested data")
    );
}

#[test]
#[cfg(unix)]
fn a_symlink_pointing_outside_the_project_root_is_rejected() {
    let tree = TempTree::new();
    let outside = tree.root().join("outside_secret.txt");
    std::fs::write(&outside, "top secret").unwrap();
    tree.manifest("proj", "main");
    std::os::unix::fs::symlink(&outside, tree.root().join("proj/escape")).unwrap();
    tree.write(
        "proj/main.obk",
        "result \u{2254} \u{2301}\u{2193}\u{232C}(\u{22}escape\u{22})\n\u{2767}\n",
    );
    let out = bin()
        .arg("run")
        .arg(tree.root().join("proj"))
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.contains("PermissionDenied"), "{stderr}");
}

#[test]
#[cfg(unix)]
fn a_symlinked_directory_escape_is_rejected_for_a_nested_path() {
    let tree = TempTree::new();
    let outside = tree.root().join("outside_dir");
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("secret.txt"), "leaked").unwrap();
    tree.manifest("proj", "main");
    std::fs::create_dir_all(tree.root().join("proj")).unwrap();
    std::os::unix::fs::symlink(&outside, tree.root().join("proj/data")).unwrap();
    tree.write(
        "proj/main.obk",
        "result \u{2254} \u{2301}\u{2193}\u{232C}(\u{22}data/secret.txt\u{22})\n\u{2767}\n",
    );
    let out = bin()
        .arg("run")
        .arg(tree.root().join("proj"))
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.contains("PermissionDenied"), "{stderr}");
}
