//! Declared-type metadata the evaluator needs at runtime: each
//! constructor's declaration order (generated ordering, `ADR-025`) and a
//! record's field names (field access, rendering). Populated from
//! `TypeDecl`s as modules are evaluated; constructor tags are unique
//! within a program, so one table per thread suffices.

use obfusku_core::ast::TypeDecl;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

#[derive(Debug, Clone)]
pub struct VariantMeta {
    /// The type this constructor belongs to.
    pub adt: Rc<str>,
    pub ordinal: usize,
    pub field_names: Option<Rc<[String]>>,
}

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
    fn builtin() -> Table {
        let mut t = Table::default();
        for tag in [
            "DivisionByZero",
            "NonExhaustiveMatch",
            "InvalidOperation",
            "Failure",
            "IntegerOverflow",
        ] {
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
