//! Language behaviour observable at the command-line boundary: overflow, partial application holes, annotations, lexical edge cases and runtime limits.

use crate::support::{TempObkDir, TempTree, bin, write_temp};

#[test]
fn uncaught_integer_overflow_is_a_clean_diagnostic_not_a_crash() {
    let f = write_temp("result \u{2254} 9223372036854775807 \u{271a} 1\n\u{2767}\n");
    let out = bin().arg("run").arg(f.path()).output().unwrap();
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.contains("IntegerOverflow"), "{stderr}");
}

#[test]
fn integer_overflow_is_catchable_by_the_existing_raise_catch_mechanism() {
    let f = write_temp(
        "result \u{2254} \u{260a} (9223372036854775807 \u{271a} 1) \u{3bb}(e) \u{2192} \u{27e1} e {\n  \
         IntegerOverflow \u{2192} 42\n  \u{27e2} DivisionByZero \u{2192} 0\n  \
         \u{27e2} NonExhaustiveMatch \u{2192} 0\n  \u{27e2} InvalidOperation(m) \u{2192} 0\n  \
         \u{27e2} Failure(t, m) \u{2192} 0\n  \u{27e2} _ \u{2192} 0\n}\n\u{2767}\n",
    );
    let out = bin().arg("run").arg(f.path()).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("42"), "{stdout}");
}

#[test]
fn trailing_hole_produces_the_same_result_as_ordinary_currying() {
    let f = write_temp(
        "\u{3bb}f(a: \u{27c1}, b: \u{27c1}, c: \u{27c1}): \u{27c1} \u{2192} a \u{271a} b \u{2731} c\n\
         partial \u{2254} f(1, \u{2022}, 3)\n\
         result \u{2254} partial(2)\n\u{2767}\n",
    );
    let out = bin().arg("run").arg(f.path()).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    // 1 + 2*3 = 7
    assert!(stdout.contains('7'), "{stdout}");
}

#[test]
fn two_holes_fill_positions_left_to_right_not_reversed() {
    // Non-commutative operator so a wrong hole order would give a
    // different, observably wrong answer: 10 - 1 - 100 = -91.
    let f = write_temp(
        "\u{3bb}sub3(a: \u{27c1}, b: \u{27c1}, c: \u{27c1}): \u{27c1} \u{2192} a \u{2620}\u{fe0e} b \u{2620}\u{fe0e} c\n\
         partial \u{2254} sub3(\u{2022}, \u{2022}, 100)\n\
         result \u{2254} partial(10)(1)\n\u{2767}\n",
    );
    let out = bin().arg("run").arg(f.path()).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("-91"), "{stdout}");
}

#[test]
fn two_holes_on_an_arity_three_function_build_a_genuine_pending_chain() {
    // The critical case: `f(•, •)` leaves ONE parameter (c) still
    // unsupplied via ordinary currying — proving this builds a real
    // curried-application chain, not "one hole = one lambda" applied
    // ad hoc. 10 - 1 - 1 = 8.
    let f = write_temp(
        "\u{3bb}sub3(a: \u{27c1}, b: \u{27c1}, c: \u{27c1}): \u{27c1} \u{2192} a \u{2620}\u{fe0e} b \u{2620}\u{fe0e} c\n\
         partial \u{2254} sub3(\u{2022}, \u{2022})\n\
         result \u{2254} partial(10)(1)(1)\n\u{2767}\n",
    );
    let out = bin().arg("run").arg(f.path()).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains('8'), "{stdout}");
}

#[test]
fn a_hole_inside_a_nested_call_belongs_to_that_call_not_the_outer_one() {
    // outer = f(•, g(3, •)) — f's own hole and g's own hole must
    // resolve completely independently. outer(5) = 5 + g(3, 10) =
    // 5 + 30 = 35.
    let f = write_temp(
        "\u{3bb}g(x: \u{27c1}, y: \u{27c1}): \u{27c1} \u{2192} x \u{2731} y\n\
         \u{3bb}f(a: \u{27c1}, cb: \u{27c1} \u{2192} \u{27c1}): \u{27c1} \u{2192} a \u{271a} cb(10)\n\
         outer \u{2254} f(\u{2022}, g(3, \u{2022}))\n\
         result \u{2254} outer(5)\n\u{2767}\n",
    );
    let out = bin().arg("run").arg(f.path()).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("35"), "{stdout}");
}

#[test]
fn a_pipe_ref_and_a_hole_in_the_same_stage_is_a_static_error() {
    let f = write_temp(
        "\u{3bb}pair(x: \u{27c1}, y: \u{27c1}): \u{27c1} \u{2192} x \u{271a} y\n\
         xs \u{2254} 5\n\
         result \u{2254} xs \u{25b7} (pair(\u{25c8}, \u{2022}))\n\u{2767}\n",
    );
    let out = bin().arg("check").arg(f.path()).output().unwrap();
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        stderr.contains('\u{25c8}') && stderr.contains('\u{2022}'),
        "{stderr}"
    );
}

#[test]
fn too_many_arguments_without_any_holes_is_unaffected_by_this_slice() {
    let f = write_temp(
        "\u{3bb}f(a: \u{27c1}, b: \u{27c1}): \u{27c1} \u{2192} a \u{271a} b\n\
         result \u{2254} f(1, 2, 3)\n\u{2767}\n",
    );
    let out = bin().arg("check").arg(f.path()).output().unwrap();
    assert_eq!(out.status.code(), Some(1), "{out:?}");
}

#[test]
fn a_local_generic_function_reused_at_two_types_type_checks_with_the_ambient_stdlib_prelude() {
    // P0-B's original repro, through the real binary: `stdlib::load()`
    // seeds several of its own polymorphic schemes (`map`/`filter`/
    // `\u{233D}`/`⊘`/`\u{205D}`) from a completely separate `Checker` before
    // this file is ever type-checked — a local generic function must
    // still generalize and be reused at two different types despite
    // that ambient prelude being present.
    let f = write_temp(
        "\u{3bb}id(x: t): t \u{2192} x\n\
         a \u{2254} id(5)\n\
         b \u{2254} id(\u{22}hi\u{22})\n\
         result \u{2254} b\n\u{2767}\n",
    );
    let out = bin().arg("run").arg(f.path()).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("\"hi\""), "{stdout}");
}

#[test]
fn an_unused_local_generic_function_checks_cleanly_with_the_ambient_stdlib_prelude() {
    let f = write_temp("\u{3bb}id(x: t): t \u{2192} x\nresult \u{2254} 5\n\u{2767}\n");
    let out = bin().arg("check").arg(f.path()).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("ok"), "{stdout}");
}

#[test]
fn a_local_generic_function_still_generalizes_alongside_an_imported_generic_value() {
    // P0-B regression, real end-to-end: an imported module's own
    // generic value and the importer's own, unrelated generic function
    // are typechecked by two genuinely different `Checker` instances
    // (`modules::resolve_module_uncached` recursively calls
    // `check_with_prelude` per imported file) — the importer's local
    // `id2` must still generalize correctly regardless of whatever
    // `TypeVarId` numbering the imported module's own checker happened
    // to use for its own generic value.
    let dir = TempObkDir::new();
    dir.write(
        "Lib",
        "idLib \u{2254}\u{27F3} \u{3bb}(x) \u{2192} x\n\u{2767}\n",
    );
    let main = dir.write(
        "main",
        "\u{27F2}Lib\n\
         \u{3bb}id2(x: t): t \u{2192} x\n\
         a \u{2254} id2(5)\n\
         b \u{2254} id2(\u{22}hi\u{22})\n\
         result \u{2254} b\n\u{2767}\n",
    );
    let out = bin().arg("run").arg(&main).output().unwrap();
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("\"hi\""), "{stdout}");
}

#[test]
fn export_immediately_after_lambda_stays_invalid_for_function_declarations() {
    // The superseded candidate '\u{03bb}\u{27f3}f(...)' stays rejected —
    // '\u{27f3}' after the return-type arrow is the only accepted form.
    let src = "\u{03bb}\u{27F3}f(x: \u{27C1}): \u{27C1} \u{2192} x\n\u{2767}\n";
    let out = bin()
        .arg("check")
        .arg(write_temp(src).path())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.contains("function name"), "{stderr}");
}

#[test]
fn value_declaration_annotation_rejects_a_genuinely_incompatible_value() {
    // P1-3a's critical regression: the annotation must be a real
    // constraint, not documentation the checker silently ignores.
    let src = "x: \u{27C1} \u{2254} \"hi\"\n\u{2767}\n";
    let out = bin()
        .arg("check")
        .arg(write_temp(src).path())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn value_declaration_annotation_does_not_disturb_valid_inference() {
    let src = "x: \u{27C1} \u{2254} 5\nresult \u{2254} x \u{271A} 1\n\u{2767}\n";
    let out = bin()
        .arg("run")
        .arg(write_temp(src).path())
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains('6'), "{stdout}");
}

#[test]
fn type_annotation_on_a_local_binding_is_a_clear_static_error_not_a_confusing_parse_failure() {
    let src = "result \u{2254}\n y: \u{27C1} \u{2254} 5\n y\n\u{2767}\n";
    let out = bin()
        .arg("check")
        .arg(write_temp(src).path())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.contains("not valid on a local binding"), "{stderr}");
}

#[test]
fn lambda_parameter_annotation_rejects_a_genuinely_incompatible_body() {
    let src = "f \u{2254} \u{3bb}(x: \u{27C1}) \u{2192} x \u{271A} \"x\"\nresult \u{2254} f(5)\n\u{2767}\n";
    let out = bin()
        .arg("check")
        .arg(write_temp(src).path())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn lambda_parameter_annotation_does_not_disturb_valid_inference() {
    let src =
        "f \u{2254} \u{3bb}(x: \u{27C1}) \u{2192} x \u{271A} 1\nresult \u{2254} f(5)\n\u{2767}\n";
    let out = bin()
        .arg("run")
        .arg(write_temp(src).path())
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains('6'), "{stdout}");
}

#[test]
fn two_independent_top_level_lambdas_reusing_a_type_variable_spelling_never_unify() {
    // Post-fix audit case: `lambda_param_vars` is a checker-lifetime
    // cache keyed by the *renamed* name, not the original spelling —
    // this is the test that would fail if desugar's per-parameter-list
    // renaming were ever bypassed or reused across unrelated top-level
    // bindings. `f` and `g` are two separate top-level `ValueDeclaration`s
    // (not nested lambdas), each independently spelling its parameter's
    // annotation `t` — applying them at genuinely incompatible types in
    // the same module/Checker must succeed, or `t` is leaking identity
    // across signatures that share nothing but a spelling.
    let src = "f \u{2254} \u{3bb}(x: t) \u{2192} x\ng \u{2254} \u{3bb}(y: t) \u{2192} y\na \u{2254} f(5)\nresult \u{2254} g(\"hi\")\n\u{2767}\n";
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
fn same_spelled_type_variable_in_two_separately_written_lambdas_stays_independent() {
    // `λ(x: t) → λ(y: t) → …` — two SEPARATE Lambda literals, not one
    // parameter list; despite reusing the spelling `t`, x and y must be
    // independently typeable.
    let src = "f \u{2254} \u{3bb}(x: t) \u{2192} \u{3bb}(y: t) \u{2192} x\ninner \u{2254} f(1)\nresult \u{2254} inner(\"hi\")\n\u{2767}\n";
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
fn non_nfc_source_file_is_a_clean_lexical_error_not_a_silent_pass() {
    // A real file on disk with an NFD-decomposed "é" in a string literal
    // — CONCRETE_SYMBOLIC_GRAMMAR.md §2 requires this rejected as a
    // lexical error, not silently renormalized.
    let src = "x \u{2254} \"caf\u{65}\u{301}\"\nresult \u{2254} x\n\u{2767}\n";
    let out = bin()
        .arg("check")
        .arg(write_temp(src).path())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.contains("Normalization Form C"), "{stderr}");
}

#[test]
fn an_exponent_without_a_fraction_makes_a_real_literal() {
    // `CONCRETE_SYMBOLIC_GRAMMAR.md` §5, `ADR-027`: `5e3` is a `Real`;
    // an `e` with no digits after it stays an identifier.
    let src = "result \u{2254} 5e3 \u{271A} 1_0.5 \u{271A} 0x10\n\u{2767}\n";
    let out = bin()
        .arg("run")
        .arg(write_temp(src).path())
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(1),
        "Real + Int must not mix: {out:?}"
    );
    let src = "result \u{2254} 5e3 \u{271A} 1_0.5\n\u{2767}\n";
    let out = bin()
        .arg("run")
        .arg(write_temp(src).path())
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    assert!(String::from_utf8(out.stdout).unwrap().contains("5010.5"));
}

#[test]
fn a_dotted_real_literal_with_an_exponent_is_unaffected() {
    let src = "result \u{2254} 5.0e10\n\u{2767}\n";
    let out = bin()
        .arg("run")
        .arg(write_temp(src).path())
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("50000000000.0"), "{stdout}");
}

#[test]
fn runaway_recursion_is_a_clean_error_not_a_stack_overflow() {
    let tree = TempTree::new();
    let file = tree.write(
        "deep.obk",
        "\u{3BB}f(n: \u{27C1}): \u{27C1} \u{2192} 1 \u{271A} f(n)\nresult \u{2254} f(1)\n\u{2767}\n",
    );
    let out = bin().arg("run").arg(file).output().unwrap();
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stderr).contains("recursion limit exceeded"));
}

#[test]
fn recursion_through_a_native_callback_is_a_clean_error() {
    let tree = TempTree::new();
    let file = tree.write(
        "deep_native.obk",
        "λstep(n: ⟁): ⟁ → ⟡ (n ≡ 0) {\n  ◉ → 0\n  ⟢ ◎ → ⌽(add, 0, ⁝(n ☠︎ 1, ⊘))\n}\n\
         λadd(acc: ⟁, x: ⟁): ⟁ → acc ✚ step(x)\nresult ≔ step(5000000)\n❧\n",
    );
    let out = bin().arg("run").arg(file).output().unwrap();
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stderr).contains("recursion limit exceeded"));
}
