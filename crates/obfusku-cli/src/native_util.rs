//! Shared plumbing for host natives (`ADR-010`): curried multi-argument
//! functions, constructors for the ambient `List`/`Optional`/`Result`
//! shapes, and typed-argument accessors that raise instead of panicking.

use obfusku_runtime::eval::Raised;
use obfusku_runtime::{NativeFn, Text, Value};
use obfusku_typecheck::{Constraint, Prelude, Scheme, Type, TypeVarId};
use std::collections::HashMap;
use std::rc::Rc;

pub type Impl = Rc<dyn Fn(&[Value]) -> Result<Value, Raised>>;

#[must_use]
pub fn adt(tag: &str, args: Vec<Value>) -> Value {
    Value::Adt {
        tag: Rc::from(tag),
        args: Rc::from(args),
    }
}

#[must_use]
pub fn s(text: &str) -> Value {
    Value::Str(Text::from(text))
}

/// Most elements a single native will allocate on a count argument; a
/// larger request raises instead of aborting the process on allocation.
pub const MAX_COUNT: usize = 1 << 28;

/// A non-negative count no larger than [`MAX_COUNT`].
pub fn count_arg(op: &str, what: &str, n: i64) -> Result<usize, Raised> {
    let count = usize::try_from(n)
        .map_err(|_| invalid(format!("'{op}' expects a non-negative {what}, found {n}")))?;
    if count > MAX_COUNT {
        return Err(invalid(format!(
            "'{op}' expects a {what} of at most {MAX_COUNT}, found {n}"
        )));
    }
    Ok(count)
}

#[must_use]
pub fn invalid(message: impl AsRef<str>) -> Raised {
    Raised(adt("InvalidOperation", vec![s(message.as_ref())]))
}

pub fn str_arg<'a>(op: &str, v: &'a Value) -> Result<&'a str, Raised> {
    match v {
        Value::Str(t) => Ok(t),
        other => Err(invalid(format!("'{op}' expects a Str, found {other:?}"))),
    }
}

pub fn int_arg(op: &str, v: &Value) -> Result<i64, Raised> {
    match v {
        Value::Int(i) => Ok(*i),
        other => Err(invalid(format!("'{op}' expects an Int, found {other:?}"))),
    }
}

pub fn bytes_arg<'a>(op: &str, v: &'a Value) -> Result<&'a [u8], Raised> {
    match v {
        Value::Bytes(b) => Ok(b),
        other => Err(invalid(format!("'{op}' expects bytes, found {other:?}"))),
    }
}

/// A `List` value built from `items`, in order.
#[must_use]
pub fn list_from(items: Vec<Value>) -> Value {
    items
        .into_iter()
        .rev()
        .fold(adt("⊘", vec![]), |tail, item| adt("⁝", vec![item, tail]))
}

/// The elements of a `List` value; iterative, so a long list cannot
/// overflow the Rust stack.
pub fn vec_from_list(op: &str, v: &Value) -> Result<Vec<Value>, Raised> {
    let mut out = Vec::new();
    let mut cur = v.clone();
    loop {
        let next = match &cur {
            Value::Adt { tag, args } if &**tag == "⊘" && args.is_empty() => return Ok(out),
            Value::Adt { tag, args } if &**tag == "⁝" && args.len() == 2 => {
                out.push(args[0].clone());
                args[1].clone()
            }
            other => return Err(invalid(format!("'{op}' expects a List, found {other:?}"))),
        };
        cur = next;
    }
}

#[must_use]
pub fn optional(v: Option<Value>) -> Value {
    match v {
        Some(x) => adt("⧫", vec![x]),
        None => adt("⦰", vec![]),
    }
}

/// A native of `arity` curried arguments, applied once all have arrived.
pub fn curried(name: &'static str, arity: usize, f: Impl) -> Value {
    fn step(name: &'static str, arity: usize, got: Vec<Value>, f: Impl) -> Value {
        Value::Native(Rc::new(NativeFn {
            name: name.to_string(),
            func: Box::new(move |arg| {
                let mut args = got.clone();
                args.push(arg);
                if args.len() == arity {
                    f(&args)
                } else {
                    Ok(step(name, arity, args, f.clone()))
                }
            }),
        }))
    }
    if arity == 0 {
        // A nullary native is just its value; callers pass it through `f`.
        return f(&[]).unwrap_or(Value::Unit);
    }
    step(name, arity, Vec::new(), f)
}

#[must_use]
pub fn func(from: Type, to: Type) -> Type {
    Type::Function(Box::new(from), Box::new(to))
}

#[must_use]
pub fn list_of(t: Type) -> Type {
    Type::Adt("List".to_string(), vec![t])
}

#[must_use]
pub fn optional_of(t: Type) -> Type {
    Type::Adt("Optional".to_string(), vec![t])
}

#[must_use]
pub fn result_of(ok: Type, err: Type) -> Type {
    Type::Adt("Result".to_string(), vec![ok, err])
}

#[must_use]
pub fn array_of(t: Type) -> Type {
    Type::Adt("Array".to_string(), vec![t])
}

#[must_use]
pub fn map_of(k: Type, v: Type) -> Type {
    Type::Adt("Map".to_string(), vec![k, v])
}

#[must_use]
pub fn set_of(k: Type) -> Type {
    Type::Adt("Set".to_string(), vec![k])
}

/// Collects a family of natives: each gets a type scheme (`∀vars.
/// constraints ⇒ ty`) and a curried value, ready to seed a prelude.
#[derive(Default)]
pub struct Registry {
    pub types: Prelude,
    pub values: HashMap<String, Value>,
}

impl Registry {
    pub fn add(
        &mut self,
        name: &'static str,
        vars: &[u32],
        constraints: &[(u32, Constraint)],
        ty: Type,
        arity: usize,
        f: Impl,
    ) {
        self.types.insert(
            name.to_string(),
            Scheme::constrained(
                vars.iter().map(|v| TypeVarId(*v)).collect(),
                constraints
                    .iter()
                    .map(|(v, c)| (TypeVarId(*v), *c))
                    .collect(),
                ty,
            ),
        );
        self.values
            .insert(name.to_string(), curried(name, arity, f));
    }

    #[must_use]
    pub fn finish(self) -> (Prelude, HashMap<String, Value>) {
        (self.types, self.values)
    }
}
