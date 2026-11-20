//! Capabilities declared in the manifest: static errors in the entry module, runtime denial elsewhere (`ADR-029`).

use crate::support::{TempTree, bin};

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
