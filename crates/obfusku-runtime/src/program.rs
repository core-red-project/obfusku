//! Running a whole module: the crate's entry points.

use crate::env::Env;
use crate::eval::{self, TraceFrame};
use crate::ir::{Compiler, GlobalCell, Globals};
use crate::value::Value;
use crate::variants;
use obfusku_core::ast::{Binding, BindingGroup, Expr, Module};
use obfusku_diagnostics::Diagnostic;
use std::rc::Rc;

/// A runtime failure: the uncaught exception or runtime error, and the
/// function activations live when it was raised (innermost first).
#[derive(Debug)]
pub struct Fault {
    /// The failure, located at the raise.
    pub diagnostic: Diagnostic,
    /// What was running, so a host can print a call trace.
    pub trace: Vec<TraceFrame>,
}

/// One evaluated top-level binding: `(name, exported, value)`.
pub type Evaluated = (String, bool, Value);

/// Evaluates an already-checked Core module and returns the value of its
/// last binding, or the first uncaught exception or runtime error.
///
/// Assumes the input passed `obfusku_typecheck::check`: an ill-typed program
/// is a caller error, reported as a runtime error where it is noticed, not
/// something this crate detects up front.
pub fn evaluate(module: &Module) -> Result<Value, Diagnostic> {
    let (_, last) = run_module(module, std::iter::empty()).map_err(|f| f.diagnostic)?;
    Ok(last)
}

/// Like [`evaluate`], but `prelude` names are bound before the module's own
/// bindings run, as if imported, and *every* top-level binding is returned
/// with its export flag, not only the last.
///
/// The module resolver uses this to pull one module's exported values out
/// for splicing into an importer.
pub fn evaluate_with_prelude(
    module: &Module,
    prelude: impl IntoIterator<Item = (String, Value)>,
) -> Result<Vec<Evaluated>, Diagnostic> {
    evaluate_with_prelude_traced(module, prelude).map_err(|f| f.diagnostic)
}

/// [`evaluate_with_prelude`] keeping the call trace of a failure.
pub fn evaluate_with_prelude_traced(
    module: &Module,
    prelude: impl IntoIterator<Item = (String, Value)>,
) -> Result<Vec<Evaluated>, Fault> {
    let (bindings, _) = run_module(module, prelude)?;
    Ok(bindings)
}

/// A function value running `body` with `param` bound, for hosts and tests
/// that need one without a whole module.
///
/// Evaluating a lambda cannot raise, so the `Unit` fallback is unreachable;
/// it exists only to keep this infallible.
#[must_use]
pub fn closure_from_core(param: &str, body: &Expr) -> Value {
    let mut globals = Globals::new();
    let lambda = Expr::Lambda {
        param: param.to_string(),
        param_ty: None,
        body: Rc::new(body.clone()),
        span: body.span(),
    };
    let node = Compiler::new(&mut globals).expr(&lambda, None);
    eval::eval(&node, &Env::root()).unwrap_or(Value::Unit)
}

fn fault(diagnostic: Diagnostic) -> Fault {
    Fault {
        diagnostic,
        trace: eval::take_trace(),
    }
}

/// Compiles and evaluates one binding against `globals`.
fn evaluate_binding(globals: &mut Globals, root: &Env, binding: &Binding) -> Result<Value, Fault> {
    let node = Compiler::new(globals).expr(&binding.value, Some(&binding.name));
    eval::eval_top(&node, root, binding.span).map_err(fault)
}

/// Runs the module's binding groups in order.
///
/// A plain `Let` is evaluated and then published under its name. A recursive
/// group publishes *empty* cells first, so each member compiles against
/// cells for all the others, and fills them as it evaluates: a member only
/// reads another's cell when it is later called, by which time it is filled
/// (`SEMANTIC_CORE.md` §11).
fn run_module(
    module: &Module,
    prelude: impl IntoIterator<Item = (String, Value)>,
) -> Result<(Vec<Evaluated>, Value), Fault> {
    variants::register(&module.type_decls);
    let mut globals = Globals::new();
    for (name, value) in prelude {
        let cell = GlobalCell::new(&name, Some(value));
        globals.insert(name, cell);
    }
    let root = Env::root();
    let mut last = Value::Unit;
    let mut all = Vec::new();

    for group in &module.bindings {
        match group {
            BindingGroup::Let(binding) => {
                let value = evaluate_binding(&mut globals, &root, binding)?;
                globals.insert(
                    binding.name.clone(),
                    GlobalCell::new(&binding.name, Some(value.clone())),
                );
                last = value.clone();
                all.push((binding.name.clone(), binding.is_exported, value));
            }
            BindingGroup::LetRec(members) => {
                let cells: Vec<_> = members
                    .iter()
                    .map(|b| {
                        let cell = GlobalCell::new(&b.name, None);
                        globals.insert(b.name.clone(), Rc::clone(&cell));
                        cell
                    })
                    .collect();
                for (binding, cell) in members.iter().zip(cells) {
                    let value = evaluate_binding(&mut globals, &root, binding)?;
                    *cell.value.borrow_mut() = Some(value.clone());
                    last = value.clone();
                    all.push((binding.name.clone(), binding.is_exported, value));
                }
            }
        }
    }
    Ok((all, last))
}
