//! Generated structural operations over any value (`ADR-025`): total
//! ordering and canonical rendering. Both are language-defined, never
//! user-supplied — the same stance as generated equality
//! (`SEMANTIC_CORE.md` §18). The type checker guarantees they are only
//! applied to types without function-typed parts.

use crate::types;
use crate::value::Value;
use std::cmp::Ordering;
use std::fmt::Write as _;

/// Total order, consistent with structural equality (`SEMANTIC_CORE.md`
/// §18): `NaN` equals itself and sorts above every other `Real`, and
/// `-0.0` equals `0.0`, so sorting and map keys are deterministic.
#[must_use]
pub fn cmp_total(a: &Value, b: &Value) -> Ordering {
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => x.cmp(y),
        (Value::Real(x), Value::Real(y)) => match (x.is_nan(), y.is_nan()) {
            (true, true) => Ordering::Equal,
            (true, false) => Ordering::Greater,
            (false, true) => Ordering::Less,
            (false, false) => x.partial_cmp(y).unwrap_or(Ordering::Equal),
        },
        (Value::Str(x), Value::Str(y)) => x.as_bytes().cmp(y.as_bytes()),
        (Value::Bool(x), Value::Bool(y)) => x.cmp(y),
        (Value::Unit, Value::Unit) => Ordering::Equal,
        (Value::Adt { tag: ta, args: aa }, Value::Adt { tag: tb, args: ab }) => {
            // The same constructor (the common case when sorting) needs
            // no declaration-order lookup.
            if ta == tb {
                return cmp_seq(aa.iter(), ab.iter());
            }
            types::ordinal(ta)
                .cmp(&types::ordinal(tb))
                .then_with(|| ta.cmp(tb))
        }
        (Value::Array(x), Value::Array(y)) => cmp_seq(x.to_vec().iter(), y.to_vec().iter()),
        (Value::Map(x), Value::Map(y)) => {
            let (ex, ey) = (x.entries(), y.entries());
            cmp_seq(
                ex.iter().flat_map(|(k, v)| [k, v]),
                ey.iter().flat_map(|(k, v)| [k, v]),
            )
        }
        (Value::Set(x), Value::Set(y)) => {
            let (ex, ey) = (x.entries(), y.entries());
            cmp_seq(ex.iter().map(|(k, _)| k), ey.iter().map(|(k, _)| k))
        }
        (Value::Bytes(x), Value::Bytes(y)) => x.cmp(y),
        // Unreachable through well-typed source (no order for functions
        // or cells); defined only so the function is total.
        _ => Ordering::Equal,
    }
}

fn cmp_seq<'a>(
    mut a: impl Iterator<Item = &'a Value>,
    mut b: impl Iterator<Item = &'a Value>,
) -> Ordering {
    loop {
        match (a.next(), b.next()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) => match cmp_total(x, y) {
                Ordering::Equal => {}
                other => return other,
            },
        }
    }
}

/// The ordering behind `<`, `>`, `≤`, `≥`: IEEE-754 for a bare `Real`
/// (`NaN` is unordered, so every comparison is `false`), otherwise
/// [`cmp_total`].
#[must_use]
pub fn cmp_operator(a: &Value, b: &Value) -> Option<Ordering> {
    match (a, b) {
        (Value::Real(x), Value::Real(y)) => x.partial_cmp(y),
        _ => Some(cmp_total(a, b)),
    }
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

/// Canonical source-like rendering. Lists print as nested `⁝`
/// constructors (iteratively, so a long list cannot overflow the stack).
#[must_use]
pub fn show(v: &Value) -> String {
    let mut out = String::new();
    show_into(v, &mut out);
    out
}

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
            let entries = s.entries();
            show_seq(entries.iter().map(|(k, _)| k), out);
            out.push(']');
        }
        Value::Bytes(b) => {
            out.push_str("⌭\"");
            for byte in b.iter() {
                let _ = write!(out, "{byte:02x}");
            }
            out.push('"');
        }
        Value::Adt { tag, args } => {
            if &**tag == "⁝" && args.len() == 2 {
                let mut depth = 0;
                let mut cur = v.clone();
                while let Value::Adt { tag, args } = &cur {
                    if &**tag != "⁝" || args.len() != 2 {
                        break;
                    }
                    out.push_str("⁝(");
                    show_into(&args[0], out);
                    out.push_str(", ");
                    depth += 1;
                    let next = args[1].clone();
                    cur = next;
                }
                show_into(&cur, out);
                out.push_str(&")".repeat(depth));
                return;
            }
            out.push_str(tag);
            if args.is_empty() {
                return;
            }
            let names = types::variant(tag).and_then(|m| m.field_names);
            match names {
                Some(names) if names.len() == args.len() => {
                    out.push_str(" { ");
                    for (i, (n, a)) in names.iter().zip(args.iter()).enumerate() {
                        if i > 0 {
                            out.push_str(" ⟢ ");
                        }
                        out.push_str(n);
                        out.push_str(": ");
                        show_into(a, out);
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
    }
}
