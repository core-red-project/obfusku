//! Streams, clocks, randomness and file management (`ADR-027`).

use crate::support::{TempTree, bin, run_in};

fn run_with_stdin(dir: &std::path::Path, src: &str, input: &[u8]) -> std::process::Output {
    use std::io::Write;
    use std::process::Stdio;
    let file = dir.join("prog.obk");
    std::fs::write(&file, src).unwrap();
    let mut child = bin()
        .arg("run")
        .arg(&file)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn stdin_is_read_to_the_end_and_end_is_detectable() {
    let tree = TempTree::new();
    let src = "before ≔ ⌁⍰⎙(∅)\nall ≔ ⌁↓⎙(∅)\nafter ≔ ⌁⍰⎙(∅)\n\
        a ≔ ⌁↑⎙(⍕(before) ✚ \"|\" ✚ ⍕(⍴(all)) ✚ \"|\" ✚ ⍕(after))\nresult ≔ 1\n❧";
    let out = run_with_stdin(tree.root(), src, b"ab\ncd\n");
    assert!(out.status.success(), "{out:?}");
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        "◎|6|◉1\n",
        "no trailing newline from ⌁↑⎙, then the final value"
    );
}

#[test]
fn byte_streams_pass_non_utf8_through() {
    let tree = TempTree::new();
    let src = "a ≔ ⌁↑⎚(⌭⍪(⌁↓⎚(∅))(⌭⇋([0, 255])))\nresult ≔ 1\n❧";
    let out = run_with_stdin(tree.root(), src, &[1, 0xfe]);
    assert!(out.status.success(), "{out:?}");
    assert_eq!(&out.stdout[..4], &[1, 0xfe, 0, 255]);
}

#[test]
fn append_copy_move_size_and_remove_stay_inside_the_root() {
    let tree = TempTree::new();
    let src = "a ≔ ⌁⊕⌬(\"log.txt\")(\"ab\")\nb ≔ ⌁⊕⌬(\"log.txt\")(\"cd\")\n\
        c ≔ ⌁⧉⌬(\"log.txt\")(\"copy.txt\")\nd ≔ ⌁⇄⌬(\"copy.txt\")(\"moved.txt\")\n\
        e ≔ ⌁↑⌂(\"tree/deep\")\nf ≔ ⌁⊖⌂(\"tree\")\ng ≔ ⌁⊖⌬(\"log.txt\")\n\
        result ≔ ⌁⍰⌭(\"moved.txt\")\n❧";
    let out = run_in(tree.root(), src);
    assert!(out.status.success(), "{out:?}");
    assert_eq!(String::from_utf8(out.stdout).unwrap(), "4\n");
    assert!(!tree.root().join("log.txt").exists());
    assert!(!tree.root().join("tree").exists());
    assert_eq!(
        std::fs::read_to_string(tree.root().join("moved.txt")).unwrap(),
        "abcd"
    );
}

#[test]
fn removal_and_moves_cannot_escape_or_remove_the_root() {
    let tree = TempTree::new();
    for body in [
        "a ≔ ⌁⊖⌬(\"../x\")",
        "a ≔ ⌁⊖⌂(\".\")",
        "a ≔ ⌁⇄⌬(\"p.obk\")(\"../x\")",
    ] {
        let src = format!("{body}\nresult ≔ 1\n❧");
        let out = run_in(tree.root(), &src);
        assert_eq!(out.status.code(), Some(1), "{body}: {out:?}");
        assert!(String::from_utf8_lossy(&out.stderr).contains("PermissionDenied"));
    }
}

#[test]
fn clocks_and_dice_have_their_contracts() {
    let tree = TempTree::new();
    let src = "t0 ≔ ⌁↓⧖(∅)\ns ≔ ⌁↑⧗(5)\nt1 ≔ ⌁↓⧖(∅)\nw ≔ ⌁↓⧗(∅)\n\
        d ≔ ⌁↓⚀(6)\nu ≔ ⌁↓⚁(∅)\n\
        ok ≔ (t1 ≥ t0 ✚ 5000000) ∧ (w > 1600000000000) ∧ (d ≥ 0) ∧ (d < 6) ∧ (u ≥ 0.0) ∧ (u < 1.0)\n\
        result ≔ ⟡ ok { ◉ → 1 ⟢ ◎ → 0 }\n❧";
    let out = run_in(tree.root(), src);
    assert!(out.status.success(), "{out:?}");
    assert_eq!(String::from_utf8(out.stdout).unwrap(), "1\n");

    let bad = run_in(tree.root(), "a ≔ ⌁↓⚀(0)\nresult ≔ 1\n❧");
    assert_eq!(bad.status.code(), Some(1));
}
