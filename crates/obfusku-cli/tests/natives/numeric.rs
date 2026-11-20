//! `↗` and `↘`: the explicit `Int`/`Real` conversions and their three-way error contract.

use crate::support::{bin, write_temp};

#[test]
fn to_real_widens_an_int_and_participates_in_real_arithmetic() {
    let f = write_temp("x ≔ 5\nresult ≔ ↗(x) ✚ 2.5\n\u{2767}\n");
    let out = bin().arg("run").arg(f.path()).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("7.5"), "{stdout}");
}

#[test]
fn to_int_truncates_toward_zero() {
    let f = write_temp("result ≔ ↘(7.9)\n\u{2767}\n");
    let out = bin().arg("run").arg(f.path()).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains('7'), "{stdout}");
}

#[test]
fn to_int_truncates_negative_fractions_toward_zero_not_down() {
    let f = write_temp("result ≔ ↘(0.0 ☠︎ 7.9)\n\u{2767}\n");
    let out = bin().arg("run").arg(f.path()).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    // -7 (truncation), not -8 (floor) — proves toward-zero, not floor.
    assert!(stdout.contains("-7"), "{stdout}");
}

#[test]
fn to_int_on_infinity_is_invalid_operation_not_integer_overflow() {
    let f = write_temp("result ≔ ↘(1.0 ÷ 0.0)\n\u{2767}\n");
    let out = bin().arg("run").arg(f.path()).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.contains("InvalidOperation"), "{stderr}");
}

#[test]
fn to_int_out_of_i64_range_is_integer_overflow_not_invalid_operation() {
    let f = write_temp("result ≔ ↘(1.0e300)\n\u{2767}\n");
    let out = bin().arg("run").arg(f.path()).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.contains("IntegerOverflow"), "{stderr}");
}
