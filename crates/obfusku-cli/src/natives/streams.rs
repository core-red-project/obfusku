//! Streams, clocks and randomness (`ADR-027`): process-wide natives that,
//! unlike `filesystem.rs`, need no root.
//!
//! Direction marks: `↑` emit, `↓` ingest, `⍰` query. Media: `⎙` text
//! stream (stdin/stdout), `⎚` byte stream, `⌖` terminal, `⧗` wall clock,
//! `⧖` monotonic clock, `⚀` dice of integers, `⚁` dice of reals.
//!
//! - `⌁↑⎙ : ⌘ → ∅` write, no newline, flushed · `⌁↓⎙ : ∅ → ⌘` read to end
//!   of input · `⌁⍰⎙ : ∅ → ○` input exhausted
//! - `⌁↑⎚ : ⌭ → ∅` / `⌁↓⎚ : ∅ → ⌭` the same over bytes
//! - `⌁⍰⌖ : ∅ → ○` stdout is a terminal
//! - `⌁↓⧗ : ∅ → ⟁` milliseconds since the Unix epoch · `⌁↓⧖ : ∅ → ⟁`
//!   monotonic nanoseconds, differences only · `⌁↑⧗ : ⟁ → ∅` sleep (ms)
//! - `⌁↓⚀ : ⟁ → ⟁` uniform in `0..n` · `⌁↓⚁ : ∅ → ⧆` uniform in `[0, 1)`
//!
//! The generator is xorshift64* seeded from the clock and process id: fast
//! and dependency-free, but **not cryptographic** — never for tokens or keys.

use super::Natives;
use super::marshal::{
    bytes_arg, bytes_type, func, int_arg, invalid, io_fail, str_arg, str_value, unit_to,
};
use super::registry::Registry;
use obfusku_runtime::Value;
use obfusku_typecheck::Type;
use std::cell::Cell;
use std::io::{BufRead, IsTerminal, Read, Write};
use std::rc::Rc;
use std::sync::OnceLock;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Next value of the thread-local xorshift64* generator.
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

/// Milliseconds since the Unix epoch; negative before it.
pub(super) fn epoch_millis(t: SystemTime) -> i64 {
    match t.duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_millis() as i64,
        Err(e) => -(e.duration().as_millis() as i64),
    }
}

/// A uniform integer in `0..n`. Rejection sampling keeps every residue
/// equally likely, which a plain `% n` would not.
fn below(n: u64) -> u64 {
    let limit = u64::MAX - u64::MAX % n;
    loop {
        let v = next_u64();
        if v < limit {
            return v % n;
        }
    }
}

/// Standard streams, clocks and dice.
#[must_use]
pub fn load() -> Natives {
    let mut r = Registry::default();

    r.add_mono(
        "⌁↑⎙",
        func(Type::Str, Type::Unit),
        1,
        Rc::new(|a| {
            let text = str_arg("⌁↑⎙", &a[0])?;
            let mut out = std::io::stdout().lock();
            out.write_all(text.as_bytes())
                .and_then(|()| out.flush())
                .map_err(|e| io_fail("write", &e))?;
            Ok(Value::Unit)
        }),
    );
    r.add_mono(
        "⌁↓⎙",
        unit_to(Type::Str),
        1,
        Rc::new(|_| {
            let mut text = String::new();
            std::io::stdin()
                .lock()
                .read_to_string(&mut text)
                .map_err(|e| io_fail("readAll", &e))?;
            Ok(str_value(&text))
        }),
    );
    r.add_mono(
        "⌁⍰⎙",
        unit_to(Type::Bool),
        1,
        Rc::new(|_| {
            let mut input = std::io::stdin().lock();
            let buf = input.fill_buf().map_err(|e| io_fail("atEnd", &e))?;
            Ok(Value::Bool(buf.is_empty()))
        }),
    );
    r.add_mono(
        "⌁↑⎚",
        func(bytes_type(), Type::Unit),
        1,
        Rc::new(|a| {
            let data = bytes_arg("⌁↑⎚", &a[0])?;
            let mut out = std::io::stdout().lock();
            out.write_all(data)
                .and_then(|()| out.flush())
                .map_err(|e| io_fail("writeBytes", &e))?;
            Ok(Value::Unit)
        }),
    );
    r.add_mono(
        "⌁↓⎚",
        unit_to(bytes_type()),
        1,
        Rc::new(|_| {
            let mut data = Vec::new();
            std::io::stdin()
                .lock()
                .read_to_end(&mut data)
                .map_err(|e| io_fail("readAllBytes", &e))?;
            Ok(Value::Bytes(Rc::from(data)))
        }),
    );
    r.add_mono(
        "⌁⍰⌖",
        unit_to(Type::Bool),
        1,
        Rc::new(|_| Ok(Value::Bool(std::io::stdout().is_terminal()))),
    );

    r.add_mono(
        "⌁↓⧗",
        unit_to(Type::Int),
        1,
        Rc::new(|_| Ok(Value::Int(epoch_millis(SystemTime::now())))),
    );
    r.add_mono(
        "⌁↓⧖",
        unit_to(Type::Int),
        1,
        Rc::new(|_| {
            static ORIGIN: OnceLock<Instant> = OnceLock::new();
            let origin = ORIGIN.get_or_init(Instant::now);
            Ok(Value::Int(origin.elapsed().as_nanos() as i64))
        }),
    );
    r.add_mono(
        "⌁↑⧗",
        func(Type::Int, Type::Unit),
        1,
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

    r.add_mono(
        "⌁↓⚀",
        func(Type::Int, Type::Int),
        1,
        Rc::new(|a| {
            let n = int_arg("⌁↓⚀", &a[0])?;
            if n <= 0 {
                return Err(invalid(format!(
                    "'⌁↓⚀' expects a positive bound, found {n}"
                )));
            }
            Ok(Value::Int(below(n as u64) as i64))
        }),
    );
    r.add_mono(
        "⌁↓⚁",
        unit_to(Type::Real),
        1,
        // The top 53 bits fill an `f64` mantissa exactly: uniform in [0, 1).
        Rc::new(|_| Ok(Value::Real((next_u64() >> 11) as f64 / (1u64 << 53) as f64))),
    );
    r.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(
        natives: &Natives,
        name: &str,
        arg: Value,
    ) -> Result<Value, obfusku_runtime::eval::Raised> {
        match &natives.1[name] {
            Value::Native(n) => (n.func)(arg),
            other => panic!("expected {name} to be a native, got {other:?}"),
        }
    }

    #[test]
    fn dice_stay_within_the_bound_and_cover_it() {
        let n = load();
        let mut seen = [false; 5];
        for _ in 0..500 {
            let Ok(Value::Int(v)) = call(&n, "⌁↓⚀", Value::Int(5)) else {
                panic!("expected an Int");
            };
            assert!((0..5).contains(&v), "{v}");
            seen[v as usize] = true;
        }
        assert!(
            seen.iter().all(|s| *s),
            "every residue should appear: {seen:?}"
        );
    }

    #[test]
    fn dice_reject_a_non_positive_bound() {
        let n = load();
        assert!(call(&n, "⌁↓⚀", Value::Int(0)).is_err());
        assert!(call(&n, "⌁↓⚀", Value::Int(-3)).is_err());
    }

    #[test]
    fn real_dice_are_in_the_unit_interval() {
        let n = load();
        for _ in 0..200 {
            let Ok(Value::Real(r)) = call(&n, "⌁↓⚁", Value::Unit) else {
                panic!("expected a Real");
            };
            assert!((0.0..1.0).contains(&r), "{r}");
        }
    }

    #[test]
    fn the_monotonic_clock_never_goes_backwards() {
        let n = load();
        let read = || match call(&n, "⌁↓⧖", Value::Unit) {
            Ok(Value::Int(t)) => t,
            other => panic!("expected an Int, got {other:?}"),
        };
        let (a, b) = (read(), read());
        assert!(b >= a);
    }

    #[test]
    fn the_wall_clock_is_after_2020() {
        let Ok(Value::Int(ms)) = call(&load(), "⌁↓⧗", Value::Unit) else {
            panic!("expected an Int");
        };
        assert!(ms > 1_577_836_800_000, "{ms}");
    }

    #[test]
    fn sleeping_a_negative_duration_raises() {
        assert!(call(&load(), "⌁↑⧗", Value::Int(-1)).is_err());
        assert!(matches!(
            call(&load(), "⌁↑⧗", Value::Int(0)),
            Ok(Value::Unit)
        ));
    }

    #[test]
    fn times_before_the_epoch_are_negative() {
        let before = UNIX_EPOCH - Duration::from_millis(1500);
        assert_eq!(epoch_millis(before), -1500);
        assert_eq!(epoch_millis(UNIX_EPOCH + Duration::from_millis(7)), 7);
    }

    #[test]
    fn below_is_uniform_enough_not_to_be_constant() {
        let draws: std::collections::HashSet<u64> = (0..200).map(|_| below(1000)).collect();
        assert!(draws.len() > 50);
    }
}
