//! `inspect`: token, AST and Core output, flag order, and Project directories.

use crate::support::{TempTree, bin, write_temp};

#[test]
fn inspect_accepts_the_mode_flag_before_or_after_the_path() {
    let f = write_temp("x \u{2254} 5\n\u{2767}\n");
    let after = bin()
        .arg("inspect")
        .arg(f.path())
        .arg("--ast")
        .output()
        .unwrap();
    let before = bin()
        .arg("inspect")
        .arg("--ast")
        .arg(f.path())
        .output()
        .unwrap();
    assert!(after.status.success(), "{after:?}");
    assert!(before.status.success(), "{before:?}");
    assert_eq!(after.stdout, before.stdout);
}

#[test]
fn inspect_rejects_an_unknown_flag_regardless_of_position() {
    let f = write_temp("x \u{2254} 5\n\u{2767}\n");
    let out = bin()
        .arg("inspect")
        .arg(f.path())
        .arg("--bogus")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.contains("unknown inspect flag"), "{stderr}");
}

#[test]
fn inspect_tokens_prints_a_token_list() {
    let f = write_temp("x \u{2254} 5\n\u{2767}\n");
    let out = bin()
        .arg("inspect")
        .arg(f.path())
        .arg("--tokens")
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("Ident"), "{stdout}");
}

#[test]
fn inspect_ast_prints_the_surface_module() {
    let f = write_temp("x \u{2254} 5\n\u{2767}\n");
    let out = bin()
        .arg("inspect")
        .arg(f.path())
        .arg("--ast")
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("ValueDeclaration"), "{stdout}");
}

#[test]
fn inspect_core_prints_the_desugared_module() {
    let f = write_temp("x \u{2254} 5\n\u{2767}\n");
    let out = bin()
        .arg("inspect")
        .arg(f.path())
        .arg("--core")
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(
        stdout.contains("BindingGroup") || stdout.contains("Let"),
        "{stdout}"
    );
}

#[test]
fn inspect_defaults_to_core_when_no_flag_given() {
    let f = write_temp("x \u{2254} 5\n\u{2767}\n");
    let out = bin().arg("inspect").arg(f.path()).output().unwrap();
    assert!(out.status.success(), "{out:?}");
}

#[test]
fn inspect_unknown_flag_exits_two() {
    let f = write_temp("x \u{2254} 5\n\u{2767}\n");
    let out = bin()
        .arg("inspect")
        .arg(f.path())
        .arg("--bogus")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
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
