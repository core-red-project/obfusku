//! Import resolution: whole-module imports, privacy, ambiguity, cycles, missing modules, and diagnostics that originate in an imported file (`ADR-018`).

use crate::support::{TempObkDir, TempTree, bin};

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
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains('6'), "{stdout}");
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
        stderr.contains("does not resolve to any") && stderr.contains("doesnotexist"),
        "{stderr}"
    );
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

#[test]
fn project_relative_import_resolves_a_module_in_a_subdirectory() {
    let tree = TempTree::new();
    tree.manifest("proj", "main");
    tree.write(
        "proj/main.obk",
        "\u{27F2}strings\nresult \u{2254} greeting\n\u{2767}\n",
    );
    tree.write(
        "proj/utils/strings.obk",
        "greeting \u{2254}\u{27f3} \u{22}hi\u{22}\n\u{2767}\n",
    );
    let out = bin()
        .arg("run")
        .arg(tree.root().join("proj"))
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    assert!(String::from_utf8(out.stdout).unwrap().contains("hi"));
}

#[test]
fn ambiguous_module_name_across_two_subdirectories_is_a_clear_error() {
    let tree = TempTree::new();
    tree.manifest("proj", "main");
    tree.write(
        "proj/main.obk",
        "\u{27F2}dup\nresult \u{2254} 1\n\u{2767}\n",
    );
    tree.write("proj/a/dup.obk", "x \u{2254}\u{27f3} 1\n\u{2767}\n");
    tree.write("proj/b/dup.obk", "x \u{2254}\u{27f3} 2\n\u{2767}\n");
    let out = bin()
        .arg("check")
        .arg(tree.root().join("proj"))
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.contains("ambiguous"), "{stderr}");
}

#[test]
fn bare_relative_filename_resolves_imports_from_the_current_directory() {
    let tree = TempTree::new();
    tree.write(
        "main.obk",
        "\u{27F2}strings\nresult \u{2254} greeting\n\u{2767}\n",
    );
    tree.write(
        "strings.obk",
        "greeting \u{2254}\u{27f3} \u{22}hi\u{22}\n\u{2767}\n",
    );
    let out = bin()
        .current_dir(tree.root())
        .arg("run")
        .arg("main.obk")
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    assert!(String::from_utf8(out.stdout).unwrap().contains("hi"));
}

#[test]
fn module_search_skips_target_and_hidden_directories() {
    let tree = TempTree::new();
    tree.manifest("proj", "main");
    tree.write(
        "proj/main.obk",
        "\u{27F2}dup\nresult \u{2254} x\n\u{2767}\n",
    );
    tree.write("proj/src/dup.obk", "x \u{2254}\u{27f3} 1\n\u{2767}\n");
    tree.write("proj/target/dup.obk", "x \u{2254}\u{27f3} 2\n\u{2767}\n");
    tree.write("proj/.hidden/dup.obk", "x \u{2254}\u{27f3} 3\n\u{2767}\n");
    let out = bin()
        .arg("run")
        .arg(tree.root().join("proj"))
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
}

#[cfg(unix)]
#[test]
fn module_search_does_not_follow_directory_symlinks() {
    let tree = TempTree::new();
    tree.manifest("proj", "main");
    tree.write(
        "proj/main.obk",
        "\u{27F2}outside\nresult \u{2254} x\n\u{2767}\n",
    );
    tree.write("elsewhere/outside.obk", "x \u{2254}\u{27f3} 1\n\u{2767}\n");
    std::os::unix::fs::symlink(tree.root().join("elsewhere"), tree.root().join("proj/link"))
        .unwrap();
    let out = bin()
        .arg("check")
        .arg(tree.root().join("proj"))
        .output()
        .unwrap();
    assert!(!out.status.success(), "{out:?}");
}
