//! Execution over Core AST — value representation, evaluator, `Cell`,
//! `Raise`/`Catch` unwinding, the TCO trampoline
//! (`spec/SEMANTIC_CORE.md` §12–§15).
//!
//! Deliberately independent of `obfusku-syntax`: by the time a program
//! is executing it is already fully desugared Core AST, so this crate
//! has no reason to know surface syntax exists. See
//! `spec/IMPLEMENTATION_ARCHITECTURE.md` §6, §11.

pub mod env;
pub mod eval;
pub mod value;

use obfusku_core::ast::{BindingGroup, Module};
use obfusku_diagnostics::Diagnostic;
pub use value::{NativeFn, Value};

/// Evaluates an already-type-checked Core module, returning the value
/// of its last binding, or the first uncaught exception/runtime error.
/// Assumes the input already passed `obfusku_typecheck::check` — an
/// ill-typed program is a caller error, not something this detects.
pub fn evaluate(module: &Module) -> Result<Value, Diagnostic> {
    let (_, last) = evaluate_all(module, std::iter::empty())?;
    Ok(last)
}

/// Like [`evaluate`], but `prelude` names are bound into the root
/// environment before the module's own bindings run — as if imported —
/// and every top-level binding's `(name, exported, Value)` is returned,
/// not only the last one. The module-resolver layer uses this to pull
/// one module's exported values out for splicing into an importer;
/// `evaluate` itself is unaffected; still just the last value.
pub fn evaluate_with_prelude(
    module: &Module,
    prelude: impl IntoIterator<Item = (String, Value)>,
) -> Result<Vec<(String, bool, Value)>, Diagnostic> {
    let (bindings, _) = evaluate_all(module, prelude)?;
    Ok(bindings)
}

#[allow(clippy::type_complexity)]
fn evaluate_all(
    module: &Module,
    prelude: impl IntoIterator<Item = (String, Value)>,
) -> Result<(Vec<(String, bool, Value)>, Value), Diagnostic> {
    let root = env::Env::root();
    for (name, v) in prelude {
        root.bind(name, v);
    }
    let mut last = Value::Unit;
    let mut all = Vec::new();
    for group in &module.bindings {
        match group {
            BindingGroup::Let(binding) => {
                let v = eval::eval_top(&binding.value, &root)?;
                root.bind(binding.name.clone(), v.clone());
                last = v.clone();
                all.push((binding.name.clone(), binding.exported, v));
            }
            BindingGroup::LetRec(members) => {
                for binding in members {
                    let v = eval::eval_top(&binding.value, &root)?;
                    root.bind(binding.name.clone(), v.clone());
                    last = v.clone();
                    all.push((binding.name.clone(), binding.exported, v));
                }
            }
        }
    }
    Ok((all, last))
}
