//! Printing type annotations (`CONCRETE_SYMBOLIC_GRAMMAR.md` §9).

use crate::glyph::{self, ARROW, PIPE};
use crate::printer::Printer;
use obfusku_syntax::ast::TypeRef;

impl Printer {
    /// A type annotation: `→` is right-associative, `▷` (type application)
    /// is left-associative and binds tighter.
    pub(crate) fn type_ref(&mut self, ty: &TypeRef) {
        match ty {
            TypeRef::Base(base, _) => self.push_str(glyph::base_type(*base)),
            TypeRef::Named(name, _) | TypeRef::Var(name, _) => self.push_str(name),
            TypeRef::Function(parameter, result, _) => {
                // A function-typed parameter would otherwise absorb the arrow.
                self.type_group_if(matches!(**parameter, TypeRef::Function(..)), parameter);
                self.spaced(ARROW);
                self.type_ref(result);
            }
            TypeRef::Apply(constructor, argument, _) => {
                self.type_ref(constructor);
                self.spaced(PIPE);
                // `▷` is left-associative: `Optional ▷ List ▷ ⟁` means
                // `(Optional ▷ List) ▷ ⟁`, so an applied or function-typed
                // right operand must be grouped to keep its meaning.
                self.type_group_if(
                    matches!(**argument, TypeRef::Apply(..) | TypeRef::Function(..)),
                    argument,
                );
            }
        }
    }

    /// A comma-separated list of types, as in tuple bodies and variant fields.
    pub(crate) fn type_list(&mut self, types: &[TypeRef]) {
        self.separated(types, ", ", Printer::type_ref);
    }

    fn type_group_if(&mut self, should_group: bool, ty: &TypeRef) {
        if should_group {
            self.parenthesized(|p| p.type_ref(ty));
        } else {
            self.type_ref(ty);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::testing::format_source;

    /// The annotation of `x: <ty> ≔ …`, extracted from formatted output.
    fn annotation(ty: &str) -> String {
        let out = format_source(&format!("x: {ty} \u{2254} 0\n\u{2767}"));
        out.lines()
            .next()
            .and_then(|l| l.strip_prefix("x: "))
            .and_then(|l| l.strip_suffix(" \u{2254} 0"))
            .unwrap_or_default()
            .to_string()
    }

    #[test]
    fn base_types_use_their_glyphs() {
        assert_eq!(annotation("⟁"), "⟁");
    }

    #[test]
    fn function_types_are_right_associative_without_parentheses() {
        assert_eq!(annotation("⟁ → ⟁ → ⟁"), "⟁ → ⟁ → ⟁");
    }

    #[test]
    fn function_typed_parameter_is_parenthesized() {
        assert_eq!(annotation("(⟁ → ⟁) → ⟁"), "(⟁ → ⟁) → ⟁");
    }

    #[test]
    fn nested_application_on_the_right_is_grouped() {
        assert_eq!(annotation("Optional ▷ (List ▷ ⟁)"), "Optional ▷ (List ▷ ⟁)");
    }

    #[test]
    fn application_on_the_left_needs_no_parentheses() {
        assert_eq!(annotation("Pair ▷ ⟁ ▷ ⌘"), "Pair ▷ ⟁ ▷ ⌘");
    }
}
