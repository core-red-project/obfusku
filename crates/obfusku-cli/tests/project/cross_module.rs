//! Types and values crossing a module boundary: ADT pattern matching and exhaustiveness, records, exceptions, generics and annotations across imports (`ADR-025`, `ADR-026`).

use crate::support::{TempObkDir, TempTree, bin, write_temp};

#[test]
fn pattern_matching_against_a_locally_declared_adt_is_unaffected() {
    // Baseline: the case that always worked, confirming the fix didn't
    // disturb it.
    let src = "Shape \u{2254} { Circle(\u{27C1}) \u{27E2} Square(\u{27C1}) }\n\
               result \u{2254} \u{27E1} Circle(2) {\n \
               Circle(r) \u{2192} r\n \u{27E2} Square(s) \u{2192} s\n}\n\u{2767}\n";
    let out = bin()
        .arg("run")
        .arg(write_temp(src).path())
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains('2'), "{stdout}");
}

#[test]
fn an_imported_constructor_can_still_be_constructed_as_a_plain_value() {
    let dir = TempObkDir::new();
    dir.write(
        "shapes",
        "Shape \u{2254} { Circle(\u{27C1}) \u{27E2} Square(\u{27C1}) }\n\u{2767}\n",
    );
    let main = dir.write(
        "main",
        "\u{27F2}shapes\nresult \u{2254} Circle(5)\n\u{2767}\n",
    );
    let out = bin().arg("run").arg(&main).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("Circle(5)"), "{stdout}");
}

#[test]
fn pattern_matching_against_an_imported_adts_constructors_works() {
    // The exact bug: this used to fail with "unknown constructor
    // 'Circle' in pattern" even though `Circle` imports and constructs
    // fine — matching against it needs the ADT's *type* metadata, not
    // just the constructor function's `Scheme`.
    let dir = TempObkDir::new();
    dir.write(
        "shapes",
        "Shape \u{2254} { Circle(\u{27C1}) \u{27E2} Square(\u{27C1}) }\n\u{2767}\n",
    );
    let main = dir.write(
        "main",
        "\u{27F2}shapes\n\
         result \u{2254} \u{27E1} Circle(7) {\n \
         Circle(r) \u{2192} r\n \u{27E2} Square(s) \u{2192} s\n}\n\u{2767}\n",
    );
    let out = bin().arg("run").arg(&main).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains('7'), "{stdout}");
}

#[test]
fn exhaustiveness_checking_works_against_an_imported_adt() {
    // A missing arm for an *imported* ADT's variant must be caught,
    // proving exhaustiveness (not just single-tag resolution) has the
    // imported ADT's full variant set, not only whichever tag the
    // pattern happened to name.
    let dir = TempObkDir::new();
    dir.write(
        "shapes",
        "Shape \u{2254} { Circle(\u{27C1}) \u{27E2} Square(\u{27C1}) }\n\u{2767}\n",
    );
    let main = dir.write(
        "main",
        "\u{27F2}shapes\n\
         result \u{2254} \u{27E1} Circle(7) {\n \
         Circle(r) \u{2192} r\n}\n\u{2767}\n",
    );
    let out = bin().arg("check").arg(&main).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.contains("non-exhaustive"), "{stderr}");
    assert!(stderr.contains("Square"), "{stderr}");
}

#[test]
fn an_imported_generic_adt_pattern_matches_correctly_at_two_different_instantiations() {
    // Mirrors P0-B's own concern one level down: a generic ADT's
    // `VariantInfo.type_params` are bare `String` names
    // (`CoreType::Param`), never a `Checker`-relative `TypeVarId`, so
    // merging registry data built by the exporting module's own
    // `Checker` carries none of `Scheme`'s cross-`Checker` identity
    // hazard — confirmed empirically here by reusing the same imported
    // generic ADT at `Int` and at `Str` in the *same* importing module.
    let dir = TempObkDir::new();
    dir.write(
        "boxes",
        "Box t \u{2254} { Empty \u{27E2} Full(t) }\n\u{2767}\n",
    );
    let main = dir.write(
        "main",
        "\u{27F2}boxes\n\
         unwrapOr \u{2254} \u{3bb}(b, default) \u{2192} \u{27E1} b {\n \
         Empty \u{2192} default\n \u{27E2} Full(x) \u{2192} x\n}\n\
         a \u{2254} unwrapOr(Full(1), 0)\n\
         b \u{2254} unwrapOr(Full(\u{22}hi\u{22}), \u{22}\u{22})\n\
         result \u{2254} b\n\u{2767}\n",
    );
    let out = bin().arg("run").arg(&main).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("\"hi\""), "{stdout}");
}

#[test]
fn stdlib_list_pattern_matching_works_without_any_local_declaration() {
    // The exact case named as mandatory: `⊘`/`\u{205D}` come from the
    // *ambient* stdlib prelude (no `⟲`, no local `List` declaration at
    // all) — previously this failed with "unknown constructor '⊘' in
    // pattern" despite `List<T>` being recorded as "Implemented,
    // verified end-to-end" in ROADMAP.md. That verification, in
    // hindsight, only ever exercised a *locally re-declared* `List`
    // (see `crates/obfusku-cli/tests/e2e.rs`'s own `list_adt_end_to_end_
    // via_explicit_type_application`), never the real ambient path a
    // user program actually takes — this is the test that closes that
    // coverage gap.
    let src = "xs \u{2254} \u{205D}(1, \u{205D}(2, ⊘))\n\
               result \u{2254} \u{27E1} xs {\n \
               ⊘ \u{2192} 0\n \u{27E2} \u{205D}(h, t) \u{2192} h\n}\n\u{2767}\n";
    let out = bin()
        .arg("run")
        .arg(write_temp(src).path())
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains('1'), "{stdout}");
}

#[test]
fn two_imports_each_declaring_their_own_unrelated_adt_do_not_collide() {
    let dir = TempObkDir::new();
    dir.write("shapes", "Shape \u{2254} { Circle(\u{27C1}) }\n\u{2767}\n");
    dir.write(
        "colors",
        "Color \u{2254} { Red \u{27E2} Green \u{27E2} Blue }\n\u{2767}\n",
    );
    let main = dir.write(
        "main",
        "\u{27F2}shapes\n\u{27F2}colors\n\
         c \u{2254} Green\n\
         result \u{2254} \u{27E1} c {\n \
         Red \u{2192} \u{22}r\u{22}\n \u{27E2} Green \u{2192} \u{22}g\u{22}\n \u{27E2} Blue \u{2192} \u{22}b\u{22}\n}\n\u{2767}\n",
    );
    let out = bin().arg("run").arg(&main).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("\"g\""), "{stdout}");
}

#[test]
fn record_shaped_adt_self_tag_does_not_falsely_collide_with_itself() {
    // Regression for a bug introduced and caught while building this
    // very matrix: a record/tuple `TypeDeclaration` deliberately
    // self-tags (its one synthesized variant's tag equals its own
    // type's tag) — the collision check must treat ADT names and
    // constructor tags as separate namespaces, or every record/tuple
    // type would spuriously collide with itself.
    let src = "Point \u{2254} { x: \u{27C1} \u{27E2} y: \u{27C1} }\n\
               p \u{2254} Point { x: 1 \u{27E2} y: 2 }\n\
               result \u{2254} \u{27E1} p {\n \
               Point { x: a \u{27E2} y: b } \u{2192} a \u{271A} b\n}\n\u{2767}\n";
    let out = bin()
        .arg("run")
        .arg(write_temp(src).path())
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains('3'), "{stdout}");
}

#[test]
fn an_exported_self_recursive_function_is_importable_and_callable() {
    // P1-2b: the case FunctionDeclaration export exists to make
    // possible at all — a self-recursive function (only expressible via
    // FunctionDeclaration/LetRec, never ValueDeclaration) exported from
    // one module, imported and called from another, correct result.
    let dir = TempObkDir::new();
    dir.write(
        "mathutil",
        "λfactorial(n: ⟁): ⟁ →⟳\n\
         \u{27E1} n {\n\
         0 \u{2192} 1\n\
         \u{27E2} _ \u{2192} n \u{2731} factorial(n \u{2620}\u{FE0E} 1)\n\
         }\n\u{2767}\n",
    );
    let main = dir.write(
        "main",
        "\u{27F2}mathutil\nresult \u{2254} factorial(5)\n\u{2767}\n",
    );
    let out = bin().arg("run").arg(&main).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("120"), "{stdout}");
}

#[test]
fn an_exported_generic_function_is_importable_and_reusable_at_two_types() {
    // P1-2b, second half: proves the FunctionDeclaration -> LetRec ->
    // Scheme -> module Prelude -> instantiate path (hardened by the
    // P0-B cross-Checker TypeVarId fix) handles a genuinely exported
    // generic function correctly, not just a monomorphic one.
    let dir = TempObkDir::new();
    dir.write("identity", "λid(x: t): t \u{2192}\u{27F3} x\n\u{2767}\n");
    let main = dir.write(
        "main",
        "\u{27F2}identity\na \u{2254} id(5)\nb \u{2254} id(\"hi\")\nresult \u{2254} b\n\u{2767}\n",
    );
    let out = bin().arg("run").arg(&main).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("\"hi\""), "{stdout}");
}

#[test]
fn value_declaration_annotation_composes_with_mutability_and_export_across_a_module_boundary() {
    // Annotation + Mutability + Export together, exercised through a
    // real import: the element type is what's annotated, not Cell<T>
    // (desugar wraps the annotation to match the MutCell-wrapped value).
    let dir = TempObkDir::new();
    dir.write(
        "counter",
        "x: \u{27C1} \u{2254}\u{02da}\u{27F3} 0\n\u{2767}\n",
    );
    let main = dir.write("main", "\u{27F2}counter\nresult \u{2254} 1\n\u{2767}\n");
    let out = bin().arg("check").arg(&main).output().unwrap();
    assert!(out.status.success(), "{out:?}");
}

#[test]
fn unannotated_lambda_parameter_still_generalizes_across_two_types() {
    let src = "id \u{2254} \u{3bb}(x) \u{2192} x\na \u{2254} id(5)\nb \u{2254} id(\"hi\")\nresult \u{2254} b\n\u{2767}\n";
    let out = bin()
        .arg("run")
        .arg(write_temp(src).path())
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("\"hi\""), "{stdout}");
}

#[test]
fn shared_type_variable_across_one_parameter_list_forces_both_arguments_equal() {
    // The critical case from the P0 matrix: it's not enough to show
    // `f ≔ λ(x: t, y: t) → x` parses — an application that respects
    // "x and y share a type" must succeed, and one that violates it
    // must fail, or the shared-fresh-var mechanism could be a no-op.
    let src = "f \u{2254} \u{3bb}(x: t, y: t) \u{2192} x\nresult \u{2254} f(1, 2)\n\u{2767}\n";
    let out = bin()
        .arg("run")
        .arg(write_temp(src).path())
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains('1'), "{stdout}");

    let bad_src =
        "f \u{2254} \u{3bb}(x: t, y: t) \u{2192} x\nresult \u{2254} f(1, \"hi\")\n\u{2767}\n";
    let bad_out = bin()
        .arg("check")
        .arg(write_temp(bad_src).path())
        .output()
        .unwrap();
    assert_eq!(bad_out.status.code(), Some(1));
}

#[test]
fn lambda_parameter_annotation_composes_with_export_and_import() {
    let dir = TempObkDir::new();
    dir.write(
        "util",
        "makeAdder \u{2254}\u{27F3} \u{3bb}(n: \u{27C1}) \u{2192} \u{3bb}(x: \u{27C1}) \u{2192} x \u{271A} n\n\u{2767}\n",
    );
    let main = dir.write(
        "main",
        "\u{27F2}util\nadd5 \u{2254} makeAdder(5)\nresult \u{2254} add5(10)\n\u{2767}\n",
    );
    let out = bin().arg("run").arg(&main).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("15"), "{stdout}");
}

#[test]
fn records_and_exceptions_cross_a_module_boundary() {
    let tree = TempTree::new();
    tree.manifest("proj", "main");
    tree.write(
        "proj/Shapes.obk",
        "Point ≔ { x: ⟁ ⟢ y: ⟁ }\n☈ BadPoint(⌘)\n\
         origin ≔⟳ Point { x: 0 ⟢ y: 0 }\n\
         λmove(p: Point): Point →⟳ p.{ x: p.x ✚ 1 }\n\
         λcheck(n: ⟁): ⟁ →⟳ ⟡ (n < 0) { ◉ → ☄ BadPoint(\"neg\") ⟢ ◎ → n }\n\
         names ≔⟳ ⁝(\"a\", ⊘)\n❧",
    );
    tree.write(
        "proj/main.obk",
        "⟲ Shapes\nmoved ≔ move(origin)\np ≔ Point { x: 7 ⟢ y: 8 }\n\
         r ≔ ☊ check(−1) λ(e) → ⟡ e { BadPoint(m) → 99 ⟢ _ → 0 }\n\
         result ≔ moved.x ✚ p.y ✚ r\n❧",
    );
    let out = bin()
        .arg("run")
        .arg(tree.root().join("proj"))
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    assert!(String::from_utf8(out.stdout).unwrap().contains("108"));
}
