//! End-to-end tests for the Obfusku Project model
//! (`ADR-017`/`ADR-018`/`ADR-019`/`ADR-020`) against the compiled
//! `obfusku` executable — bare-file compatibility, manifest discovery
//! and validation, project-relative module resolution, the
//! project-root filesystem boundary (including the symlink-escape
//! fix), and CLI directory acceptance across `run`/`check`/`fmt`/
//! `inspect`.

use std::sync::atomic::{AtomicU32, Ordering};

fn bin() -> std::process::Command {
    std::process::Command::new(env!("CARGO_BIN_EXE_obfusku"))
}

/// A scratch directory tree, deleted on drop. Every helper takes a
/// path relative to the tree's own root, creating parent directories
/// as needed — used to build both flat and nested Project layouts.
struct TempTree(std::path::PathBuf);

impl TempTree {
    fn new() -> Self {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("obfusku-project-test-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create temp tree root");
        TempTree(dir)
    }

    fn root(&self) -> &std::path::Path {
        &self.0
    }

    fn write(&self, rel_path: &str, content: &str) -> std::path::PathBuf {
        let path = self.0.join(rel_path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create parent dirs");
        }
        std::fs::write(&path, content).expect("write file");
        path
    }

    fn manifest(&self, rel_dir: &str, entry: &str) -> std::path::PathBuf {
        self.write(
            &format!("{rel_dir}/obfusku.toml"),
            &format!("format = 1\nentry = \"{entry}\"\n"),
        )
    }
}

impl Drop for TempTree {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

// ── Bare file: the Project model must not change this path at all ──

#[test]
fn bare_file_with_no_manifest_anywhere_runs_exactly_as_before() {
    let tree = TempTree::new();
    let main = tree.write("main.obk", "result \u{2254} 1 \u{271a} 2\n\u{2767}\n");
    let out = bin().arg("run").arg(&main).output().unwrap();
    assert!(out.status.success(), "{:?}", out);
    assert!(String::from_utf8(out.stdout).unwrap().contains("Int(3)"));
}

#[test]
fn bare_file_check_with_no_manifest_anywhere_works_unchanged() {
    let tree = TempTree::new();
    let main = tree.write("main.obk", "result \u{2254} 1\n\u{2767}\n");
    let out = bin().arg("check").arg(&main).output().unwrap();
    assert!(out.status.success(), "{:?}", out);
}

// ── Project: discovery, entry resolution ────────────────────────────

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
    assert!(out.status.success(), "{:?}", out);
    assert!(String::from_utf8(out.stdout).unwrap().contains("Int(42)"));
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
    assert!(out.status.success(), "{:?}", out);
    assert!(String::from_utf8(out.stdout).unwrap().contains("Int(5)"));
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
    assert!(out.status.success(), "{:?}", out);
    assert!(String::from_utf8(out.stdout).unwrap().contains("Int(7)"));
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
    assert!(out.status.success(), "{:?}", out);
    assert!(String::from_utf8(out.stdout).unwrap().contains("Int(9)"));
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

    assert!(via_directory.status.success(), "{:?}", via_directory);
    assert!(via_file.status.success(), "{:?}", via_file);
    assert_eq!(
        String::from_utf8(via_directory.stdout).unwrap(),
        String::from_utf8(via_file.stdout).unwrap(),
        "file-invocation must produce byte-identical output to directory-invocation"
    );
}

// ── Project-relative imports, including nested subdirectories ──────

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
    assert!(out.status.success(), "{:?}", out);
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

// ── Manifest validation ──────────────────────────────────────────────

#[test]
fn a_directory_with_no_manifest_is_a_clear_error_not_bare_file_fallback() {
    let tree = TempTree::new();
    tree.write("proj/main.obk", "result \u{2254} 1\n\u{2767}\n");
    let out = bin()
        .arg("run")
        .arg(tree.root().join("proj"))
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn missing_entry_field_is_rejected_as_an_invalid_manifest() {
    let tree = TempTree::new();
    tree.write("proj/obfusku.toml", "format = 1\n");
    tree.write("proj/main.obk", "result \u{2254} 1\n\u{2767}\n");
    let out = bin()
        .arg("run")
        .arg(tree.root().join("proj"))
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.contains("invalid manifest"), "{stderr}");
}

#[test]
fn unsupported_format_is_rejected_with_a_clear_message() {
    let tree = TempTree::new();
    tree.write("proj/obfusku.toml", "format = 2\nentry = \"main\"\n");
    tree.write("proj/main.obk", "result \u{2254} 1\n\u{2767}\n");
    let out = bin()
        .arg("run")
        .arg(tree.root().join("proj"))
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.contains("unsupported manifest format"), "{stderr}");
}

#[test]
fn malformed_toml_is_rejected_as_an_invalid_manifest() {
    let tree = TempTree::new();
    tree.write("proj/obfusku.toml", "this is not [ valid toml");
    tree.write("proj/main.obk", "result \u{2254} 1\n\u{2767}\n");
    let out = bin()
        .arg("run")
        .arg(tree.root().join("proj"))
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.contains("invalid manifest"), "{stderr}");
}

#[test]
fn entry_naming_a_nonexistent_module_is_rejected() {
    let tree = TempTree::new();
    tree.manifest("proj", "does_not_exist");
    tree.write("proj/main.obk", "result \u{2254} 1\n\u{2767}\n");
    let out = bin()
        .arg("run")
        .arg(tree.root().join("proj"))
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.contains("does_not_exist"), "{stderr}");
}

#[test]
fn unknown_manifest_fields_are_tolerated_at_a_supported_format() {
    let tree = TempTree::new();
    tree.write(
        "proj/obfusku.toml",
        "format = 1\nentry = \"main\"\nname = \"demo\"\nsomething_future = \"ignored\"\n",
    );
    tree.write("proj/main.obk", "result \u{2254} 1\n\u{2767}\n");
    let out = bin()
        .arg("run")
        .arg(tree.root().join("proj"))
        .output()
        .unwrap();
    assert!(out.status.success(), "{:?}", out);
}

// ── Project-root filesystem boundary ────────────────────────────────

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
    assert!(out.status.success(), "{:?}", out);
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
    assert!(out.status.success(), "{:?}", out);
    assert!(String::from_utf8(out.stdout)
        .unwrap()
        .contains("nested data"));
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

// ── A moved/copied project remains executable ───────────────────────

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
    assert!(out.status.success(), "{:?}", out);
    assert!(String::from_utf8(out.stdout).unwrap().contains("Int(100)"));
}

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

// ── CLI: fmt / inspect accept a Project directory ───────────────────

#[test]
fn fmt_accepts_a_project_directory_and_formats_its_entry_module() {
    let tree = TempTree::new();
    tree.manifest("proj", "main");
    tree.write("proj/main.obk", "result\u{2254}1\n\u{2767}\n");
    let out = bin()
        .arg("fmt")
        .arg(tree.root().join("proj"))
        .output()
        .unwrap();
    assert!(out.status.success(), "{:?}", out);
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("result \u{2254} 1"), "{stdout}");
}

#[test]
fn inspect_accepts_a_project_directory_and_inspects_its_entry_module() {
    let tree = TempTree::new();
    tree.manifest("proj", "main");
    tree.write("proj/main.obk", "result \u{2254} 1\n\u{2767}\n");
    let out = bin()
        .arg("inspect")
        .arg(tree.root().join("proj"))
        .arg("--tokens")
        .output()
        .unwrap();
    assert!(out.status.success(), "{:?}", out);
}

#[test]
fn check_accepts_a_project_directory() {
    let tree = TempTree::new();
    tree.manifest("proj", "main");
    tree.write("proj/main.obk", "result \u{2254} 1\n\u{2767}\n");
    let out = bin()
        .arg("check")
        .arg(tree.root().join("proj"))
        .output()
        .unwrap();
    assert!(out.status.success(), "{:?}", out);
}

// ── `build`: requires a real Project, certifies a valid one, writes
//    nothing to disk. ──────────────────────────────────────────────

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
    assert!(out.status.success(), "{:?}", out);
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

// ── `test`: deliberately still unsupported, with a corrected reason —
//    blocked on a language-level construct, not the artifact model
//    (which is now resolved). ────────────────────────────────────────

#[test]
fn test_command_is_still_unsupported_with_the_corrected_reason() {
    let out = bin().arg("test").output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        stderr.contains("language-level testing construct"),
        "{stderr}"
    );
    assert!(
        !stderr.contains("artifact/module distribution"),
        "the old, now-inaccurate blocker reason must not remain: {stderr}"
    );
}
