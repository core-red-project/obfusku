//! Parser conformance tests (`spec/CONCRETE_SYMBOLIC_GRAMMAR.md`), grouped by what they exercise.

pub use obfusku_diagnostics::SourceMap;
pub use obfusku_syntax::ast::{Argument, Declaration, Expression, Stage};
pub use obfusku_syntax::ast::{BaseType, BinOp, TypeRef, UnOp};
pub use obfusku_syntax::{lexer, parser};

/// Parses `source` and panics with the diagnostics if it fails —
/// convenience for the "should succeed" tests below.
pub fn parse_ok(source: &str) -> obfusku_syntax::ast::Module {
    let mut map = SourceMap::new();
    let id = map.add_file(source);
    let tokens = lexer::tokenize(source, id).unwrap_or_else(|d| panic!("lex error: {d:?}"));
    parser::parse(&tokens, id).unwrap_or_else(|ds| panic!("parse error(s): {ds:?}"))
}

/// Parses `source` and returns the error — convenience for the "should
/// fail" tests below.
pub fn parse_err(source: &str) -> String {
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

// ── resource limits ────────────────────────────────────────────────────
