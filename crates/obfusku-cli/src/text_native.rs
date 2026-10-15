//! `Str` combinators and text↔number conversion (`ADR-023`) — pure
//! host natives, for the same reason `Array`'s are (`ADR-021`): `Str` has
//! no structural pattern form, so none of this can be written in
//! Obfusku. All are curried and total except where noted.
//!
//! - `⫽ : Str → Str → List<Str>` — split on a non-empty separator
//! - `⫴ : Str → List<Str> → Str` — join with a separator
//! - `⌫ : Str → Str` — trim surrounding whitespace
//! - `∋ : Str → Str → Bool` — needle, haystack
//! - `⍴ : Str → Int` — length in characters
//! - `⊏ : Int → Int → Str → Str` — start, length, text; characters,
//!   clamped to the text (a negative start or length raises)
//! - `↦ : Str → Result<Int, Str>`, `⇝ : Str → Result<Real, Str>` — strict
//!   parse, no surrounding whitespace, `Real` must be finite
//! - `⇧ ⇩ : Str → Str` case · `⊢ ⊣ : Str → Str → Bool` starts/ends with ·
//!   `⍸ : Str → Str → Optional<Int>` first index · `⤳ : Str → Str → Str → Str`
//!   replace all (needle, replacement, text) · `⌯ : Str → Int` / `⌮ : Int → Str`
//!   code point of the first scalar / scalar of a code point · `⍂`/`⌶ : Str →
//!   List<Str>` scalars / grapheme clusters (`ADR-027`)
//! - `⍕ : t → Str` — canonical rendering of any value without functions
//! - `⚑ : List<Str> → List<⌑>` — classify command-line arguments (`⌑`
//!   is declared in `stdlib.obk`): `--k=v` → `➤(k, ⧫(v))`, `--k` →
//!   `➤(k, ⦰)`, `-abc` → one `➤` per letter, `-` and everything after a
//!   bare `--` → `◇(word)`. A value given as a separate word is left as
//!   the next `◇`; the program decides.

use crate::native_util::{
    Impl, Registry, adt, func, int_arg, invalid, list_from, list_of, optional, optional_of,
    result_of, s, str_arg, vec_from_list,
};
use obfusku_runtime::eval::Raised;
use obfusku_runtime::{Value, show};
use obfusku_typecheck::{Constraint, Prelude, Type, TypeVarId};
use std::collections::HashMap;
use std::rc::Rc;

fn list_str() -> Type {
    list_of(Type::Str)
}

fn strings_of_list(op: &str, v: &Value) -> Result<Vec<String>, Raised> {
    vec_from_list(op, v)?
        .iter()
        .map(|x| str_arg(op, x).map(str::to_string))
        .collect()
}

fn list_of_strings(items: &[String]) -> Value {
    list_from(items.iter().map(|x| s(x)).collect())
}

#[must_use]
pub fn load() -> (Prelude, HashMap<String, Value>) {
    let mut r = Registry::default();
    let mut add = |name: &'static str, ty: Type, arity: usize, f: Impl| {
        r.add(name, &[], &[], ty, arity, f);
    };

    add(
        "⫽",
        func(Type::Str, func(Type::Str, list_str())),
        2,
        Rc::new(|a| {
            let sep = str_arg("⫽", &a[0])?;
            if sep.is_empty() {
                return Err(invalid("'⫽' expects a non-empty separator"));
            }
            let text = str_arg("⫽", &a[1])?;
            Ok(list_of_strings(
                &text.split(sep).map(str::to_string).collect::<Vec<_>>(),
            ))
        }),
    );
    add(
        "⫴",
        func(Type::Str, func(list_str(), Type::Str)),
        2,
        Rc::new(|a| {
            let sep = str_arg("⫴", &a[0])?;
            Ok(s(&strings_of_list("⫴", &a[1])?.join(sep)))
        }),
    );
    add(
        "⌫",
        func(Type::Str, Type::Str),
        1,
        Rc::new(|a| Ok(s(str_arg("⌫", &a[0])?.trim()))),
    );
    add(
        "∋",
        func(Type::Str, func(Type::Str, Type::Bool)),
        2,
        Rc::new(|a| {
            Ok(Value::Bool(
                str_arg("∋", &a[1])?.contains(str_arg("∋", &a[0])?),
            ))
        }),
    );
    add(
        "⍴",
        func(Type::Str, Type::Int),
        1,
        Rc::new(|a| Ok(Value::Int(str_arg("⍴", &a[0])?.chars().count() as i64))),
    );
    add(
        "⊏",
        func(Type::Int, func(Type::Int, func(Type::Str, Type::Str))),
        3,
        Rc::new(|a| {
            let start = int_arg("⊏", &a[0])?;
            let len = int_arg("⊏", &a[1])?;
            if start < 0 || len < 0 {
                return Err(invalid(format!(
                    "'⊏' expects non-negative start and length, found {start}, {len}"
                )));
            }
            let text = str_arg("⊏", &a[2])?;
            let piece: String = text
                .chars()
                .skip(start as usize)
                .take(len as usize)
                .collect();
            Ok(s(&piece))
        }),
    );
    add(
        "↦",
        func(Type::Str, result_of(Type::Int, Type::Str)),
        1,
        Rc::new(|a| {
            let text = str_arg("↦", &a[0])?;
            Ok(match text.parse::<i64>() {
                Ok(n) => adt("✓", vec![Value::Int(n)]),
                Err(e) => adt("✗", vec![s(&format!("not an Int: {text:?} ({e})"))]),
            })
        }),
    );
    add(
        "⇝",
        func(Type::Str, result_of(Type::Real, Type::Str)),
        1,
        Rc::new(|a| {
            let text = str_arg("⇝", &a[0])?;
            Ok(match text.parse::<f64>() {
                Ok(r) if r.is_finite() => adt("✓", vec![Value::Real(r)]),
                _ => adt("✗", vec![s(&format!("not a finite Real: {text:?}"))]),
            })
        }),
    );
    add(
        "⚑",
        func(
            list_str(),
            Type::Adt("List".to_string(), vec![Type::Adt("⌑".to_string(), vec![])]),
        ),
        1,
        Rc::new(|a| {
            let mut out = Vec::new();
            let mut only_positional = false;
            for word in strings_of_list("⚑", &a[0])? {
                out.push(classify(&word, &mut only_positional));
            }
            Ok(out
                .into_iter()
                .flatten()
                .rev()
                .fold(adt("⊘", vec![]), |tail, arg| adt("⁝", vec![arg, tail])))
        }),
    );

    // Case, search, replacement and code points (`ADR-027`). Positions
    // and lengths count Unicode scalar values, as `⍴` and `⊏` do;
    // `⌶` is the grapheme-cluster view.
    add(
        "⇧",
        func(Type::Str, Type::Str),
        1,
        Rc::new(|a| Ok(s(&str_arg("⇧", &a[0])?.to_uppercase()))),
    );
    add(
        "⇩",
        func(Type::Str, Type::Str),
        1,
        Rc::new(|a| Ok(s(&str_arg("⇩", &a[0])?.to_lowercase()))),
    );
    add(
        "⊢",
        func(Type::Str, func(Type::Str, Type::Bool)),
        2,
        Rc::new(|a| {
            Ok(Value::Bool(
                str_arg("⊢", &a[1])?.starts_with(str_arg("⊢", &a[0])?),
            ))
        }),
    );
    add(
        "⊣",
        func(Type::Str, func(Type::Str, Type::Bool)),
        2,
        Rc::new(|a| {
            Ok(Value::Bool(
                str_arg("⊣", &a[1])?.ends_with(str_arg("⊣", &a[0])?),
            ))
        }),
    );
    add(
        "⍸",
        func(Type::Str, func(Type::Str, optional_of(Type::Int))),
        2,
        Rc::new(|a| {
            let needle = str_arg("⍸", &a[0])?;
            let text = str_arg("⍸", &a[1])?;
            Ok(optional(text.find(needle).map(|byte| {
                Value::Int(text[..byte].chars().count() as i64)
            })))
        }),
    );
    add(
        "⤳",
        func(Type::Str, func(Type::Str, func(Type::Str, Type::Str))),
        3,
        Rc::new(|a| {
            let needle = str_arg("⤳", &a[0])?;
            if needle.is_empty() {
                return Err(invalid("'⤳' expects a non-empty needle"));
            }
            Ok(s(
                &str_arg("⤳", &a[2])?.replace(needle, str_arg("⤳", &a[1])?)
            ))
        }),
    );
    add(
        "⌯",
        func(Type::Str, Type::Int),
        1,
        Rc::new(|a| match str_arg("⌯", &a[0])?.chars().next() {
            Some(c) => Ok(Value::Int(i64::from(u32::from(c)))),
            None => Err(invalid("'⌯' expects a non-empty Str")),
        }),
    );
    add(
        "⌮",
        func(Type::Int, Type::Str),
        1,
        Rc::new(|a| {
            let n = int_arg("⌮", &a[0])?;
            u32::try_from(n)
                .ok()
                .and_then(char::from_u32)
                .map(|c| s(&c.to_string()))
                .ok_or_else(|| invalid(format!("'⌮' expects a Unicode scalar value, found {n}")))
        }),
    );
    add(
        "⍂",
        func(Type::Str, list_str()),
        1,
        Rc::new(|a| {
            Ok(list_from(
                str_arg("⍂", &a[0])?
                    .chars()
                    .map(|c| s(&c.to_string()))
                    .collect(),
            ))
        }),
    );
    add(
        "⌶",
        func(Type::Str, list_str()),
        1,
        Rc::new(|a| {
            use unicode_segmentation::UnicodeSegmentation;
            Ok(list_from(
                str_arg("⌶", &a[0])?.graphemes(true).map(s).collect(),
            ))
        }),
    );

    // `⍕ : t → Str` for any `t` with a generated rendering (`ADR-025`).
    r.add(
        "⍕",
        &[0],
        &[(0, Constraint::Show)],
        func(Type::Var(TypeVarId(0)), Type::Str),
        1,
        Rc::new(|a| Ok(s(&show(&a[0])))),
    );

    r.finish()
}

fn flag(name: &str, value: Option<&str>) -> Value {
    let value = match value {
        Some(v) => adt("⧫", vec![s(v)]),
        None => adt("⦰", vec![]),
    };
    adt("➤", vec![s(name), value])
}

/// One argument word → zero or more `⌑` values.
fn classify(word: &str, only_positional: &mut bool) -> Vec<Value> {
    let positional = || vec![adt("◇", vec![s(word)])];
    if *only_positional {
        return positional();
    }
    if word == "--" {
        *only_positional = true;
        return Vec::new();
    }
    if let Some(rest) = word.strip_prefix("--") {
        return match rest.split_once('=') {
            Some(("", _)) => positional(),
            Some((k, v)) => vec![flag(k, Some(v))],
            None => vec![flag(rest, None)],
        };
    }
    match word.strip_prefix('-') {
        Some(letters) if !letters.is_empty() => letters
            .chars()
            .map(|c| flag(&c.to_string(), None))
            .collect(),
        _ => positional(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(name: &str, args: Vec<Value>) -> Result<Value, Raised> {
        let (_, values) = load();
        let mut cur = values.get(name).unwrap().clone();
        for a in args {
            cur = match &cur {
                Value::Native(f) => (f.func)(a)?,
                other => panic!("expected a native, got {other:?}"),
            };
        }
        Ok(cur)
    }

    fn list(items: &[&str]) -> Value {
        list_of_strings(
            &items
                .iter()
                .map(std::string::ToString::to_string)
                .collect::<Vec<_>>(),
        )
    }

    fn render(v: &Value) -> Vec<String> {
        strings_of_list("test", v).unwrap()
    }

    #[test]
    fn split_keeps_empty_fields_and_rejects_an_empty_separator() {
        let v = call("⫽", vec![s(","), s("a,,b")]).unwrap();
        assert_eq!(render(&v), ["a", "", "b"]);
        assert!(call("⫽", vec![s(""), s("abc")]).is_err());
    }

    #[test]
    fn join_inverts_split() {
        let v = call("⫴", vec![s("-"), list(&["a", "b", "c"])]).unwrap();
        assert!(matches!(v, Value::Str(ref t) if &**t == "a-b-c"));
    }

    #[test]
    fn length_and_slice_count_characters_not_bytes() {
        assert!(matches!(call("⍴", vec![s("añb")]), Ok(Value::Int(3))));
        let v = call("⊏", vec![Value::Int(1), Value::Int(1), s("añb")]).unwrap();
        assert!(matches!(v, Value::Str(ref t) if &**t == "ñ"));
    }

    #[test]
    fn slice_clamps_and_rejects_negatives() {
        let v = call("⊏", vec![Value::Int(2), Value::Int(99), s("abc")]).unwrap();
        assert!(matches!(v, Value::Str(ref t) if &**t == "c"));
        assert!(call("⊏", vec![Value::Int(-1), Value::Int(1), s("abc")]).is_err());
    }

    #[test]
    fn parse_int_is_strict() {
        for bad in [" 5", "5 ", "", "1.5", "9223372036854775808"] {
            let v = call("↦", vec![s(bad)]).unwrap();
            assert!(
                matches!(v, Value::Adt { ref tag, .. } if &**tag == "✗"),
                "{bad:?}"
            );
        }
        let v = call("↦", vec![s("-42")]).unwrap();
        assert!(
            matches!(v, Value::Adt { ref tag, ref args } if &**tag == "✓"
            && matches!(args[0], Value::Int(-42)))
        );
    }

    #[test]
    fn parse_real_rejects_non_finite() {
        for bad in ["nan", "inf", "-inf", "abc"] {
            let v = call("⇝", vec![s(bad)]).unwrap();
            assert!(
                matches!(v, Value::Adt { ref tag, .. } if &**tag == "✗"),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn render_numbers() {
        assert!(matches!(call("⍕", vec![Value::Int(-7)]), Ok(Value::Str(ref t)) if &**t == "-7"));
        assert!(
            matches!(call("⍕", vec![Value::Real(1.0)]), Ok(Value::Str(ref t)) if &**t == "1.0")
        );
        assert!(
            matches!(call("⍕", vec![s("a\"b")]), Ok(Value::Str(ref t)) if &**t == "\"a\\\"b\"")
        );
    }

    fn arg_summary(v: &Value) -> Vec<String> {
        let mut out = Vec::new();
        let mut cur = v.clone();
        while let Value::Adt { tag, args } = &cur {
            if &**tag != "⁝" {
                break;
            }
            let Value::Adt { tag, args: a } = &args[0] else {
                panic!("expected an argument value");
            };
            out.push(match (&**tag, &a[..]) {
                ("➤", [Value::Str(k), Value::Adt { tag: t, args: v }]) if &**t == "⧫" => {
                    let Value::Str(v) = &v[0] else { panic!() };
                    format!("{k}={v}")
                }
                ("➤", [Value::Str(k), _]) => format!("{k}"),
                ("◇", [Value::Str(p)]) => format!("_{p}"),
                other => panic!("unexpected {other:?}"),
            });
            let next = args[1].clone();
            cur = next;
        }
        out
    }

    #[test]
    fn flags_are_classified() {
        let v = call(
            "⚑",
            vec![list(&[
                "--depth=10",
                "--verbose",
                "-ab",
                "file",
                "-",
                "--",
                "--x",
                "-y",
            ])],
        )
        .unwrap();
        assert_eq!(
            arg_summary(&v),
            [
                "depth=10", "verbose", "a", "b", "_file", "_-", "_--x", "_-y"
            ]
        );
    }

    #[test]
    fn empty_flag_name_is_positional() {
        let v = call("⚑", vec![list(&["--=x"])]).unwrap();
        assert_eq!(arg_summary(&v), ["_--=x"]);
    }

    #[test]
    fn mistyped_call_raises_instead_of_panicking() {
        assert!(call("⌫", vec![Value::Int(1)]).is_err());
        assert!(call("⫴", vec![s(","), Value::Int(1)]).is_err());
        assert!(call("⚑", vec![Value::Int(1)]).is_err());
    }
}
