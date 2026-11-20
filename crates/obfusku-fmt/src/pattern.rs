//! Printing match patterns (`CONCRETE_SYMBOLIC_GRAMMAR.md` §11).

use crate::glyph::{FALSE, TRUE};
use crate::printer::Printer;
use obfusku_syntax::ast::Pattern;

impl Printer {
    /// A pattern. Patterns are self-delimiting, so none ever needs
    /// parentheses.
    pub(crate) fn pattern(&mut self, pattern: &Pattern) {
        match pattern {
            Pattern::Var(name, _) => self.push_str(name),
            Pattern::Wildcard(_) => self.push('_'),
            Pattern::Int(value, _) => self.int(*value),
            Pattern::Real(value, _) => self.real(*value),
            Pattern::Str(value, _) => self.string_literal(value),
            Pattern::Bool(value, _) => self.push(if *value { TRUE } else { FALSE }),
            Pattern::Constructor { tag, args, .. } => {
                self.push_str(tag);
                if !args.is_empty() {
                    self.parenthesized(|p| p.separated(args, ", ", Printer::pattern));
                }
            }
            Pattern::NamedConstructor { tag, fields, .. } => {
                self.push_str(tag);
                self.push(' ');
                self.braced_fields(
                    fields.iter().map(|(name, pattern)| (name, pattern)),
                    Printer::pattern,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::testing::format_source;

    /// The arm patterns of `⟡ x { … }`, one per line.
    fn arms(patterns: &[&str]) -> String {
        let body = patterns
            .iter()
            .map(|p| format!("{p} → 0"))
            .collect::<Vec<_>>()
            .join(" ⟢\n");
        let out = format_source(&format!("v ≔ ⟡ 1 {{\n{body}\n}}\n❧"));
        out.lines()
            .filter_map(|l| {
                l.trim()
                    .strip_suffix(" → 0")
                    .or(l.trim().strip_suffix(" → 0 ⟢"))
            })
            .collect::<Vec<_>>()
            .join("|")
    }

    #[test]
    fn literals_variables_and_wildcards_print_plainly() {
        assert_eq!(arms(&["1", "_"]), "1|_");
    }

    #[test]
    fn booleans_use_their_glyphs() {
        assert_eq!(arms(&["◉", "◎"]), "◉|◎");
    }

    #[test]
    fn constructor_patterns_print_their_arguments() {
        assert_eq!(arms(&["Pair(a, b)", "Nil"]), "Pair(a, b)|Nil");
    }

    #[test]
    fn named_constructor_patterns_print_their_fields() {
        assert_eq!(
            arms(&["Point { x: a ⟢ y: _ }"]),
            "Point { x: a \u{27E2} y: _ }"
        );
    }

    #[test]
    fn nested_patterns_print_recursively() {
        assert_eq!(arms(&["Some(Pair(1, _))"]), "Some(Pair(1, _))");
    }
}
