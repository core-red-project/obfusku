//! Capabilities declared in `obfusku.toml` (`ADR-029`): the natives a
//! program may call, by family. A denied native keeps its type, so the
//! program still checks, but calling it is a static error in the entry
//! module and `Failure("PermissionDenied", …)` anywhere else.

use crate::manifest::{Capabilities, FsAccess};
use crate::natives::failure;
use obfusku_core::ast::{BindingGroup, Expr, Module};
use obfusku_diagnostics::{Diagnostic, Severity};
use obfusku_runtime::Value;
use obfusku_runtime::eval::Raised;
use obfusku_runtime::value::NativeFn;
use std::collections::{HashMap, HashSet};

const CONSOLE: &[&str] = &["⌁↑", "⌁↓", "⌁↑⍉", "⌁↑⎙", "⌁↓⎙", "⌁⍰⎙", "⌁↑⎚", "⌁↓⎚", "⌁⍰⌖"];
const FS: &[&str] = &[
    "⌁↓⌬",
    "⌁↑⌬",
    "⌁↓⌂",
    "⌁↑⌂",
    "⌁⍰⌬",
    "⌁⍰⌂",
    "⌁⊕⌬",
    "⌁⊖⌬",
    "⌁⊖⌂",
    "⌁⇄⌬",
    "⌁⧉⌬",
    "⌁⍰⌭",
    "⌁⍰⧗",
    "⌁↓⌭",
    "⌁↑⌭",
];
const ENV: &[&str] = &["⌁↓⎈"];
const ARGS: &[&str] = &["⌁↓⌥"];
const CLOCK: &[&str] = &["⌁↓⧗", "⌁↓⧖", "⌁↑⧗"];
const RANDOM: &[&str] = &["⌁↓⚀", "⌁↓⚁"];
const EXIT: &[&str] = &["⌁↯"];

/// Native name → the capability that gates it, for every denied family.
#[must_use]
pub fn denied(caps: &Capabilities) -> HashMap<&'static str, &'static str> {
    let families: [(&str, bool, &[&'static str]); 7] = [
        ("console", caps.console, CONSOLE),
        ("fs", caps.fs != FsAccess::None, FS),
        ("env", caps.env, ENV),
        ("args", caps.args, ARGS),
        ("clock", caps.clock, CLOCK),
        ("random", caps.random, RANDOM),
        ("exit", caps.exit, EXIT),
    ];
    let mut out = HashMap::new();
    for (family, allowed, names) in families {
        if !allowed {
            for name in names {
                out.insert(*name, family_label(family));
            }
        }
    }
    out
}

fn family_label(family: &str) -> &'static str {
    match family {
        "console" => "console",
        "fs" => "fs",
        "env" => "env",
        "args" => "args",
        "clock" => "clock",
        "random" => "random",
        _ => "exit",
    }
}

fn denial(name: &str, family: &str) -> Value {
    let message = format!(
        "'{name}' needs the '{family}' capability; declare it in [capabilities] of obfusku.toml"
    );
    Value::Native(std::rc::Rc::new(NativeFn {
        name: name.to_string(),
        func: Box::new(move |_| Err(Raised(failure("PermissionDenied", &message)))),
    }))
}

/// Replaces every denied native's value with one that raises.
pub fn deny_natives(denied: &HashMap<&str, &str>, values: &mut HashMap<String, Value>) {
    for (name, family) in denied {
        if values.contains_key(*name) {
            values.insert((*name).to_string(), denial(name, family));
        }
    }
}

/// Entry-module uses of a denied native, as errors at the use site. A
/// name the module declares itself (`ADR-028` shadowing) is not the native.
#[must_use]
pub fn denied_uses(module: &Module, denied: &HashMap<&str, &str>) -> Vec<Diagnostic> {
    if denied.is_empty() {
        return Vec::new();
    }
    let own: HashSet<&str> = module
        .bindings
        .iter()
        .flat_map(|g| match g {
            BindingGroup::Let(b) => vec![b.name.as_str()],
            BindingGroup::LetRec(bs) => bs.iter().map(|b| b.name.as_str()).collect(),
        })
        .collect();
    let mut out = Vec::new();
    let mut visit = |e: &Expr| walk(e, denied, &own, &mut out);
    for group in &module.bindings {
        match group {
            BindingGroup::Let(b) => visit(&b.value),
            BindingGroup::LetRec(bs) => bs.iter().for_each(|b| visit(&b.value)),
        }
    }
    out
}

fn walk(e: &Expr, denied: &HashMap<&str, &str>, own: &HashSet<&str>, out: &mut Vec<Diagnostic>) {
    let mut go = |x: &Expr| walk(x, denied, own, out);
    match e {
        Expr::Var(name, span) => {
            if let Some(family) = denied.get(name.as_str())
                && !own.contains(name.as_str())
            {
                out.push(Diagnostic {
                    severity: Severity::Error,
                    message: format!(
                        "'{name}' needs the '{family}' capability; declare it in [capabilities] \
                         of obfusku.toml"
                    ),
                    primary: *span,
                });
            }
        }
        Expr::Lit(..) => {}
        Expr::Lambda { body, .. }
        | Expr::Catch {
            handler_body: body, ..
        } => {
            if let Expr::Catch { body: inner, .. } = e {
                go(inner);
            }
            go(body);
        }
        Expr::Apply { func, arg, .. } => {
            go(func);
            go(arg);
        }
        Expr::MutCell { initial, .. } => go(initial),
        Expr::MutRead { cell, .. } => go(cell),
        Expr::MutRebind {
            cell, new_value, ..
        } => {
            go(cell);
            go(new_value);
        }
        Expr::Constructor { args, .. } => args.iter().for_each(go),
        Expr::Match {
            scrutinee, arms, ..
        } => {
            go(scrutinee);
            for a in arms {
                go(&a.result);
            }
        }
        Expr::Raise { value, .. } => go(value),
        Expr::BinaryOp { lhs, rhs, .. } => {
            go(lhs);
            go(rhs);
        }
        Expr::UnaryOp { operand, .. } => go(operand),
        Expr::Let { value, body, .. } => {
            go(value);
            go(body);
        }
        Expr::LetRec { bindings, body, .. } => {
            for b in bindings {
                go(&b.value);
            }
            go(body);
        }
        Expr::ArrayLiteral { elements, .. } => elements.iter().for_each(go),
        Expr::Index { array, index, .. } => {
            go(array);
            go(index);
        }
        Expr::Field { base, .. } => go(base),
        Expr::Update { base, fields, .. } => {
            go(base);
            for (_, v) in fields {
                go(v);
            }
        }
    }
}
