//! Surface-to-Core lowering conformance tests (`SEMANTIC_CORE.md` §16), grouped by what they exercise.

// Tests destructure one expected shape and fail on anything else.
#![allow(clippy::match_wildcard_for_single_variants)]

pub use obfusku_core::ast::{BinOp as CoreBinOp, UnOp as CoreUnOp};
pub use obfusku_core::ast::{Binding, Expr, Literal};
pub use obfusku_diagnostics::SourceMap;
pub use obfusku_syntax::{desugar, lexer, parser};

/// Flattens every `BindingGroup` into its member `Binding`s, in order.
/// Tests that care about the `Let`/`LetRec` distinction inspect
/// `module.bindings` directly instead.
pub fn flat(module: &obfusku_core::ast::Module) -> Vec<&Binding> {
    user_groups(module)
        .iter()
        .flat_map(obfusku_core::ast::BindingGroup::bindings)
        .collect()
}

/// Every `desugar` output starts with five synthesized `Exception`
/// constructor bindings (§15.2, P0-D's `IntegerOverflow` included)
/// before the user's own source — skipped here so index-based
/// assertions mean "the user's first binding."
pub fn user_groups(module: &obfusku_core::ast::Module) -> &[obfusku_core::ast::BindingGroup] {
    &module.bindings[5..]
}

pub fn lower_ok(source: &str) -> obfusku_core::ast::Module {
    let mut map = SourceMap::new();
    let id = map.add_file(source);
    let tokens = lexer::tokenize(source, id).unwrap_or_else(|d| panic!("lex error: {d:?}"));
    let surface = parser::parse(&tokens, id).unwrap_or_else(|ds| panic!("parse error(s): {ds:?}"));
    desugar::desugar(&surface).unwrap_or_else(|ds| panic!("desugar error(s): {ds:?}"))
}

pub fn lower_err(source: &str) -> String {
    let mut map = SourceMap::new();
    let id = map.add_file(source);
    let tokens = lexer::tokenize(source, id).unwrap_or_else(|d| panic!("lex error: {d:?}"));
    let surface = parser::parse(&tokens, id).unwrap_or_else(|ds| panic!("parse error(s): {ds:?}"));
    match desugar::desugar(&surface) {
        Err(ds) => ds[0].message.clone(),
        Ok(m) => panic!("expected a desugar error, got a valid Core module: {m:?}"),
    }
}
