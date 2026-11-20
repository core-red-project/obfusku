//! Printing top-level and local declarations
//! (`CONCRETE_SYMBOLIC_GRAMMAR.md` §8).

use crate::glyph::{ARROW, BIND, EXCEPTION, EXPORT, FIELD_SEPARATOR, IMPORT, LAMBDA, MUTABLE};
use crate::precedence::Prec;
use crate::printer::Printer;
use obfusku_syntax::ast::{
    Declaration, Expression, FnParameter, FunctionDeclaration, TypeDeclaration, ValueDeclaration,
    VariantDecl,
};

impl Printer {
    /// A declaration, indented `level` steps and without a trailing newline.
    pub(crate) fn declaration(&mut self, declaration: &Declaration, level: usize) {
        match declaration {
            Declaration::Value(value) => self.value_declaration(value, level),
            Declaration::Function(function) => self.function_declaration(function, level),
            Declaration::Type(ty) => self.type_declaration(ty, level),
            Declaration::Import(import) => {
                self.indent(level);
                self.push(IMPORT);
                self.push_str(&import.module_name);
            }
            Declaration::Exception(variant) => {
                self.indent(level);
                self.push(EXCEPTION);
                self.push(' ');
                self.variant(variant);
            }
        }
    }

    /// §8.1: `name ≔ ˚? ⟳? Expression`, with `Mutability` before `Export`.
    pub(crate) fn value_declaration(&mut self, declaration: &ValueDeclaration, level: usize) {
        self.indent(level);
        self.push_str(&declaration.name);
        if let Some(ty) = &declaration.ty {
            self.push_str(": ");
            self.type_ref(ty);
        }
        self.spaced(BIND);
        if declaration.mutable {
            self.push(MUTABLE);
        }
        if declaration.exported {
            self.push(EXPORT);
        }
        if !declaration.mutable && starts_with_mutability_glyph(&declaration.value) {
            self.parenthesized(|p| p.expression(&declaration.value, Prec::BOTTOM));
        } else {
            self.expression(&declaration.value, Prec::BOTTOM);
        }
    }

    /// §8.2: `λ name ( FnParameter,* ) : Type → Expression`.
    pub(crate) fn function_declaration(&mut self, declaration: &FunctionDeclaration, level: usize) {
        self.indent(level);
        self.push(LAMBDA);
        self.push_str(&declaration.name);
        self.parenthesized(|p| p.separated(&declaration.params, ", ", Printer::fn_parameter));
        self.push_str(": ");
        self.type_ref(&declaration.return_type);
        self.push(' ');
        self.push_str(ARROW);
        if declaration.exported {
            self.push(EXPORT);
        }
        self.push(' ');
        self.expression(&declaration.body, Prec::BOTTOM);
    }

    fn fn_parameter(&mut self, parameter: &FnParameter) {
        self.push_str(&parameter.name);
        self.push_str(": ");
        self.type_ref(&parameter.ty);
    }

    /// §8.3: `TypeBody ::= SumBody | RecordBody | TupleBody`.
    ///
    /// Told apart exactly as the parser builds them: a single variant named
    /// like the declaration is a record (if it has field names) or a tuple;
    /// anything else is a sum.
    fn type_declaration(&mut self, declaration: &TypeDeclaration, level: usize) {
        self.indent(level);
        self.push_str(&declaration.tag);
        for parameter in &declaration.type_params {
            self.push(' ');
            self.push_str(parameter);
        }
        self.spaced(BIND);

        match declaration.variants.as_slice() {
            [only] if only.tag == declaration.tag => match &only.field_names {
                Some(names) => self.braced_fields(names.iter().zip(&only.fields), |p, ty| {
                    p.type_ref(ty);
                }),
                None => self.parenthesized(|p| p.type_list(&only.fields)),
            },
            variants => {
                self.push_str("{ ");
                self.separated(variants, FIELD_SEPARATOR, Printer::variant);
                self.push_str(" }");
            }
        }
    }

    fn variant(&mut self, variant: &VariantDecl) {
        self.push_str(&variant.tag);
        if !variant.fields.is_empty() {
            self.parenthesized(|p| p.type_list(&variant.fields));
        }
    }
}

/// `≔` followed directly by `˚` always parses as the mutability marker
/// (§8.1's documented ambiguity), never as the start of a reference or an
/// explicit-capture rebind, which both print with a leading `˚`. An
/// *immutable* binding whose value is one of those must therefore be
/// parenthesized (`x ≔ (˚y)`), or it re-parses as a *mutable* binding of a
/// different value.
const fn starts_with_mutability_glyph(value: &Expression) -> bool {
    matches!(
        value,
        Expression::ExplicitRef(..) | Expression::Rebind { explicit: true, .. }
    )
}

#[cfg(test)]
mod tests {
    use crate::testing::format_source;

    fn formatted(declaration: &str) -> String {
        format_source(&format!("{declaration}\n❧"))
            .trim_end_matches("\n❧\n")
            .to_string()
    }

    #[test]
    fn value_declaration_prints_annotation_and_markers_in_order() {
        assert_eq!(formatted("x:⟁≔˚⟳1"), "x: ⟁ ≔ ˚⟳1");
    }

    #[test]
    fn immutable_binding_of_an_explicit_reference_is_parenthesized() {
        assert_eq!(formatted("y ≔ ˚ 1\nx ≔ (˚y)"), "y ≔ ˚1\nx ≔ (˚y)");
    }

    #[test]
    fn mutable_binding_keeps_its_value_unparenthesized() {
        assert_eq!(formatted("x ≔ ˚ 1"), "x ≔ ˚1");
    }

    #[test]
    fn function_declaration_prints_signature_and_export_marker() {
        assert_eq!(formatted("λf(a:⟁,b:⟁):⟁→⟳a"), "λf(a: ⟁, b: ⟁): ⟁ →⟳ a");
    }

    #[test]
    fn sum_type_prints_its_variants_with_separators() {
        assert_eq!(
            formatted("Opt a ≔ { None ⟢ Some(a) }"),
            "Opt a ≔ { None \u{27E2} Some(a) }"
        );
    }

    #[test]
    fn record_type_prints_named_fields() {
        assert_eq!(
            formatted("Point ≔ { x: ⟁ ⟢ y: ⟁ }"),
            "Point ≔ { x: ⟁ \u{27E2} y: ⟁ }"
        );
    }

    #[test]
    fn tuple_type_prints_positional_fields() {
        assert_eq!(formatted("Pair ≔ (⟁, ⌘)"), "Pair ≔ (⟁, ⌘)");
    }

    #[test]
    fn import_and_exception_declarations_print_their_glyphs() {
        assert_eq!(formatted("⟲ Foo\n☈ Boom(⌘)"), "⟲Foo\n☈ Boom(⌘)");
    }
}
