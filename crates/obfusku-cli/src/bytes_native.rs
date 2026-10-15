//! `⌭` byte strings (`ADR-027`) — an immutable `Value::Bytes`. The
//! family mirrors `Str`/`Array` by glyph: `⌭` followed by the glyph of the
//! operation it twins.
//!
//! - `⌭# : ⌭ → Int` length · `⌭⌷ : Int → Int → ⌭ → ⌭` slice (start,
//!   length; clamped) · `⌭⍪ : ⌭ → ⌭ → ⌭` concat · `⌭⊠ : Int → Int → ⌭`
//!   `n` copies of a byte · `⌭⌕ : ⌭ → Int → Optional<Int>` element
//! - `⌭⇋ : Array<Int> → ⌭` · `⌭⇌ : ⌭ → Array<Int>` (elements in 0..=255)
//! - `⌭⌘ : Str → ⌭` UTF-8 encode · `⌭⍕ : ⌭ → Result<Str, Str>` strict decode

use crate::native_util::{
    Registry, array_of, bytes_arg, count_arg, func, int_arg, invalid, optional, optional_of,
    result_of, s,
};
use obfusku_runtime::Value;
use obfusku_runtime::pvec::PVec;
use obfusku_typecheck::{Prelude, Type};
use std::collections::HashMap;
use std::rc::Rc;

fn bytes(data: Vec<u8>) -> Value {
    Value::Bytes(Rc::from(data))
}

fn byte(op: &str, n: i64) -> Result<u8, obfusku_runtime::eval::Raised> {
    u8::try_from(n).map_err(|_| invalid(format!("'{op}' expects a byte in 0..=255, found {n}")))
}

#[must_use]
pub fn load() -> (Prelude, HashMap<String, Value>) {
    let mut r = Registry::default();
    let b = || Type::Adt("⌭".to_string(), vec![]);
    let mut add = |name: &'static str, ty: Type, arity: usize, f: crate::native_util::Impl| {
        r.add(name, &[], &[], ty, arity, f);
    };

    add(
        "⌭#",
        func(b(), Type::Int),
        1,
        Rc::new(|a| Ok(Value::Int(bytes_arg("⌭#", &a[0])?.len() as i64))),
    );
    add(
        "⌭⌷",
        func(Type::Int, func(Type::Int, func(b(), b()))),
        3,
        Rc::new(|a| {
            let (start, len) = (int_arg("⌭⌷", &a[0])?, int_arg("⌭⌷", &a[1])?);
            let data = bytes_arg("⌭⌷", &a[2])?;
            if start < 0 || len < 0 {
                return Err(invalid(format!(
                    "'⌭⌷' expects non-negative start and length, found {start}, {len}"
                )));
            }
            let from = (start as usize).min(data.len());
            let to = from.saturating_add(len as usize).min(data.len());
            Ok(bytes(data[from..to].to_vec()))
        }),
    );
    add(
        "⌭⍪",
        func(b(), func(b(), b())),
        2,
        Rc::new(|a| {
            let mut out = bytes_arg("⌭⍪", &a[0])?.to_vec();
            out.extend_from_slice(bytes_arg("⌭⍪", &a[1])?);
            Ok(bytes(out))
        }),
    );
    add(
        "⌭⊠",
        func(Type::Int, func(Type::Int, b())),
        2,
        Rc::new(|a| {
            let n = count_arg("⌭⊠", "count", int_arg("⌭⊠", &a[0])?)?;
            Ok(bytes(vec![byte("⌭⊠", int_arg("⌭⊠", &a[1])?)?; n]))
        }),
    );
    add(
        "⌭⌕",
        func(b(), func(Type::Int, optional_of(Type::Int))),
        2,
        Rc::new(|a| {
            let data = bytes_arg("⌭⌕", &a[0])?;
            let i = int_arg("⌭⌕", &a[1])?;
            Ok(optional(
                usize::try_from(i)
                    .ok()
                    .and_then(|i| data.get(i))
                    .map(|b| Value::Int(i64::from(*b))),
            ))
        }),
    );
    add(
        "⌭⇋",
        func(array_of(Type::Int), b()),
        1,
        Rc::new(|a| {
            let Value::Array(items) = &a[0] else {
                return Err(invalid(format!("'⌭⇋' expects an Array, found {:?}", a[0])));
            };
            items
                .to_vec()
                .iter()
                .map(|v| match v {
                    Value::Int(n) => byte("⌭⇋", *n),
                    other => Err(invalid(format!(
                        "'⌭⇋' expects Int elements, found {other:?}"
                    ))),
                })
                .collect::<Result<Vec<u8>, _>>()
                .map(bytes)
        }),
    );
    add(
        "⌭⇌",
        func(b(), array_of(Type::Int)),
        1,
        Rc::new(|a| {
            Ok(Value::Array(PVec::from_vec(
                bytes_arg("⌭⇌", &a[0])?
                    .iter()
                    .map(|x| Value::Int(i64::from(*x)))
                    .collect(),
            )))
        }),
    );
    add(
        "⌭⌘",
        func(Type::Str, b()),
        1,
        Rc::new(|a| match &a[0] {
            Value::Str(t) => Ok(bytes(t.as_bytes().to_vec())),
            other => Err(invalid(format!("'⌭⌘' expects a Str, found {other:?}"))),
        }),
    );
    add(
        "⌭⍕",
        func(b(), result_of(Type::Str, Type::Str)),
        1,
        Rc::new(|a| {
            Ok(match std::str::from_utf8(bytes_arg("⌭⍕", &a[0])?) {
                Ok(text) => crate::native_util::adt("✓", vec![s(text)]),
                Err(e) => crate::native_util::adt("✗", vec![s(&format!("not UTF-8: {e}"))]),
            })
        }),
    );

    r.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(name: &str, args: Vec<Value>) -> Value {
        let (_, values) = load();
        let mut cur = values.get(name).unwrap().clone();
        for a in args {
            cur = match &cur {
                Value::Native(f) => (f.func)(a).unwrap(),
                other => panic!("expected a native, got {other:?}"),
            };
        }
        cur
    }

    #[test]
    fn slice_is_clamped_and_concat_appends() {
        let v = call("⌭⍪", vec![bytes(vec![1, 2]), bytes(vec![3])]);
        let sliced = call("⌭⌷", vec![Value::Int(1), Value::Int(99), v]);
        assert!(matches!(sliced, Value::Bytes(ref b) if **b == [2, 3]));
    }

    #[test]
    fn utf8_round_trips_and_invalid_input_is_an_err() {
        let enc = call("⌭⌘", vec![s("añ")]);
        assert!(matches!(&enc, Value::Bytes(b) if b.len() == 3));
        let dec = call("⌭⍕", vec![enc]);
        assert!(matches!(dec, Value::Adt { ref tag, .. } if &**tag == "✓"));
        let bad = call("⌭⍕", vec![bytes(vec![0xff])]);
        assert!(matches!(bad, Value::Adt { ref tag, .. } if &**tag == "✗"));
    }
}
