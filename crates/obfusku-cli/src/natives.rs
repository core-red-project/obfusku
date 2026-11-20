//! Host-provided natives — the runtime ↔ host boundary (`ADR-010`).
//!
//! `obfusku-runtime` never touches `std::io`, the clock or the environment
//! itself (see [`obfusku_runtime::value::NativeFn`]). This crate is the
//! host: it builds each [`obfusku_runtime::Value::Native`] closure and
//! hand-writes a [`obfusku_typecheck::Scheme`] for it, then seeds both into
//! the same ambient prelude [`crate::stdlib::load`] fills. To the checker
//! and the evaluator a native is an ordinary prelude value, which is what
//! keeps the language core free of any host knowledge.
//!
//! **There is no compiler-checked link between a native's `Scheme` and its
//! Rust behavior.** A scheme that lies is caught only by tests, so every
//! family keeps tests that pair the registered type with an actual call.
//!
//! # Families
//!
//! | Module | Natives | Needs |
//! |---|---|---|
//! | `process` | console lines, arguments, environment, exit | arguments |
//! | `streams` | stdin/stdout streams, clocks, dice (`ADR-027`) | – |
//! | `filesystem` | files, directories, bytes (`ADR-011`, `ADR-024`) | a root |
//! | `collections` | `List`/`Array`/`Map`/`Set` (`ADR-021`, `ADR-025`) | – |
//! | `numeric`, `text`, `bytes` | conversions and combinators (`ADR-023`, `ADR-027`) | – |
//!
//! `capability` filters the result by what `obfusku.toml` grants
//! (`ADR-029`); `marshal` and `registry` are the shared plumbing.
//!
//! The filesystem family is optional: the REPL has no entry file to scope
//! a root against, so it simply does not get one.

pub mod bytes;
pub mod capability;
pub mod collections;
pub mod filesystem;
pub mod marshal;
pub mod numeric;
pub mod process;
pub mod registry;
pub mod streams;
pub mod text;

use obfusku_runtime::Value;
use obfusku_typecheck::Prelude;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// A family's contribution to the ambient prelude: type schemes and values.
pub type Natives = (Prelude, HashMap<String, Value>);

/// Every native a program may see.
///
/// `fs_root` scopes the filesystem family; `None` leaves it out. `args` are
/// the program arguments behind `⌁↓⌥`.
#[must_use]
pub fn load(fs_root: Option<&Path>, args: &[String]) -> Natives {
    let mut families = vec![
        process::load(args),
        streams::load(),
        collections::load(),
        numeric::load(),
        text::load(),
        bytes::load(),
    ];
    if let Some(root) = fs_root {
        // Canonicalized once: symlinks in the root itself are resolved here
        // so every later comparison is against one stable path.
        let root = Rc::new(root.canonicalize().unwrap_or_else(|_| PathBuf::from(root)));
        families.push(filesystem::load(&root));
    }
    let mut types = Prelude::new();
    let mut values = HashMap::new();
    for (family_types, family_values) in families {
        types.extend(family_types);
        values.extend(family_values);
    }
    (types, values)
}

#[cfg(test)]
mod tests {
    use super::marshal::failure;
    use super::*;
    use obfusku_core::types::Type as CoreType;
    use obfusku_runtime::Text;
    use obfusku_runtime::eval::Raised;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// The full host set rooted at `root`, as the CLI assembles it.
    fn load(root: &Path) -> Natives {
        super::load(Some(root), &[])
    }

    /// A fresh temp directory per test, used as the filesystem root.
    fn test_root() -> PathBuf {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("obfusku-natives-test-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create temp root");
        dir
    }

    /// The one guarantee `check_with_prelude` can never verify on its
    /// own: that `print`'s hand-written `Scheme` (`Str → Unit`) matches
    /// what its `Value::Native` closure actually does when called.
    #[test]
    fn print_scheme_matches_actual_native_behavior() {
        let (types, values) = load(&test_root());
        let scheme = types.get("⌁↑").expect("print must be registered");
        assert_eq!(
            scheme.ty.to_core(),
            Some(CoreType::Function(
                Box::new(CoreType::Str),
                Box::new(CoreType::Unit)
            )),
        );

        let Some(Value::Native(native)) = values.get("⌁↑") else {
            panic!("expected print to be a Value::Native");
        };
        let result = (native.func)(Value::Str(Text::from("hello")));
        assert!(matches!(result, Ok(Value::Unit)));
    }

    #[test]
    fn print_native_rejects_a_non_str_argument_without_panicking() {
        let (_, values) = load(&test_root());
        let Some(Value::Native(native)) = values.get("⌁↑") else {
            panic!("expected print to be a Value::Native");
        };
        let result = (native.func)(Value::Int(5));
        assert!(result.is_err(), "a mistyped call must raise, never panic");
    }

    #[test]
    fn read_line_scheme_is_unit_to_str() {
        // Real-stdin behavior (a genuine line, and real EOF) is covered
        // by `obfusku-cli`'s CLI E2E tests with piped stdin — this
        // crate's own process-global `std::io::stdin()` can't be
        // swapped out for a fake per-test here.
        let (types, _) = load(&test_root());
        let scheme = types.get("⌁↓").expect("readLine must be registered");
        assert_eq!(
            scheme.ty.to_core(),
            Some(CoreType::Function(
                Box::new(CoreType::Unit),
                Box::new(CoreType::Str)
            )),
        );
    }

    #[test]
    fn read_line_native_rejects_a_non_unit_argument_without_panicking() {
        let (_, values) = load(&test_root());
        let Some(Value::Native(native)) = values.get("⌁↓") else {
            panic!("expected readLine to be a Value::Native");
        };
        let result = (native.func)(Value::Int(5));
        assert!(result.is_err(), "a mistyped call must raise, never panic");
    }

    #[test]
    fn failure_helper_matches_the_built_in_failure_shape() {
        // `Failure`'s two-`Str`-field shape (`SEMANTIC_CORE.md` §15.2) —
        // this must stay in sync with `obfusku-typecheck::adt`'s
        // hard-registered `AdtInfo` for `Failure`.
        let v = failure("EOF", "end of input");
        match &v {
            Value::Adt { tag, args } => {
                assert_eq!(&**tag, "Failure");
                assert_eq!(args.len(), 2);
                assert!(matches!(&args[0], Value::Str(s) if &**s == "EOF"));
                assert!(matches!(&args[1], Value::Str(s) if &**s == "end of input"));
            }
            other => panic!("expected a Failure Adt value, got {other:?}"),
        }
    }

    #[test]
    fn read_write_schemes_match_the_curried_signature() {
        let (types, _) = load(&test_root());
        assert_eq!(
            types.get("⌁↓⌬").unwrap().ty.to_core(),
            Some(CoreType::Function(
                Box::new(CoreType::Str),
                Box::new(CoreType::Str)
            )),
        );
        assert_eq!(
            types.get("⌁↑⌬").unwrap().ty.to_core(),
            Some(CoreType::Function(
                Box::new(CoreType::Str),
                Box::new(CoreType::Function(
                    Box::new(CoreType::Str),
                    Box::new(CoreType::Unit)
                )),
            )),
        );
    }

    #[test]
    fn write_then_read_round_trips_within_the_root() {
        let root = test_root();
        let (_, values) = load(&root);
        let Some(Value::Native(write)) = values.get("⌁↑⌬") else {
            panic!("expected writeFile to be a Value::Native");
        };
        let applied = (write.func)(Value::Str(Text::from("greeting.txt"))).expect("apply path");
        let Value::Native(write_content) = &applied else {
            panic!("expected writeFile(path) to return a Value::Native");
        };
        let result = (write_content.func)(Value::Str(Text::from("hello, file")));
        assert!(matches!(result, Ok(Value::Unit)), "{result:?}");

        let Some(Value::Native(read)) = values.get("⌁↓⌬") else {
            panic!("expected readFile to be a Value::Native");
        };
        let contents = (read.func)(Value::Str(Text::from("greeting.txt")));
        assert!(
            matches!(&contents, Ok(Value::Str(s)) if &**s == "hello, file"),
            "{contents:?}"
        );
    }

    #[test]
    fn read_file_outside_the_root_via_parent_traversal_is_permission_denied() {
        let (_, values) = load(&test_root());
        let Some(Value::Native(read)) = values.get("⌁↓⌬") else {
            panic!("expected readFile to be a Value::Native");
        };
        let result = (read.func)(Value::Str(Text::from("../secret.txt")));
        match &result {
            Err(Raised(Value::Adt { tag, args })) => {
                assert_eq!(&**tag, "Failure");
                assert!(matches!(&args[0], Value::Str(s) if &**s == "PermissionDenied"));
            }
            other => panic!("expected a PermissionDenied Failure, got {other:?}"),
        }
    }

    #[test]
    fn read_file_with_an_absolute_path_is_permission_denied() {
        let (_, values) = load(&test_root());
        let Some(Value::Native(read)) = values.get("⌁↓⌬") else {
            panic!("expected readFile to be a Value::Native");
        };
        let result = (read.func)(Value::Str(Text::from("/etc/passwd")));
        match &result {
            Err(Raised(Value::Adt { tag, args })) => {
                assert_eq!(&**tag, "Failure");
                assert!(matches!(&args[0], Value::Str(s) if &**s == "PermissionDenied"));
            }
            other => panic!("expected a PermissionDenied Failure, got {other:?}"),
        }
    }

    #[test]
    fn a_deeper_relative_subpath_within_the_root_is_permitted() {
        let root = test_root();
        std::fs::create_dir_all(root.join("sub")).unwrap();
        let (_, values) = load(&root);
        let Some(Value::Native(write)) = values.get("⌁↑⌬") else {
            panic!("expected writeFile to be a Value::Native");
        };
        let applied = (write.func)(Value::Str(Text::from("sub/nested.txt"))).expect("apply path");
        let Value::Native(write_content) = &applied else {
            panic!("expected writeFile(path) to return a Value::Native");
        };
        let result = (write_content.func)(Value::Str(Text::from("nested")));
        assert!(matches!(result, Ok(Value::Unit)), "{result:?}");
        assert!(root.join("sub/nested.txt").exists());
    }

    #[test]
    fn read_file_that_does_not_exist_is_an_io_error_not_permission_denied() {
        let (_, values) = load(&test_root());
        let Some(Value::Native(read)) = values.get("⌁↓⌬") else {
            panic!("expected readFile to be a Value::Native");
        };
        let result = (read.func)(Value::Str(Text::from("does-not-exist.txt")));
        match &result {
            Err(Raised(Value::Adt { tag, args })) => {
                assert_eq!(&**tag, "Failure");
                assert!(matches!(&args[0], Value::Str(s) if &**s == "IOError"));
            }
            other => panic!("expected an IOError Failure, got {other:?}"),
        }
    }

    /// A genuine OS-level permission failure — distinct from every other
    /// test in this file, which only exercises `resolve_within_root`'s
    /// *lexical* rejection (a path that never reaches the filesystem at
    /// all). This one creates a real file *inside* the allowed root
    /// (so lexical resolution succeeds) and revokes read permission on
    /// it via a real `chmod`, so the failure genuinely originates from
    /// `std::fs::read_to_string`'s own `Err`, not from
    /// `resolve_within_root`. Unix-only (`chmod` semantics), and
    /// skipped rather than falsely asserted when running as root, since
    /// root ignores Unix permission bits entirely — asserting the tag
    /// in that case would be testing "did we run as root," not "does a
    /// real OS permission error map to the right tag."
    #[cfg(unix)]
    #[test]
    fn a_real_os_permission_error_maps_to_permission_denied_not_io_error() {
        use std::os::unix::fs::PermissionsExt;

        let root = test_root();
        let path = root.join("no-read.txt");
        std::fs::write(&path, "secret").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();

        // Running as root ignores permission bits — chmod 0 still reads
        // successfully. Confirm that's actually what's happening before
        // skipping, rather than silently swallowing a real failure.
        if std::fs::read_to_string(&path).is_ok() {
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
            eprintln!("SKIPPED: permission test requires non-root execution.");
            return;
        }

        let (_, values) = load(&root);
        let Some(Value::Native(read)) = values.get("⌁↓⌬") else {
            panic!("expected readFile to be a Value::Native");
        };
        let result = (read.func)(Value::Str(Text::from("no-read.txt")));
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        match &result {
            Err(Raised(Value::Adt { tag, args })) => {
                assert_eq!(&**tag, "Failure");
                assert!(
                    matches!(&args[0], Value::Str(s) if &**s == "PermissionDenied"),
                    "expected PermissionDenied for a genuine OS permission error, got {:?}",
                    args[0]
                );
            }
            other => panic!("expected a PermissionDenied Failure, got {other:?}"),
        }
    }
}
