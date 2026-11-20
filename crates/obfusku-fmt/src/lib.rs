//! The canonical source formatter.
//!
//! One fixed style, not configurable: `IMPLEMENTATION_ARCHITECTURE.md` §13
//! mandates the architecture, not the exact spacing, so a single layout is
//! chosen here and every program converges to it. Formatting twice gives the
//! same text as formatting once.
//!
//! **Why it works on the surface AST.** Desugaring is lossy (`⟲`, local
//! functions and pipes all disappear), so printing the Core AST could not
//! reproduce what the author wrote. The formatter consumes the parser's
//! output instead, and depends on nothing past `obfusku-syntax`: no checker,
//! no runtime. Ill-typed or unfinished code therefore still formats, which is
//! when an editor needs it most.
//!
//! **Correctness contract.** Re-parsing `format(m)` must give a structurally
//! equivalent module, and running it must give the same result
//! (`obfusku-cli/tests/formatter`). Where bare printing could be read two
//! ways, the printer adds parentheses: an extra pair is harmless, a wrong
//! re-parse silently changes the program.
//!
//! **Comments** are trivia the lexer discards, and the AST has no place for
//! them. [`format_with_comments`] re-attaches them by source position; plain
//! [`format()`] drops them.
//!
//! # Map
//!
//! | Module | Prints |
//! |---|---|
//! | `comments` | the module as a whole, with comments re-attached |
//! | `declaration` | values, functions, types, imports, exceptions |
//! | `expression` | every expression form, with minimal parentheses |
//! | `pattern` | match patterns |
//! | `type_ref` | type annotations |
//! | `literal` | integers, reals and strings that re-lex to the same value |
//! | `precedence` | the ladder deciding when an operand needs parentheses |
//! | `glyph` | the language's glyphs, named once |
//! | `printer` | the output buffer, indentation depth and list helpers |

#![warn(missing_docs)]

mod comments;
mod declaration;
mod expression;
mod glyph;
mod literal;
mod pattern;
mod precedence;
mod printer;
mod type_ref;

use obfusku_syntax::ast::Module;
use obfusku_syntax::lexer::Comment;

/// Formats a surface module as canonical source text, without comments.
///
/// Use [`format_with_comments`] to keep them.
#[must_use]
pub fn format(module: &Module) -> String {
    format_with_comments(module, &[], "")
}

/// Formats a surface module and keeps every comment of `source`.
///
/// `comments` are those the lexer returned for `source`, in order. A comment
/// between declarations stays between them; one on the same line after a
/// declaration stays on that line; one inside a declaration moves to just
/// above it. A blank line before a declaration (or its leading comments) is
/// kept as one blank line.
#[must_use]
pub fn format_with_comments(module: &Module, comments: &[Comment], source: &str) -> String {
    comments::print_module(module, comments, source)
}

/// Parsing helpers shared by the modules' unit tests.
#[cfg(test)]
mod testing {
    use obfusku_diagnostics::SourceId;
    use obfusku_syntax::{lexer, parser};

    /// Parses and formats `source`, dropping comments.
    pub(crate) fn format_source(source: &str) -> String {
        let id = SourceId(0);
        let tokens = lexer::tokenize(source, id).unwrap_or_else(|d| panic!("lex: {d:?}"));
        let module = parser::parse(&tokens, id).unwrap_or_else(|d| panic!("parse: {d:?}"));
        crate::format(&module)
    }

    /// Parses and formats `source`, keeping comments.
    pub(crate) fn format_source_with_comments(source: &str) -> String {
        let id = SourceId(0);
        let (tokens, comments) =
            lexer::tokenize_with_comments(source, id).unwrap_or_else(|d| panic!("lex: {d:?}"));
        let module = parser::parse(&tokens, id).unwrap_or_else(|d| panic!("parse: {d:?}"));
        crate::format_with_comments(&module, &comments, source)
    }
}

#[cfg(test)]
mod tests {
    use super::testing::format_source;

    #[test]
    fn formatting_is_idempotent() {
        let source = "x ≔ ⟡ 1 { 1 → ◉ ⟢ _ → ◎ }\nλf(a: ⟁): ⟁ → a ✚ 1\n❧";
        let once = format_source(source);
        assert_eq!(format_source(&once), once);
    }

    #[test]
    fn every_module_ends_with_the_terminator_and_a_newline() {
        assert!(format_source("x ≔ 1\n❧").ends_with("\n❧\n"));
    }

    #[test]
    fn declarations_are_printed_one_per_line() {
        assert_eq!(format_source("a≔1 b≔2 ❧"), "a ≔ 1\nb ≔ 2\n❧\n");
    }

    #[test]
    fn plain_format_drops_comments() {
        assert_eq!(format_source("a ≔ 1 // gone\n❧"), "a ≔ 1\n❧\n");
    }
}
