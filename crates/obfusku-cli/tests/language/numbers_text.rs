//! Numbers, text and bytes (`ADR-027`).

use crate::support::{as_int, run_full_err, run_full_ok, run_str};
use obfusku_runtime::Value;

#[test]
fn the_minimum_integer_is_writable_and_survives_formatting() {
    let src = "m ≔ −9223372036854775808\nresult ≔ m\n❧";
    assert_eq!(as_int(run_full_ok(src)), i64::MIN);
    let mut map = obfusku_diagnostics::SourceMap::new();
    let id = map.add_file(src);
    let tokens = obfusku_syntax::lexer::tokenize(src, id).unwrap();
    let module = obfusku_syntax::parser::parse(&tokens, id).unwrap();
    assert!(obfusku_fmt::format(&module).contains("−9223372036854775808"));
    let bare = run_full_err("m ≔ 9223372036854775808\nresult ≔ m\n❧");
    assert!(bare[0].message.contains("out of range"), "{bare:?}");
}

#[test]
fn integer_natives_check_overflow_and_domains() {
    assert_eq!(
        as_int(run_full_ok(
            "result ≔ ⋆(3)(4) ✚ ∥(−5) ✚ ⊓(6)(3) ✚ ⊔(6)(3) ✚ ⊗(6)(3)\n❧"
        )),
        81 + 5 + 2 + 7 + 5
    );
    assert_eq!(
        as_int(run_full_ok("result ≔ ⋙(−8)(1) ✚ ⋘(1)(62) ✚ ∼(0)\n❧")),
        -4 + (1 << 62) - 1
    );
    for bad in [
        "result ≔ ⋆(2)(63)\n❧",
        "result ≔ ∥(−9223372036854775808)\n❧",
        "result ≔ ⋆(2)(−1)\n❧",
        "result ≔ ⋘(1)(64)\n❧",
    ] {
        assert!(!run_full_err(bad).is_empty(), "{bad}");
    }
    assert_eq!(as_int(run_full_ok("result ≔ ⋆(−2)(63)\n❧")), i64::MIN);
}

#[test]
fn min_and_max_use_the_generated_order() {
    assert_eq!(
        run_str("result ≔ ⊥(\"pear\")(\"fig\") ✚ ⊤(\"pear\")(\"fig\")\n❧"),
        "figpear"
    );
    assert_eq!(as_int(run_full_ok("result ≔ ⊤(⊥(7)(3))(2)\n❧")), 3);
    let ds = run_full_err("result ≔ ⊥(λ(x: ⟁) → x)(λ(x: ⟁) → x)\n❧");
    assert!(!ds.is_empty());
}

#[test]
fn real_functions_follow_ieee_and_never_raise() {
    let v = run_full_ok(
        "result ≔ ⍕(⌊(−1.5)) ✚ \"|\" ✚ ⍕(⌃(1.2)) ✚ \"|\" ✚ ⍕(⌾(2.5)) ✚ \"|\" ✚ ⍕(⍥(√(−1.0))) ✚ \"|\" ✚ ⍕(⋇(2.0)(10.0)) ✚ \"|\" ✚ ⍕(∞)\n❧",
    );
    match &v {
        Value::Str(s) => assert_eq!(&**s, "\"-2.0|2.0|3.0|◎|1024.0|∞\"".trim_matches('"')),
        other => panic!("{other:?}"),
    }
}

#[test]
fn text_functions_count_scalars_and_graphemes() {
    assert_eq!(
        run_str("result ≔ ⍕(⍸(\"é\")(\"aéé\")) ✚ ⇩(\"ÀB\") ✚ ⍕(⌯(\"A\")) ✚ ⌮(66)\n❧"),
        "⧫(1)àb65B"
    );
    assert_eq!(
        as_int(run_full_ok(
            "result ≔ #(⇋(⌶(\"e\\u{301}👍🏽\"))) ✚ #(⇋(⍂(\"e\\u{301}👍🏽\")))\n❧"
        )),
        2 + 4
    );
    assert!(!run_full_err("result ≔ ⌮(55296)\n❧").is_empty());
    assert!(!run_full_err("result ≔ ⤳(\"\")(\"x\")(\"abc\")\n❧").is_empty());
}

#[test]
fn bytes_are_a_typed_ordered_value() {
    assert_eq!(run_str("result ≔ ⍕(⌭⌘(\"hi\"))\n❧"), "⌭\"6869\"");
    assert_eq!(
        as_int(run_full_ok("result ≔ ⌭#(⌭⍪(⌭⌘(\"ab\"))(⌭⊠(3)(7)))\n❧")),
        5
    );
    assert_eq!(
        as_int(run_full_ok(
            "b ≔ ⌭⌘(\"a\")\nresult ≔ ⟡ (b ≡ ⌭⇋([97])) { ◉ → 1 ⟢ ◎ → 0 }\n❧"
        )),
        1
    );
    let ds = run_full_err("result ≔ ⌭#([1])\n❧");
    assert!(!ds.is_empty(), "an Array is not bytes");
}
