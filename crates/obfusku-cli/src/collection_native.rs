//! `Array`, `Map` and `Set` natives (`ADR-021`, `ADR-025`) — host-provided
//! because these types have no pattern form (`SEMANTIC_CORE.md` §9.1) and
//! call back into user functions via [`obfusku_runtime::eval::apply_value`].
//! All are curried and persistent: every update returns a new value and
//! leaves its argument untouched. Keys order by the generated total order
//! (`obfusku_runtime::ops::cmp_total`), so `Map`/`Set` need `Ord` keys.
//!
//! List — `⟐`/`⌿`/`⌽` map / filter / fold, over `List<t>` (`ADR-028`).
//!
//! Array — `GLYPH_SYSTEM_DESIGN.md` §10.5, §10.7:
//! - `# : Array<t> → Int` length · `⊙ : Array<t> → Int → t → Array<t>` set
//! - `⊡`/`⊟`/`⊞` map / filter / fold
//! - `⊕ : Array<t> → t → Array<t>` push · `⍪ : Array<t> → Array<t> → Array<t>` concat
//! - `⌷ : Int → Int → Array<t> → Array<t>` slice (start, length; clamped)
//! - `⍳ : Int → Array<Int>` range `0..n` · `⊠ : Int → t → Array<t>` replicate
//! - `⍋ : Array<t> → Array<t>` stable ascending sort (`Ord t`)
//! - `⇋ : List<t> → Array<t>` · `⇌ : Array<t> → List<t>`
//!
//! Map — `⍬` empty · `⊎` insert · `⊖` remove · `⌕` lookup (`Optional`) ·
//! `⊛` size · `⌸` keys · `⌹` values.
//! Set — `⍭` empty · `⊌` insert · `⊝` remove · `∈` member · `⊚` size ·
//! `⌻` elements.

use crate::native_util::{
    Registry, array_of, count_arg, func, int_arg, invalid, list_from, list_of, map_of, optional,
    optional_of, set_of, vec_from_list,
};
use obfusku_runtime::Value;
use obfusku_runtime::eval::{Raised, apply_value};
use obfusku_runtime::ops::cmp_total;
use obfusku_runtime::pmap::PMap;
use obfusku_runtime::pvec::PVec;
use obfusku_typecheck::{Constraint, Prelude, Type, TypeVarId};
use std::collections::HashMap;
use std::rc::Rc;

fn var(id: u32) -> Type {
    Type::Var(TypeVarId(id))
}

fn array_arg(op: &str, v: &Value) -> Result<PVec, Raised> {
    match v {
        Value::Array(a) => Ok(a.clone()),
        other => Err(invalid(format!("'{op}' expects an Array, found {other:?}"))),
    }
}

fn map_arg(op: &str, v: &Value) -> Result<PMap, Raised> {
    match v {
        Value::Map(m) => Ok(m.clone()),
        other => Err(invalid(format!("'{op}' expects a Map, found {other:?}"))),
    }
}

fn set_arg(op: &str, v: &Value) -> Result<PMap, Raised> {
    match v {
        Value::Set(m) => Ok(m.clone()),
        other => Err(invalid(format!("'{op}' expects a Set, found {other:?}"))),
    }
}

fn non_negative(op: &str, what: &str, n: i64) -> Result<usize, Raised> {
    usize::try_from(n)
        .map_err(|_| invalid(format!("'{op}' expects a non-negative {what}, found {n}")))
}

#[must_use]
pub fn load() -> (Prelude, HashMap<String, Value>) {
    let mut r = Registry::default();
    let ord = Constraint::Ord;
    let (t, u, k, v) = (var(0), var(1), var(0), var(1));

    // ── List combinators ───────────────────────────────────────────────
    // Natives rather than recursive source: they run in constant stack
    // over any length (`ADR-028`).
    let list_t = list_of(t.clone());
    r.add(
        "⟐",
        &[0, 1],
        &[],
        func(
            func(t.clone(), u.clone()),
            func(list_t.clone(), list_of(u.clone())),
        ),
        2,
        Rc::new(|a| {
            let mut out = Vec::new();
            for x in vec_from_list("⟐", &a[1])? {
                out.push(apply_value(&a[0], x)?);
            }
            Ok(list_from(out))
        }),
    );
    r.add(
        "⌿",
        &[0],
        &[],
        func(
            func(t.clone(), Type::Bool),
            func(list_t.clone(), list_t.clone()),
        ),
        2,
        Rc::new(|a| {
            let mut out = Vec::new();
            for x in vec_from_list("⌿", &a[1])? {
                match apply_value(&a[0], x.clone())? {
                    Value::Bool(true) => out.push(x),
                    Value::Bool(false) => {}
                    other => {
                        return Err(invalid(format!(
                            "'⌿' expects a Bool-returning predicate, found {other:?}"
                        )));
                    }
                }
            }
            Ok(list_from(out))
        }),
    );
    r.add(
        "⌽",
        &[0, 1],
        &[],
        func(
            func(u.clone(), func(t.clone(), u.clone())),
            func(u.clone(), func(list_t, u.clone())),
        ),
        3,
        Rc::new(|a| {
            let mut acc = a[1].clone();
            for x in vec_from_list("⌽", &a[2])? {
                let partial = apply_value(&a[0], acc)?;
                acc = apply_value(&partial, x)?;
            }
            Ok(acc)
        }),
    );

    // ── Array ──────────────────────────────────────────────────────────
    r.add(
        "#",
        &[0],
        &[],
        func(array_of(t.clone()), Type::Int),
        1,
        Rc::new(|a| Ok(Value::Int(array_arg("#", &a[0])?.len() as i64))),
    );
    r.add(
        "⊙",
        &[0],
        &[],
        func(
            array_of(t.clone()),
            func(Type::Int, func(t.clone(), array_of(t.clone()))),
        ),
        3,
        Rc::new(|a| {
            let arr = array_arg("⊙", &a[0])?;
            let index = int_arg("⊙", &a[1])?;
            let len = arr.len();
            usize::try_from(index)
                .ok()
                .and_then(|i| arr.set(i, a[2].clone()))
                .map(Value::Array)
                .ok_or_else(|| {
                    invalid(format!(
                        "'⊙' index out of bounds: index {index}, length {len}"
                    ))
                })
        }),
    );
    r.add(
        "⊡",
        &[0, 1],
        &[],
        func(
            func(t.clone(), u.clone()),
            func(array_of(t.clone()), array_of(u.clone())),
        ),
        2,
        Rc::new(|a| {
            let mut out = Vec::new();
            for x in array_arg("⊡", &a[1])?.to_vec() {
                out.push(apply_value(&a[0], x)?);
            }
            Ok(Value::Array(PVec::from_vec(out)))
        }),
    );
    r.add(
        "⊟",
        &[0],
        &[],
        func(
            func(t.clone(), Type::Bool),
            func(array_of(t.clone()), array_of(t.clone())),
        ),
        2,
        Rc::new(|a| {
            let mut out = Vec::new();
            for x in array_arg("⊟", &a[1])?.to_vec() {
                match apply_value(&a[0], x.clone())? {
                    Value::Bool(true) => out.push(x),
                    Value::Bool(false) => {}
                    other => {
                        return Err(invalid(format!(
                            "'⊟' expects a Bool-returning predicate, found {other:?}"
                        )));
                    }
                }
            }
            Ok(Value::Array(PVec::from_vec(out)))
        }),
    );
    r.add(
        "⊞",
        &[0, 1],
        &[],
        func(
            func(u.clone(), func(t.clone(), u.clone())),
            func(u.clone(), func(array_of(t.clone()), u.clone())),
        ),
        3,
        Rc::new(|a| {
            let mut acc = a[1].clone();
            for x in array_arg("⊞", &a[2])?.to_vec() {
                let partial = apply_value(&a[0], acc)?;
                acc = apply_value(&partial, x)?;
            }
            Ok(acc)
        }),
    );
    r.add(
        "⊕",
        &[0],
        &[],
        func(array_of(t.clone()), func(t.clone(), array_of(t.clone()))),
        2,
        Rc::new(|a| Ok(Value::Array(array_arg("⊕", &a[0])?.push(a[1].clone())))),
    );
    r.add(
        "⍪",
        &[0],
        &[],
        func(
            array_of(t.clone()),
            func(array_of(t.clone()), array_of(t.clone())),
        ),
        2,
        Rc::new(|a| {
            Ok(Value::Array(
                array_arg("⍪", &a[0])?.concat(&array_arg("⍪", &a[1])?),
            ))
        }),
    );
    r.add(
        "⌷",
        &[0],
        &[],
        func(
            Type::Int,
            func(Type::Int, func(array_of(t.clone()), array_of(t.clone()))),
        ),
        3,
        Rc::new(|a| {
            let start = non_negative("⌷", "start", int_arg("⌷", &a[0])?)?;
            let len = non_negative("⌷", "length", int_arg("⌷", &a[1])?)?;
            Ok(Value::Array(array_arg("⌷", &a[2])?.slice(start, len)))
        }),
    );
    r.add(
        "⍳",
        &[],
        &[],
        func(Type::Int, array_of(Type::Int)),
        1,
        Rc::new(|a| {
            let n = count_arg("⍳", "count", int_arg("⍳", &a[0])?)?;
            Ok(Value::Array(PVec::from_vec(
                (0..n as i64).map(Value::Int).collect(),
            )))
        }),
    );
    r.add(
        "⊠",
        &[0],
        &[],
        func(Type::Int, func(t.clone(), array_of(t.clone()))),
        2,
        Rc::new(|a| {
            let n = count_arg("⊠", "count", int_arg("⊠", &a[0])?)?;
            Ok(Value::Array(PVec::from_vec(vec![a[1].clone(); n])))
        }),
    );
    r.add(
        "⍋",
        &[0],
        &[(0, ord)],
        func(array_of(t.clone()), array_of(t.clone())),
        1,
        Rc::new(|a| {
            let mut items = array_arg("⍋", &a[0])?.to_vec();
            items.sort_by(cmp_total);
            Ok(Value::Array(PVec::from_vec(items)))
        }),
    );
    r.add(
        "⇋",
        &[0],
        &[],
        func(list_of(t.clone()), array_of(t.clone())),
        1,
        Rc::new(|a| Ok(Value::Array(PVec::from_vec(vec_from_list("⇋", &a[0])?)))),
    );
    r.add(
        "⇌",
        &[0],
        &[],
        func(array_of(t.clone()), list_of(t.clone())),
        1,
        Rc::new(|a| Ok(list_from(array_arg("⇌", &a[0])?.to_vec()))),
    );

    // ── Map ────────────────────────────────────────────────────────────
    r.add(
        "⍬",
        &[0, 1],
        &[],
        map_of(k.clone(), v.clone()),
        0,
        Rc::new(|_| Ok(Value::Map(PMap::new()))),
    );
    r.add(
        "⊎",
        &[0, 1],
        &[(0, ord)],
        func(
            map_of(k.clone(), v.clone()),
            func(k.clone(), func(v.clone(), map_of(k.clone(), v.clone()))),
        ),
        3,
        Rc::new(|a| {
            Ok(Value::Map(
                map_arg("⊎", &a[0])?.insert(a[1].clone(), a[2].clone()),
            ))
        }),
    );
    r.add(
        "⊖",
        &[0, 1],
        &[(0, ord)],
        func(
            map_of(k.clone(), v.clone()),
            func(k.clone(), map_of(k.clone(), v.clone())),
        ),
        2,
        Rc::new(|a| Ok(Value::Map(map_arg("⊖", &a[0])?.remove(&a[1])))),
    );
    r.add(
        "⌕",
        &[0, 1],
        &[(0, ord)],
        func(
            map_of(k.clone(), v.clone()),
            func(k.clone(), optional_of(v.clone())),
        ),
        2,
        Rc::new(|a| Ok(optional(map_arg("⌕", &a[0])?.get(&a[1]).cloned()))),
    );
    r.add(
        "⊛",
        &[0, 1],
        &[],
        func(map_of(k.clone(), v.clone()), Type::Int),
        1,
        Rc::new(|a| Ok(Value::Int(map_arg("⊛", &a[0])?.len() as i64))),
    );
    r.add(
        "⌸",
        &[0, 1],
        &[],
        func(map_of(k.clone(), v.clone()), list_of(k.clone())),
        1,
        Rc::new(|a| {
            Ok(list_from(
                map_arg("⌸", &a[0])?
                    .entries()
                    .into_iter()
                    .map(|(key, _)| key)
                    .collect(),
            ))
        }),
    );
    r.add(
        "⌹",
        &[0, 1],
        &[],
        func(map_of(k.clone(), v.clone()), list_of(v.clone())),
        1,
        Rc::new(|a| {
            Ok(list_from(
                map_arg("⌹", &a[0])?
                    .entries()
                    .into_iter()
                    .map(|(_, val)| val)
                    .collect(),
            ))
        }),
    );

    // ── Set ────────────────────────────────────────────────────────────
    r.add(
        "⍭",
        &[0],
        &[],
        set_of(k.clone()),
        0,
        Rc::new(|_| Ok(Value::Set(PMap::new()))),
    );
    r.add(
        "⊌",
        &[0],
        &[(0, ord)],
        func(set_of(k.clone()), func(k.clone(), set_of(k.clone()))),
        2,
        Rc::new(|a| {
            Ok(Value::Set(
                set_arg("⊌", &a[0])?.insert(a[1].clone(), Value::Unit),
            ))
        }),
    );
    r.add(
        "⊝",
        &[0],
        &[(0, ord)],
        func(set_of(k.clone()), func(k.clone(), set_of(k.clone()))),
        2,
        Rc::new(|a| Ok(Value::Set(set_arg("⊝", &a[0])?.remove(&a[1])))),
    );
    r.add(
        "∈",
        &[0],
        &[(0, ord)],
        func(set_of(k.clone()), func(k.clone(), Type::Bool)),
        2,
        Rc::new(|a| Ok(Value::Bool(set_arg("∈", &a[0])?.get(&a[1]).is_some()))),
    );
    r.add(
        "⊚",
        &[0],
        &[],
        func(set_of(k.clone()), Type::Int),
        1,
        Rc::new(|a| Ok(Value::Int(set_arg("⊚", &a[0])?.len() as i64))),
    );
    r.add(
        "⌻",
        &[0],
        &[],
        func(set_of(k.clone()), list_of(k.clone())),
        1,
        Rc::new(|a| {
            Ok(list_from(
                set_arg("⌻", &a[0])?
                    .entries()
                    .into_iter()
                    .map(|(key, _)| key)
                    .collect(),
            ))
        }),
    );

    r.finish()
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

    fn ints(v: &Value) -> Vec<i64> {
        match v {
            Value::Array(a) => a
                .to_vec()
                .iter()
                .map(|x| match x {
                    Value::Int(i) => *i,
                    other => panic!("expected Int, got {other:?}"),
                })
                .collect(),
            other => panic!("expected an Array, got {other:?}"),
        }
    }

    fn array(xs: &[i64]) -> Value {
        Value::Array(PVec::from_vec(xs.iter().copied().map(Value::Int).collect()))
    }

    #[test]
    fn length_rejects_a_non_array_without_panicking() {
        assert!(call("#", vec![Value::Int(5)]).is_err());
    }

    #[test]
    fn set_is_persistent_and_bounds_checked() {
        let original = array(&[1, 2, 3]);
        let out = call("⊙", vec![original.clone(), Value::Int(1), Value::Int(99)]).unwrap();
        assert_eq!(ints(&out), [1, 99, 3]);
        assert_eq!(ints(&original), [1, 2, 3]);
        assert!(call("⊙", vec![original, Value::Int(3), Value::Int(0)]).is_err());
    }

    #[test]
    fn push_concat_slice_range_replicate() {
        let a = array(&[1, 2]);
        assert_eq!(
            ints(&call("⊕", vec![a.clone(), Value::Int(3)]).unwrap()),
            [1, 2, 3]
        );
        assert_eq!(
            ints(&call("⍪", vec![a.clone(), array(&[7])]).unwrap()),
            [1, 2, 7]
        );
        assert_eq!(
            ints(&call("⌷", vec![Value::Int(1), Value::Int(5), array(&[1, 2, 3])]).unwrap()),
            [2, 3]
        );
        assert_eq!(ints(&call("⍳", vec![Value::Int(4)]).unwrap()), [0, 1, 2, 3]);
        assert_eq!(
            ints(&call("⊠", vec![Value::Int(2), Value::Int(9)]).unwrap()),
            [9, 9]
        );
        assert!(call("⍳", vec![Value::Int(-1)]).is_err());
        assert!(call("⍳", vec![Value::Int(i64::MAX)]).is_err());
        assert!(call("⊠", vec![Value::Int(1 << 40), Value::Int(0)]).is_err());
    }

    #[test]
    fn sort_is_ascending_and_does_not_touch_the_input() {
        let a = array(&[3, 1, 2]);
        assert_eq!(ints(&call("⍋", vec![a.clone()]).unwrap()), [1, 2, 3]);
        assert_eq!(ints(&a), [3, 1, 2]);
    }

    #[test]
    fn map_insert_lookup_remove_keys_values() {
        let (_, values) = load();
        let Value::Map(empty) = values.get("⍬").unwrap() else {
            panic!("⍬ must be a Map value");
        };
        assert!(empty.is_empty());
        let m = call(
            "⊎",
            vec![Value::Map(empty.clone()), Value::Int(2), Value::Int(20)],
        )
        .unwrap();
        let m = call("⊎", vec![m, Value::Int(1), Value::Int(10)]).unwrap();
        let found = call("⌕", vec![m.clone(), Value::Int(1)]).unwrap();
        assert!(matches!(found, Value::Adt { ref tag, .. } if &**tag == "⧫"));
        let missing = call("⌕", vec![m.clone(), Value::Int(9)]).unwrap();
        assert!(matches!(missing, Value::Adt { ref tag, .. } if &**tag == "⦰"));
        let keys = vec_from_list("test", &call("⌸", vec![m.clone()]).unwrap()).unwrap();
        assert!(matches!(keys.as_slice(), [Value::Int(1), Value::Int(2)]));
        let removed = call("⊖", vec![m.clone(), Value::Int(1)]).unwrap();
        assert!(matches!(call("⊛", vec![removed]), Ok(Value::Int(1))));
        assert!(matches!(call("⊛", vec![m]), Ok(Value::Int(2))));
    }

    #[test]
    fn set_membership_ignores_duplicates() {
        let (_, values) = load();
        let s = values.get("⍭").unwrap().clone();
        let s = call("⊌", vec![s, Value::Int(1)]).unwrap();
        let s = call("⊌", vec![s, Value::Int(1)]).unwrap();
        assert!(matches!(call("⊚", vec![s.clone()]), Ok(Value::Int(1))));
        assert!(matches!(
            call("∈", vec![s, Value::Int(1)]),
            Ok(Value::Bool(true))
        ));
    }
}
