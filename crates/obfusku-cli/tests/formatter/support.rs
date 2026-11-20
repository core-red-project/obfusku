//! Shared harness: parse, format, re-parse, and compare behaviour before and after.

use obfusku_syntax::{lexer, parser};

pub fn parse_ok(source: &str) -> obfusku_syntax::ast::Module {
    let mut map = obfusku_diagnostics::SourceMap::new();
    let id = map.add_file(source);
    let tokens = lexer::tokenize(source, id).unwrap_or_else(|d| panic!("lex error: {d:?}"));
    parser::parse(&tokens, id).unwrap_or_else(|ds| panic!("parse error(s) in {source:?}: {ds:?}"))
}

/// Formats `source`, asserts the result re-parses without error, and
/// returns the formatted text.
pub fn format_and_reparse(source: &str) -> String {
    let module = parse_ok(source);
    let formatted = obfusku_fmt::format(&module);
    parse_ok(&formatted); // must not panic
    formatted
}

/// The stronger check: runs `source` and `format(parse(source))` both
/// through the real pipeline and asserts identical runtime results —
/// proving the formatter didn't change program *behavior*, not merely
/// that its output happens to parse.
pub fn assert_behavior_preserved(source: &str) {
    let formatted = format_and_reparse(source);
    let before = obfusku_cli::run_source(source);
    let after = obfusku_cli::run_source(&formatted);
    match (before, after) {
        (Ok(b), Ok(a)) => assert_eq!(
            format!("{:?}", b.0),
            format!("{:?}", a.0),
            "formatting changed the result of:\n{source}\n---formatted to---\n{formatted}"
        ),
        (Err(be), Err(ae)) => assert_eq!(
            be.len(),
            ae.len(),
            "formatting changed the error count of:\n{source}\n---formatted to---\n{formatted}"
        ),
        (b, a) => panic!(
            "formatting changed success/failure of:\n{source}\n---formatted to---\n{formatted}\n\
             before: {b:?}\nafter: {a:?}"
        ),
    }
}
