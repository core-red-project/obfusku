//! The type information that survives to run time: where each constructor
//! sits in its type, and what a record's fields are called.
//!
//! The checker has the full type declarations, but a running program holds
//! only `Value::Adt { tag, args }`. Two things still need the declaration:
//! the generated *ordering* (constructors compare by declaration order,
//! `ADR-025`) and *records* (field access and update by name, and rendering).
//!
//! **Why a thread-local table.** Constructor tags are unique within a
//! program, so a single tag-keyed table answers every question. Natives are
//! plain `Fn(Value)` with no evaluator handle, so the table cannot be passed
//! to them; a per-thread table is how `show` and `cmp_total` reach it from
//! anywhere. The cost is that one thread evaluates one program at a time,
//! which [`register`] makes explicit by replacing earlier declarations.

use obfusku_core::ast::TypeDecl;
use obfusku_core::exception::BUILTIN_EXCEPTIONS;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// What is known about one constructor.
#[derive(Debug, Clone)]
pub struct VariantMeta {
    /// The type this constructor belongs to.
    pub adt: Rc<str>,
    /// Its position among its type's constructors, in declaration order.
    pub ordinal: usize,
    /// The field names, when it was declared as a record.
    pub field_names: Option<Rc<[String]>>,
}

/// Every known constructor, and how many each type has so far.
#[derive(Default)]
struct Table {
    variants: HashMap<String, VariantMeta>,
    /// Variants declared so far per type name — `Exception` grows as
    /// modules declare their own (`ADR-026`).
    counts: HashMap<String, usize>,
}

thread_local! {
    static TABLE: RefCell<Table> = RefCell::new(Table::builtin());
}

impl Table {
    /// The constructors every program has: the built-in `Exception`s the
    /// evaluator itself raises.
    fn builtin() -> Table {
        let mut t = Table::default();
        for (tag, _) in BUILTIN_EXCEPTIONS {
            t.add("Exception", tag, None);
        }
        t
    }

    /// Adds or replaces `tag`. A tag already known to `adt` keeps its
    /// ordinal (an `Exception` extension is re-registered by every module
    /// that imports it); a new one takes the next.
    fn add(&mut self, adt: &str, tag: &str, field_names: Option<Vec<String>>) {
        let ordinal = if let Some(existing) = self.variants.get(tag) {
            existing.ordinal
        } else {
            let next = self.counts.entry(adt.to_string()).or_insert(0);
            *next += 1;
            *next - 1
        };
        self.variants.insert(
            tag.to_string(),
            VariantMeta {
                adt: Rc::from(adt),
                ordinal,
                field_names: field_names.map(Rc::from),
            },
        );
    }
}

/// Records every constructor of `decls`. A type declared again (another
/// program evaluated on this thread) replaces the earlier declaration of
/// the same name; `Exception` only ever gains variants.
pub fn register(decls: &[TypeDecl]) {
    TABLE.with(|t| {
        let mut t = t.borrow_mut();
        for td in decls {
            if td.tag != "Exception" {
                t.counts.remove(&td.tag);
                t.variants.retain(|_, m| &*m.adt != td.tag.as_str());
            }
            for v in &td.variants {
                t.add(&td.tag, &v.tag, v.field_names.clone());
            }
        }
    });
}

/// The metadata of constructor `tag`, if declared.
#[must_use]
pub fn variant(tag: &str) -> Option<VariantMeta> {
    TABLE.with(|t| t.borrow().variants.get(tag).cloned())
}

/// Declaration position of `tag` within its type, without cloning metadata.
#[must_use]
pub fn ordinal(tag: &str) -> Option<usize> {
    TABLE.with(|t| t.borrow().variants.get(tag).map(|m| m.ordinal))
}

/// Position of field `name` in the record constructed by `tag`.
#[must_use]
pub fn field_index(tag: &str, name: &str) -> Option<usize> {
    variant(tag)?.field_names?.iter().position(|n| n == name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use obfusku_core::ast::{TypeDecl, VariantDecl};
    use obfusku_diagnostics::Span;

    fn decl(tag: &str, variants: &[(&str, Option<&[&str]>)]) -> TypeDecl {
        TypeDecl {
            tag: tag.to_string(),
            type_params: Vec::new(),
            variants: variants
                .iter()
                .map(|(name, fields)| VariantDecl {
                    tag: (*name).to_string(),
                    fields: Vec::new(),
                    field_names: fields.map(|f| f.iter().map(|s| (*s).to_string()).collect()),
                    span: Span::default(),
                })
                .collect(),
            span: Span::default(),
        }
    }

    #[test]
    fn the_builtin_exceptions_are_known_in_order() {
        assert_eq!(ordinal("DivisionByZero"), Some(0));
        assert_eq!(ordinal("IntegerOverflow"), Some(4));
        assert_eq!(&*variant("Failure").unwrap().adt, "Exception");
    }

    #[test]
    fn registering_assigns_declaration_order() {
        register(&[decl("VmColor", &[("VmRed", None), ("VmGreen", None)])]);
        assert_eq!(ordinal("VmRed"), Some(0));
        assert_eq!(ordinal("VmGreen"), Some(1));
        assert_eq!(ordinal("VmUnknown"), None);
    }

    #[test]
    fn a_record_exposes_its_field_positions() {
        register(&[decl("VmPoint", &[("VmPoint", Some(&["x", "y"]))])]);
        assert_eq!(field_index("VmPoint", "y"), Some(1));
        assert_eq!(field_index("VmPoint", "z"), None);
        assert_eq!(field_index("VmColorless", "x"), None);
    }

    #[test]
    fn redeclaring_a_type_replaces_it() {
        register(&[decl("VmShape", &[("VmCircle", None), ("VmSquare", None)])]);
        register(&[decl("VmShape", &[("VmSquare", None)])]);
        assert_eq!(ordinal("VmSquare"), Some(0));
        assert_eq!(ordinal("VmCircle"), None);
    }

    #[test]
    fn exception_only_gains_variants_and_keeps_ordinals() {
        register(&[decl("Exception", &[("VmBoom", None)])]);
        let first = ordinal("VmBoom");
        register(&[decl("Exception", &[("VmBoom", None), ("VmBang", None)])]);
        assert_eq!(ordinal("VmBoom"), first);
        assert_eq!(ordinal("VmBang"), first.map(|o| o + 1));
        assert_eq!(ordinal("DivisionByZero"), Some(0));
    }
}
