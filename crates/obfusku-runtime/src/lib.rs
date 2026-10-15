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
pub mod ir;
pub mod ops;
pub mod pmap;
pub mod pvec;
pub mod text;
pub mod types;
pub mod value;

use obfusku_core::ast::{BindingGroup, Module};
use obfusku_diagnostics::Diagnostic;
pub use ops::show;
pub use text::Text;
pub use value::{NativeFn, Value};

/// A runtime failure: the uncaught exception or runtime error, and the
/// function activations live when it was raised (innermost first).
#[derive(Debug)]
pub struct Fault {
    pub diagnostic: Diagnostic,
    pub trace: Vec<eval::TraceFrame>,
}

/// Evaluates an already-type-checked Core module, returning the value
/// of its last binding, or the first uncaught exception/runtime error.
/// Assumes the input already passed `obfusku_typecheck::check` — an
/// ill-typed program is a caller error, not something this detects.
pub fn evaluate(module: &Module) -> Result<Value, Diagnostic> {
    let (_, last) = evaluate_all(module, std::iter::empty()).map_err(|f| f.diagnostic)?;
    Ok(last)
}

/// Like [`evaluate`], but `prelude` names are bound before the module's
/// own bindings run — as if imported — and every top-level binding's
/// `(name, exported, Value)` is returned, not only the last one. The
/// module-resolver layer uses this to pull one module's exported values
/// out for splicing into an importer.
pub fn evaluate_with_prelude(
    module: &Module,
    prelude: impl IntoIterator<Item = (String, Value)>,
) -> Result<Vec<(String, bool, Value)>, Diagnostic> {
    evaluate_with_prelude_traced(module, prelude).map_err(|f| f.diagnostic)
}

/// [`evaluate_with_prelude`] keeping the call trace of a failure.
pub fn evaluate_with_prelude_traced(
    module: &Module,
    prelude: impl IntoIterator<Item = (String, Value)>,
) -> Result<Vec<(String, bool, Value)>, Fault> {
    let (bindings, _) = evaluate_all(module, prelude)?;
    Ok(bindings)
}

/// A closure value running `body` with `param` bound — for hosts and
/// tests that need a function value without a whole module.
#[must_use]
pub fn closure_from_core(param: &str, body: &obfusku_core::ast::Expr) -> Value {
    let mut globals = ir::Globals::new();
    let mut compiler = ir::Compiler::new(&mut globals);
    let lambda = obfusku_core::ast::Expr::Lambda {
        param: param.to_string(),
        param_ty: None,
        body: std::rc::Rc::new(body.clone()),
        span: body.span(),
    };
    let node = compiler.expr(&lambda, None);
    eval::eval(&node, &env::Env::root()).unwrap_or(Value::Unit)
}

fn fault(diagnostic: Diagnostic) -> Fault {
    Fault {
        diagnostic,
        trace: eval::take_trace(),
    }
}

#[allow(clippy::type_complexity)]
fn evaluate_all(
    module: &Module,
    prelude: impl IntoIterator<Item = (String, Value)>,
) -> Result<(Vec<(String, bool, Value)>, Value), Fault> {
    use ir::{Compiler, GlobalCell, Globals};
    use std::cell::RefCell;
    use std::rc::Rc;

    ir::register_types(&module.type_decls);
    let mut globals = Globals::new();
    let cell = |name: &str, value: Option<Value>| {
        Rc::new(GlobalCell {
            name: Rc::from(name),
            value: RefCell::new(value),
        })
    };
    for (name, v) in prelude {
        let c = cell(&name, Some(v));
        globals.insert(name, c);
    }
    let root = env::Env::root();
    let mut last = Value::Unit;
    let mut all = Vec::new();
    for group in &module.bindings {
        match group {
            BindingGroup::Let(binding) => {
                let node = Compiler::new(&mut globals).expr(&binding.value, Some(&binding.name));
                let v = eval::eval_top(&node, &root, binding.span).map_err(fault)?;
                globals.insert(binding.name.clone(), cell(&binding.name, Some(v.clone())));
                last = v.clone();
                all.push((binding.name.clone(), binding.exported, v));
            }
            BindingGroup::LetRec(members) => {
                let cells: Vec<_> = members
                    .iter()
                    .map(|b| {
                        let c = cell(&b.name, None);
                        globals.insert(b.name.clone(), Rc::clone(&c));
                        c
                    })
                    .collect();
                for (binding, c) in members.iter().zip(cells) {
                    let node =
                        Compiler::new(&mut globals).expr(&binding.value, Some(&binding.name));
                    let v = eval::eval_top(&node, &root, binding.span).map_err(fault)?;
                    *c.value.borrow_mut() = Some(v.clone());
                    last = v.clone();
                    all.push((binding.name.clone(), binding.exported, v));
                }
            }
        }
    }
    Ok((all, last))
}
