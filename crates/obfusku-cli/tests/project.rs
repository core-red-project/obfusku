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
    tree.write("proj/obfusku.toml", "format = 3\nentry = \"main\"\n");
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
    assert!(out.status.success(), "{out:?}");
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
    assert!(out.status.success(), "{out:?}");
    assert!(String::from_utf8(out.stdout).unwrap().contains("100"));
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
    assert!(out.status.success(), "{out:?}");
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
    assert!(out.status.success(), "{out:?}");
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
    assert!(out.status.success(), "{out:?}");
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

// ── `test` ───────────────────────────────────────────────────────────

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

// ── Bare relative filename, no manifest ─────────────────────────────

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

// ── CLI contract ─────────────────────────────────────────────────────

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

#[test]
fn a_huge_range_count_is_a_catchable_error_not_an_abort() {
    let tree = TempTree::new();
    let file = tree.write("big.obk", "r ≔ ⍳(9223372036854775807)\n❧\n");
    let out = bin().arg("run").arg(file).output().unwrap();
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stderr).contains("at most"));
}

// ── Runtime limits ───────────────────────────────────────────────────

#[test]
fn runaway_recursion_is_a_clean_error_not_a_stack_overflow() {
    let tree = TempTree::new();
    let file = tree.write(
        "deep.obk",
        "\u{3BB}f(n: \u{27C1}): \u{27C1} \u{2192} 1 \u{271A} f(n)\nresult \u{2254} f(1)\n\u{2767}\n",
    );
    let out = bin().arg("run").arg(file).output().unwrap();
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stderr).contains("recursion limit exceeded"));
}

#[test]
fn recursion_through_a_native_callback_is_a_clean_error() {
    let tree = TempTree::new();
    let file = tree.write(
        "deep_native.obk",
        "λstep(n: ⟁): ⟁ → ⟡ (n ≡ 0) {\n  ◉ → 0\n  ⟢ ◎ → ⌽(add, 0, ⁝(n ☠︎ 1, ⊘))\n}\n\
         λadd(acc: ⟁, x: ⟁): ⟁ → acc ✚ step(x)\nresult ≔ step(5000000)\n❧\n",
    );
    let out = bin().arg("run").arg(file).output().unwrap();
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stderr).contains("recursion limit exceeded"));
}

// ── Process natives and bundling (`ADR-022`) ─────────────────────────

const ARGS_PROGRAM: &str = "\u{3BB}first(xs: List \u{25B7} \u{2318}): \u{2318} \u{2192} \
    \u{27E1} xs {\n  \u{2298} \u{2192} \u{22}none\u{22}\n  \u{27E2} \u{205D}(h, tl) \u{2192} h\n}\n\
    shown \u{2254} \u{2301}\u{2191}(first(\u{2301}\u{2193}\u{2325}(\u{2205})))\n\
    result \u{2254} 1\n\u{2767}\n";

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
fn bundled_program_is_a_standalone_executable_taking_its_own_arguments() {
    let tree = TempTree::new();
    let file = tree.write("args.obk", ARGS_PROGRAM);
    let exe = tree.root().join("tool");
    let built = bin()
        .arg("bundle")
        .arg(&file)
        .arg("-o")
        .arg(&exe)
        .output()
        .unwrap();
    assert!(built.status.success(), "{built:?}");
    // Source is gone: the executable must not depend on it.
    std::fs::remove_file(&file).unwrap();
    let out = std::process::Command::new(&exe)
        .args(["--version", "z"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    // Bundled programs do not print the final value, only their own output.
    assert_eq!(String::from_utf8(out.stdout).unwrap(), "--version\n");
}

#[test]
fn bundle_refuses_a_program_that_does_not_type_check() {
    let tree = TempTree::new();
    let file = tree.write("bad.obk", "x \u{2254} 1 \u{271A} \u{22}a\u{22}\n\u{2767}\n");
    let built = bin()
        .arg("bundle")
        .arg(&file)
        .arg("-o")
        .arg(tree.root().join("bad-out"))
        .output()
        .unwrap();
    assert_eq!(built.status.code(), Some(1));
    assert!(!tree.root().join("bad-out").exists());
}

#[test]
fn bundled_project_resolves_imports_and_reads_the_callers_directory() {
    let tree = TempTree::new();
    tree.manifest("proj", "main");
    tree.write(
        "proj/main.obk",
        "\u{27F2}strings\nshown \u{2254} \u{2301}\u{2191}(\u{2301}\u{2193}\u{232C}(\u{22}data.txt\u{22}) \
         \u{271A} greeting)\nresult \u{2254} 1\n\u{2767}\n",
    );
    tree.write(
        "proj/utils/strings.obk",
        "greeting \u{2254}\u{27f3} \u{22}!\u{22}\n\u{2767}\n",
    );
    let exe = tree.root().join("tool");
    let built = bin()
        .arg("bundle")
        .arg(tree.root().join("proj"))
        .arg("-o")
        .arg(&exe)
        .output()
        .unwrap();
    assert!(built.status.success(), "{built:?}");
    let cwd = tree.root().join("cwd");
    std::fs::create_dir_all(&cwd).unwrap();
    std::fs::write(cwd.join("data.txt"), "hello").unwrap();
    let out = std::process::Command::new(&exe)
        .current_dir(&cwd)
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    assert_eq!(String::from_utf8(out.stdout).unwrap(), "hello!\n");
}

#[cfg(unix)]
#[test]
fn bundled_program_writes_nothing_to_the_temp_directory() {
    let tree = TempTree::new();
    tree.manifest("proj", "main");
    tree.write(
        "proj/main.obk",
        "\u{27F2}strings\nshown \u{2254} \u{2301}\u{2191}(greeting)\nresult \u{2254} 1\n\u{2767}\n",
    );
    tree.write(
        "proj/strings.obk",
        "greeting \u{2254}\u{27f3} \u{22}hi\u{22}\n\u{2767}\n",
    );
    let exe = tree.root().join("tool");
    assert!(
        bin()
            .arg("bundle")
            .arg(tree.root().join("proj"))
            .arg("-o")
            .arg(&exe)
            .output()
            .unwrap()
            .status
            .success()
    );
    let scratch = tree.root().join("scratch");
    std::fs::create_dir_all(&scratch).unwrap();
    let out = std::process::Command::new(&exe)
        .env("TMPDIR", &scratch)
        .output()
        .unwrap();
    assert_eq!(String::from_utf8(out.stdout).unwrap(), "hi\n");
    assert_eq!(std::fs::read_dir(&scratch).unwrap().count(), 0);
}

// ── Text natives, stderr and exit codes (`ADR-023`) ──────────────────

#[test]
fn text_natives_split_trim_parse_and_render_end_to_end() {
    let tree = TempTree::new();
    // parts = ⫽(",", "a, b ,c"); shown = ⌁↑(⫴("-", ⟐(⌫, parts)))
    // n = ↦("12") unwrapped, printed with ⍕
    let src = "parts \u{2254} \u{2AFD}(\u{22},\u{22}, \u{22}a, b ,c\u{22})\n\
        joined \u{2254} \u{2AF4}(\u{22}-\u{22}, \u{27D0}(\u{232B}, parts))\n\
        a \u{2254} \u{2301}\u{2191}(joined)\n\
        n \u{2254} \u{27E1} \u{21A6}(\u{22}12\u{22}) {\n  \u{2713}(v) \u{2192} v\n  \
        \u{27E2} \u{2717}(m) \u{2192} 0\n}\n\
        b \u{2254} \u{2301}\u{2191}(\u{2355}(n \u{271A} 1))\n\
        result \u{2254} 1\n\u{2767}\n";
    let file = tree.write("text.obk", src);
    let out = bin().arg("run").arg(&file).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    assert_eq!(String::from_utf8(out.stdout).unwrap(), "a-b-c\n13\n1\n");
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

// ── Directories, existence, bytes, flags (`ADR-024`) ─────────────────

fn run_in(dir: &std::path::Path, src: &str) -> std::process::Output {
    let file = dir.join("prog.obk");
    std::fs::write(&file, src).unwrap();
    bin().arg("run").arg(&file).output().unwrap()
}

#[test]
fn directories_existence_and_bytes_round_trip_inside_the_root() {
    let tree = TempTree::new();
    let src = "a \u{2254} \u{2301}\u{2191}\u{2302}(\u{22}out/sub\u{22})\n\
        b \u{2254} \u{2301}\u{2191}\u{232C}(\u{22}out/sub/f.txt\u{22}, \u{22}x\u{22})\n\
        c \u{2254} \u{2301}\u{2191}\u{232D}(\u{22}out/b.bin\u{22}, ⌭⇋([0, 255, 10]))\n\
        d \u{2254} \u{2301}\u{2191}(\u{2AF4}(\u{22},\u{22}, \u{2301}\u{2193}\u{2302}(\u{22}out\u{22})))\n\
        e \u{2254} \u{2301}\u{2191}(\u{2355}(⌭#(\u{2301}\u{2193}\u{232D}(\u{22}out/b.bin\u{22}))))\n\
        f \u{2254} \u{27E1} \u{2301}\u{2370}\u{232C}(\u{22}out/sub/f.txt\u{22}) {\n  \
        \u{25C9} \u{2192} \u{2301}\u{2191}(\u{22}yes\u{22})\n  \
        \u{27E2} \u{25CE} \u{2192} \u{2301}\u{2191}(\u{22}no\u{22})\n}\n\
        g \u{2254} \u{27E1} \u{2301}\u{2370}\u{2302}(\u{22}out/b.bin\u{22}) {\n  \
        \u{25C9} \u{2192} \u{2301}\u{2191}(\u{22}dir\u{22})\n  \
        \u{27E2} \u{25CE} \u{2192} \u{2301}\u{2191}(\u{22}notdir\u{22})\n}\n\
        result \u{2254} 1\n\u{2767}\n";
    let out = run_in(tree.root(), src);
    assert!(out.status.success(), "{out:?}");
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        "b.bin,sub\n3\nyes\nnotdir\n1\n"
    );
    assert_eq!(
        std::fs::read(tree.root().join("out/b.bin")).unwrap(),
        [0, 255, 10]
    );
}

#[test]
fn directory_listing_cannot_escape_the_root() {
    let tree = TempTree::new();
    let src = "a \u{2254} \u{2301}\u{2193}\u{2302}(\u{22}..\u{22})\n\u{2767}\n";
    let out = run_in(tree.root(), src);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stderr).contains("PermissionDenied"));
}

#[test]
fn bytes_outside_0_to_255_are_rejected() {
    let tree = TempTree::new();
    let src = "a \u{2254} \u{2301}\u{2191}\u{232D}(\u{22}x.bin\u{22}, ⌭⇋([256]))\n\u{2767}\n";
    let out = run_in(tree.root(), src);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(!tree.root().join("x.bin").exists());
}

#[test]
fn flags_are_classified_for_the_program() {
    let tree = TempTree::new();
    let src = "\u{3BB}describe(a: ⌑): \u{2318} \u{2192} \u{27E1} a {\n  \
        ➤(k, v) \u{2192} \u{27E1} v {\n    \u{29B0} \u{2192} k\n    \
        \u{27E2} \u{29EB}(x) \u{2192} k \u{271A} \u{22}=\u{22} \u{271A} x\n  }\n  \
        \u{27E2} ◇(p) \u{2192} \u{22}_\u{22} \u{271A} p\n}\n\
        shown \u{2254} \u{2301}\u{2191}(\u{2AF4}(\u{22} \u{22}, \
        \u{27D0}(describe, \u{2691}(\u{2301}\u{2193}\u{2325}(\u{2205})))))\n\
        result \u{2254} 1\n\u{2767}\n";
    let file = tree.write("flags.obk", src);
    let out = bin()
        .arg("run")
        .arg(&file)
        .args(["--", "--depth=3", "-v", "file", "--", "--raw"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        "depth=3 v _file _--raw\n1\n"
    );
}

// ── Streams, clocks, randomness, file management (`ADR-027`) ─────────

fn run_with_stdin(dir: &std::path::Path, src: &str, input: &[u8]) -> std::process::Output {
    use std::io::Write;
    use std::process::Stdio;
    let file = dir.join("prog.obk");
    std::fs::write(&file, src).unwrap();
    let mut child = bin()
        .arg("run")
        .arg(&file)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn stdin_is_read_to_the_end_and_end_is_detectable() {
    let tree = TempTree::new();
    let src = "before ≔ ⌁⍰⎙(∅)\nall ≔ ⌁↓⎙(∅)\nafter ≔ ⌁⍰⎙(∅)\n\
        a ≔ ⌁↑⎙(⍕(before) ✚ \"|\" ✚ ⍕(⍴(all)) ✚ \"|\" ✚ ⍕(after))\nresult ≔ 1\n❧";
    let out = run_with_stdin(tree.root(), src, b"ab\ncd\n");
    assert!(out.status.success(), "{out:?}");
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        "◎|6|◉1\n",
        "no trailing newline from ⌁↑⎙, then the final value"
    );
}

#[test]
fn byte_streams_pass_non_utf8_through() {
    let tree = TempTree::new();
    let src = "a ≔ ⌁↑⎚(⌭⍪(⌁↓⎚(∅))(⌭⇋([0, 255])))\nresult ≔ 1\n❧";
    let out = run_with_stdin(tree.root(), src, &[1, 0xfe]);
    assert!(out.status.success(), "{out:?}");
    assert_eq!(&out.stdout[..4], &[1, 0xfe, 0, 255]);
}

#[test]
fn append_copy_move_size_and_remove_stay_inside_the_root() {
    let tree = TempTree::new();
    let src = "a ≔ ⌁⊕⌬(\"log.txt\")(\"ab\")\nb ≔ ⌁⊕⌬(\"log.txt\")(\"cd\")\n\
        c ≔ ⌁⧉⌬(\"log.txt\")(\"copy.txt\")\nd ≔ ⌁⇄⌬(\"copy.txt\")(\"moved.txt\")\n\
        e ≔ ⌁↑⌂(\"tree/deep\")\nf ≔ ⌁⊖⌂(\"tree\")\ng ≔ ⌁⊖⌬(\"log.txt\")\n\
        result ≔ ⌁⍰⌭(\"moved.txt\")\n❧";
    let out = run_in(tree.root(), src);
    assert!(out.status.success(), "{out:?}");
    assert_eq!(String::from_utf8(out.stdout).unwrap(), "4\n");
    assert!(!tree.root().join("log.txt").exists());
    assert!(!tree.root().join("tree").exists());
    assert_eq!(
        std::fs::read_to_string(tree.root().join("moved.txt")).unwrap(),
        "abcd"
    );
}

#[test]
fn removal_and_moves_cannot_escape_or_remove_the_root() {
    let tree = TempTree::new();
    for body in [
        "a ≔ ⌁⊖⌬(\"../x\")",
        "a ≔ ⌁⊖⌂(\".\")",
        "a ≔ ⌁⇄⌬(\"p.obk\")(\"../x\")",
    ] {
        let src = format!("{body}\nresult ≔ 1\n❧");
        let out = run_in(tree.root(), &src);
        assert_eq!(out.status.code(), Some(1), "{body}: {out:?}");
        assert!(String::from_utf8_lossy(&out.stderr).contains("PermissionDenied"));
    }
}

#[test]
fn clocks_and_dice_have_their_contracts() {
    let tree = TempTree::new();
    let src = "t0 ≔ ⌁↓⧖(∅)\ns ≔ ⌁↑⧗(5)\nt1 ≔ ⌁↓⧖(∅)\nw ≔ ⌁↓⧗(∅)\n\
        d ≔ ⌁↓⚀(6)\nu ≔ ⌁↓⚁(∅)\n\
        ok ≔ (t1 ≥ t0 ✚ 5000000) ∧ (w > 1600000000000) ∧ (d ≥ 0) ∧ (d < 6) ∧ (u ≥ 0.0) ∧ (u < 1.0)\n\
        result ≔ ⟡ ok { ◉ → 1 ⟢ ◎ → 0 }\n❧";
    let out = run_in(tree.root(), src);
    assert!(out.status.success(), "{out:?}");
    assert_eq!(String::from_utf8(out.stdout).unwrap(), "1\n");

    let bad = run_in(tree.root(), "a ≔ ⌁↓⚀(0)\nresult ≔ 1\n❧");
    assert_eq!(bad.status.code(), Some(1));
}

// ── Records, exceptions and the stdlib across modules (`ADR-025/026`) ──

#[test]
fn records_and_exceptions_cross_a_module_boundary() {
    let tree = TempTree::new();
    tree.manifest("proj", "main");
    tree.write(
        "proj/Shapes.obk",
        "Point ≔ { x: ⟁ ⟢ y: ⟁ }\n☈ BadPoint(⌘)\n\
         origin ≔⟳ Point { x: 0 ⟢ y: 0 }\n\
         λmove(p: Point): Point →⟳ p.{ x: p.x ✚ 1 }\n\
         λcheck(n: ⟁): ⟁ →⟳ ⟡ (n < 0) { ◉ → ☄ BadPoint(\"neg\") ⟢ ◎ → n }\n\
         names ≔⟳ ⁝(\"a\", ⊘)\n❧",
    );
    tree.write(
        "proj/main.obk",
        "⟲ Shapes\nmoved ≔ move(origin)\np ≔ Point { x: 7 ⟢ y: 8 }\n\
         r ≔ ☊ check(−1) λ(e) → ⟡ e { BadPoint(m) → 99 ⟢ _ → 0 }\n\
         result ≔ moved.x ✚ p.y ✚ r\n❧",
    );
    let out = bin()
        .arg("run")
        .arg(tree.root().join("proj"))
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    assert!(String::from_utf8(out.stdout).unwrap().contains("108"));
}

// ── Runtime traces and long lists (`ADR-028`) ────────────────────────

#[test]
fn an_uncaught_exception_reports_the_call_chain() {
    let tree = TempTree::new();
    let src = "λdiv(a: ⟁, b: ⟁): ⟁ → a ÷ b\nλmid(n: ⟁): ⟁ → div(10, n) ✚ 1\n\
        λtop(n: ⟁): ⟁ → mid(n) ✚ 1\nresult ≔ top(0)\n❧";
    let out = run_in(tree.root(), src);
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8(out.stderr).unwrap();
    let div = stderr.find("called 'div'").expect(&stderr);
    let mid = stderr.find("called 'mid'").expect(&stderr);
    let top = stderr.find("called 'top'").expect(&stderr);
    assert!(div < mid && mid < top, "innermost first: {stderr}");
}

#[test]
fn a_caught_exception_leaves_no_stale_trace() {
    let tree = TempTree::new();
    let src = "λdiv(a: ⟁, b: ⟁): ⟁ → a ÷ b\n\
        safe ≔ ☊ div(1, 0) { _ → 0 }\nfail ≔ ☄ Failure(\"x\", \"y\")\nresult ≔ safe\n❧";
    let out = run_in(tree.root(), src);
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(!stderr.contains("called 'div'"), "{stderr}");
}

#[test]
fn list_combinators_run_over_a_million_elements() {
    let tree = TempTree::new();
    let src = "λbuild(i: ⟁, acc: List ▷ ⟁): List ▷ ⟁ → ⟡ (i ≡ 0) { ◉ → acc ⟢ ◎ → build(i ☠︎ 1, ⁝(i, acc)) }\n\
        ys ≔ ⟐(λ(n: ⟁) → n ✚ 1, build(1000000, ⊘))\n\
        result ≔ ⌽(λ(a: ⟁, n: ⟁) → a ✚ n, 0, ⌿(λ(n: ⟁) → (n ⌗ 2) ≡ 0, ys))\n❧";
    let out = run_in(tree.root(), src);
    assert!(out.status.success(), "{out:?}");
    assert_eq!(String::from_utf8(out.stdout).unwrap(), "250000500000\n");
}

// ── Manifest format 2: capabilities, entry path, bundle output (`ADR-029`) ──

#[test]
fn a_denied_capability_is_a_static_error_in_check() {
    let tree = TempTree::new();
    tree.write(
        "p/obfusku.toml",
        "format = 2\nentry = \"main\"\n[capabilities]\nconsole = true\n",
    );
    tree.write("p/main.obk", "e ≔ ⌁↓⎈(\"HOME\")\n❧\n");
    let out = bin()
        .arg("check")
        .arg(tree.root().join("p"))
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("needs the 'env' capability"), "{stderr}");
}

#[test]
fn a_denied_capability_in_an_imported_module_raises_permission_denied_at_runtime() {
    let tree = TempTree::new();
    tree.write(
        "p/obfusku.toml",
        "format = 2\nentry = \"main\"\n[capabilities]\nconsole = true\n",
    );
    tree.write("p/lib.obk", "e ≔⟳ ⌁↓⎈(\"HOME\")\n❧\n");
    tree.write("p/main.obk", "⟲ lib\nr ≔ 1\n❧\n");
    let out = bin()
        .arg("run")
        .arg(tree.root().join("p"))
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stderr).contains("PermissionDenied"));
}

#[test]
fn without_a_capabilities_section_everything_is_allowed() {
    let tree = TempTree::new();
    tree.write("p/obfusku.toml", "format = 2\nentry = \"main\"\n");
    tree.write("p/main.obk", "e ≔ ⌁↓⎈(\"PATH\")\n❧\n");
    let out = bin()
        .arg("run")
        .arg(tree.root().join("p"))
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
}

#[test]
fn an_entry_path_disambiguates_what_a_module_name_cannot() {
    let tree = TempTree::new();
    tree.write(
        "p/obfusku.toml",
        "format = 2\n[entry]\npath = \"app/main.obk\"\n",
    );
    tree.write("p/app/main.obk", "r ≔ 1\n❧\n");
    tree.write("p/other/main.obk", "r ≔ 2\n❧\n");
    let out = bin()
        .arg("run")
        .arg(tree.root().join("p"))
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "1");
}

#[test]
fn an_unknown_key_in_a_format_2_manifest_is_rejected_with_a_suggestion() {
    let tree = TempTree::new();
    tree.write("p/obfusku.toml", "format = 2\nentrty = \"main\"\n");
    tree.write("p/main.obk", "r ≔ 1\n❧\n");
    let out = bin()
        .arg("run")
        .arg(tree.root().join("p"))
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stderr).contains("did you mean 'entry'"));
}

#[test]
fn a_path_dependency_supplies_modules_and_a_bundle_carries_them() {
    let tree = TempTree::new();
    tree.write("greet/obfusku.toml", "format = 2\nentry = \"greet\"\n");
    tree.write("greet/greet.obk", "word ≔⟳ \"from-dep\"\n❧\n");
    tree.write("greet/tests/greet.obk", "word ≔⟳ \"WRONG\"\n❧\n");
    tree.write(
        "app/obfusku.toml",
        "format = 2\nentry = \"main\"\n[bundle]\noutput = \"appbin\"\n\
         [dependencies]\ngreet = { path = \"../greet\" }\n",
    );
    tree.write("app/main.obk", "⟲ greet\nr ≔ ⌁↑(word)\n❧\n");
    let app = tree.root().join("app");

    let run = bin().arg("run").arg(&app).output().unwrap();
    assert!(run.status.success(), "{run:?}");
    assert!(String::from_utf8_lossy(&run.stdout).contains("from-dep"));

    let out = bin()
        .arg("bundle")
        .arg(&app)
        .current_dir(tree.root())
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let exe = tree.root().join("appbin");
    assert!(exe.exists());
    // The dependency's sources travel inside the executable.
    std::fs::remove_dir_all(tree.root().join("greet")).unwrap();
    let ran = std::process::Command::new(&exe).output().unwrap();
    assert!(ran.status.success(), "{ran:?}");
    assert_eq!(String::from_utf8_lossy(&ran.stdout).trim(), "from-dep");
}

#[test]
fn fetch_pins_a_git_dependency_and_run_works_offline_from_the_lock() {
    if std::process::Command::new("git")
        .arg("--version")
        .output()
        .is_err()
    {
        return;
    }
    let tree = TempTree::new();
    let dep = tree.root().join("dep");
    std::fs::create_dir_all(&dep).unwrap();
    std::fs::write(dep.join("obfusku.toml"), "format = 2\nentry = \"util\"\n").unwrap();
    std::fs::write(dep.join("util.obk"), "word ≔⟳ \"pinned\"\n❧\n").unwrap();
    let git = |args: &[&str]| {
        let st = std::process::Command::new("git")
            .args([
                "-c",
                "user.email=t@t",
                "-c",
                "user.name=t",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .current_dir(&dep)
            .output()
            .unwrap();
        assert!(st.status.success(), "{st:?}");
    };
    git(&["init", "-q"]);
    git(&["add", "-A"]);
    git(&["commit", "-qm", "init"]);
    git(&["tag", "v1.0.0"]);

    let url = format!("file://{}", dep.display());
    tree.write(
        "app/obfusku.toml",
        &format!(
            "format = 2\nentry = \"main\"\n[dependencies]\nutil = {{ git = \"{url}\", tag = \"v1.0.0\" }}\n"
        ),
    );
    tree.write("app/main.obk", "⟲ util\nr ≔ word\n❧\n");
    let app = tree.root().join("app");
    let home = tree.root().join("home");

    let before = bin()
        .env("OBFUSKU_HOME", &home)
        .arg("run")
        .arg(&app)
        .output()
        .unwrap();
    assert_eq!(before.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&before.stderr).contains("obfusku fetch"));

    let fetched = bin()
        .env("OBFUSKU_HOME", &home)
        .arg("fetch")
        .arg(&app)
        .output()
        .unwrap();
    assert!(fetched.status.success(), "{fetched:?}");
    let lock = std::fs::read_to_string(app.join("obfusku.lock")).unwrap();
    assert!(
        lock.contains("tag:v1.0.0") && lock.contains("rev = \""),
        "{lock}"
    );

    // The origin disappears; the pinned copy in the cache is enough.
    std::fs::remove_dir_all(&dep).unwrap();
    let after = bin()
        .env("OBFUSKU_HOME", &home)
        .arg("run")
        .arg(&app)
        .output()
        .unwrap();
    assert!(after.status.success(), "{after:?}");
    assert_eq!(String::from_utf8_lossy(&after.stdout).trim(), "\"pinned\"");
}
