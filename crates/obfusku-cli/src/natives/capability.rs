//! Capability enforcement (`ADR-029`): which natives a program may call.
//!
//! `obfusku.toml` can declare a `[capabilities]` table; once present,
//! everything not declared is denied. A denied native keeps its **type**,
//! so the program still type-checks and a library that merely *contains* a
//! call to it stays usable. What changes is the use site:
//!
//! - in the entry module, a use is a static error at the offending name
//!   ([`denied_uses`]) — the author learns before anything runs;
//! - anywhere else (imports, dependencies) the value is swapped for one that
//!   raises `Failure("PermissionDenied", …)` ([`deny_natives`]), because a
//!   dependency's source is not the author's to edit.
//!
//! Capabilities are coarse families, not per-native switches, so the
//! manifest stays readable and a new native never silently lands outside
//! every family.

use super::marshal::failure;
use crate::project::manifest::{Capabilities, FsAccess};
use obfusku_core::ast::{Expr, Module};
use obfusku_diagnostics::{Diagnostic, Severity};
use obfusku_runtime::Value;
use obfusku_runtime::eval::Raised;
use obfusku_runtime::value::NativeFn;
use std::collections::{HashMap, HashSet};

/// The natives behind each capability family. Listed by glyph, not derived
/// from the registries, so adding a native is a conscious decision about
/// which family gates it.
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

/// Native name → the family that gates it, for every denied family.
#[must_use]
pub fn denied(caps: &Capabilities) -> HashMap<&'static str, &'static str> {
    let families: [(&'static str, bool, &[&'static str]); 7] = [
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
                out.insert(*name, family);
            }
        }
    }
    out
}

/// The one wording of a denial, shared by the static and runtime paths so
/// both point at the same fix.
fn denial_message(name: &str, family: &str) -> String {
    format!(
        "'{name}' needs the '{family}' capability; declare it in [capabilities] of obfusku.toml"
    )
}

/// A native that raises `PermissionDenied` when called, with the declared fix.
fn denial(name: &str, family: &str) -> Value {
    let message = denial_message(name, family);
    Value::Native(std::rc::Rc::new(NativeFn {
        name: name.to_string(),
        func: Box::new(move |_| Err(Raised(failure("PermissionDenied", &message)))),
    }))
}

/// Replaces every denied native's value with one that raises, so a use that
/// escapes static detection still cannot run.
pub fn deny_natives(denied: &HashMap<&str, &str>, values: &mut HashMap<String, Value>) {
    for (name, family) in denied {
        if values.contains_key(*name) {
            values.insert((*name).to_string(), denial(name, family));
        }
    }
}

/// Entry-module uses of a denied native, as errors at the use site. A name
/// the module declares itself is not the native (`ADR-028` shadowing), so
/// it is skipped rather than reported falsely.
#[must_use]
pub fn denied_uses(module: &Module, denied: &HashMap<&str, &str>) -> Vec<Diagnostic> {
    if denied.is_empty() {
        return Vec::new();
    }
    let own: HashSet<&str> = module.bound_names().collect();
    let mut out = Vec::new();
    for binding in module.all_bindings() {
        binding.value.walk(&mut |e| {
            if let Expr::Var(name, span) = e
                && let Some(family) = denied.get(name.as_str())
                && !own.contains(name.as_str())
            {
                out.push(Diagnostic {
                    severity: Severity::Error,
                    message: denial_message(name, family),
                    primary: *span,
                });
            }
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use obfusku_diagnostics::SourceMap;

    fn caps(f: impl FnOnce(&mut Capabilities)) -> Capabilities {
        let mut c = Capabilities::ALL;
        f(&mut c);
        c
    }

    fn module(source: &str) -> Module {
        let mut map = SourceMap::new();
        let id = map.add_file(source);
        obfusku_syntax::parse_and_desugar(source, id).expect("test source parses")
    }

    #[test]
    fn allowing_everything_denies_nothing() {
        assert!(denied(&Capabilities::ALL).is_empty());
    }

    #[test]
    fn each_denied_family_gates_its_natives() {
        let d = denied(&caps(|c| c.env = false));
        assert_eq!(d.get("⌁↓⎈"), Some(&"env"));
        assert_eq!(d.len(), 1);
        let d = denied(&caps(|c| c.exit = false));
        assert_eq!(d.get("⌁↯"), Some(&"exit"));
        let d = denied(&caps(|c| c.fs = FsAccess::None));
        assert!(d.keys().all(|k| FS.contains(k)) && d.len() == FS.len());
    }

    #[test]
    fn project_and_cwd_filesystem_access_both_allow_the_family() {
        for fs in [FsAccess::Project, FsAccess::Cwd] {
            assert!(denied(&caps(|c| c.fs = fs)).is_empty());
        }
    }

    #[test]
    fn families_do_not_overlap() {
        let all: Vec<&str> = [CONSOLE, FS, ENV, ARGS, CLOCK, RANDOM, EXIT].concat();
        let unique: HashSet<&&str> = all.iter().collect();
        assert_eq!(
            all.len(),
            unique.len(),
            "a native must belong to one family"
        );
    }

    #[test]
    fn a_denied_native_is_replaced_by_one_that_raises_permission_denied() {
        let denied = HashMap::from([("⌁↓⎈", "env")]);
        let mut values = HashMap::from([
            ("⌁↓⎈".to_string(), Value::Unit),
            ("other".to_string(), Value::Unit),
        ]);
        deny_natives(&denied, &mut values);
        let Value::Native(n) = &values["⌁↓⎈"] else {
            panic!("expected a native")
        };
        let err = (n.func)(Value::Unit).unwrap_err();
        let text = format!("{err:?}");
        assert!(
            text.contains("PermissionDenied") && text.contains("'env' capability"),
            "{text}"
        );
        assert!(matches!(values["other"], Value::Unit));
    }

    #[test]
    fn denying_a_native_the_program_does_not_have_adds_nothing() {
        let mut values = HashMap::new();
        deny_natives(&HashMap::from([("⌁↯", "exit")]), &mut values);
        assert!(values.is_empty());
    }

    #[test]
    fn a_use_of_a_denied_native_is_an_error_at_the_use() {
        let m = module("e ≔ ⌁↓⎈(\"HOME\")\n❧\n");
        let found = denied_uses(&m, &HashMap::from([("⌁↓⎈", "env")]));
        assert_eq!(found.len(), 1);
        assert!(found[0].message.contains("needs the 'env' capability"));
    }

    #[test]
    fn a_module_that_declares_the_name_itself_is_not_flagged() {
        let m = module("λ⌁↓⎈(n: ⌘): ⌘ → n\nr ≔ ⌁↓⎈(\"x\")\n❧\n");
        assert!(denied_uses(&m, &HashMap::from([("⌁↓⎈", "env")])).is_empty());
    }

    #[test]
    fn no_denied_natives_means_no_walk() {
        let m = module("r ≔ 1\n❧\n");
        assert!(denied_uses(&m, &HashMap::new()).is_empty());
    }
}
