//! `Array<T>` combinators — host-provided natives despite doing no I/O
//! (`ADR-021`): `SEMANTIC_CORE.md` §9.1 rules out structural pattern
//! matching against `Array`, so unlike `List`'s `⟐`/`⌿`/`⌽` (ordinary
//! `.obk` source in `stdlib.obk`), these cannot be hand-written in
//! Obfusku at all and must be implemented in Rust, calling back into
//! user-supplied functions via [`obfusku_runtime::eval::apply_value`].
//!
//! Registered unconditionally (`load`'s result), the same as
//! `natives::load_io_only`'s `⌁↑`/`⌁↓` — no filesystem root or other
//! capability is threaded through any of these five, so they are
//! available in the REPL exactly as in `run`/`check`.
//!
//! Surface (`GLYPH_SYSTEM_DESIGN.md` §10.5):
//! - `⊡ : (t → u) → Array<t> → Array<u>` — map
//! - `⊟ : (t → Bool) → Array<t> → Array<t>` — filter
//! - `⊞ : (u → t → u) → u → Array<t> → u` — fold
//! - `# : Array<t> → Int` — length
//! - `⊙ : Array<t> → Int → t → Array<t>` — set (persistent)

use obfusku_runtime::eval::{apply_value, Raised};
use obfusku_runtime::{NativeFn, Value};
use obfusku_typecheck::{Prelude, Scheme, Type, TypeVarId};
use std::collections::HashMap;
use std::rc::Rc;

fn var(id: u32) -> Type {
    Type::Var(TypeVarId(id))
}

fn func(from: Type, to: Type) -> Type {
    Type::Function(Box::new(from), Box::new(to))
}

fn array_of(elem: Type) -> Type {
    Type::Adt("Array".to_string(), vec![elem])
}

fn type_error(op: &str, expected: &str, found: &Value) -> Raised {
    Raised(Value::Adt {
        tag: Rc::from("InvalidOperation"),
        args: Rc::from(vec![Value::Str(Rc::from(
            format!("'{op}' expects {expected}, found {found:?}").as_str(),
        ))]),
    })
}

fn out_of_bounds(op: &str, index: i64, len: usize) -> Raised {
    Raised(Value::Adt {
        tag: Rc::from("InvalidOperation"),
        args: Rc::from(vec![Value::Str(Rc::from(
            format!("'{op}' index out of bounds: index {index}, length {len}").as_str(),
        ))]),
    })
}

fn as_array(op: &str, v: Value) -> Result<Rc<[Value]>, Raised> {
    match v {
        Value::Array(a) => Ok(a),
        other => Err(type_error(op, "an Array", &other)),
    }
}

fn as_int(op: &str, v: Value) -> Result<i64, Raised> {
    match v {
        Value::Int(i) => Ok(i),
        other => Err(type_error(op, "an Int", &other)),
    }
}

/// Every `Array` native's `Scheme` and `Value`, ready to seed a prelude
/// environment — merged into `run`/`check`/the REPL alongside
/// `natives::load_io_only`.
pub fn load() -> (Prelude, HashMap<String, Value>) {
    let mut types = Prelude::new();
    let mut values = HashMap::new();

    types.insert(
        "#".to_string(),
        Scheme {
            vars: vec![TypeVarId(0)],
            ty: func(array_of(var(0)), Type::Int),
        },
    );
    values.insert("#".to_string(), Value::Native(Rc::new(length_native())));

    types.insert(
        "⊙".to_string(),
        Scheme {
            vars: vec![TypeVarId(0)],
            ty: func(
                array_of(var(0)),
                func(Type::Int, func(var(0), array_of(var(0)))),
            ),
        },
    );
    values.insert("⊙".to_string(), Value::Native(Rc::new(set_native())));

    types.insert(
        "⊡".to_string(),
        Scheme {
            vars: vec![TypeVarId(0), TypeVarId(1)],
            ty: func(
                func(var(0), var(1)),
                func(array_of(var(0)), array_of(var(1))),
            ),
        },
    );
    values.insert("⊡".to_string(), Value::Native(Rc::new(map_native())));

    types.insert(
        "⊟".to_string(),
        Scheme {
            vars: vec![TypeVarId(0)],
            ty: func(
                func(var(0), Type::Bool),
                func(array_of(var(0)), array_of(var(0))),
            ),
        },
    );
    values.insert("⊟".to_string(), Value::Native(Rc::new(filter_native())));

    types.insert(
        "⊞".to_string(),
        Scheme {
            vars: vec![TypeVarId(0), TypeVarId(1)],
            ty: func(
                func(var(1), func(var(0), var(1))),
                func(var(1), func(array_of(var(0)), var(1))),
            ),
        },
    );
    values.insert("⊞".to_string(), Value::Native(Rc::new(fold_native())));

    (types, values)
}

fn length_native() -> NativeFn {
    NativeFn {
        name: "#".to_string(),
        func: Box::new(|arg| {
            let arr = as_array("#", arg)?;
            Ok(Value::Int(arr.len() as i64))
        }),
    }
}

fn set_native() -> NativeFn {
    NativeFn {
        name: "⊙".to_string(),
        func: Box::new(|arr_arg| {
            let arr = as_array("⊙", arr_arg)?;
            Ok(Value::Native(Rc::new(NativeFn {
                name: "⊙ (applied to an array)".to_string(),
                func: Box::new(move |idx_arg| {
                    let arr = Rc::clone(&arr);
                    let index = as_int("⊙", idx_arg)?;
                    Ok(Value::Native(Rc::new(NativeFn {
                        name: "⊙ (applied to an array and an index)".to_string(),
                        func: Box::new(move |value_arg| {
                            let len = arr.len();
                            let Some(i) = usize::try_from(index).ok().filter(|&i| i < len) else {
                                return Err(out_of_bounds("⊙", index, len));
                            };
                            let mut out: Vec<Value> = arr.iter().cloned().collect();
                            out[i] = value_arg;
                            Ok(Value::Array(Rc::from(out)))
                        }),
                    })))
                }),
            })))
        }),
    }
}

fn map_native() -> NativeFn {
    NativeFn {
        name: "⊡".to_string(),
        func: Box::new(|f_arg| {
            Ok(Value::Native(Rc::new(NativeFn {
                name: "⊡ (applied to a function)".to_string(),
                func: Box::new(move |arr_arg| {
                    let arr = as_array("⊡", arr_arg)?;
                    let mut out = Vec::with_capacity(arr.len());
                    for v in arr.iter() {
                        out.push(apply_value(f_arg.clone(), v.clone())?);
                    }
                    Ok(Value::Array(Rc::from(out)))
                }),
            })))
        }),
    }
}

fn filter_native() -> NativeFn {
    NativeFn {
        name: "⊟".to_string(),
        func: Box::new(|f_arg| {
            Ok(Value::Native(Rc::new(NativeFn {
                name: "⊟ (applied to a predicate)".to_string(),
                func: Box::new(move |arr_arg| {
                    let arr = as_array("⊟", arr_arg)?;
                    let mut out = Vec::new();
                    for v in arr.iter() {
                        match apply_value(f_arg.clone(), v.clone())? {
                            Value::Bool(true) => out.push(v.clone()),
                            Value::Bool(false) => {}
                            other => {
                                return Err(type_error("⊟", "a Bool-returning predicate", &other))
                            }
                        }
                    }
                    Ok(Value::Array(Rc::from(out)))
                }),
            })))
        }),
    }
}

fn fold_native() -> NativeFn {
    NativeFn {
        name: "⊞".to_string(),
        func: Box::new(|f_arg| {
            Ok(Value::Native(Rc::new(NativeFn {
                name: "⊞ (applied to a function)".to_string(),
                func: Box::new(move |seed_arg| {
                    let f_arg = f_arg.clone();
                    Ok(Value::Native(Rc::new(NativeFn {
                        name: "⊞ (applied to a function and a seed)".to_string(),
                        func: Box::new(move |arr_arg| {
                            let arr = as_array("⊞", arr_arg)?;
                            let mut acc = seed_arg.clone();
                            for v in arr.iter() {
                                let partial = apply_value(f_arg.clone(), acc)?;
                                acc = apply_value(partial, v.clone())?;
                            }
                            Ok(acc)
                        }),
                    })))
                }),
            })))
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call1(name: &str, values: &HashMap<String, Value>, arg: Value) -> Result<Value, Raised> {
        match values.get(name).unwrap() {
            Value::Native(f) => (f.func)(arg),
            _ => panic!("expected a native"),
        }
    }

    #[test]
    fn length_scheme_matches_actual_behavior() {
        let (_, values) = load();
        let arr = Value::Array(Rc::from(vec![Value::Int(1), Value::Int(2), Value::Int(3)]));
        assert!(matches!(call1("#", &values, arr), Ok(Value::Int(3))));
    }

    #[test]
    fn length_rejects_a_non_array_argument_without_panicking() {
        let (_, values) = load();
        assert!(call1("#", &values, Value::Int(5)).is_err());
    }

    #[test]
    fn set_is_persistent_and_leaves_the_original_untouched() {
        let (_, values) = load();
        let original = Rc::from(vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
        let arr = Value::Array(Rc::clone(&original));
        let with_index = call1("⊙", &values, arr).unwrap();
        let with_value = match with_index {
            Value::Native(f) => (f.func)(Value::Int(1)).unwrap(),
            _ => panic!("expected curried native"),
        };
        let result = match with_value {
            Value::Native(f) => (f.func)(Value::Int(99)).unwrap(),
            _ => panic!("expected curried native"),
        };
        match result {
            Value::Array(a) => assert!(matches!(
                (&a[0], &a[1], &a[2]),
                (Value::Int(1), Value::Int(99), Value::Int(3))
            )),
            other => panic!("expected an Array, got {other:?}"),
        }
        assert!(matches!(
            (&original[0], &original[1], &original[2]),
            (Value::Int(1), Value::Int(2), Value::Int(3))
        ));
    }
}
