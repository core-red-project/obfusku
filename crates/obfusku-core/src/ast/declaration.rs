//! Algebraic data type declarations (`SEMANTIC_CORE.md` §9, §9.3).

use crate::types::Type;
use obfusku_diagnostics::Span;

/// `Tag params ≔ { variants }`.
///
/// One declaration form covers sum types, tuples and records: a tuple is a
/// one-variant type, and a record is a one-variant type whose fields are
/// named (§9.3). That is why there is no separate "record" node anywhere in
/// the Core.
#[derive(Debug, Clone)]
pub struct TypeDecl {
    /// The type's name; also the namespace its constructors live in.
    pub tag: String,
    /// Type parameter names, in declaration order.
    pub type_params: Vec<String>,
    /// The variants. A type with no variants is uninhabited.
    pub variants: Vec<VariantDecl>,
    /// The whole declaration.
    pub span: Span,
}

/// One constructor of a [`TypeDecl`].
///
/// Field types may reference [`Type::Param`] for one of the enclosing
/// declaration's `type_params`; the checker substitutes a fresh variable at
/// each use, which is what makes constructors generic.
#[derive(Debug, Clone)]
pub struct VariantDecl {
    /// The constructor's tag.
    pub tag: String,
    /// One type per field, in order.
    pub fields: Vec<Type>,
    /// `Some(names)` — one per field, in declared order — only for a record's
    /// single variant; `None` for sum variants and tuples.
    pub field_names: Option<Vec<String>>,
    /// The whole variant.
    pub span: Span,
}

impl VariantDecl {
    /// Whether this variant's fields are named (a record variant).
    #[must_use]
    pub fn is_record(&self) -> bool {
        self.field_names.is_some()
    }

    /// The position of the field called `name`, if this is a record variant
    /// that has one. Positions are what the runtime stores, so this is the
    /// bridge between `base.name` and positional access (§9.3).
    #[must_use]
    pub fn field_index(&self, name: &str) -> Option<usize> {
        self.field_names.as_ref()?.iter().position(|n| n == name)
    }
}

impl TypeDecl {
    /// The variant with constructor `tag`, if any.
    #[must_use]
    pub fn variant(&self, tag: &str) -> Option<&VariantDecl> {
        self.variants.iter().find(|v| v.tag == tag)
    }

    /// Whether this is a record: exactly one variant, with named fields.
    #[must_use]
    pub fn is_record(&self) -> bool {
        matches!(self.variants.as_slice(), [v] if v.is_record())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn variant(tag: &str, fields: usize, names: Option<&[&str]>) -> VariantDecl {
        VariantDecl {
            tag: tag.to_string(),
            fields: vec![Type::Int; fields],
            field_names: names.map(|ns| ns.iter().map(|n| (*n).to_string()).collect()),
            span: Span::default(),
        }
    }

    fn decl(tag: &str, variants: Vec<VariantDecl>) -> TypeDecl {
        TypeDecl {
            tag: tag.to_string(),
            type_params: vec![],
            variants,
            span: Span::default(),
        }
    }

    #[test]
    fn a_record_has_one_variant_with_named_fields() {
        let point = decl("Point", vec![variant("Point", 2, Some(&["x", "y"]))]);
        assert!(point.is_record());
        assert!(point.variants[0].is_record());
    }

    #[test]
    fn sums_and_tuples_are_not_records() {
        let sum = decl("Shape", vec![variant("A", 1, None), variant("B", 1, None)]);
        let tuple = decl("Pair", vec![variant("Pair", 2, None)]);
        let two_named = decl(
            "Odd",
            vec![variant("A", 1, Some(&["a"])), variant("B", 1, Some(&["b"]))],
        );
        assert!(!sum.is_record() && !tuple.is_record() && !two_named.is_record());
    }

    #[test]
    fn field_index_maps_names_to_positions() {
        let v = variant("P", 3, Some(&["x", "y", "z"]));
        assert_eq!(v.field_index("y"), Some(1));
        assert_eq!(v.field_index("z"), Some(2));
        assert_eq!(v.field_index("w"), None);
    }

    #[test]
    fn unnamed_variants_have_no_field_names_to_look_up() {
        assert_eq!(variant("P", 2, None).field_index("x"), None);
    }

    #[test]
    fn variant_lookup_is_by_constructor_tag() {
        let d = decl("T", vec![variant("A", 0, None), variant("B", 1, None)]);
        assert_eq!(d.variant("B").map(|v| v.fields.len()), Some(1));
        assert!(d.variant("C").is_none());
    }

    #[test]
    fn an_empty_type_is_uninhabited_and_not_a_record() {
        let d = decl("Never", vec![]);
        assert!(!d.is_record() && d.variant("X").is_none());
    }
}
