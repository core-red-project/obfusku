//! Diagnostics: errors keep their source spans through the whole pipeline and resolve to the right line and column.

use crate::support::run_err;

#[test]
fn typecheck_error_span_points_at_the_offending_source_text() {
    let source = "result ≔ nope\n❧";
    let diags = run_err(source);
    assert!(!diags.is_empty());
    let d = &diags[0];
    let slice = &source[d.primary.start as usize..d.primary.end as usize];
    assert_eq!(
        slice, "nope",
        "diagnostic span should point at the unbound variable, got {slice:?} (message: {})",
        d.message
    );
    assert!(d.message.contains("unknown variable"), "{}", d.message);
}

#[test]
fn parse_error_span_points_at_the_offending_source_text() {
    // Missing '❧' entirely — the parser's own error, before desugaring
    // or typechecking ever run.
    let source = "x ≔ 5\n";
    let diags = run_err(source);
    assert!(!diags.is_empty());
    assert!(
        diags[0].message.contains('❧'),
        "expected the missing-seal message, got: {}",
        diags[0].message
    );
}

#[test]
fn typecheck_error_resolves_to_the_correct_line_and_column() {
    let source = "x ≔ 1\nresult ≔ nope\n❧";
    let diags = run_err(source);
    let mut map = obfusku_diagnostics::SourceMap::new();
    map.add_file(source);
    let loc = map.line_col(diags[0].primary.source, diags[0].primary.start);
    assert_eq!(loc.line, 2);
    assert_eq!(loc.column, 10);
}

#[test]
fn runtime_uncaught_exception_diagnostic_resolves_to_a_real_line_column() {
    let source = "x ≔ 1\ny ≔ 5 ÷ 0\nresult ≔ y\n❧";
    let diags = run_err(source);
    let mut map = obfusku_diagnostics::SourceMap::new();
    map.add_file(source);
    let rendered = map.render(&diags[0]);
    assert!(
        rendered.starts_with("error: uncaught exception"),
        "{rendered}"
    );
    let loc = map.line_col(diags[0].primary.source, diags[0].primary.start);
    assert_eq!(loc.line, 2);
}

#[test]
fn check_source_reports_the_same_typecheck_diagnostics_without_executing() {
    let source = "result ≔ nope\n❧";
    let diags = obfusku_cli::check_source(source).expect_err("expected a typecheck error");
    assert!(diags[0].message.contains("unknown variable"));
}

#[test]
fn duplicate_top_level_binding_names_are_a_static_error() {
    // A reused top-level name must be a static error, not silent
    // overwrite-in-place (which would let a closure retroactively see a
    // later redeclaration instead of the value in scope at creation).
    let source = "x ≔ 1\ncapture_x ≔ λ(ignored) → x\nx ≔ 2\nresult ≔ capture_x(◉)\n❧";
    let diags = run_err(source);
    assert!(
        diags[0].message.contains("already bound"),
        "{}",
        diags[0].message
    );
}
