//! Stdlib as an implicit prelude — `IMPLEMENTATION_ARCHITECTURE.md` §15's
//! favored direction: ordinary `.obk` source, run through the same
//! parse/desugar/typecheck/evaluate pipeline as user code, its whole
//! top-level surface made ambiently available (no explicit `⟲`) via the
//! same `Prelude`/`check_with_prelude`/`evaluate_with_prelude` machinery
//! `modules::resolve_module` already uses for imports — the only
//! difference is *which* bindings get exposed: every top-level name
//! here, not just `⟳`-exported ones (there is no other module to keep
//! anything private from).
//!
//! `List<T>` and its `map`/`filter`/`fold` combinators are the only
//! contents so far (§15/§20's "most of stdlib is Obfusku source, only
//! I/O needs a native hook" — I/O stays fully out of scope here).
//! I/O, `run_source`/`check_source` (REPL), and `Array<T>` combinators
//! are deliberately not covered by this slice.

use obfusku_diagnostics::{Diagnostic, SourceMap};
use obfusku_runtime::Value;
use obfusku_typecheck::{Prelude, Scheme, TypeResult};
use std::collections::HashMap;

const SOURCE: &str = include_str!("stdlib.obk");

/// Parses, desugars, type-checks, and evaluates the stdlib source
/// (registering its text in `source_map`, so any diagnostic it could
/// ever produce — none expected from this fixed, pre-verified source,
/// but the contract should hold regardless — still renders correctly),
/// returning every one of its top-level names as a `Prelude`/value pair
/// ready to seed another module's `check_with_prelude`/
/// `evaluate_with_prelude` call, unconditionally rather than only on an
/// explicit `ImportDeclaration`.
#[allow(clippy::type_complexity)]
pub fn load(
    source_map: &mut SourceMap,
) -> Result<
    (
        Prelude,
        HashMap<String, Value>,
        Vec<obfusku_core::ast::TypeDecl>,
    ),
    Vec<Diagnostic>,
> {
    let source_id = source_map.add_file(SOURCE);
    let tokens = obfusku_syntax::lexer::tokenize(SOURCE, source_id).map_err(|d| vec![d])?;
    let surface = obfusku_syntax::parser::parse(&tokens, source_id)?;
    let core_module = obfusku_syntax::desugar::desugar(&surface)?;
    let typed = obfusku_typecheck::check(&core_module)?;
    let evaluated = obfusku_runtime::evaluate_with_prelude(&core_module, std::iter::empty())
        .map_err(|d| vec![d])?;

    // `Exception`'s built-in constructors (`modules::BUILTIN_EXPORTS`)
    // are synthesized into *every* module, this one included — treating
    // them as stdlib-provided would collide with the importer's own
    // copy of the same universally-available built-ins.
    let mut types = Prelude::new();
    for tb in &typed.bindings {
        if crate::modules::BUILTIN_EXPORTS.contains(&tb.name.as_str()) {
            continue;
        }
        let scheme = match &tb.ty {
            TypeResult::Monomorphic(core_ty) => Scheme::monomorphic(
                obfusku_typecheck::instantiate_core_type(core_ty, &HashMap::new()),
            ),
            TypeResult::Polymorphic(scheme) => scheme.clone(),
        };
        types.insert(tb.name.clone(), scheme);
    }

    let mut values = HashMap::new();
    for (name, _exported, value) in evaluated {
        if crate::modules::BUILTIN_EXPORTS.contains(&name.as_str()) {
            continue;
        }
        values.insert(name, value);
    }

    // `List<T>`'s own `TypeDeclaration` — needed so a user program can
    // pattern-match against `Nil`/`Cons` (previously only their
    // constructor *functions* crossed into the ambient environment; see
    // `obfusku_typecheck::check_with_prelude_and_types`'s own doc
    // comment for why this was a real, previously-undiscovered gap).
    let type_decls = core_module.type_decls.clone();

    Ok((types, values, type_decls))
}
