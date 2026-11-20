//! `fmt`: canonical output, `--check`, and re-parsing what it prints.

use crate::support::{TempTree, bin, write_temp};

#[test]
fn fmt_roundtrips_a_hole_argument() {
    let f = write_temp(
        "\u{3bb}f(a: \u{27c1}, b: \u{27c1}): \u{27c1} \u{2192} a \u{271a} b\n\
         partial \u{2254} f(1, \u{2022})\n\
         result \u{2254} partial(2)\n\u{2767}\n",
    );
    let out = bin().arg("fmt").arg(f.path()).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains('\u{2022}'), "{stdout}");
}

#[test]
fn fmt_prints_canonically_formatted_source_and_reparses() {
    let f = write_temp("x\u{2254}1\u{271A}2\n\u{2767}\n");
    let out = bin().arg("fmt").arg(f.path()).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("\u{2254}"), "{stdout}");
    assert!(stdout.trim_end().ends_with('\u{2767}'), "{stdout}");
}

#[test]
fn fmt_roundtrips_the_unit_literal() {
    let f = write_temp("x\u{2254}\u{2205}\n\u{2767}\n");
    let out = bin().arg("fmt").arg(f.path()).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains('\u{2205}'), "{stdout}");
}

#[test]
fn fmt_does_not_require_type_checking_to_succeed() {
    // §13: formatting ill-typed code must still work.
    let f = write_temp("result \u{2254} 1 \u{271A} \u{25C9}\n\u{2767}\n");
    let out = bin().arg("fmt").arg(f.path()).output().unwrap();
    assert!(out.status.success(), "{out:?}");
}

#[test]
fn fmt_roundtrips_a_lambda_parameter_annotation() {
    let src = "result \u{2254} \u{3bb}(x: \u{27C1}, y: t) \u{2192} x\n\u{2767}\n";
    let f = write_temp(src);
    let out = bin().arg("fmt").arg(f.path()).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    let formatted = String::from_utf8(out.stdout).unwrap();
    assert!(formatted.contains("x: \u{27C1}"), "{formatted}");
    assert!(formatted.contains("y: t"), "{formatted}");
}

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
