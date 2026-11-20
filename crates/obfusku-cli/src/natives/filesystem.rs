//! Filesystem natives, confined to a root (`ADR-011`, `ADR-019`, `ADR-024`).
//!
//! The root is a capability held in closure-captured state, invisible to
//! Obfusku source: a program cannot name, widen or replace it. Every path
//! argument goes through `resolve_within_root`, so one check guards every
//! native instead of each re-implementing it.
//!
//! - `⌁↓⌬ : ⌘ → ⌘` read file · `⌁↑⌬ : ⌘ → ⌘ → ∅` write file ·
//!   `⌁⊕⌬ : ⌘ → ⌘ → ∅` append · `⌁⊖⌬ : ⌘ → ∅` remove file ·
//!   `⌁⇄⌬ : ⌘ → ⌘ → ∅` move · `⌁⧉⌬ : ⌘ → ⌘ → ∅` copy
//! - `⌁↓⌂ : ⌘ → List<⌘>` sorted directory entries · `⌁↑⌂ : ⌘ → ∅` create
//!   directories · `⌁⊖⌂ : ⌘ → ∅` remove a directory tree (never the root)
//! - `⌁⍰⌬ : ⌘ → ○` exists · `⌁⍰⌂ : ⌘ → ○` is a directory ·
//!   `⌁⍰⌭ : ⌘ → ⟁` size in bytes · `⌁⍰⧗ : ⌘ → ⟁` modified, ms since epoch
//! - `⌁↓⌭ : ⌘ → ⌭` / `⌁↑⌭ : ⌘ → ⌭ → ∅` read / write bytes
//!
//! An escaping path raises `Failure("PermissionDenied", …)`; any other OS
//! error maps through `io_error_tag`.

use super::Natives;
use super::marshal::{
    bytes_arg, bytes_type, failure, func, io_fail, list_from, list_of, str_arg, str_value,
};
use super::registry::{Impl, Registry};
use super::streams::epoch_millis;
use obfusku_runtime::Value;
use obfusku_runtime::eval::Raised;
use obfusku_typecheck::Type;
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::rc::Rc;

/// `ADR-019`'s symlink-escape check: the deepest existing ancestor of
/// `resolved` (inclusive) is canonicalized and must stay within
/// `root_canonical`.
///
/// Walking up to the nearest *real* ancestor is sufficient, not merely
/// "check the target if it exists": components below it do not exist yet
/// (a write target being created) and so cannot themselves be symlinks.
#[must_use]
pub fn escapes_root_via_symlink(root_canonical: &Path, resolved: &Path) -> bool {
    let mut probe = resolved.to_path_buf();
    loop {
        if let Ok(real) = probe.canonicalize() {
            return !real.starts_with(root_canonical);
        }
        if !probe.pop() {
            return false;
        }
    }
}

/// Resolves `user_path` against `root`, which must already be canonical.
///
/// Two layers, in this order. First lexically — no filesystem access, so it
/// behaves identically whether the target exists yet or not: an absolute
/// path is rejected outright, as is any `..` that would climb above `root`.
/// Then `ADR-019`'s symlink check, which catches a path that is lexically
/// well-formed but resolves through a symlink to somewhere outside `root`.
pub(crate) fn resolve_within_root(root: &Path, user_path: &str) -> Option<PathBuf> {
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
    let resolved: PathBuf = stack.iter().collect();
    if escapes_root_via_symlink(root, &resolved) {
        return None;
    }
    Some(resolved)
}

/// The argument `v` as a path inside `root`, or `PermissionDenied`.
fn within(root: &Path, op: &str, v: &Value) -> Result<PathBuf, Raised> {
    let path = str_arg(op, v)?;
    resolve_within_root(root, path).ok_or_else(|| {
        Raised(failure(
            "PermissionDenied",
            &format!("{op}: '{path}' is outside the allowed root"),
        ))
    })
}

/// A one-path native: resolves argument 0, runs `op`, maps OS errors.
/// `label` names the operation in error messages.
fn on_path(
    root: &Rc<PathBuf>,
    label: &'static str,
    op: impl Fn(&Path) -> std::io::Result<Value> + 'static,
) -> Impl {
    let root = Rc::clone(root);
    Rc::new(move |a| {
        let path = within(&root, label, &a[0])?;
        op(&path).map_err(|e| io_fail(label, &e))
    })
}

/// A two-path native (move, copy): both arguments must stay inside `root`.
fn on_two_paths(
    root: &Rc<PathBuf>,
    label: &'static str,
    op: fn(&Path, &Path) -> std::io::Result<()>,
) -> Impl {
    let root = Rc::clone(root);
    Rc::new(move |a| {
        let (from, to) = (within(&root, label, &a[0])?, within(&root, label, &a[1])?);
        op(&from, &to).map_err(|e| io_fail(label, &e))?;
        Ok(Value::Unit)
    })
}

/// Filesystem natives scoped to `root`, which must already be canonical.
#[must_use]
pub fn load(root: &Rc<PathBuf>) -> Natives {
    let mut r = Registry::default();
    let str_to = |ret: Type| func(Type::Str, ret);
    let str2_to = |ret: Type| func(Type::Str, func(Type::Str, ret));

    r.add_mono(
        "⌁↓⌬",
        str_to(Type::Str),
        1,
        on_path(root, "readFile", |p| {
            std::fs::read_to_string(p).map(|text| str_value(&text))
        }),
    );
    let write_root = Rc::clone(root);
    r.add_mono(
        "⌁↑⌬",
        str2_to(Type::Unit),
        2,
        Rc::new(move |a| {
            let path = within(&write_root, "writeFile", &a[0])?;
            let content = str_arg("writeFile", &a[1])?;
            std::fs::write(&path, content).map_err(|e| io_fail("writeFile", &e))?;
            Ok(Value::Unit)
        }),
    );
    let append_root = Rc::clone(root);
    r.add_mono(
        "⌁⊕⌬",
        str2_to(Type::Unit),
        2,
        Rc::new(move |a| {
            let path = within(&append_root, "⌁⊕⌬", &a[0])?;
            let text = str_arg("⌁⊕⌬", &a[1])?;
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .and_then(|mut f| f.write_all(text.as_bytes()))
                .map_err(|e| io_fail("⌁⊕⌬", &e))?;
            Ok(Value::Unit)
        }),
    );
    r.add_mono(
        "⌁⊖⌬",
        str_to(Type::Unit),
        1,
        on_path(root, "⌁⊖⌬", |p| {
            std::fs::remove_file(p).map(|()| Value::Unit)
        }),
    );
    r.add_mono(
        "⌁⇄⌬",
        str2_to(Type::Unit),
        2,
        on_two_paths(root, "⌁⇄⌬", |from, to| std::fs::rename(from, to)),
    );
    r.add_mono(
        "⌁⧉⌬",
        str2_to(Type::Unit),
        2,
        on_two_paths(root, "⌁⧉⌬", |from, to| {
            std::fs::copy(from, to).map(|_| ())
        }),
    );

    r.add_mono(
        "⌁↓⌂",
        str_to(list_of(Type::Str)),
        1,
        on_path(root, "⌁↓⌂", |p| {
            let mut entries: Vec<String> = std::fs::read_dir(p)?
                .map(|e| e.map(|e| e.file_name().to_string_lossy().into_owned()))
                .collect::<Result<_, _>>()?;
            // Directory order is OS-defined; sorting keeps programs portable.
            entries.sort();
            Ok(list_from(entries.iter().map(|n| str_value(n)).collect()))
        }),
    );
    r.add_mono(
        "⌁↑⌂",
        str_to(Type::Unit),
        1,
        on_path(root, "⌁↑⌂", |p| {
            std::fs::create_dir_all(p).map(|()| Value::Unit)
        }),
    );
    let tree_root = Rc::clone(root);
    r.add_mono(
        "⌁⊖⌂",
        str_to(Type::Unit),
        1,
        Rc::new(move |a| {
            let path = within(&tree_root, "⌁⊖⌂", &a[0])?;
            // Removing the root would delete the sandbox the program runs in.
            if path == *tree_root {
                return Err(Raised(failure(
                    "PermissionDenied",
                    "⌁⊖⌂: the project root itself cannot be removed",
                )));
            }
            std::fs::remove_dir_all(&path).map_err(|e| io_fail("⌁⊖⌂", &e))?;
            Ok(Value::Unit)
        }),
    );

    r.add_mono(
        "⌁⍰⌬",
        str_to(Type::Bool),
        1,
        on_path(root, "⌁⍰⌬", |p| Ok(Value::Bool(p.exists()))),
    );
    r.add_mono(
        "⌁⍰⌂",
        str_to(Type::Bool),
        1,
        on_path(root, "⌁⍰⌂", |p| Ok(Value::Bool(p.is_dir()))),
    );
    r.add_mono(
        "⌁⍰⌭",
        str_to(Type::Int),
        1,
        on_path(root, "⌁⍰⌭", |p| {
            std::fs::metadata(p).map(|m| Value::Int(m.len() as i64))
        }),
    );
    r.add_mono(
        "⌁⍰⧗",
        str_to(Type::Int),
        1,
        on_path(root, "⌁⍰⧗", |p| {
            let modified = std::fs::metadata(p).and_then(|m| m.modified())?;
            Ok(Value::Int(epoch_millis(modified)))
        }),
    );

    r.add_mono(
        "⌁↓⌭",
        str_to(bytes_type()),
        1,
        on_path(root, "⌁↓⌭", |p| {
            std::fs::read(p).map(|data| Value::Bytes(Rc::from(data)))
        }),
    );
    let bytes_root = Rc::clone(root);
    r.add_mono(
        "⌁↑⌭",
        func(Type::Str, func(bytes_type(), Type::Unit)),
        2,
        Rc::new(move |a| {
            let path = within(&bytes_root, "⌁↑⌭", &a[0])?;
            std::fs::write(&path, bytes_arg("⌁↑⌭", &a[1])?).map_err(|e| io_fail("⌁↑⌭", &e))?;
            Ok(Value::Unit)
        }),
    );
    r.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::scratch::Scratch;
    use obfusku_runtime::Text;

    fn fs(root: &Path) -> Natives {
        load(&Rc::new(root.to_path_buf()))
    }

    fn call(n: &Natives, name: &str, args: &[&str]) -> Result<Value, Raised> {
        let mut v = n.1[name].clone();
        for a in args {
            let Value::Native(f) = &v else {
                panic!("applied too many arguments")
            };
            v = (f.func)(Value::Str(Text::from(*a)))?;
        }
        Ok(v)
    }

    fn failure_tag(r: &Result<Value, Raised>) -> String {
        match r {
            Err(Raised(Value::Adt { args, .. })) => match &args[0] {
                Value::Str(s) => s.to_string(),
                other => panic!("{other:?}"),
            },
            other => panic!("expected a Failure, got {other:?}"),
        }
    }

    #[test]
    fn resolution_stays_inside_the_root_lexically() {
        let s = Scratch::new();
        let root = s.root();
        assert_eq!(
            resolve_within_root(root, "a/b.txt"),
            Some(root.join("a/b.txt"))
        );
        assert_eq!(
            resolve_within_root(root, "a/../b.txt"),
            Some(root.join("b.txt"))
        );
        assert_eq!(resolve_within_root(root, "./x"), Some(root.join("x")));
        assert_eq!(resolve_within_root(root, "../x"), None);
        assert_eq!(resolve_within_root(root, "a/../../x"), None);
        assert_eq!(resolve_within_root(root, "/etc/passwd"), None);
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_pointing_outside_the_root_is_an_escape() {
        let outside = Scratch::new();
        outside.write("secret.txt", "s");
        let s = Scratch::new();
        std::os::unix::fs::symlink(outside.root(), s.root().join("link")).unwrap();
        assert_eq!(resolve_within_root(s.root(), "link/secret.txt"), None);
        assert!(escapes_root_via_symlink(
            s.root(),
            &s.root().join("link/new.txt")
        ));
        assert!(!escapes_root_via_symlink(
            s.root(),
            &s.root().join("plain/new.txt")
        ));
    }

    #[test]
    fn write_append_and_read_agree() {
        let s = Scratch::new();
        let n = fs(s.root());
        call(&n, "⌁↑⌬", &["f.txt", "one"]).unwrap();
        call(&n, "⌁⊕⌬", &["f.txt", "-two"]).unwrap();
        assert_eq!(
            std::fs::read_to_string(s.root().join("f.txt")).unwrap(),
            "one-two"
        );
        let read = call(&n, "⌁↓⌬", &["f.txt"]).unwrap();
        assert!(matches!(&read, Value::Str(t) if &**t == "one-two"));
    }

    #[test]
    fn directory_listing_is_sorted() {
        let s = Scratch::new();
        for f in ["b", "c", "a"] {
            s.write(&format!("d/{f}"), "");
        }
        let n = fs(s.root());
        let list = call(&n, "⌁↓⌂", &["d"]).unwrap();
        let names: Vec<String> = crate::natives::marshal::vec_from_list("t", &list)
            .unwrap()
            .into_iter()
            .map(|v| match &v {
                Value::Str(t) => t.to_string(),
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(names, ["a", "b", "c"]);
    }

    #[test]
    fn the_root_itself_cannot_be_removed() {
        let s = Scratch::new();
        let n = fs(s.root());
        assert_eq!(failure_tag(&call(&n, "⌁⊖⌂", &["."])), "PermissionDenied");
        assert!(s.root().is_dir());
    }

    #[test]
    fn a_subtree_can_be_removed() {
        let s = Scratch::new();
        s.write("t/x/y.txt", "1");
        call(&fs(s.root()), "⌁⊖⌂", &["t"]).unwrap();
        assert!(!s.root().join("t").exists());
    }

    #[test]
    fn copy_and_move_keep_both_ends_inside_the_root() {
        let s = Scratch::new();
        s.write("a.txt", "x");
        let n = fs(s.root());
        call(&n, "⌁⧉⌬", &["a.txt", "b.txt"]).unwrap();
        call(&n, "⌁⇄⌬", &["b.txt", "c.txt"]).unwrap();
        assert!(s.root().join("a.txt").exists() && s.root().join("c.txt").exists());
        assert!(!s.root().join("b.txt").exists());
        assert_eq!(
            failure_tag(&call(&n, "⌁⧉⌬", &["a.txt", "../out.txt"])),
            "PermissionDenied"
        );
        assert_eq!(
            failure_tag(&call(&n, "⌁⇄⌬", &["../in.txt", "z.txt"])),
            "PermissionDenied"
        );
    }

    #[test]
    fn queries_report_existence_kind_and_size() {
        let s = Scratch::new();
        s.write("d/f.txt", "12345");
        let n = fs(s.root());
        assert!(matches!(
            call(&n, "⌁⍰⌬", &["d/f.txt"]).unwrap(),
            Value::Bool(true)
        ));
        assert!(matches!(
            call(&n, "⌁⍰⌬", &["nope"]).unwrap(),
            Value::Bool(false)
        ));
        assert!(matches!(
            call(&n, "⌁⍰⌂", &["d"]).unwrap(),
            Value::Bool(true)
        ));
        assert!(matches!(
            call(&n, "⌁⍰⌂", &["d/f.txt"]).unwrap(),
            Value::Bool(false)
        ));
        assert!(matches!(
            call(&n, "⌁⍰⌭", &["d/f.txt"]).unwrap(),
            Value::Int(5)
        ));
        assert!(matches!(call(&n, "⌁⍰⧗", &["d/f.txt"]).unwrap(), Value::Int(ms) if ms > 0));
    }

    #[test]
    fn mkdir_creates_nested_directories() {
        let s = Scratch::new();
        call(&fs(s.root()), "⌁↑⌂", &["a/b/c"]).unwrap();
        assert!(s.root().join("a/b/c").is_dir());
    }

    #[test]
    fn bytes_round_trip_through_the_filesystem() {
        let s = Scratch::new();
        let n = fs(s.root());
        let write = call(&n, "⌁↑⌭", &["blob"]).unwrap();
        let Value::Native(w) = &write else {
            panic!("expected a native")
        };
        (w.func)(Value::Bytes(Rc::from(vec![0u8, 255, 7]))).unwrap();
        let read = call(&n, "⌁↓⌭", &["blob"]).unwrap();
        assert!(matches!(&read, Value::Bytes(b) if **b == [0u8, 255, 7]));
    }

    #[test]
    fn every_filesystem_native_denies_an_escaping_path() {
        let s = Scratch::new();
        let n = fs(s.root());
        for name in [
            "⌁↓⌬",
            "⌁⊖⌬",
            "⌁↓⌂",
            "⌁↑⌂",
            "⌁⍰⌬",
            "⌁⍰⌂",
            "⌁⍰⌭",
            "⌁⍰⧗",
            "⌁↓⌭",
            "⌁⊖⌂",
        ] {
            assert_eq!(
                failure_tag(&call(&n, name, &["../x"])),
                "PermissionDenied",
                "{name}"
            );
        }
    }
}
