//! Pipeline entry points over a single piece of text: no files, no imports.
//!
//! These are the operations that make sense on source alone — `run` and
//! `check` of a self-contained snippet, `fmt`, and `inspect`. Anything that
//! must find other files lives in the parent module.

use super::RunResult;
use obfusku_diagnostics::{Diagnostic, SourceMap};

/// The `run` pipeline: parse, desugar, type-check, evaluate.
///
/// Returns the last evaluated binding value alongside any non-fatal
/// diagnostics (such as unreachable match arms).
pub fn run_source(source: &str) -> Result<(RunResult, Vec<Diagnostic>), Vec<Diagnostic>> {
    let mut source_map = SourceMap::new();
    let source_id = source_map.add_file(source);

    let core_module = obfusku_syntax::parse_and_desugar(source, source_id)?;
    let typed = obfusku_typecheck::check(&core_module)?;
    obfusku_runtime::evaluate(&core_module)
        .map(|v| (RunResult(v), typed.warnings))
        .map_err(|d| vec![d])
}

/// The `check` pipeline: parse, desugar, type-check — no execution. Returns
/// the warnings on success, the fatal diagnostics on failure.
pub fn check_source(source: &str) -> Result<Vec<Diagnostic>, Vec<Diagnostic>> {
    let mut source_map = SourceMap::new();
    let source_id = source_map.add_file(source);

    let core_module = obfusku_syntax::parse_and_desugar(source, source_id)?;
    obfusku_typecheck::check(&core_module).map(|typed| typed.warnings)
}

/// The `fmt` pipeline: parse the surface syntax only (desugaring would lose
/// what the formatter must reproduce) and print it canonically, keeping the
/// comments the lexer set aside.
pub fn fmt_source(source: &str) -> Result<String, Vec<Diagnostic>> {
    let mut source_map = SourceMap::new();
    let source_id = source_map.add_file(source);

    let (tokens, comments) =
        obfusku_syntax::lexer::tokenize_with_comments(source, source_id).map_err(|d| vec![d])?;
    let module = obfusku_syntax::parser::parse(&tokens, source_id)?;
    Ok(obfusku_fmt::format_with_comments(
        &module, &comments, source,
    ))
}

/// Which intermediate representation [`inspect_source`] prints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InspectKind {
    /// The lexer's token stream.
    Tokens,
    /// The surface AST.
    Ast,
    /// The desugared Core AST.
    Core,
}

/// The `inspect` pipeline: exposes an intermediate representation through
/// the `Debug` output of the types `obfusku-syntax` and `obfusku-core`
/// already publish. There is deliberately no dedicated pretty-printer: the
/// contract promises none, and a second printer would drift from the types.
pub fn inspect_source(source: &str, kind: InspectKind) -> Result<String, Vec<Diagnostic>> {
    let mut source_map = SourceMap::new();
    let source_id = source_map.add_file(source);

    let tokens = obfusku_syntax::lexer::tokenize(source, source_id).map_err(|d| vec![d])?;
    if kind == InspectKind::Tokens {
        return Ok(format!("{tokens:#?}"));
    }
    let module = obfusku_syntax::parser::parse(&tokens, source_id)?;
    if kind == InspectKind::Ast {
        return Ok(format!("{module:#?}"));
    }
    let core_module = obfusku_syntax::desugar::desugar(&module)?;
    Ok(format!("{core_module:#?}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use obfusku_runtime::Value;

    #[test]
    fn run_source_reports_the_last_binding() {
        let (result, warnings) = run_source("a ≔ 1\nb ≔ a ✚ 2\n❧\n").unwrap();
        assert!(matches!(result.0, Value::Int(3)));
        assert!(warnings.is_empty());
    }

    #[test]
    fn run_source_reports_a_private_last_binding_too() {
        let (result, _) = run_source("a ≔⟳ 1\nb ≔ 9\n❧\n").unwrap();
        assert!(matches!(result.0, Value::Int(9)));
    }

    #[test]
    fn a_type_error_is_a_diagnostic_not_a_run() {
        assert!(run_source("a ≔ 1 ✚ \"x\"\n❧\n").is_err());
        assert!(check_source("a ≔ 1 ✚ \"x\"\n❧\n").is_err());
    }

    #[test]
    fn check_source_does_not_execute() {
        // An evaluation that would fail at runtime still checks.
        assert!(check_source("a ≔ 1 ÷ 0\n❧\n").is_ok());
        assert!(run_source("a ≔ 1 ÷ 0\n❧\n").is_err());
    }

    #[test]
    fn formatting_is_idempotent() {
        let once = fmt_source("a   ≔   1\n❧\n").unwrap();
        assert_eq!(fmt_source(&once).unwrap(), once);
    }

    #[test]
    fn formatting_keeps_comments() {
        let out = fmt_source("// note\na ≔ 1\n❧\n").unwrap();
        assert!(out.contains("note"), "{out}");
    }

    #[test]
    fn a_syntax_error_stops_formatting() {
        assert!(fmt_source("a ≔ \n❧\n").is_err());
    }

    #[test]
    fn inspect_prints_each_stage_and_stops_early_on_errors() {
        let src = "a ≔ 1\n❧\n";
        for kind in [InspectKind::Tokens, InspectKind::Ast, InspectKind::Core] {
            assert!(!inspect_source(src, kind).unwrap().is_empty());
        }
        assert!(inspect_source("a ≔ \n❧\n", InspectKind::Ast).is_err());
    }
}
