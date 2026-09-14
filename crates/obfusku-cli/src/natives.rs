//! Host-provided native capabilities — the runtime-↔-host boundary
//! decided for I/O: `obfusku-runtime` never touches `std::io` itself
//! (see [`obfusku_runtime::value::NativeFn`]'s own doc comment); this
//! crate, as the host, constructs each [`obfusku_runtime::Value::Native`]
//! closure and hand-authors a [`Scheme`] for it, then seeds both into
//! the same ambient prelude `stdlib::load` already populates — a native
//! is otherwise an ordinary prelude value, indistinguishable to
//! `check_with_prelude`/`evaluate_with_prelude` from a `.obk`-sourced
//! one.
//!
//! Unlike stdlib source, there is no compiler-checked link between a
//! native's hand-written `Scheme` and its actual Rust behavior — that
//! must be verified by tests instead (see this module's own tests).
//!
//! Standard native functions: `print`, `readLine`, `readFile`, and `writeFile`.
//!
//! `readLine : ∅ → ⌘` takes the Unit value (`∅`) as its argument, since
//! `Apply` is arity-1. EOF or read error raises built-in `Failure`.
//!
//! **Filesystem capability model** (`readFile`/`writeFile`):
//! - Paths are strings validated at runtime by the closure.
//! - The filesystem root is the entry file's directory (`base_dir`).
//! - Escapes (`..` or absolute paths outside root) raise `Failure("PermissionDenied", ...)`.
//! - Capabilities live in closure-captured state, invisible to Obfusku source.

use obfusku_core::types::Type as CoreType;
use obfusku_runtime::eval::Raised;
use obfusku_runtime::{NativeFn, Value};
use obfusku_typecheck::{Prelude, Scheme};
use std::collections::HashMap;
use std::io::BufRead;
use std::path::{Component, Path, PathBuf};
use std::rc::Rc;

/// `print`/`readLine` only — the two natives with no filesystem-root
/// question at all. Split out from [`load`] so a caller with no single
/// "entry file" to scope a filesystem capability against (the REPL,
/// which has no file on disk at all) can still get ordinary I/O without
/// this crate inventing a filesystem policy for that caller — that
/// question stays with the entry-file-based `load` below, deliberately
/// out of scope here (`LANGUAGE_SPEC.md` §5's still-open artifact
/// model).
pub fn load_io_only() -> (Prelude, HashMap<String, Value>) {
    let mut types = Prelude::new();
    let mut values = HashMap::new();

    types.insert(
        "print".to_string(),
        Scheme::monomorphic(obfusku_typecheck::instantiate_core_type(
            &CoreType::Function(Box::new(CoreType::Str), Box::new(CoreType::Unit)),
            &HashMap::new(),
        )),
    );
    values.insert("print".to_string(), Value::Native(Rc::new(print_native())));

    types.insert(
        "readLine".to_string(),
        Scheme::monomorphic(obfusku_typecheck::instantiate_core_type(
            &CoreType::Function(Box::new(CoreType::Unit), Box::new(CoreType::Str)),
            &HashMap::new(),
        )),
    );
    values.insert(
        "readLine".to_string(),
        Value::Native(Rc::new(read_line_native())),
    );

    (types, values)
}

/// Every native's `Scheme` and `Value`, ready to seed prelude environments.
/// `base_dir` becomes the filesystem root `readFile`/`writeFile` are scoped to.
pub fn load(base_dir: &Path) -> (Prelude, HashMap<String, Value>) {
    let (mut types, mut values) = load_io_only();

    // Canonicalized once here: resolves symlinks in `base_dir` itself so
    // subsequent operations compare against a consistent root.
    let root: Rc<PathBuf> = Rc::new(
        base_dir
            .canonicalize()
            .unwrap_or_else(|_| base_dir.to_path_buf()),
    );

    types.insert(
        "readFile".to_string(),
        Scheme::monomorphic(obfusku_typecheck::instantiate_core_type(
            &CoreType::Function(Box::new(CoreType::Str), Box::new(CoreType::Str)),
            &HashMap::new(),
        )),
    );
    values.insert(
        "readFile".to_string(),
        Value::Native(Rc::new(read_file_native(Rc::clone(&root)))),
    );

    types.insert(
        "writeFile".to_string(),
        Scheme::monomorphic(obfusku_typecheck::instantiate_core_type(
            &CoreType::Function(
                Box::new(CoreType::Str),
                Box::new(CoreType::Function(
                    Box::new(CoreType::Str),
                    Box::new(CoreType::Unit),
                )),
            ),
            &HashMap::new(),
        )),
    );
    values.insert(
        "writeFile".to_string(),
        Value::Native(Rc::new(write_file_native(root))),
    );

    (types, values)
}

/// Resolves `user_path` against `root` *lexically* — no filesystem
/// access, so it works identically whether the target exists yet
/// (`writeFile` creating a new file) or not. Rejects an absolute
/// `user_path` outright, and rejects any `..` that would climb above
/// `root` itself; `Some` only for a path that stays within `root`.
fn resolve_within_root(root: &Path, user_path: &str) -> Option<PathBuf> {
    let candidate = Path::new(user_path);
    if candidate.is_absolute() {
        return None;
    }
    let mut stack: Vec<Component> = root.components().collect();
    let root_len = stack.len();
    for comp in candidate.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                if stack.len() <= root_len {
                    return None;
                }
                stack.pop();
            }
            Component::Normal(_) => stack.push(comp),
            Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    Some(stack.iter().collect())
}

/// Constructs a `Failure(tag, message)` value (§15.2).
fn failure(tag: &str, message: &str) -> Value {
    Value::Adt {
        tag: Rc::from("Failure"),
        args: Rc::from(vec![
            Value::Str(Rc::from(tag)),
            Value::Str(Rc::from(message)),
        ]),
    }
}

/// Maps OS permission errors to `"PermissionDenied"`, other I/O errors to `"IOError"`.
fn io_error_tag(e: &std::io::Error) -> &'static str {
    if e.kind() == std::io::ErrorKind::PermissionDenied {
        "PermissionDenied"
    } else {
        "IOError"
    }
}

fn print_native() -> NativeFn {
    NativeFn {
        name: "print".to_string(),
        func: Box::new(|arg| match arg {
            Value::Str(s) => {
                println!("{s}");
                Ok(Value::Unit)
            }
            // Unreachable through well-typed source.
            other => Err(Raised(Value::Adt {
                tag: Rc::from("InvalidOperation"),
                args: Rc::from(vec![Value::Str(Rc::from(
                    format!("'print' expects a Str, found {other:?}").as_str(),
                ))]),
            })),
        }),
    }
}

fn read_line_native() -> NativeFn {
    NativeFn {
        name: "readLine".to_string(),
        func: Box::new(|arg| {
            // Unreachable through well-typed source.
            if !matches!(arg, Value::Unit) {
                return Err(Raised(Value::Adt {
                    tag: Rc::from("InvalidOperation"),
                    args: Rc::from(vec![Value::Str(Rc::from(
                        format!("'readLine' expects Unit, found {arg:?}").as_str(),
                    ))]),
                }));
            }
            let mut line = String::new();
            match std::io::stdin().lock().read_line(&mut line) {
                Ok(0) => Err(Raised(failure("EOF", "readLine: end of input"))),
                Ok(_) => {
                    if line.ends_with('\n') {
                        line.pop();
                        if line.ends_with('\r') {
                            line.pop();
                        }
                    }
                    Ok(Value::Str(Rc::from(line.as_str())))
                }
                Err(e) => Err(Raised(failure("IOError", &format!("readLine: {e}")))),
            }
        }),
    }
}

fn read_file_native(root: Rc<PathBuf>) -> NativeFn {
    NativeFn {
        name: "readFile".to_string(),
        func: Box::new(move |arg| {
            let path = match &arg {
                Value::Str(s) => s.as_ref(),
                other => {
                    return Err(Raised(Value::Adt {
                        tag: Rc::from("InvalidOperation"),
                        args: Rc::from(vec![Value::Str(Rc::from(
                            format!("'readFile' expects a Str, found {other:?}").as_str(),
                        ))]),
                    }))
                }
            };
            let Some(resolved) = resolve_within_root(&root, path) else {
                return Err(Raised(failure(
                    "PermissionDenied",
                    &format!("readFile: '{path}' is outside the allowed root"),
                )));
            };
            match std::fs::read_to_string(&resolved) {
                Ok(contents) => Ok(Value::Str(Rc::from(contents.as_str()))),
                Err(e) => Err(Raised(failure(io_error_tag(&e), &format!("readFile: {e}")))),
            }
        }),
    }
}

/// `writeFile : Str → Str → Unit` — curried native function. The first call
/// binds the path; the second call writes the content.
fn write_file_native(root: Rc<PathBuf>) -> NativeFn {
    NativeFn {
        name: "writeFile".to_string(),
        func: Box::new(move |path_arg| {
            let path = match &path_arg {
                Value::Str(s) => s.clone(),
                other => {
                    return Err(Raised(Value::Adt {
                        tag: Rc::from("InvalidOperation"),
                        args: Rc::from(vec![Value::Str(Rc::from(
                            format!("'writeFile' expects a Str, found {other:?}").as_str(),
                        ))]),
                    }))
                }
            };
            let root = Rc::clone(&root);
            Ok(Value::Native(Rc::new(NativeFn {
                name: "writeFile (applied to a path)".to_string(),
                func: Box::new(move |content_arg| {
                    let content = match &content_arg {
                        Value::Str(s) => s.as_ref(),
                        other => {
                            return Err(Raised(Value::Adt {
                                tag: Rc::from("InvalidOperation"),
                                args: Rc::from(vec![Value::Str(Rc::from(
                                    format!("'writeFile' expects a Str, found {other:?}").as_str(),
                                ))]),
                            }))
                        }
                    };
                    let Some(resolved) = resolve_within_root(&root, &path) else {
                        return Err(Raised(failure(
                            "PermissionDenied",
                            &format!("writeFile: '{path}' is outside the allowed root"),
                        )));
                    };
                    match std::fs::write(&resolved, content) {
                        Ok(()) => Ok(Value::Unit),
                        Err(e) => Err(Raised(failure(
                            io_error_tag(&e),
                            &format!("writeFile: {e}"),
                        ))),
                    }
                }),
            })))
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

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
        let scheme = types.get("print").expect("print must be registered");
        assert_eq!(
            scheme.ty.to_core(),
            Some(CoreType::Function(
                Box::new(CoreType::Str),
                Box::new(CoreType::Unit)
            )),
        );

        let Some(Value::Native(native)) = values.get("print") else {
            panic!("expected print to be a Value::Native");
        };
        let result = (native.func)(Value::Str(Rc::from("hello")));
        assert!(matches!(result, Ok(Value::Unit)));
    }

    #[test]
    fn print_native_rejects_a_non_str_argument_without_panicking() {
        let (_, values) = load(&test_root());
        let Some(Value::Native(native)) = values.get("print") else {
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
        let scheme = types.get("readLine").expect("readLine must be registered");
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
        let Some(Value::Native(native)) = values.get("readLine") else {
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
        match v {
            Value::Adt { tag, args } => {
                assert_eq!(&*tag, "Failure");
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
            types.get("readFile").unwrap().ty.to_core(),
            Some(CoreType::Function(
                Box::new(CoreType::Str),
                Box::new(CoreType::Str)
            )),
        );
        assert_eq!(
            types.get("writeFile").unwrap().ty.to_core(),
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
        let Some(Value::Native(write)) = values.get("writeFile") else {
            panic!("expected writeFile to be a Value::Native");
        };
        let applied = (write.func)(Value::Str(Rc::from("greeting.txt"))).expect("apply path");
        let Value::Native(write_content) = applied else {
            panic!("expected writeFile(path) to return a Value::Native");
        };
        let result = (write_content.func)(Value::Str(Rc::from("hello, file")));
        assert!(matches!(result, Ok(Value::Unit)), "{result:?}");

        let Some(Value::Native(read)) = values.get("readFile") else {
            panic!("expected readFile to be a Value::Native");
        };
        let contents = (read.func)(Value::Str(Rc::from("greeting.txt")));
        assert!(
            matches!(&contents, Ok(Value::Str(s)) if &**s == "hello, file"),
            "{contents:?}"
        );
    }

    #[test]
    fn read_file_outside_the_root_via_parent_traversal_is_permission_denied() {
        let (_, values) = load(&test_root());
        let Some(Value::Native(read)) = values.get("readFile") else {
            panic!("expected readFile to be a Value::Native");
        };
        let result = (read.func)(Value::Str(Rc::from("../secret.txt")));
        match result {
            Err(Raised(Value::Adt { tag, args })) => {
                assert_eq!(&*tag, "Failure");
                assert!(matches!(&args[0], Value::Str(s) if &**s == "PermissionDenied"));
            }
            other => panic!("expected a PermissionDenied Failure, got {other:?}"),
        }
    }

    #[test]
    fn read_file_with_an_absolute_path_is_permission_denied() {
        let (_, values) = load(&test_root());
        let Some(Value::Native(read)) = values.get("readFile") else {
            panic!("expected readFile to be a Value::Native");
        };
        let result = (read.func)(Value::Str(Rc::from("/etc/passwd")));
        match result {
            Err(Raised(Value::Adt { tag, args })) => {
                assert_eq!(&*tag, "Failure");
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
        let Some(Value::Native(write)) = values.get("writeFile") else {
            panic!("expected writeFile to be a Value::Native");
        };
        let applied = (write.func)(Value::Str(Rc::from("sub/nested.txt"))).expect("apply path");
        let Value::Native(write_content) = applied else {
            panic!("expected writeFile(path) to return a Value::Native");
        };
        let result = (write_content.func)(Value::Str(Rc::from("nested")));
        assert!(matches!(result, Ok(Value::Unit)), "{result:?}");
        assert!(root.join("sub/nested.txt").exists());
    }

    #[test]
    fn read_file_that_does_not_exist_is_an_io_error_not_permission_denied() {
        let (_, values) = load(&test_root());
        let Some(Value::Native(read)) = values.get("readFile") else {
            panic!("expected readFile to be a Value::Native");
        };
        let result = (read.func)(Value::Str(Rc::from("does-not-exist.txt")));
        match result {
            Err(Raised(Value::Adt { tag, args })) => {
                assert_eq!(&*tag, "Failure");
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
        let Some(Value::Native(read)) = values.get("readFile") else {
            panic!("expected readFile to be a Value::Native");
        };
        let result = (read.func)(Value::Str(Rc::from("no-read.txt")));
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        match result {
            Err(Raised(Value::Adt { tag, args })) => {
                assert_eq!(&*tag, "Failure");
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
