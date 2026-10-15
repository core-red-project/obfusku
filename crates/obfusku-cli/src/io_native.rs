//! Streams, clocks, randomness and file management (`ADR-027`): the part
//! of the `⌁` family beyond `print`/`readLine`/`readFile`/`writeFile`.
//! Direction marks: `↑` emit, `↓` ingest, `⍰` query, `⊕` append, `⊖`
//! remove, `⇄` move, `⧉` copy. Media: `⎙` text stream (stdin/stdout),
//! `⎚` byte stream, `⌖` terminal, `⧗` wall clock, `⧖` monotonic clock,
//! `⚀` dice of integers, `⚁` dice of reals; `⌬ ⌂ ⌭` as in `ADR-024`.
//!
//! - `⌁↑⎙ : ⌘ → ∅` write, no newline, flushed · `⌁↓⎙ : ∅ → ⌘` read to end
//!   of input · `⌁⍰⎙ : ∅ → ○` input exhausted
//! - `⌁↑⎚ : ⌭ → ∅` / `⌁↓⎚ : ∅ → ⌭` the same over bytes
//! - `⌁⍰⌖ : ∅ → ○` stdout is a terminal
//! - `⌁↓⧗ : ∅ → ⟁` milliseconds since the Unix epoch · `⌁↓⧖ : ∅ → ⟁`
//!   monotonic nanoseconds, differences only · `⌁↑⧗ : ⟁ → ∅` sleep (ms)
//! - `⌁↓⚀ : ⟁ → ⟁` uniform in `0..n` · `⌁↓⚁ : ∅ → ⧆` uniform in `[0, 1)`;
//!   xorshift64*, not cryptographic — never for tokens or keys
//! - rooted like `⌁↓⌬` (`ADR-011`, `ADR-019`): `⌁⊕⌬ : ⌘ → ⌘ → ∅` append ·
//!   `⌁⊖⌬ : ⌘ → ∅` remove file · `⌁⊖⌂ : ⌘ → ∅` remove directory tree ·
//!   `⌁⇄⌬ : ⌘ → ⌘ → ∅` move · `⌁⧉⌬ : ⌘ → ⌘ → ∅` copy file ·
//!   `⌁⍰⌭ : ⌘ → ⟁` size in bytes · `⌁⍰⧗ : ⌘ → ⟁` modified, ms since epoch ·
//!   `⌁↓⌭ : ⌘ → ⌭` / `⌁↑⌭ : ⌘ → ⌭ → ∅` read / write bytes

use crate::native_util::{Impl, Registry, bytes_arg, func, int_arg, invalid, s, str_arg};
use crate::natives::{failure, io_error_tag, resolve_within_root};
use obfusku_runtime::Value;
use obfusku_runtime::eval::Raised;
use obfusku_typecheck::{Prelude, Type};
use std::cell::Cell;
use std::collections::HashMap;
use std::io::{BufRead, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::OnceLock;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

fn io_fail(op: &str, e: &std::io::Error) -> Raised {
    Raised(failure(io_error_tag(e), &format!("{op}: {e}")))
}

fn bytes_type() -> Type {
    Type::Adt("⌭".to_string(), vec![])
}

fn unit_to(t: Type) -> Type {
    func(Type::Unit, t)
}

fn next_u64() -> u64 {
    thread_local! {
        static STATE: Cell<u64> = const { Cell::new(0) };
    }
    STATE.with(|st| {
        let mut x = st.get();
        if x == 0 {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos() as u64);
            x = (nanos ^ (u64::from(std::process::id()) << 32)) | 1;
        }
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        st.set(x);
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    })
}

fn epoch_millis(t: SystemTime) -> i64 {
    match t.duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_millis() as i64,
        Err(e) => -(e.duration().as_millis() as i64),
    }
}

/// Process-wide streams, clocks and randomness: no filesystem root needed.
#[must_use]
pub fn load_streams() -> (Prelude, HashMap<String, Value>) {
    let mut r = Registry::default();
    let mut add = |name: &'static str, ty: Type, f: Impl| r.add(name, &[], &[], ty, 1, f);

    add(
        "⌁↑⎙",
        func(Type::Str, Type::Unit),
        Rc::new(|a| {
            let text = str_arg("⌁↑⎙", &a[0])?;
            let mut out = std::io::stdout().lock();
            out.write_all(text.as_bytes())
                .and_then(|()| out.flush())
                .map_err(|e| io_fail("write", &e))?;
            Ok(Value::Unit)
        }),
    );
    add(
        "⌁↓⎙",
        unit_to(Type::Str),
        Rc::new(|_| {
            let mut text = String::new();
            std::io::stdin()
                .lock()
                .read_to_string(&mut text)
                .map_err(|e| io_fail("readAll", &e))?;
            Ok(s(&text))
        }),
    );
    add(
        "⌁⍰⎙",
        unit_to(Type::Bool),
        Rc::new(|_| {
            let mut input = std::io::stdin().lock();
            let buf = input.fill_buf().map_err(|e| io_fail("atEnd", &e))?;
            Ok(Value::Bool(buf.is_empty()))
        }),
    );
    add(
        "⌁↑⎚",
        func(bytes_type(), Type::Unit),
        Rc::new(|a| {
            let data = bytes_arg("⌁↑⎚", &a[0])?;
            let mut out = std::io::stdout().lock();
            out.write_all(data)
                .and_then(|()| out.flush())
                .map_err(|e| io_fail("writeBytes", &e))?;
            Ok(Value::Unit)
        }),
    );
    add(
        "⌁↓⎚",
        unit_to(bytes_type()),
        Rc::new(|_| {
            let mut data = Vec::new();
            std::io::stdin()
                .lock()
                .read_to_end(&mut data)
                .map_err(|e| io_fail("readAllBytes", &e))?;
            Ok(Value::Bytes(Rc::from(data)))
        }),
    );
    add(
        "⌁⍰⌖",
        unit_to(Type::Bool),
        Rc::new(|_| Ok(Value::Bool(std::io::stdout().is_terminal()))),
    );
    add(
        "⌁↓⧗",
        unit_to(Type::Int),
        Rc::new(|_| Ok(Value::Int(epoch_millis(SystemTime::now())))),
    );
    add(
        "⌁↓⧖",
        unit_to(Type::Int),
        Rc::new(|_| {
            static ORIGIN: OnceLock<Instant> = OnceLock::new();
            let origin = ORIGIN.get_or_init(Instant::now);
            Ok(Value::Int(origin.elapsed().as_nanos() as i64))
        }),
    );
    add(
        "⌁↑⧗",
        func(Type::Int, Type::Unit),
        Rc::new(|a| {
            let ms = int_arg("⌁↑⧗", &a[0])?;
            let ms = u64::try_from(ms).map_err(|_| {
                invalid(format!(
                    "'⌁↑⧗' expects non-negative milliseconds, found {ms}"
                ))
            })?;
            std::thread::sleep(Duration::from_millis(ms));
            Ok(Value::Unit)
        }),
    );
    add(
        "⌁↓⚀",
        func(Type::Int, Type::Int),
        Rc::new(|a| {
            let n = int_arg("⌁↓⚀", &a[0])?;
            if n <= 0 {
                return Err(invalid(format!(
                    "'⌁↓⚀' expects a positive bound, found {n}"
                )));
            }
            let n = n as u64;
            // Rejection sampling keeps every residue equally likely.
            let limit = u64::MAX - u64::MAX % n;
            loop {
                let v = next_u64();
                if v < limit {
                    return Ok(Value::Int((v % n) as i64));
                }
            }
        }),
    );
    add(
        "⌁↓⚁",
        unit_to(Type::Real),
        Rc::new(|_| Ok(Value::Real((next_u64() >> 11) as f64 / (1u64 << 53) as f64))),
    );

    r.finish()
}

fn within(root: &Path, op: &str, v: &Value) -> Result<PathBuf, Raised> {
    let path = str_arg(op, v)?;
    resolve_within_root(root, path).ok_or_else(|| {
        Raised(failure(
            "PermissionDenied",
            &format!("{op}: '{path}' is outside the allowed root"),
        ))
    })
}

/// Filesystem natives scoped to `root` (already canonical).
#[must_use]
pub fn load_fs(root: &Rc<PathBuf>) -> (Prelude, HashMap<String, Value>) {
    let mut r = Registry::default();
    let str2 = |ret: Type| func(Type::Str, func(Type::Str, ret));
    let path_fn = |name: &'static str,
                   root: &Rc<PathBuf>,
                   op: fn(&Path, &str) -> Result<Value, Raised>|
     -> Impl {
        let root = Rc::clone(root);
        Rc::new(move |a| {
            let path = within(&root, name, &a[0])?;
            op(&path, name)
        })
    };

    r.add(
        "⌁⊖⌬",
        &[],
        &[],
        func(Type::Str, Type::Unit),
        1,
        path_fn("⌁⊖⌬", root, |p, n| {
            std::fs::remove_file(p).map_err(|e| io_fail(n, &e))?;
            Ok(Value::Unit)
        }),
    );
    let tree_root = Rc::clone(root);
    r.add(
        "⌁⊖⌂",
        &[],
        &[],
        func(Type::Str, Type::Unit),
        1,
        Rc::new(move |a| {
            let path = within(&tree_root, "⌁⊖⌂", &a[0])?;
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
    r.add(
        "⌁⍰⌭",
        &[],
        &[],
        func(Type::Str, Type::Int),
        1,
        path_fn("⌁⍰⌭", root, |p, n| {
            let meta = std::fs::metadata(p).map_err(|e| io_fail(n, &e))?;
            Ok(Value::Int(meta.len() as i64))
        }),
    );
    r.add(
        "⌁⍰⧗",
        &[],
        &[],
        func(Type::Str, Type::Int),
        1,
        path_fn("⌁⍰⧗", root, |p, n| {
            let modified = std::fs::metadata(p)
                .and_then(|m| m.modified())
                .map_err(|e| io_fail(n, &e))?;
            Ok(Value::Int(epoch_millis(modified)))
        }),
    );
    r.add(
        "⌁↓⌭",
        &[],
        &[],
        func(Type::Str, bytes_type()),
        1,
        path_fn("⌁↓⌭", root, |p, n| {
            let data = std::fs::read(p).map_err(|e| io_fail(n, &e))?;
            Ok(Value::Bytes(Rc::from(data)))
        }),
    );

    let two = |name: &'static str,
               root: &Rc<PathBuf>,
               op: fn(&Path, &Path) -> std::io::Result<()>|
     -> Impl {
        let root = Rc::clone(root);
        Rc::new(move |a| {
            let (from, to) = (within(&root, name, &a[0])?, within(&root, name, &a[1])?);
            op(&from, &to).map_err(|e| io_fail(name, &e))?;
            Ok(Value::Unit)
        })
    };
    r.add(
        "⌁⇄⌬",
        &[],
        &[],
        str2(Type::Unit),
        2,
        two("⌁⇄⌬", root, |from, to| std::fs::rename(from, to)),
    );
    r.add(
        "⌁⧉⌬",
        &[],
        &[],
        str2(Type::Unit),
        2,
        two("⌁⧉⌬", root, |from, to| {
            std::fs::copy(from, to).map(|_| ())
        }),
    );

    let append_root = Rc::clone(root);
    r.add(
        "⌁⊕⌬",
        &[],
        &[],
        str2(Type::Unit),
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
    let write_root = Rc::clone(root);
    r.add(
        "⌁↑⌭",
        &[],
        &[],
        func(Type::Str, func(bytes_type(), Type::Unit)),
        2,
        Rc::new(move |a| {
            let path = within(&write_root, "⌁↑⌭", &a[0])?;
            std::fs::write(&path, bytes_arg("⌁↑⌭", &a[1])?).map_err(|e| io_fail("⌁↑⌭", &e))?;
            Ok(Value::Unit)
        }),
    );

    r.finish()
}
