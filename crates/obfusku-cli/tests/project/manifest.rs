//! `obfusku.toml`: validation of format 1 and format 2, entry paths and unknown keys (`ADR-020`, `ADR-029`).

use crate::support::{TempTree, bin};

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
