//! `Str` combinators and argument-flag classification (`ADR-023`, `ADR-024`).

use crate::support::{TempTree, bin};

#[test]
fn text_natives_split_trim_parse_and_render_end_to_end() {
    let tree = TempTree::new();
    // parts = ⫽(",", "a, b ,c"); shown = ⌁↑(⫴("-", ⟐(⌫, parts)))
    // n = ↦("12") unwrapped, printed with ⍕
    let src = "parts \u{2254} \u{2AFD}(\u{22},\u{22}, \u{22}a, b ,c\u{22})\n\
        joined \u{2254} \u{2AF4}(\u{22}-\u{22}, \u{27D0}(\u{232B}, parts))\n\
        a \u{2254} \u{2301}\u{2191}(joined)\n\
        n \u{2254} \u{27E1} \u{21A6}(\u{22}12\u{22}) {\n  \u{2713}(v) \u{2192} v\n  \
        \u{27E2} \u{2717}(m) \u{2192} 0\n}\n\
        b \u{2254} \u{2301}\u{2191}(\u{2355}(n \u{271A} 1))\n\
        result \u{2254} 1\n\u{2767}\n";
    let file = tree.write("text.obk", src);
    let out = bin().arg("run").arg(&file).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    assert_eq!(String::from_utf8(out.stdout).unwrap(), "a-b-c\n13\n1\n");
}

#[test]
fn flags_are_classified_for_the_program() {
    let tree = TempTree::new();
    let src = "\u{3BB}describe(a: ⌑): \u{2318} \u{2192} \u{27E1} a {\n  \
        ➤(k, v) \u{2192} \u{27E1} v {\n    \u{29B0} \u{2192} k\n    \
        \u{27E2} \u{29EB}(x) \u{2192} k \u{271A} \u{22}=\u{22} \u{271A} x\n  }\n  \
        \u{27E2} ◇(p) \u{2192} \u{22}_\u{22} \u{271A} p\n}\n\
        shown \u{2254} \u{2301}\u{2191}(\u{2AF4}(\u{22} \u{22}, \
        \u{27D0}(describe, \u{2691}(\u{2301}\u{2193}\u{2325}(\u{2205})))))\n\
        result \u{2254} 1\n\u{2767}\n";
    let file = tree.write("flags.obk", src);
    let out = bin()
        .arg("run")
        .arg(&file)
        .args(["--", "--depth=3", "-v", "file", "--", "--raw"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        "depth=3 v _file _--raw\n1\n"
    );
}
