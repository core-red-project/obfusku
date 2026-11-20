//! The interactive session behind `obfusku repl` (`ADR-013`).
//!
//! **Incremental, not replayed.** Each line is checked and evaluated once,
//! against everything accepted so far, and its bindings are folded back into
//! the session. Replaying the history on every line would be simpler but
//! would re-run side effects (`print`, file writes) each time.

use crate::natives;
use crate::project::exports::{export_types, export_values};
use crate::report::render;
use crate::stdlib;
use obfusku_core::ast::{BindingGroup, Expr, TypeDecl};
use obfusku_diagnostics::{Diagnostic, SourceMap};
use obfusku_runtime::Value;
use obfusku_typecheck::Prelude;
use std::collections::{HashMap, HashSet};

/// A REPL session. Seeded with the stdlib and the natives that make sense
/// without a project: console and process natives, streams and the pure
/// families — but **no filesystem**, because a REPL has no entry file to
/// scope a root against and inventing a policy for it would be a decision
/// nobody made (`LANGUAGE_SPEC.md` §5).
pub struct ReplSession {
    /// Every line ever submitted, accepted or not, so any span renders.
    source_map: SourceMap,
    type_prelude: Prelude,
    value_prelude: HashMap<String, Value>,
    type_decls: Vec<TypeDecl>,
    /// Names a previous `submit` declared `≔˚`. Threaded into the next
    /// desugaring so a later line that reads or rebinds one lowers to
    /// `MutRead`/`MutRebind`, as it would inside one file, instead of a bare
    /// `Var` that would use the raw cell where its element type is expected.
    mut_names: HashSet<String>,
}

impl ReplSession {
    /// A session seeded with the standard library.
    pub fn new() -> Result<Self, Vec<Diagnostic>> {
        let mut source_map = SourceMap::new();
        let (mut type_prelude, mut value_prelude, type_decls) = stdlib::load(&mut source_map)?;
        let (native_types, native_values) = natives::load(None, &[]);
        type_prelude.extend(native_types);
        value_prelude.extend(native_values);
        Ok(Self {
            source_map,
            type_prelude,
            value_prelude,
            type_decls,
            mut_names: HashSet::new(),
        })
    }

    /// Parses, type-checks and evaluates one input line.
    ///
    /// On success its declarations become available to later lines; on
    /// failure the session is left exactly as it was. Returns the value of
    /// the last binding and any warnings.
    pub fn submit(&mut self, line: &str) -> Result<(Value, Vec<Diagnostic>), Vec<Diagnostic>> {
        // A trailing `❧` terminates the module the line is wrapped in.
        let candidate = format!("{line}\n\u{2767}\n");
        let source_id = self.source_map.add_file(&candidate);
        let tokens = obfusku_syntax::lexer::tokenize(&candidate, source_id).map_err(|d| vec![d])?;
        let surface = obfusku_syntax::parser::parse(&tokens, source_id)?;
        let core_module = obfusku_syntax::desugar::desugar_with_ambient(
            &surface,
            &self.mut_names,
            &self.type_decls,
        )?;
        let typed = obfusku_typecheck::check_with_prelude_and_types(
            &core_module,
            self.type_prelude.clone(),
            self.type_decls.clone(),
        )?;
        let bindings =
            obfusku_runtime::evaluate_with_prelude(&core_module, self.value_prelude.clone())
                .map_err(|d| vec![d])?;

        // Nothing below can fail, so the session only changes on success.
        let last = bindings.last().map_or(Value::Unit, |(_, _, v)| v.clone());
        self.type_prelude.extend(export_types(&typed, false));
        self.value_prelude.extend(export_values(bindings, false));
        for group in &core_module.bindings {
            if let BindingGroup::Let(b) = group
                && matches!(b.value, Expr::MutCell { .. })
            {
                self.mut_names.insert(b.name.clone());
            }
        }
        self.type_decls.extend(core_module.type_decls);

        Ok((last, typed.warnings))
    }

    /// Renders diagnostics from a failed [`ReplSession::submit`] against this
    /// session's accumulated `SourceMap`.
    #[must_use]
    pub fn render(&self, diagnostics: &[Diagnostic]) -> String {
        render::diagnostics_map(&self.source_map, diagnostics)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn int(v: &Value) -> i64 {
        match v {
            Value::Int(i) => *i,
            other => panic!("expected an Int, got {other:?}"),
        }
    }

    #[test]
    fn a_binding_persists_into_later_lines() {
        let mut s = ReplSession::new().unwrap();
        s.submit("x ≔ 5").unwrap();
        let (v, _) = s.submit("y ≔ x ✚ 1").unwrap();
        assert_eq!(int(&v), 6);
    }

    #[test]
    fn a_failed_line_leaves_the_session_untouched() {
        let mut s = ReplSession::new().unwrap();
        s.submit("x ≔ 1").unwrap();
        assert!(s.submit("x ≔ 1 ✚ \"no\"").is_err());
        let (v, _) = s.submit("z ≔ x ✚ 1").unwrap();
        assert_eq!(int(&v), 2);
    }

    #[test]
    fn a_type_declaration_is_usable_on_the_next_line() {
        let mut s = ReplSession::new().unwrap();
        s.submit("Pair ≔ { Pair(⟁, ⟁) }").unwrap();
        assert!(s.submit("p ≔ Pair(1, 2)").is_ok());
    }

    #[test]
    fn the_stdlib_is_ambient() {
        let mut s = ReplSession::new().unwrap();
        assert!(s.submit("l ≔ ⁝(1, ⊘)").is_ok());
    }

    #[test]
    fn there_is_no_filesystem_in_the_repl() {
        let mut s = ReplSession::new().unwrap();
        assert!(s.submit("t ≔ ⌁↓⌬(\"x\")").is_err());
    }

    #[test]
    fn diagnostics_render_against_the_sessions_own_lines() {
        let mut s = ReplSession::new().unwrap();
        let err = s.submit("q ≔ nope").unwrap_err();
        assert!(s.render(&err).contains("nope"));
    }
}
