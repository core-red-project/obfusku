//! Conformance tests for `spec/CONCRETE_SYMBOLIC_GRAMMAR.md`'s parser:
//! literals, bindings, lambda, application, pipeline, `FunctionDeclaration`,
//! `Match`/`Constructor`, operators, types, and `LocalBinding`.

use obfusku_diagnostics::SourceMap;
use obfusku_syntax::ast::{Argument, Declaration, Expression, Stage};
use obfusku_syntax::{lexer, parser};

/// Parses `source` and panics with the diagnostics if it fails —
/// convenience for the "should succeed" tests below.
fn parse_ok(source: &str) -> obfusku_syntax::ast::Module {
    let mut map = SourceMap::new();
    let id = map.add_file(source);
    let tokens = lexer::tokenize(source, id).unwrap_or_else(|d| panic!("lex error: {d:?}"));
    parser::parse(&tokens, id).unwrap_or_else(|ds| panic!("parse error(s): {ds:?}"))
}

/// Parses `source` and returns the error — convenience for the "should
/// fail" tests below.
fn parse_err(source: &str) -> String {
    let mut map = SourceMap::new();
    let id = map.add_file(source);
    match lexer::tokenize(source, id) {
        Err(d) => d.message,
        Ok(tokens) => match parser::parse(&tokens, id) {
            Err(ds) => ds[0].message.clone(),
            Ok(m) => panic!("expected a parse error, but got a valid module: {m:?}"),
        },
    }
}

// ── valid programs ─────────────────────────────────────────────────────

#[test]
fn smallest_legal_program_is_an_empty_sealed_module() {
    let module = parse_ok("❧");
    assert!(module.declarations.is_empty());
}

#[test]
fn one_binding_with_an_int_literal() {
    let module = parse_ok("x ≔ 5\n❧");
    assert_eq!(module.declarations.len(), 1);
    let Declaration::Value(decl) = &module.declarations[0] else {
        panic!("expected a Value declaration")
    };
    assert_eq!(decl.name, "x");
    assert!(!decl.mutable);
    assert!(!decl.exported);
    assert!(matches!(decl.value, Expression::Int(5, _)));
}

#[test]
fn real_string_and_bool_literals() {
    let module = parse_ok(
        r#"
        a ≔ 3.14
        b ≔ "hello"
        c ≔ ◉
        d ≔ ◎
        ❧
        "#,
    );
    assert_eq!(module.declarations.len(), 4);
}

#[test]
fn unit_literal_in_expression_position_parses_as_expression_unit() {
    // `∅` in expression position is the one Unit value — the same
    // glyph that's `TypeRef::Base(BaseType::Unit, _)` in type position,
    // disambiguated purely by where it occurs.
    let module = parse_ok("x ≔ ∅\n❧");
    let Declaration::Value(decl) = &module.declarations[0] else {
        panic!("expected a Value declaration")
    };
    assert!(matches!(decl.value, Expression::Unit(_)));
}

#[test]
fn mutable_and_export_modifiers_compose() {
    let module = parse_ok(
        r#"
        w ≔ 1
        x ≔˚ 1
        y ≔⟳ 1
        z ≔˚⟳ 1
        ❧
        "#,
    );
    let flags: Vec<(bool, bool)> = module
        .declarations
        .iter()
        .map(|decl| {
            let Declaration::Value(d) = decl else {
                panic!("expected a Value declaration")
            };
            (d.mutable, d.exported)
        })
        .collect();
    assert_eq!(
        flags,
        vec![(false, false), (true, false), (false, true), (true, true)]
    );
}

#[test]
fn anonymous_lambda_identity_function() {
    let module = parse_ok("f ≔ λ(x) → x\n❧");
    let Declaration::Value(decl) = &module.declarations[0] else {
        panic!("expected a Value declaration")
    };
    match &decl.value {
        Expression::Lambda { params, body, .. } => {
            assert_eq!(params.len(), 1);
            assert_eq!(params[0].name, "x");
            assert!(matches!(**body, Expression::Reference(ref n, _) if n == "x"));
        }
        other => panic!("expected a Lambda, got {other:?}"),
    }
}

#[test]
fn application_of_a_reference() {
    let module = parse_ok("y ≔ f(5)\n❧");
    let Declaration::Value(decl) = &module.declarations[0] else {
        panic!("expected a Value declaration")
    };
    match &decl.value {
        Expression::Application { callable, args, .. } => {
            assert!(matches!(**callable, Expression::Reference(ref n, _) if n == "f"));
            assert_eq!(args.len(), 1);
        }
        other => panic!("expected an Application, got {other:?}"),
    }
}

// ── P1-1: `Argument ::= Expression | "•"` (§7.2/§12.1) ─────────────────

#[test]
fn a_hole_argument_parses_as_a_hole_not_an_expression() {
    let module = parse_ok("y ≔ f(1, •, 3)\n❧");
    let Declaration::Value(decl) = &module.declarations[0] else {
        panic!("expected a Value declaration")
    };
    match &decl.value {
        Expression::Application { args, .. } => {
            assert_eq!(args.len(), 3);
            assert!(matches!(args[0], Argument::Expr(Expression::Int(1, _))));
            assert!(matches!(args[1], Argument::Hole(_)));
            assert!(matches!(args[2], Argument::Expr(Expression::Int(3, _))));
        }
        other => panic!("expected an Application, got {other:?}"),
    }
}

#[test]
fn multiple_holes_in_one_application_all_parse() {
    let module = parse_ok("y ≔ f(•, 2, •)\n❧");
    let Declaration::Value(decl) = &module.declarations[0] else {
        panic!("expected a Value declaration")
    };
    match &decl.value {
        Expression::Application { args, .. } => {
            assert!(matches!(args[0], Argument::Hole(_)));
            assert!(matches!(args[1], Argument::Expr(Expression::Int(2, _))));
            assert!(matches!(args[2], Argument::Hole(_)));
        }
        other => panic!("expected an Application, got {other:?}"),
    }
}

#[test]
fn pipe_stage_mixing_pipe_ref_and_hole_is_a_parse_error() {
    // CONCRETE_SYMBOLIC_GRAMMAR.md §12.1/§16.1's reserved-invalid
    // combination: presence of '◈' forces expression-stage
    // interpretation, where '•' has no meaning.
    let msg = parse_err("y ≔ xs ▷ (f(◈, •))\n❧");
    assert!(msg.contains('◈') && msg.contains('•'), "message was: {msg}");
}

#[test]
fn pipeline_with_a_bare_callable_stage() {
    let module = parse_ok("z ≔ xs ▷ f\n❧");
    let Declaration::Value(decl) = &module.declarations[0] else {
        panic!("expected a Value declaration")
    };
    match &decl.value {
        Expression::Pipe { subject, stage, .. } => {
            assert!(matches!(**subject, Expression::Reference(ref n, _) if n == "xs"));
            assert!(
                matches!(**stage, Stage::Callable(Expression::Reference(ref n, _)) if n == "f")
            );
        }
        other => panic!("expected a Pipe, got {other:?}"),
    }
}

#[test]
fn nested_pipeline_with_pipe_reference_inside_parens() {
    // CONCRETE_SYMBOLIC_GRAMMAR.md §12/§16.1's hardest case: the inner
    // "◈" belongs to the inner pipe's stage, never the outer one.
    let module = parse_ok("z ≔ xs ▷ (◈ ▷ g)\n❧");
    let Declaration::Value(decl) = &module.declarations[0] else {
        panic!("expected a Value declaration")
    };
    match &decl.value {
        Expression::Pipe { stage, .. } => match &**stage {
            Stage::Expr(Expression::Pipe {
                subject,
                stage: inner,
                ..
            }) => {
                assert!(matches!(**subject, Expression::PipeRef(_)));
                assert!(
                    matches!(**inner, Stage::Callable(Expression::Reference(ref n, _)) if n == "g")
                );
            }
            other => panic!("expected the outer stage to be a nested Pipe, got {other:?}"),
        },
        other => panic!("expected a Pipe, got {other:?}"),
    }
}

#[test]
fn declaration_boundary_falls_out_of_ordinary_parsing() {
    // CONCRETE_SYMBOLIC_GRAMMAR.md §15's worked example, using pipe
    // instead of "✚" (not implemented this slice): a newline before a
    // valid continuation token does not end the declaration.
    let module = parse_ok(
        r#"
        x ≔ a
          ▷ f
        y ≔ b
        ❧
        "#,
    );
    assert_eq!(module.declarations.len(), 2);
    let Declaration::Value(x) = &module.declarations[0] else {
        panic!("expected a Value declaration")
    };
    assert!(matches!(x.value, Expression::Pipe { .. }));
    let Declaration::Value(y) = &module.declarations[1] else {
        panic!("expected a Value declaration")
    };
    assert!(matches!(y.value, Expression::Reference(ref n, _) if n == "b"));
}

#[test]
fn string_escapes_match_the_closed_grammar_exactly() {
    // CONCRETE_SYMBOLIC_GRAMMAR.md §5's EscapeSequence production:
    // exactly \" \\ \n \t \r \0.
    let module = parse_ok(
        r#"x ≔ "a\nb\tc\rd\0e\"f\\g"
❧"#,
    );
    let Declaration::Value(decl) = &module.declarations[0] else {
        panic!("expected a Value declaration")
    };
    match &decl.value {
        Expression::Str(s, _) => assert_eq!(s, "a\nb\tc\rd\0e\"f\\g"),
        other => panic!("expected a Str, got {other:?}"),
    }
}

#[test]
fn raw_newline_is_permitted_inside_a_string_literal() {
    let module = parse_ok("x ≔ \"line one\nline two\"\n❧");
    let Declaration::Value(decl) = &module.declarations[0] else {
        panic!("expected a Value declaration")
    };
    assert!(matches!(&decl.value, Expression::Str(s, _) if s == "line one\nline two"));
}

#[test]
fn line_comment_is_skipped() {
    let module = parse_ok("x ≔ 1 // this is a comment, not part of the value\n❧");
    let Declaration::Value(decl) = &module.declarations[0] else {
        panic!("expected a Value declaration")
    };
    assert!(matches!(decl.value, Expression::Int(1, _)));
}

#[test]
fn nested_block_comments_are_skipped() {
    // §2.1: block comments nest — this is only well-defined if the
    // parser sees straight through a comment-inside-a-comment to the
    // real outermost closer.
    let module = parse_ok("x ≔ ⌈ outer ⌈ inner ⌉ still outer ⌉ 1\n❧");
    let Declaration::Value(decl) = &module.declarations[0] else {
        panic!("expected a Value declaration")
    };
    assert!(matches!(decl.value, Expression::Int(1, _)));
}

#[test]
fn comment_glyphs_inside_a_string_are_literal_content_not_comments() {
    let module = parse_ok(
        r#"x ≔ "// not a comment ⌈ either"
❧"#,
    );
    let Declaration::Value(decl) = &module.declarations[0] else {
        panic!("expected a Value declaration")
    };
    assert!(matches!(&decl.value, Expression::Str(s, _) if s == "// not a comment ⌈ either"));
}

#[test]
fn unsupported_escape_sequence_is_rejected() {
    let msg = parse_err(
        r#"x ≔ "\q"
❧"#,
    );
    assert!(
        msg.contains("unsupported escape sequence"),
        "message was: {msg}"
    );
}

#[test]
fn unterminated_block_comment_is_rejected() {
    let msg = parse_err("x ≔ ⌈ never closed\n❧");
    assert!(
        msg.contains("unterminated block comment"),
        "message was: {msg}"
    );
}

#[test]
fn lambda_body_extends_across_a_pipe_before_the_next_declaration() {
    let module = parse_ok("f ≔ λ(x) → x ▷ g\ny ≔ 1\n❧");
    assert_eq!(module.declarations.len(), 2);
    let Declaration::Value(f) = &module.declarations[0] else {
        panic!("expected a Value declaration")
    };
    match &f.value {
        Expression::Lambda { body, .. } => assert!(matches!(**body, Expression::Pipe { .. })),
        other => panic!("expected a Lambda, got {other:?}"),
    }
}

// ── invalid programs ────────────────────────────────────────────────────

#[test]
fn missing_seal_is_rejected() {
    let msg = parse_err("x ≔ 5");
    assert!(msg.contains('❧'), "message was: {msg}");
}

#[test]
fn empty_input_is_rejected() {
    let msg = parse_err("");
    assert!(msg.contains('❧'), "message was: {msg}");
}

#[test]
fn unterminated_string_is_rejected() {
    let msg = parse_err("x ≔ \"abc");
    assert!(msg.contains("unterminated"), "message was: {msg}");
}

#[test]
fn ascii_plus_is_never_valid_obfusku_syntax() {
    // The glyph '✚', never ASCII '+' — no ASCII-operator fallback.
    let msg = parse_err("x ≔ 1 + 2\n❧");
    assert!(msg.contains("unexpected character"), "message was: {msg}");
}

#[test]
fn reversed_modifier_order_is_rejected() {
    // §8.1's production is Mutability? Export?, in that fixed order —
    // reversing them is a parse error.
    let msg = parse_err("x ≔⟳˚ 5\n❧");
    assert!(
        msg.contains("expected a name after '˚'"),
        "message was: {msg}"
    );
}

#[test]
fn unknown_identifier_where_a_declaration_or_seal_is_expected_is_rejected() {
    let msg = parse_err("x ≔ 5\n)\n❧");
    assert!(msg.contains("expected a declaration"), "message was: {msg}");
}

// ── P1-3a: ValueDeclaration's `(":" Type)?` annotation (§8.1) ──────────

#[test]
fn value_declaration_accepts_a_type_annotation_before_bind() {
    let module = parse_ok("x: ⟁ ≔ 5\n❧");
    let Declaration::Value(vd) = &module.declarations[0] else {
        panic!("expected a Value declaration")
    };
    assert!(matches!(
        &vd.ty,
        Some(obfusku_syntax::ast::TypeRef::Base(
            obfusku_syntax::ast::BaseType::Int,
            _
        ))
    ));
}

#[test]
fn value_declaration_without_annotation_has_no_declared_type() {
    let module = parse_ok("x ≔ 5\n❧");
    let Declaration::Value(vd) = &module.declarations[0] else {
        panic!("expected a Value declaration")
    };
    assert!(vd.ty.is_none());
}

#[test]
fn value_declaration_annotation_composes_with_mutability_and_export() {
    // Annotation comes before '≔'; Mutability/Export still follow '≔'
    // in their already-fixed order (§8.1) — the two modifier families
    // don't interact.
    let module = parse_ok("x: \u{27C1} \u{2254}\u{02da}\u{27F3} 5\n\u{2767}\n");
    let Declaration::Value(vd) = &module.declarations[0] else {
        panic!("expected a Value declaration")
    };
    assert!(vd.ty.is_some());
    assert!(vd.mutable);
    assert!(vd.exported);
}

#[test]
fn type_annotation_is_rejected_on_a_local_binding() {
    // §8.1: the annotation is module-boundary-only (§9); locally the
    // type is always inferred.
    let msg = parse_err("result \u{2254}\n y: \u{27C1} \u{2254} 5\n y\n\u{2767}\n");
    assert!(msg.contains("not valid on a local binding"), "{msg}");
}

// ── ADTs, Match, and Pattern (CONCRETE_SYMBOLIC_GRAMMAR.md §8.3/§10/§11) ─

#[test]
fn generic_adt_declaration_with_sum_body() {
    // The user's own suggested first example, using exactly this
    // grammar's SumBody production — no invented syntax.
    let module = parse_ok("Option t ≔ { Some(t) ⟢ None }\n❧");
    let Declaration::Type(td) = &module.declarations[0] else {
        panic!("expected a Type declaration")
    };
    assert_eq!(td.tag, "Option");
    assert_eq!(td.type_params, vec!["t".to_string()]);
    assert_eq!(td.variants.len(), 2);
    assert_eq!(td.variants[0].tag, "Some");
    assert_eq!(td.variants[0].fields.len(), 1);
    assert_eq!(td.variants[1].tag, "None");
    assert!(td.variants[1].fields.is_empty()); // bare nullary variant, no parens
}

#[test]
fn export_mark_on_a_type_declaration_is_rejected() {
    // P1-2c / §8.4's own 1.0 contract: `TypeDeclaration` is a deliberate
    // exception to the export mark, not a third site for it — every
    // type and its variants are exported unconditionally, with no `⟳`
    // ever written or accepted here at all.
    let msg = parse_err("Shape \u{2254}\u{27F3} { Circle(\u{27C1}) }\n\u{2767}\n");
    assert!(msg.contains("Export"), "{msg}");
}

#[test]
fn non_generic_adt_declaration() {
    // CONCRETE_SYMBOLIC_GRAMMAR.md §8.3's own canonical example.
    let module = parse_ok("Shape ≔ { Circle(⧆) ⟢ Rectangle(⧆, ⧆) }\n❧");
    let Declaration::Type(td) = &module.declarations[0] else {
        panic!("expected a Type declaration")
    };
    assert!(td.type_params.is_empty());
    assert_eq!(td.variants[1].fields.len(), 2);
}

#[test]
fn constructor_application_parses_as_ordinary_application() {
    // §7.6: PositionalConstruction is syntactically identical to
    // Application with an uppercase Callable — no special grammar.
    let module = parse_ok("Option t ≔ { Some(t) ⟢ None }\nx ≔ Some(5)\n❧");
    let Declaration::Value(decl) = &module.declarations[1] else {
        panic!("expected a Value declaration")
    };
    match &decl.value {
        Expression::Application { callable, args, .. } => {
            assert!(matches!(**callable, Expression::Reference(ref n, _) if n == "Some"));
            assert_eq!(args.len(), 1);
        }
        other => panic!("expected an Application, got {other:?}"),
    }
}

#[test]
fn bare_nullary_constructor_reference_parses_as_an_ordinary_reference() {
    // §7.1's fix: an uppercase bare identifier is an ordinary Reference,
    // which is what makes `None` (no parens) parseable at all.
    let module = parse_ok("Option t ≔ { Some(t) ⟢ None }\nx ≔ None\n❧");
    let Declaration::Value(decl) = &module.declarations[1] else {
        panic!("expected a Value declaration")
    };
    assert!(matches!(&decl.value, Expression::Reference(n, _) if n == "None"));
}

#[test]
fn match_over_an_option_like_adt() {
    let module = parse_ok(
        r#"
        Option t ≔ { Some(t) ⟢ None }
        x ≔ Some(5)
        y ≔ ⟡ x {
          Some(n) → n
          ⟢ None → 0
        }
        ❧
        "#,
    );
    let Declaration::Value(decl) = &module.declarations[2] else {
        panic!("expected a Value declaration")
    };
    match &decl.value {
        Expression::Match {
            scrutinee, arms, ..
        } => {
            assert!(matches!(**scrutinee, Expression::Reference(ref n, _) if n == "x"));
            assert_eq!(arms.len(), 2);
            match &arms[0].pattern {
                obfusku_syntax::ast::Pattern::Constructor { tag, args, .. } => {
                    assert_eq!(tag, "Some");
                    assert_eq!(args.len(), 1);
                    assert!(
                        matches!(&args[0], obfusku_syntax::ast::Pattern::Var(n, _) if n == "n")
                    );
                }
                other => panic!("expected a Constructor pattern, got {other:?}"),
            }
            assert!(matches!(
                &arms[1].pattern,
                obfusku_syntax::ast::Pattern::Constructor { tag, args, .. }
                    if tag == "None" && args.is_empty()
            ));
        }
        other => panic!("expected a Match, got {other:?}"),
    }
}

#[test]
fn wildcard_and_literal_patterns_parse() {
    let module = parse_ok(
        r#"
        y ≔ ⟡ 5 {
          0 → "zero"
          ⟢ _ → "other"
        }
        ❧
        "#,
    );
    let Declaration::Value(decl) = &module.declarations[0] else {
        panic!("expected a Value declaration")
    };
    match &decl.value {
        Expression::Match { arms, .. } => {
            assert!(matches!(
                arms[0].pattern,
                obfusku_syntax::ast::Pattern::Int(0, _)
            ));
            assert!(matches!(
                arms[1].pattern,
                obfusku_syntax::ast::Pattern::Wildcard(_)
            ));
        }
        other => panic!("expected a Match, got {other:?}"),
    }
}

#[test]
fn nested_constructor_pattern_parses() {
    let module = parse_ok(
        r#"
        Option t ≔ { Some(t) ⟢ None }
        y ≔ ⟡ x {
          Some(Some(n)) → n
          ⟢ Some(None) → 0
          ⟢ None → 0
        }
        ❧
        "#,
    );
    let Declaration::Value(decl) = &module.declarations[1] else {
        panic!("expected a Value declaration")
    };
    match &decl.value {
        Expression::Match { arms, .. } => match &arms[0].pattern {
            obfusku_syntax::ast::Pattern::Constructor { tag, args, .. } => {
                assert_eq!(tag, "Some");
                assert!(
                    matches!(&args[0], obfusku_syntax::ast::Pattern::Constructor { tag, .. } if tag == "Some")
                );
            }
            other => panic!("expected a Constructor pattern, got {other:?}"),
        },
        other => panic!("expected a Match, got {other:?}"),
    }
}

// ── invalid ADT/Match programs ──────────────────────────────────────────

#[test]
fn optional_and_result_are_ordinary_adt_declarations() {
    // Same SumBody production as Option; constructor names aren't
    // spec-mandated, chosen here to avoid colliding with Exception's tags.
    let module = parse_ok("Optional t ≔ { Some(t) ⟢ None }\nResult t e ≔ { Ok(t) ⟢ Err(e) }\n❧");
    let Declaration::Type(opt) = &module.declarations[0] else {
        panic!("expected a Type declaration")
    };
    assert_eq!(opt.type_params, vec!["t".to_string()]);
    let Declaration::Type(res) = &module.declarations[1] else {
        panic!("expected a Type declaration")
    };
    assert_eq!(res.type_params, vec!["t".to_string(), "e".to_string()]);
    assert_eq!(res.variants[0].tag, "Ok");
    assert_eq!(res.variants[1].tag, "Err");
}

#[test]
fn result_match_parses_with_two_independent_constructors() {
    let module = parse_ok(
        r#"
        Result t e ≔ { Ok(t) ⟢ Err(e) }
        y ≔ ⟡ r {
          Ok(n) → n
          ⟢ Err(msg) → 0
        }
        ❧
        "#,
    );
    let Declaration::Value(decl) = &module.declarations[1] else {
        panic!("expected a Value declaration")
    };
    assert!(matches!(&decl.value, Expression::Match { .. }));
}

#[test]
fn match_missing_closing_brace_is_rejected() {
    let msg = parse_err("y ≔ ⟡ x { Some(n) → n\n❧");
    assert!(msg.contains("'}'"), "message was: {msg}");
}

#[test]
fn type_declaration_missing_bind_is_rejected() {
    let msg = parse_err("Shape { Circle(⧆) }\n❧");
    assert!(msg.contains("'≔'"), "message was: {msg}");
}

#[test]
fn zero_parameter_lambda_is_rejected() {
    // CONCRETE_SYMBOLIC_GRAMMAR.md §7.3: excluded at the grammar level,
    // not merely unimplemented — there is no Unit value for a
    // zero-parameter Lambda to lower to.
    let msg = parse_err("f ≔ λ() → 1\n❧");
    assert!(msg.contains("zero-parameter lambdas"), "message was: {msg}");
}

#[test]
fn zero_argument_application_is_rejected() {
    // CONCRETE_SYMBOLIC_GRAMMAR.md §7.2: same reasoning as above.
    let msg = parse_err("y ≔ f()\n❧");
    assert!(
        msg.contains("zero-argument application"),
        "message was: {msg}"
    );
}

// ---------------------------------------------------------------------
// FunctionDeclaration — `CONCRETE_SYMBOLIC_GRAMMAR.md` §8.2
// ---------------------------------------------------------------------

#[test]
fn function_declaration_parses_with_mandatory_annotations() {
    let module = parse_ok("λf(x: ⟁): ⟁ → x\n❧");
    let Declaration::Function(fd) = &module.declarations[0] else {
        panic!("expected a Function declaration")
    };
    assert_eq!(fd.name, "f");
    assert_eq!(fd.params.len(), 1);
    assert_eq!(fd.params[0].name, "x");
    assert!(matches!(
        &fd.params[0].ty,
        obfusku_syntax::ast::TypeRef::Base(obfusku_syntax::ast::BaseType::Int, _)
    ));
    assert!(matches!(
        &fd.return_type,
        obfusku_syntax::ast::TypeRef::Base(obfusku_syntax::ast::BaseType::Int, _)
    ));
    assert!(matches!(&fd.body, Expression::Reference(n, _) if n == "x"));
}

#[test]
fn function_declaration_with_multiple_parameters() {
    let module = parse_ok("λf(x: ⟁, y: ⟁): ⟁ → x\n❧");
    let Declaration::Function(fd) = &module.declarations[0] else {
        panic!("expected a Function declaration")
    };
    assert_eq!(fd.params.len(), 2);
    assert_eq!(fd.params[1].name, "y");
}

#[test]
fn anonymous_lambda_at_declaration_position_is_still_an_error_without_a_binder() {
    // 'λ(' at declaration position isn't a FunctionDeclaration (no name)
    // and isn't a legal bare-expression declaration either.
    let msg = parse_err("λ(x) → x\n❧");
    assert!(msg.contains("function name"), "{msg}");
}

#[test]
fn zero_parameter_function_declaration_is_rejected() {
    let msg = parse_err("λf(): ⟁ → 1\n❧");
    assert!(msg.contains("at least one parameter"), "{msg}");
}

#[test]
fn function_declaration_missing_parameter_type_annotation_is_rejected() {
    let msg = parse_err("λf(x): ⟁ → x\n❧");
    assert!(msg.contains("':'"), "{msg}");
}

#[test]
fn function_declaration_missing_return_type_annotation_is_rejected() {
    let msg = parse_err("λf(x: ⟁) → x\n❧");
    assert!(msg.contains("':'"), "{msg}");
}

#[test]
fn function_declaration_accepts_export_after_return_arrow() {
    // §8.4 (post-amendment): export mark follows the declaration's own
    // '≔' or '→'; FunctionDeclaration has no '≔', so it's '→⟳'.
    let module = parse_ok("λf(x: ⟁): ⟁ →⟳ x\n❧");
    let Declaration::Function(fd) = &module.declarations[0] else {
        panic!("expected a Function declaration")
    };
    assert!(fd.exported);
}

#[test]
fn function_declaration_without_export_defaults_to_not_exported() {
    let module = parse_ok("λf(x: ⟁): ⟁ → x\n❧");
    let Declaration::Function(fd) = &module.declarations[0] else {
        panic!("expected a Function declaration")
    };
    assert!(!fd.exported);
}

#[test]
fn export_mark_immediately_after_lambda_is_rejected() {
    // The superseded `λ⟳f(...)` candidate stays invalid — §8.4 fixed the
    // position to '→⟳', not 'λ⟳'.
    let msg = parse_err("λ⟳f(x: ⟁): ⟁ → x\n❧");
    assert!(msg.contains("function name"), "{msg}");
}

#[test]
fn export_mark_is_rejected_on_a_local_function_declaration() {
    // §8.4: export is restricted to top-level Module declarations by a
    // static check, not a parse-time restriction, since the grammar
    // shape is identical between top-level and local FunctionDeclaration.
    let msg = parse_err("result ≔ λf(x: ⟁): ⟁ →⟳ x\n f(1)\n❧");
    assert!(
        msg.contains("not valid on a local function declaration"),
        "{msg}"
    );
}

#[test]
fn anonymous_lambda_in_expression_position_is_unaffected_by_function_declaration() {
    // Anonymous Lambda (no annotations, no name) must still work exactly
    // as before at expression position — FunctionDeclaration only claims
    // declaration position.
    let module = parse_ok("f ≔ λ(x) → x\n❧");
    let Declaration::Value(decl) = &module.declarations[0] else {
        panic!("expected a Value declaration")
    };
    assert!(matches!(&decl.value, Expression::Lambda { .. }));
}

// ── P0: anonymous `Lambda`'s optional `Parameter` annotation (§7.3) ────

#[test]
fn lambda_parameter_accepts_an_optional_type_annotation() {
    let module = parse_ok("f \u{2254} \u{3bb}(x: \u{27C1}) \u{2192} x\n\u{2767}\n");
    let Declaration::Value(decl) = &module.declarations[0] else {
        panic!("expected a Value declaration")
    };
    let Expression::Lambda { params, .. } = &decl.value else {
        panic!("expected a Lambda")
    };
    assert!(matches!(
        &params[0].ty,
        Some(obfusku_syntax::ast::TypeRef::Base(
            obfusku_syntax::ast::BaseType::Int,
            _
        ))
    ));
}

#[test]
fn lambda_parameter_without_annotation_has_no_declared_type() {
    let module = parse_ok("f \u{2254} \u{3bb}(x) \u{2192} x\n\u{2767}\n");
    let Declaration::Value(decl) = &module.declarations[0] else {
        panic!("expected a Value declaration")
    };
    let Expression::Lambda { params, .. } = &decl.value else {
        panic!("expected a Lambda")
    };
    assert!(params[0].ty.is_none());
}

#[test]
fn lambda_multiple_parameters_mix_annotated_and_unannotated() {
    let module = parse_ok("f \u{2254} \u{3bb}(x: \u{27C1}, y) \u{2192} x\n\u{2767}\n");
    let Declaration::Value(decl) = &module.declarations[0] else {
        panic!("expected a Value declaration")
    };
    let Expression::Lambda { params, .. } = &decl.value else {
        panic!("expected a Lambda")
    };
    assert!(params[0].ty.is_some());
    assert!(params[1].ty.is_none());
}

// ---------------------------------------------------------------------
// Arithmetic / comparison / equality / logical operators (§7.1a/§13,
// §20.2).
// ---------------------------------------------------------------------

use obfusku_syntax::ast::{BaseType, BinOp, TypeRef, UnOp};

fn expr_of(module: &obfusku_syntax::ast::Module) -> &Expression {
    let Declaration::Value(decl) = &module.declarations[0] else {
        panic!("expected a Value declaration")
    };
    &decl.value
}

fn as_binop(e: &Expression) -> (BinOp, &Expression, &Expression) {
    match e {
        Expression::BinaryOp { op, lhs, rhs, .. } => (*op, lhs, rhs),
        other => panic!("expected a BinaryOp, got {other:?}"),
    }
}

fn as_int_lit(e: &Expression) -> i64 {
    match e {
        Expression::Int(v, _) => *v,
        other => panic!("expected an Int literal, got {other:?}"),
    }
}

#[test]
fn all_operator_tokens_lex_without_error() {
    // One pass proving every glyph from the frozen grammar actually
    // lexes — including the two-codepoint `☠︎` and the two-ASCII-char
    // compound comparison/equality tokens.
    let _ = parse_ok(
        "x ≔ 1 ✚ 2 ☠︎ 3 ✱ 4 ÷ 5 ⌗ 6\n\
         y ≔ (1 < 2) ∧ (3 <= 4) ∧ (5 > 6) ∧ (7 >= 8)\n\
         z ≔ (1 == 2) ∧ (3 != 4)\n\
         w ≔ ¬◉ ∨ ◎ ⊻ ◉\n\
         n ≔ −5\n❧",
    );
}

#[test]
fn multiplicative_binds_tighter_than_additive() {
    // 1 ✚ 2 ✱ 3  ==  1 ✚ (2 ✱ 3), not (1 ✚ 2) ✱ 3
    let module = parse_ok("x ≔ 1 ✚ 2 ✱ 3\n❧");
    let (op, lhs, rhs) = as_binop(expr_of(&module));
    assert_eq!(op, BinOp::Add);
    assert_eq!(as_int_lit(lhs), 1);
    let (inner_op, inner_lhs, inner_rhs) = as_binop(rhs);
    assert_eq!(inner_op, BinOp::Mul);
    assert_eq!(as_int_lit(inner_lhs), 2);
    assert_eq!(as_int_lit(inner_rhs), 3);
}

#[test]
fn additive_is_left_associative() {
    // 1 ✚ 2 ☠︎ 3  ==  (1 ✚ 2) ☠︎ 3
    let module = parse_ok("x ≔ 1 ✚ 2 ☠︎ 3\n❧");
    let (op, lhs, rhs) = as_binop(expr_of(&module));
    assert_eq!(op, BinOp::Sub);
    assert_eq!(as_int_lit(rhs), 3);
    let (inner_op, inner_lhs, inner_rhs) = as_binop(lhs);
    assert_eq!(inner_op, BinOp::Add);
    assert_eq!(as_int_lit(inner_lhs), 1);
    assert_eq!(as_int_lit(inner_rhs), 2);
}

#[test]
fn additive_binds_tighter_than_comparison() {
    // 1 ✚ 2 < 3 ✱ 4  ==  (1 ✚ 2) < (3 ✱ 4)
    let module = parse_ok("x ≔ 1 ✚ 2 < 3 ✱ 4\n❧");
    let (op, lhs, rhs) = as_binop(expr_of(&module));
    assert_eq!(op, BinOp::Lt);
    assert_eq!(as_binop(lhs).0, BinOp::Add);
    assert_eq!(as_binop(rhs).0, BinOp::Mul);
}

#[test]
fn comparison_binds_tighter_than_equality() {
    let module = parse_ok("x ≔ 1 < 2 == 3 > 4\n❧");
    let (op, lhs, rhs) = as_binop(expr_of(&module));
    assert_eq!(op, BinOp::Eq);
    assert_eq!(as_binop(lhs).0, BinOp::Lt);
    assert_eq!(as_binop(rhs).0, BinOp::Gt);
}

#[test]
fn equality_binds_tighter_than_and() {
    let module = parse_ok("x ≔ 1 == 2 ∧ 3 != 4\n❧");
    let (op, lhs, rhs) = as_binop(expr_of(&module));
    assert_eq!(op, BinOp::And);
    assert_eq!(as_binop(lhs).0, BinOp::Eq);
    assert_eq!(as_binop(rhs).0, BinOp::NotEq);
}

#[test]
fn and_binds_tighter_than_xor_binds_tighter_than_or() {
    // a ∧ b ∨ c ⊻ d  ==  (a ∧ b) ∨ (c ⊻ d)
    let module = parse_ok("x ≔ 1 ∧ 2 ∨ 3 ⊻ 4\n❧");
    let (op, lhs, rhs) = as_binop(expr_of(&module));
    assert_eq!(op, BinOp::Or);
    assert_eq!(as_binop(lhs).0, BinOp::And);
    assert_eq!(as_binop(rhs).0, BinOp::Xor);
}

#[test]
fn logical_operators_are_left_associative() {
    let module = parse_ok("x ≔ 1 ∨ 2 ∨ 3\n❧");
    let (op, lhs, rhs) = as_binop(expr_of(&module));
    assert_eq!(op, BinOp::Or);
    assert_eq!(as_int_lit(rhs), 3);
    assert_eq!(as_binop(lhs).0, BinOp::Or);
}

#[test]
fn comparison_does_not_chain() {
    let msg = parse_err("x ≔ 1 < 2 < 3\n❧");
    assert!(msg.contains("do not chain"), "{msg}");
}

#[test]
fn equality_does_not_chain() {
    let msg = parse_err("x ≔ 1 == 2 == 3\n❧");
    assert!(msg.contains("do not chain"), "{msg}");
}

#[test]
fn mixed_comparison_operators_still_do_not_chain() {
    let msg = parse_err("x ≔ 1 <= 2 >= 3\n❧");
    assert!(msg.contains("do not chain"), "{msg}");
}

#[test]
fn unary_negation_binds_tighter_than_multiplicative() {
    // −2 ✱ 3  ==  (−2) ✱ 3
    let module = parse_ok("x ≔ −2 ✱ 3\n❧");
    let (op, lhs, rhs) = as_binop(expr_of(&module));
    assert_eq!(op, BinOp::Mul);
    assert!(matches!(lhs, Expression::UnaryOp { op: UnOp::Neg, .. }));
    assert_eq!(as_int_lit(rhs), 3);
}

#[test]
fn unary_not_nests_right_to_left() {
    // ¬¬◉  ==  ¬(¬◉)
    let module = parse_ok("x ≔ ¬¬◉\n❧");
    match expr_of(&module) {
        Expression::UnaryOp {
            op: UnOp::Not,
            operand,
            ..
        } => {
            assert!(matches!(
                **operand,
                Expression::UnaryOp { op: UnOp::Not, .. }
            ));
        }
        other => panic!("expected nested UnaryOp, got {other:?}"),
    }
}

#[test]
fn binary_subtract_and_unary_negate_are_distinct_glyphs_no_ambiguity() {
    // 5 ☠︎ −2  ==  5 minus (negate 2) — both glyphs present, unambiguous
    // since '☠︎' (binary) and '−' (unary) are different tokens entirely.
    let module = parse_ok("x ≔ 5 ☠︎ −2\n❧");
    let (op, lhs, rhs) = as_binop(expr_of(&module));
    assert_eq!(op, BinOp::Sub);
    assert_eq!(as_int_lit(lhs), 5);
    assert!(matches!(rhs, Expression::UnaryOp { op: UnOp::Neg, .. }));
}

#[test]
fn application_binds_tighter_than_unary() {
    // ¬f(x)  ==  ¬(f(x)), not (¬f)(x)
    let module = parse_ok("y ≔ ¬f(x)\n❧");
    match expr_of(&module) {
        Expression::UnaryOp {
            op: UnOp::Not,
            operand,
            ..
        } => {
            assert!(matches!(**operand, Expression::Application { .. }));
        }
        other => panic!("expected UnaryOp wrapping an Application, got {other:?}"),
    }
}

#[test]
fn pipe_wraps_the_whole_operator_ladder_as_its_subject() {
    // 1 ✚ 2 ▷ f  ==  (1 ✚ 2) ▷ f — pipe's subject descends through the
    // full ladder, per §13's "▷ wraps the ladder" resolution.
    let module = parse_ok("y ≔ 1 ✚ 2 ▷ f\n❧");
    match expr_of(&module) {
        Expression::Pipe { subject, .. } => {
            assert_eq!(as_binop(subject).0, BinOp::Add);
        }
        other => panic!("expected a Pipe, got {other:?}"),
    }
}

#[test]
fn string_concatenation_operator_parses_as_add() {
    let module = parse_ok("x ≔ \"a\" ✚ \"b\"\n❧");
    assert_eq!(as_binop(expr_of(&module)).0, BinOp::Add);
}

#[test]
fn modulo_operator_parses() {
    let module = parse_ok("x ≔ 7 ⌗ 3\n❧");
    assert_eq!(as_binop(expr_of(&module)).0, BinOp::Mod);
}

#[test]
fn bare_lt_gt_are_distinct_from_le_ge_tokens() {
    // '<' alone parses as a comparison; '<=' must NOT be lexed as '<'
    // followed by a stray '=' (which would be a lex error, since bare
    // '=' is not otherwise valid).
    let module = parse_ok("x ≔ 1 <= 2\n❧");
    assert_eq!(as_binop(expr_of(&module)).0, BinOp::Le);
    let module2 = parse_ok("x ≔ 1 < 2\n❧");
    assert_eq!(as_binop(expr_of(&module2)).0, BinOp::Lt);
}

#[test]
fn bare_ascii_equals_alone_is_a_lex_error() {
    // '=' alone (not part of '==') is not a valid token anywhere in the
    // grammar — confirms no accidental partial-match fallback exists.
    let msg = parse_err("x ≔ 1 = 2\n❧");
    assert!(msg.contains("unexpected character"), "{msg}");
}

// ── NFC source validation — `CONCRETE_SYMBOLIC_GRAMMAR.md` §2 ─────────

#[test]
fn non_nfc_source_is_a_lexical_error() {
    // 'e' + U+0301 COMBINING ACUTE ACCENT (NFD-decomposed "é"), inside an
    // otherwise ordinary string literal — must be rejected before any
    // token is produced, not silently renormalized.
    let msg = parse_err("x \u{2254} \"caf\u{65}\u{301}\"\n\u{2767}\n");
    assert!(msg.contains("Normalization Form C"), "{msg}");
}

#[test]
fn nfc_source_with_a_precomposed_accent_is_unaffected() {
    // The NFC-composed form of the same character ("é" as one codepoint,
    // U+00E9) must lex with no error at all.
    let module = parse_ok("x \u{2254} \"caf\u{e9}\"\n\u{2767}\n");
    assert_eq!(module.declarations.len(), 1);
}

// ---------------------------------------------------------------------
// Raise / Catch — `CONCRETE_SYMBOLIC_GRAMMAR.md` §7.8, `SEMANTIC_CORE.md`
// §15/§15.1/§15.2.
// ---------------------------------------------------------------------

#[test]
fn raise_parses_its_operand() {
    let module = parse_ok("result ≔ ☄ DivisionByZero\n❧");
    let Declaration::Value(decl) = &module.declarations[0] else {
        panic!("expected a Value declaration")
    };
    match &decl.value {
        Expression::Raise(value, _) => {
            assert!(matches!(**value, Expression::Reference(ref n, _) if n == "DivisionByZero"));
        }
        other => panic!("expected a Raise, got {other:?}"),
    }
}

#[test]
fn catch_parses_body_and_lambda_handler() {
    let module = parse_ok("result ≔ ☊ (☄ DivisionByZero) λ(e) → 0\n❧");
    let Declaration::Value(decl) = &module.declarations[0] else {
        panic!("expected a Value declaration")
    };
    match &decl.value {
        Expression::Catch { body, handler, .. } => {
            assert!(matches!(**body, Expression::Raise(..)));
            assert!(matches!(**handler, Expression::Lambda { .. }));
        }
        other => panic!("expected a Catch, got {other:?}"),
    }
}

// ---------------------------------------------------------------------
// BaseType sigils vs Tag — `CONCRETE_SYMBOLIC_GRAMMAR.md` §9,
// `GLYPH_SYSTEM_DESIGN.md` §227-245. `BaseType` and `Tag` are genuinely
// distinct productions — `Int` written as a bare word is a `Tag`
// (an ADT reference), never an alias for `⟁`.
// ---------------------------------------------------------------------

#[test]
fn bare_word_int_parses_as_an_ordinary_tag_never_as_the_base_type() {
    // "Int" as a bare word is never an alias for `⟁` — it parses as
    // `TypeRef::Named` (an ordinary Tag reference), never `TypeRef::Base`.
    let module = parse_ok("λf(x: Int): Int → x\n❧");
    let Declaration::Function(fd) = &module.declarations[0] else {
        panic!("expected a Function declaration")
    };
    assert!(matches!(&fd.params[0].ty, TypeRef::Named(n, _) if n == "Int"));
    assert!(!matches!(&fd.params[0].ty, TypeRef::Base(..)));
}

#[test]
fn a_user_declared_tag_still_works_as_an_annotation() {
    let module = parse_ok("Nat ≔ { Zero ⟢ Succ(Nat) }\nλf(x: Nat): Nat → x\n❧");
    let Declaration::Function(fd) = &module.declarations[1] else {
        panic!("expected a Function declaration")
    };
    assert!(matches!(&fd.params[0].ty, TypeRef::Named(n, _) if n == "Nat"));
}

#[test]
fn function_typed_return_needs_explicit_parens_not_ambiguous_with_the_declarations_own_arrow() {
    let module = parse_ok("λcompose(f: (⟁ → ⟁)): (⟁ → ⟁) → f\n❧");
    let Declaration::Function(fd) = &module.declarations[0] else {
        panic!("expected a Function declaration")
    };
    assert!(matches!(&fd.return_type, TypeRef::Function(_, _, _)));
}

#[test]
fn type_application_parses_and_binds_tighter_than_function_type() {
    // g: Array ▷ Int → Int == (Array ▷ Int) → Int, per §9's own example.
    let module = parse_ok("Array t ≔ { Empty }\nλg(x: Array ▷ ⟁): ⟁ → 1\n❧");
    let Declaration::Function(fd) = &module.declarations[1] else {
        panic!("expected a Function declaration")
    };
    match &fd.params[0].ty {
        TypeRef::Apply(lhs, rhs, _) => {
            assert!(matches!(**lhs, TypeRef::Named(ref n, _) if n == "Array"));
            assert!(matches!(**rhs, TypeRef::Base(BaseType::Int, _)));
        }
        other => panic!("expected a TypeApplication, got {other:?}"),
    }
}

// ── RecordBody / TupleBody (ABSTRACT_GRAMMAR.md §2.3/§4,
//    CONCRETE_SYMBOLIC_GRAMMAR.md §8.3/§9) ────────────────────────────

#[test]
fn record_body_declaration_parses_one_self_tagged_variant_with_field_names() {
    let module = parse_ok("Point ≔ { x: ⧆ ⟢ y: ⧆ }\n❧");
    let Declaration::Type(td) = &module.declarations[0] else {
        panic!("expected a Type declaration")
    };
    assert_eq!(td.tag, "Point");
    assert_eq!(td.variants.len(), 1);
    assert_eq!(td.variants[0].tag, "Point");
    assert_eq!(td.variants[0].fields.len(), 2);
    assert_eq!(
        td.variants[0].field_names,
        Some(vec!["x".to_string(), "y".to_string()])
    );
}

#[test]
fn tuple_body_declaration_parses_one_self_tagged_variant_with_no_field_names() {
    let module = parse_ok("Pair ≔ (⟁, ⌘)\n❧");
    let Declaration::Type(td) = &module.declarations[0] else {
        panic!("expected a Type declaration")
    };
    assert_eq!(td.tag, "Pair");
    assert_eq!(td.variants.len(), 1);
    assert_eq!(td.variants[0].tag, "Pair");
    assert_eq!(td.variants[0].fields.len(), 2);
    assert_eq!(td.variants[0].field_names, None);
}

#[test]
fn sum_body_variant_still_has_no_field_names() {
    let module = parse_ok("Option t ≔ { Some(t) ⟢ None }\n❧");
    let Declaration::Type(td) = &module.declarations[0] else {
        panic!("expected a Type declaration")
    };
    assert_eq!(td.variants[0].field_names, None);
}

#[test]
fn named_construction_parses_tag_and_fields() {
    let module = parse_ok("Point ≔ { x: ⧆ ⟢ y: ⧆ }\np ≔ Point { x: 1.0 ⟢ y: 2.0 }\n❧");
    let Declaration::Value(vd) = &module.declarations[1] else {
        panic!("expected a Value declaration")
    };
    let Expression::NamedConstruction { tag, fields, .. } = &vd.value else {
        panic!("expected a NamedConstruction, got {:?}", vd.value)
    };
    assert_eq!(tag, "Point");
    assert_eq!(fields.len(), 2);
    assert_eq!(fields[0].0, "x");
    assert_eq!(fields[1].0, "y");
}

#[test]
fn named_constructor_pattern_parses_tag_and_fields() {
    let module = parse_ok(
        r#"
        Point ≔ { x: ⧆ ⟢ y: ⧆ }
        p ≔ Point { x: 1.0 ⟢ y: 2.0 }
        q ≔ ⟡ p {
          Point { x: a ⟢ y: b } → a
        }
        ❧
        "#,
    );
    let Declaration::Value(decl) = &module.declarations[2] else {
        panic!("expected a Value declaration")
    };
    let Expression::Match { arms, .. } = &decl.value else {
        panic!("expected a Match, got {:?}", decl.value)
    };
    match &arms[0].pattern {
        obfusku_syntax::ast::Pattern::NamedConstructor { tag, fields, .. } => {
            assert_eq!(tag, "Point");
            assert_eq!(fields.len(), 2);
            assert_eq!(fields[0].0, "x");
            assert_eq!(fields[1].0, "y");
        }
        other => panic!("expected a NamedConstructor pattern, got {other:?}"),
    }
}

// ── LocalBinding (ABSTRACT_GRAMMAR.md §3.6, CONCRETE_SYMBOLIC_GRAMMAR.md
//    §7.4: `ValueDeclaration Expression` | `FunctionDeclaration+
//    Expression`) ────────────────────────────────────────────────────

#[test]
fn local_value_binding_parses_as_localvalue_with_its_continuation_as_body() {
    let module = parse_ok("result ≔ x ≔ 5\n x ✚ 1\n❧");
    let Declaration::Value(decl) = &module.declarations[0] else {
        panic!("expected a Value declaration")
    };
    match &decl.value {
        Expression::LocalValue {
            decl: inner, body, ..
        } => {
            assert_eq!(inner.name, "x");
            assert!(matches!(inner.value, Expression::Int(5, _)));
            assert!(matches!(**body, Expression::BinaryOp { .. }));
        }
        other => panic!("expected a LocalValue, got {other:?}"),
    }
}

#[test]
fn nested_local_value_bindings_chain_through_body() {
    let module = parse_ok("result ≔ x ≔ 1\n y ≔ 2\n x ✚ y\n❧");
    let Declaration::Value(decl) = &module.declarations[0] else {
        panic!("expected a Value declaration")
    };
    let Expression::LocalValue {
        decl: outer,
        body: outer_body,
        ..
    } = &decl.value
    else {
        panic!("expected an outer LocalValue, got {:?}", decl.value)
    };
    assert_eq!(outer.name, "x");
    match &**outer_body {
        Expression::LocalValue {
            decl: inner, body, ..
        } => {
            assert_eq!(inner.name, "y");
            assert!(matches!(**body, Expression::BinaryOp { .. }));
        }
        other => panic!("expected a nested LocalValue, got {other:?}"),
    }
}

#[test]
fn local_function_declaration_parses_as_localfunctions_with_one_member() {
    let module = parse_ok("result ≔ λf(n: ⟁): ⟁ → n\n f(5)\n❧");
    let Declaration::Value(decl) = &module.declarations[0] else {
        panic!("expected a Value declaration")
    };
    match &decl.value {
        Expression::LocalFunctions { decls, body, .. } => {
            assert_eq!(decls.len(), 1);
            assert_eq!(decls[0].name, "f");
            assert!(matches!(**body, Expression::Application { .. }));
        }
        other => panic!("expected a LocalFunctions, got {other:?}"),
    }
}

#[test]
fn adjacent_local_function_declarations_form_one_localfunctions_group() {
    let module = parse_ok("result ≔ λisEven(n: ⟁): ○ → n\n λisOdd(n: ⟁): ○ → n\n isEven(4)\n❧");
    let Declaration::Value(decl) = &module.declarations[0] else {
        panic!("expected a Value declaration")
    };
    match &decl.value {
        Expression::LocalFunctions { decls, .. } => {
            assert_eq!(decls.len(), 2);
            assert_eq!(decls[0].name, "isEven");
            assert_eq!(decls[1].name, "isOdd");
        }
        other => panic!("expected a LocalFunctions, got {other:?}"),
    }
}

#[test]
fn bare_lambda_followed_by_lparen_is_still_an_anonymous_lambda_not_a_local_function() {
    // The ambiguity this slice had to resolve: `λ(` (anonymous Lambda,
    // §7.3) vs `λname(` (local FunctionDeclaration, §8.2) — one token of
    // lookahead after `λ` disambiguates them.
    let module = parse_ok("result ≔ λ(n) → n\n❧");
    let Declaration::Value(decl) = &module.declarations[0] else {
        panic!("expected a Value declaration")
    };
    assert!(matches!(decl.value, Expression::Lambda { .. }));
}

#[test]
fn local_binding_export_modifier_is_rejected() {
    let msg = parse_err("result ≔ x ≔⟳ 5\n x\n❧");
    assert!(msg.contains("not valid on a local binding"), "{msg}");
}

// ── ArrayExpression (CONCRETE_SYMBOLIC_GRAMMAR.md §7.9) ────────────────

#[test]
fn array_literal_parses_its_elements() {
    let module = parse_ok("xs ≔ [1, 2, 3]\n❧");
    let Declaration::Value(decl) = &module.declarations[0] else {
        panic!("expected a Value declaration")
    };
    match &decl.value {
        Expression::ArrayLiteral { elements, .. } => {
            assert_eq!(elements.len(), 3);
            assert!(matches!(elements[0], Expression::Int(1, _)));
            assert!(matches!(elements[2], Expression::Int(3, _)));
        }
        other => panic!("expected an ArrayLiteral, got {other:?}"),
    }
}

#[test]
fn empty_array_literal_parses() {
    let module = parse_ok("xs ≔ []\n❧");
    let Declaration::Value(decl) = &module.declarations[0] else {
        panic!("expected a Value declaration")
    };
    match &decl.value {
        Expression::ArrayLiteral { elements, .. } => assert!(elements.is_empty()),
        other => panic!("expected an ArrayLiteral, got {other:?}"),
    }
}

#[test]
fn index_expr_parses_array_and_index() {
    let module = parse_ok("xs ≔ [1, 2, 3]\nresult ≔ xs[0]\n❧");
    let Declaration::Value(decl) = &module.declarations[1] else {
        panic!("expected a Value declaration")
    };
    match &decl.value {
        Expression::Index { array, index, .. } => {
            assert!(matches!(**array, Expression::Reference(ref n, _) if n == "xs"));
            assert!(matches!(**index, Expression::Int(0, _)));
        }
        other => panic!("expected an Index, got {other:?}"),
    }
}

#[test]
fn chained_indexing_parses_left_to_right() {
    let module = parse_ok("xs ≔ [[1, 2], [3, 4]]\nresult ≔ xs[0][1]\n❧");
    let Declaration::Value(decl) = &module.declarations[1] else {
        panic!("expected a Value declaration")
    };
    match &decl.value {
        Expression::Index { array, index, .. } => {
            assert!(matches!(**index, Expression::Int(1, _)));
            assert!(matches!(**array, Expression::Index { .. }));
        }
        other => panic!("expected an Index, got {other:?}"),
    }
}

#[test]
fn indexing_composes_with_application() {
    let module = parse_ok("f ≔ λ(x) → x\nresult ≔ f([1, 2, 3])[0]\n❧");
    let Declaration::Value(decl) = &module.declarations[1] else {
        panic!("expected a Value declaration")
    };
    match &decl.value {
        Expression::Index { array, .. } => {
            assert!(matches!(**array, Expression::Application { .. }));
        }
        other => panic!("expected an Index, got {other:?}"),
    }
}

// ── ImportDeclaration ────────────────────────────────────────────────

#[test]
fn import_declaration_parses_the_module_name() {
    let module = parse_ok("\u{27F2}shapes\n\u{2767}\n");
    let Declaration::Import(id) = &module.declarations[0] else {
        panic!("expected an Import declaration")
    };
    assert_eq!(id.module_name, "shapes");
}

#[test]
fn import_declaration_missing_a_module_name_is_rejected() {
    let msg = parse_err("\u{27F2}\n\u{2767}\n");
    assert!(msg.contains("expected a module name"), "{msg}");
}

#[test]
fn multiple_import_declarations_parse_in_order() {
    let module = parse_ok("\u{27F2}shapes\n\u{27F2}colors\nx \u{2254} 5\n\u{2767}\n");
    let Declaration::Import(a) = &module.declarations[0] else {
        panic!("expected an Import declaration")
    };
    let Declaration::Import(b) = &module.declarations[1] else {
        panic!("expected an Import declaration")
    };
    assert_eq!(a.module_name, "shapes");
    assert_eq!(b.module_name, "colors");
    assert!(matches!(module.declarations[2], Declaration::Value(_)));
}
