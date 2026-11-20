//! The program's own process: console lines, arguments, environment, exit.
//!
//! Standard natives of the `⌁` host-effect family (`GLYPH_SYSTEM_DESIGN.md`
//! §10.2, `ADR-010`, `ADR-022`, `ADR-023`). None of them touches the
//! filesystem, so none needs a root (`filesystem.rs` owns that question).
//!
//! - `⌁↑ : ⌘ → ∅` print a line · `⌁↓ : ∅ → ⌘` read a line (EOF raises
//!   `Failure("EOF", …)`) · `⌁↑⍉ : ⌘ → ∅` print a line to stderr
//! - `⌁↓⌥ : ∅ → List<⌘>` program arguments (`run <file> -- <args>`; empty
//!   for the REPL and `check`) · `⌁↓⎈ : ⌘ → Optional<⌘>` environment variable
//! - `⌁↯ : ⟁ → a` terminate with an exit code in `0..=255`
//!
//! `⌁↓` takes `∅` because `Apply` is arity-1: "no argument" is spelled with
//! the unit value.

use super::Natives;
use super::marshal::{
    failure, func, int_arg, invalid, io_fail, list_from, optional, optional_of, str_arg, str_value,
    unit_arg, unit_to,
};
use super::registry::Registry;
use obfusku_runtime::Value;
use obfusku_typecheck::{Type, TypeVarId};
use std::io::{BufRead, Write};
use std::rc::Rc;

/// Writes `text` and a newline. `println!` would panic on a closed pipe
/// (`obfusku run … | head`); this reports it as a catchable `IOError`.
fn emit_line(
    mut out: impl Write,
    op: &'static str,
    text: &str,
) -> Result<Value, obfusku_runtime::eval::Raised> {
    writeln!(out, "{text}").map_err(|e| io_fail(op, &e))?;
    Ok(Value::Unit)
}

/// Reads one line without its terminator (`\n` or `\r\n`).
fn read_line() -> Result<Value, obfusku_runtime::eval::Raised> {
    let mut line = String::new();
    match std::io::stdin().lock().read_line(&mut line) {
        Ok(0) => Err(obfusku_runtime::eval::Raised(failure(
            "EOF",
            "readLine: end of input",
        ))),
        Ok(_) => {
            if line.ends_with('\n') {
                line.pop();
                if line.ends_with('\r') {
                    line.pop();
                }
            }
            Ok(str_value(&line))
        }
        Err(e) => Err(io_fail("readLine", &e)),
    }
}

/// Console, arguments, environment and exit. `args` is what the host
/// passes the program.
#[must_use]
pub fn load(args: &[String]) -> Natives {
    let mut r = Registry::default();
    r.add_mono(
        "⌁↑",
        func(Type::Str, Type::Unit),
        1,
        Rc::new(|a| emit_line(std::io::stdout().lock(), "print", str_arg("⌁↑", &a[0])?)),
    );
    r.add_mono(
        "⌁↓",
        unit_to(Type::Str),
        1,
        Rc::new(|a| {
            unit_arg("⌁↓", &a[0])?;
            read_line()
        }),
    );
    r.add_mono(
        "⌁↑⍉",
        func(Type::Str, Type::Unit),
        1,
        Rc::new(|a| emit_line(std::io::stderr().lock(), "eprint", str_arg("⌁↑⍉", &a[0])?)),
    );
    r.add(
        "⌁↯",
        &[0],
        &[],
        func(Type::Int, Type::Var(TypeVarId(0))),
        1,
        Rc::new(|a| {
            let code = int_arg("⌁↯", &a[0])?;
            if !(0..=255).contains(&code) {
                return Err(invalid(format!(
                    "'⌁↯' expects an Int in 0..=255, found {code}"
                )));
            }
            // `process::exit` skips destructors, so flush explicitly.
            let _ = std::io::stdout().flush();
            std::process::exit(code as i32)
        }),
    );

    let arguments = list_from(args.iter().map(|a| str_value(a)).collect());
    r.add_mono(
        "⌁↓⌥",
        unit_to(super::marshal::list_of(Type::Str)),
        1,
        Rc::new(move |a| {
            unit_arg("⌁↓⌥", &a[0])?;
            Ok(arguments.clone())
        }),
    );
    r.add_mono(
        "⌁↓⎈",
        func(Type::Str, optional_of(Type::Str)),
        1,
        Rc::new(|a| {
            let name = str_arg("⌁↓⎈", &a[0])?;
            Ok(optional(std::env::var(name).ok().map(|v| str_value(&v))))
        }),
    );
    r.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::natives::marshal::vec_from_list;

    fn call(
        natives: &Natives,
        name: &str,
        arg: Value,
    ) -> Result<Value, obfusku_runtime::eval::Raised> {
        match &natives.1[name] {
            Value::Native(n) => (n.func)(arg),
            other => panic!("expected {name} to be a native, got {other:?}"),
        }
    }

    #[test]
    fn arguments_are_returned_in_order() {
        let n = load(&["a".to_string(), "b".to_string()]);
        let list = call(&n, "⌁↓⌥", Value::Unit).unwrap();
        let items = vec_from_list("t", &list).unwrap();
        assert!(matches!(&items[..], [Value::Str(a), Value::Str(b)] if &**a == "a" && &**b == "b"));
    }

    #[test]
    fn no_arguments_is_the_empty_list() {
        let n = load(&[]);
        let list = call(&n, "⌁↓⌥", Value::Unit).unwrap();
        assert!(vec_from_list("t", &list).unwrap().is_empty());
    }

    #[test]
    fn arguments_native_rejects_a_non_unit_argument() {
        assert!(call(&load(&[]), "⌁↓⌥", Value::Int(1)).is_err());
    }

    #[test]
    fn an_unset_environment_variable_is_none() {
        let n = load(&[]);
        let v = call(&n, "⌁↓⎈", str_value("OBFUSKU_SURELY_UNSET_VARIABLE")).unwrap();
        assert!(matches!(&v, Value::Adt { tag, .. } if &**tag == "⦰"));
    }

    #[test]
    fn a_set_environment_variable_is_some() {
        let n = load(&[]);
        let v = call(&n, "⌁↓⎈", str_value("PATH")).unwrap();
        assert!(matches!(&v, Value::Adt { tag, .. } if &**tag == "⧫"));
    }

    #[test]
    fn exit_rejects_codes_outside_a_byte_without_exiting() {
        let n = load(&[]);
        for code in [-1, 256, i64::MAX] {
            assert!(call(&n, "⌁↯", Value::Int(code)).is_err(), "{code}");
        }
        assert!(call(&n, "⌁↯", Value::Str(obfusku_runtime::Text::from("x"))).is_err());
    }

    #[test]
    fn exit_is_polymorphic_in_its_result() {
        let scheme = &load(&[]).0["⌁↯"];
        assert_eq!(scheme.vars.len(), 1);
    }

    #[test]
    fn eprint_rejects_a_non_str_argument() {
        assert!(call(&load(&[]), "⌁↑⍉", Value::Int(1)).is_err());
    }
}
