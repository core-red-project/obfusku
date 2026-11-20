//! File and directory natives confined to the project root (`ADR-011`, `ADR-019`, `ADR-024`).

use crate::support::{TempObkDir, TempTree, bin, run_in};

#[test]
fn write_then_read_a_file_within_the_entry_directory() {
    let dir = TempObkDir::new();
    let main = dir.write(
        "main",
        "unused \u{2254} ⌁↑⌬(\u{22}data.txt\u{22})(\u{22}hello from obfusku\u{22})\n\
         result \u{2254} ⌁↓⌬(\u{22}data.txt\u{22})\n\u{2767}\n",
    );
    let out = bin().arg("run").arg(&main).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("hello from obfusku"), "{stdout}");
    assert_eq!(
        std::fs::read_to_string(dir.0.join("data.txt")).unwrap(),
        "hello from obfusku"
    );
}

#[test]
fn read_file_with_a_parent_traversal_path_is_permission_denied() {
    let dir = TempObkDir::new();
    let main = dir.write(
        "main",
        "result \u{2254} ⌁↓⌬(\u{22}../secret.txt\u{22})\n\u{2767}\n",
    );
    let out = bin().arg("run").arg(&main).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.contains("PermissionDenied"), "{stderr}");
}

#[test]
fn read_file_with_an_absolute_path_is_permission_denied() {
    let dir = TempObkDir::new();
    let main = dir.write(
        "main",
        "result \u{2254} ⌁↓⌬(\u{22}/etc/passwd\u{22})\n\u{2767}\n",
    );
    let out = bin().arg("run").arg(&main).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.contains("PermissionDenied"), "{stderr}");
}

#[test]
fn a_denied_filesystem_access_is_catchable_by_the_existing_mechanism() {
    let dir = TempObkDir::new();
    let main = dir.write(
        "main",
        "result \u{2254} \u{260a} (⌁↓⌬(\u{22}/etc/passwd\u{22})) \u{3bb}(e) \u{2192} \u{27e1} e {\n  \
         Failure(tag, msg) \u{2192} tag\n  \u{27e2} DivisionByZero \u{2192} \u{22}div0\u{22}\n  \
         \u{27e2} NonExhaustiveMatch \u{2192} \u{22}nem\u{22}\n  \
         \u{27e2} InvalidOperation(m) \u{2192} m\n  \u{27e2} IntegerOverflow \u{2192} \u{22}overflow\u{22}\n  \
         \u{27e2} _ \u{2192} \u{22}other\u{22}\n}\n\u{2767}\n",
    );
    let out = bin().arg("run").arg(&main).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("PermissionDenied"), "{stdout}");
}

#[test]
fn read_file_in_a_subdirectory_of_the_entry_directory_is_permitted() {
    let dir = TempObkDir::new();
    std::fs::create_dir_all(dir.0.join("sub")).unwrap();
    std::fs::write(dir.0.join("sub").join("nested.txt"), "nested content").unwrap();
    let main = dir.write(
        "main",
        "result \u{2254} ⌁↓⌬(\u{22}sub/nested.txt\u{22})\n\u{2767}\n",
    );
    let out = bin().arg("run").arg(&main).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("nested content"), "{stdout}");
}

#[test]
fn read_file_that_does_not_exist_is_an_io_error_not_a_crash() {
    let dir = TempObkDir::new();
    let main = dir.write(
        "main",
        "result \u{2254} ⌁↓⌬(\u{22}does-not-exist.txt\u{22})\n\u{2767}\n",
    );
    let out = bin().arg("run").arg(&main).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.contains("IOError"), "{stderr}");
}

#[test]
#[cfg(unix)]
fn a_real_os_permission_error_reads_as_permission_denied_end_to_end() {
    // Distinct from the lexical-scoping denials above (a path that never
    // reaches the filesystem at all): a real file *inside* the allowed
    // root, with its own read permission revoked via `chmod`, so the
    // failure genuinely originates from the OS. Environment-dependent:
    // skipped, not falsely asserted, when running as root (which ignores
    // Unix permission bits) — a skip here means no evidence was
    // collected in that environment, not that the behavior was verified.
    use std::os::unix::fs::PermissionsExt;

    let dir = TempObkDir::new();
    let target = dir.0.join("no-read.txt");
    std::fs::write(&target, "secret").unwrap();
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o000)).unwrap();
    if std::fs::read_to_string(&target).is_ok() {
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o644)).unwrap();
        eprintln!(
            "SKIPPED: a_real_os_permission_error_reads_as_permission_denied_end_to_end \
             requires non-root execution — running as root ignores Unix permission bits, \
             so this environment cannot produce the permission boundary this test exists \
             to exercise. This is not positive evidence that permission-error \
             classification is correct here; it means no such evidence was collected."
        );
        return;
    }

    let main = dir.write(
        "main",
        "result \u{2254} ⌁↓⌬(\u{22}no-read.txt\u{22})\n\u{2767}\n",
    );
    let out = bin().arg("run").arg(&main).output().unwrap();
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.contains("PermissionDenied"), "{stderr}");
}

#[test]
fn a_local_binding_may_shadow_the_read_file_name() {
    let dir = TempObkDir::new();
    let main = dir.write("main", "⌁↓⌬ \u{2254} 5\nresult \u{2254} ⌁↓⌬\n\u{2767}\n");
    let out = bin().arg("check").arg(&main).output().unwrap();
    assert!(out.status.success(), "{out:?}");
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
