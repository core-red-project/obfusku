//! Canonical rendering of a value as source-like text.

use crate::value::Value;
use crate::variants;
use std::fmt::Write as _;

/// Renders `v` the way a program would write it.
///
/// Used for `show`, for the REPL, and for the "uncaught exception: …"
/// message, so all three agree. Lists print as nested `⁝` constructors, built
/// iteratively so a long list cannot overflow the stack.
#[must_use]
pub fn show(v: &Value) -> String {
    let mut out = String::new();
    show_into(v, &mut out);
    out
}

fn escape(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            '\0' => out.push_str("\\0"),
            c => out.push(c),
        }
    }
    out.push('"');
}

/// Comma-separated values.
fn show_seq<'a>(items: impl Iterator<Item = &'a Value>, out: &mut String) {
    for (i, x) in items.enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        show_into(x, out);
    }
}

fn show_into(v: &Value, out: &mut String) {
    match v {
        Value::Int(i) => out.push_str(&i.to_string()),
        Value::Real(r) if r.is_nan() => out.push_str("NaN"),
        Value::Real(r) if r.is_infinite() => {
            out.push_str(if *r > 0.0 { "∞" } else { "−∞" });
        }
        Value::Real(r) => drop(write!(out, "{r:?}")),
        Value::Str(s) => escape(s, out),
        Value::Bool(true) => out.push('◉'),
        Value::Bool(false) => out.push('◎'),
        Value::Unit => out.push('∅'),
        Value::Closure(_) => out.push_str("<function>"),
        Value::Native(f) => drop(write!(out, "<native {}>", f.name)),
        Value::Cell(c) => {
            out.push_str("˚(");
            show_into(&c.borrow(), out);
            out.push(')');
        }
        Value::Array(a) => {
            out.push('[');
            show_seq(a.to_vec().iter(), out);
            out.push(']');
        }
        Value::Map(m) => {
            out.push_str("Map[");
            for (i, (k, val)) in m.entries().iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                show_into(k, out);
                out.push_str(" ↦ ");
                show_into(val, out);
            }
            out.push(']');
        }
        Value::Set(s) => {
            out.push_str("Set[");
            show_seq(s.entries().iter().map(|(k, _)| k), out);
            out.push(']');
        }
        Value::Bytes(b) => {
            out.push_str("⌭\"");
            for byte in b.iter() {
                let _ = write!(out, "{byte:02x}");
            }
            out.push('"');
        }
        Value::Adt { tag, args } => show_adt(v, tag, args, out),
    }
}

/// The tag of the list constructor.
const CONS: &str = "⁝";

fn is_cons(tag: &str, args: &[Value]) -> bool {
    tag == CONS && args.len() == 2
}

/// A constructor application: a list spine, a record, or a plain `Tag(args)`.
fn show_adt(v: &Value, tag: &str, args: &[Value], out: &mut String) {
    if is_cons(tag, args) {
        show_list(v, out);
        return;
    }
    out.push_str(tag);
    if args.is_empty() {
        return;
    }
    match variants::variant(tag).and_then(|m| m.field_names) {
        Some(names) if names.len() == args.len() => {
            out.push_str(" { ");
            for (i, (name, arg)) in names.iter().zip(args).enumerate() {
                if i > 0 {
                    out.push_str(" ⟢ ");
                }
                out.push_str(name);
                out.push_str(": ");
                show_into(arg, out);
            }
            out.push_str(" }");
        }
        _ => {
            out.push('(');
            show_seq(args.iter(), out);
            out.push(')');
        }
    }
}

/// `⁝(a, ⁝(b, ∅))`, walking the spine in a loop instead of recursing so the
/// depth is the list's length only in the closing parentheses.
fn show_list(list: &Value, out: &mut String) {
    let mut depth = 0;
    let mut current = list.clone();
    while let Value::Adt { tag, args } = &current {
        if !is_cons(tag, args) {
            break;
        }
        out.push_str("⁝(");
        show_into(&args[0], out);
        out.push_str(", ");
        depth += 1;
        let next = args[1].clone();
        current = next;
    }
    show_into(&current, out);
    out.push_str(&")".repeat(depth));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistent::{PMap, PVec, Text};
    use std::rc::Rc;

    fn adt(tag: &str, args: Vec<Value>) -> Value {
        Value::Adt {
            tag: Rc::from(tag),
            args: Rc::from(args),
        }
    }

    #[test]
    fn scalars_render_with_the_languages_glyphs() {
        assert_eq!(show(&Value::Int(-3)), "-3");
        assert_eq!(show(&Value::Bool(true)), "◉");
        assert_eq!(show(&Value::Bool(false)), "◎");
        assert_eq!(show(&Value::Unit), "∅");
    }

    #[test]
    fn reals_render_with_a_decimal_point_and_special_values_by_name() {
        assert_eq!(show(&Value::Real(2.0)), "2.0");
        assert_eq!(show(&Value::Real(f64::NAN)), "NaN");
        assert_eq!(show(&Value::Real(f64::INFINITY)), "∞");
        assert_eq!(show(&Value::Real(f64::NEG_INFINITY)), "−∞");
    }

    #[test]
    fn strings_are_quoted_and_escaped() {
        let s = Value::Str(Text::from("a\"b\\c\nd"));
        assert_eq!(show(&s), "\"a\\\"b\\\\c\\nd\"");
    }

    #[test]
    fn arrays_maps_sets_and_bytes_render_their_contents() {
        let arr = Value::Array(PVec::from_vec(vec![Value::Int(1), Value::Int(2)]));
        assert_eq!(show(&arr), "[1, 2]");
        let map = Value::Map(PMap::new().insert(Value::Int(1), Value::Int(2)));
        assert_eq!(show(&map), "Map[1 ↦ 2]");
        let set = Value::Set(PMap::new().insert(Value::Int(1), Value::Unit));
        assert_eq!(show(&set), "Set[1]");
        assert_eq!(
            show(&Value::Bytes(Rc::from([0xab_u8, 0x01].as_slice()))),
            "⌭\"ab01\""
        );
    }

    #[test]
    fn a_cell_shows_its_contents() {
        let cell = Value::Cell(Rc::new(std::cell::RefCell::new(Value::Int(5))));
        assert_eq!(show(&cell), "˚(5)");
    }

    #[test]
    fn constructors_render_positionally() {
        assert_eq!(show(&adt("None", vec![])), "None");
        assert_eq!(
            show(&adt("Pair", vec![Value::Int(1), Value::Int(2)])),
            "Pair(1, 2)"
        );
    }

    #[test]
    fn lists_render_as_nested_cons_and_survive_great_length() {
        let list = adt(
            "⁝",
            vec![Value::Int(1), adt("⁝", vec![Value::Int(2), Value::Unit])],
        );
        assert_eq!(show(&list), "⁝(1, ⁝(2, ∅))");

        let mut long = Value::Unit;
        for i in 0..50_000 {
            long = adt("⁝", vec![Value::Int(i), long]);
        }
        assert!(show(&long).ends_with(&")".repeat(50_000)));
    }

    #[test]
    fn records_render_with_their_field_names() {
        crate::variants::register(&[obfusku_core::ast::TypeDecl {
            tag: "ShowPoint".into(),
            type_params: vec![],
            variants: vec![obfusku_core::ast::VariantDecl {
                tag: "ShowPoint".into(),
                fields: vec![],
                field_names: Some(vec!["x".into(), "y".into()]),
                span: obfusku_diagnostics::Span::default(),
            }],
            span: obfusku_diagnostics::Span::default(),
        }]);
        assert_eq!(
            show(&adt("ShowPoint", vec![Value::Int(1), Value::Int(2)])),
            "ShowPoint { x: 1 ⟢ y: 2 }"
        );
    }

    #[test]
    fn functions_render_without_internals() {
        let native = Value::Native(Rc::new(crate::value::NativeFn {
            name: "f".into(),
            func: Box::new(Ok),
        }));
        assert_eq!(show(&native), "<native f>");
    }
}
