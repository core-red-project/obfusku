//! `bundle`: a program as a standalone executable (`ADR-022`).

use crate::support::{ARGS_PROGRAM, TempTree, bin};

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
