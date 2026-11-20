//! Declaring a family of natives: a type scheme and a runtime value, together.
//!
//! The checker only sees a [`Scheme`]; the evaluator only sees a [`Value`].
//! Nothing links the two, so a native is always registered through
//! [`Registry::add`], which takes both at once and keeps a native's name,
//! scheme and arity in one place instead of three parallel tables.

use obfusku_runtime::eval::Raised;
use obfusku_runtime::{NativeFn, Value};
use obfusku_typecheck::{Constraint, Prelude, Scheme, Type, TypeVarId};
use std::collections::HashMap;
use std::rc::Rc;

/// Body of a native, called once all `arity` curried arguments have arrived.
pub type Impl = Rc<dyn Fn(&[Value]) -> Result<Value, Raised>>;

/// A native of `arity` curried arguments, applied once all have arrived.
///
/// Currying is done here because `Apply` is arity-1 (`SEMANTIC_CORE.md`
/// §7): a two-argument native is a function returning a function. Partial
/// applications are ordinary values and can be passed around and reused, so
/// each step clones the arguments gathered so far instead of consuming them.
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

/// Collects a family of natives: each gets a type scheme (`∀vars.
/// constraints ⇒ ty`) and a curried value, ready to seed a prelude.
#[derive(Default)]
pub struct Registry {
    types: Prelude,
    values: HashMap<String, Value>,
}

impl Registry {
    /// Registers `name` with the scheme `∀vars. constraints ⇒ ty`.
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

    /// Registers a native whose type has no variables, the common case.
    pub fn add_mono(&mut self, name: &'static str, ty: Type, arity: usize, f: Impl) {
        self.add(name, &[], &[], ty, arity, f);
    }

    /// Splits into the type prelude and the value table.
    #[must_use]
    pub fn finish(self) -> (Prelude, HashMap<String, Value>) {
        (self.types, self.values)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::natives::marshal::{func, int_arg};

    fn call(v: &Value, arg: Value) -> Result<Value, Raised> {
        match v {
            Value::Native(n) => (n.func)(arg),
            other => panic!("expected a native, got {other:?}"),
        }
    }

    fn add_native() -> Value {
        curried(
            "add",
            2,
            Rc::new(|a| Ok(Value::Int(int_arg("add", &a[0])? + int_arg("add", &a[1])?))),
        )
    }

    #[test]
    fn a_curried_native_runs_once_all_arguments_have_arrived() {
        let partial = call(&add_native(), Value::Int(1)).unwrap();
        assert!(matches!(call(&partial, Value::Int(2)), Ok(Value::Int(3))));
    }

    #[test]
    fn a_partial_application_can_be_reused() {
        let add_ten = call(&add_native(), Value::Int(10)).unwrap();
        assert!(matches!(call(&add_ten, Value::Int(1)), Ok(Value::Int(11))));
        assert!(matches!(call(&add_ten, Value::Int(5)), Ok(Value::Int(15))));
    }

    #[test]
    fn a_nullary_native_is_just_its_value() {
        let v = curried("zero", 0, Rc::new(|_| Ok(Value::Int(0))));
        assert!(matches!(v, Value::Int(0)));
    }

    #[test]
    fn add_mono_registers_scheme_and_value_under_one_name() {
        let mut r = Registry::default();
        r.add_mono(
            "id",
            func(Type::Int, Type::Int),
            1,
            Rc::new(|a| Ok(a[0].clone())),
        );
        let (types, values) = r.finish();
        assert_eq!(
            types["id"].ty.to_core(),
            func(Type::Int, Type::Int).to_core()
        );
        assert!(matches!(values["id"], Value::Native(_)));
    }

    #[test]
    fn add_records_quantified_variables() {
        let mut r = Registry::default();
        let a = Type::Var(TypeVarId(0));
        r.add(
            "const",
            &[0],
            &[],
            func(a.clone(), a),
            1,
            Rc::new(|a| Ok(a[0].clone())),
        );
        let (types, _) = r.finish();
        assert_eq!(types["const"].vars, vec![TypeVarId(0)]);
    }
}
