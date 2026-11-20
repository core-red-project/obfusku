//! Placing comments back around the declarations they belonged to.
//!
//! The lexer sets comments aside as trivia and the surface AST has no slot for
//! them, so the formatter re-attaches each one by *position* in the original
//! source. The rules are few and local, to keep the result predictable:
//!
//! - a comment between declarations stays between them;
//! - a comment on the same line after a declaration stays on that line;
//! - a comment *inside* a declaration moves to just above it (the canonical
//!   layout may put the surrounding text on different lines, so "where inside"
//!   has no stable meaning);
//! - a blank line before a declaration, or before its leading comments, is
//!   kept as exactly one blank line.

use crate::glyph::END_OF_MODULE;
use crate::printer::Printer;
use obfusku_syntax::ast::Module;
use obfusku_syntax::lexer::Comment;

/// Prints `module` with `comments` re-attached; `source` is the text both
/// were produced from, used to read the gaps between them.
pub(crate) fn print_module(module: &Module, comments: &[Comment], source: &str) -> String {
    let mut layout = Layout {
        printer: Printer::default(),
        comments,
        source,
        next: 0,
        previous_end: 0,
    };
    for declaration in &module.declarations {
        let span = declaration.span();
        let (start, end) = (span.start as usize, span.end as usize);

        layout.leading_comments(end);
        layout.blank_line_before(start.max(layout.previous_end));
        layout.printer.declaration(declaration, 0);
        layout.trailing_comment(end);
        layout.printer.push('\n');
        layout.previous_end = end;
    }
    layout.remaining_comments();
    layout.printer.push(END_OF_MODULE);
    layout.printer.push('\n');
    layout.printer.finish()
}

/// The cursor over the comment list, plus the output built so far.
struct Layout<'a> {
    printer: Printer,
    comments: &'a [Comment],
    source: &'a str,
    /// Index of the first comment not yet emitted; comments are in source
    /// order, so everything before it is done.
    next: usize,
    /// Byte offset where the last emitted declaration or comment ended.
    previous_end: usize,
}

impl Layout<'_> {
    /// Emits one blank line when the source had at least one between
    /// `previous_end` and `to`, except at the very start of the output.
    fn blank_line_before(&mut self, to: usize) {
        if !self.printer.is_empty() && has_blank_line(self.source, self.previous_end, to) {
            self.printer.push('\n');
        }
    }

    /// Emits every comment that starts before `declaration_end`, each on its
    /// own line: those before the declaration, and those inside it.
    fn leading_comments(&mut self, declaration_end: usize) {
        let first = self.next;
        while self
            .comments
            .get(self.next)
            .is_some_and(|c| (c.span.start as usize) < declaration_end)
        {
            self.next += 1;
        }
        for comment in &self.comments[first..self.next] {
            if !self.printer.is_empty()
                && has_blank_line(self.source, self.previous_end, comment.span.start as usize)
            {
                self.printer.push('\n');
            }
            self.printer.push_str(&comment.text);
            self.printer.push('\n');
            self.previous_end = comment.span.end as usize;
        }
    }

    /// Keeps comments that sit on the declaration's last line on that line.
    fn trailing_comment(&mut self, declaration_end: usize) {
        while let Some(comment) = self.comments.get(self.next) {
            let from = comment.span.start as usize;
            let on_same_line = from >= declaration_end
                && self
                    .source
                    .get(declaration_end..from)
                    .is_some_and(|gap| !gap.contains('\n') && !comment.text.contains('\n'));
            if !on_same_line {
                break;
            }
            self.printer.push(' ');
            self.printer.push_str(&comment.text);
            self.next += 1;
        }
    }

    /// Comments after the last declaration, one per line.
    fn remaining_comments(&mut self) {
        for comment in &self.comments[self.next..] {
            self.printer.push_str(&comment.text);
            self.printer.push('\n');
        }
    }
}

/// Whether `source[from..to]` holds an empty line: two line breaks with only
/// whitespace between them.
fn has_blank_line(source: &str, from: usize, to: usize) -> bool {
    source.get(from..to).is_some_and(|gap| {
        let mut newlines = 0;
        for c in gap.chars() {
            match c {
                '\n' => newlines += 1,
                c if c.is_whitespace() => {}
                _ => newlines = 0,
            }
            if newlines >= 2 {
                return true;
            }
        }
        false
    })
}

#[cfg(test)]
mod tests {
    use super::has_blank_line;
    use crate::testing::format_source_with_comments as format;

    #[test]
    fn has_blank_line_needs_two_line_breaks_with_only_whitespace_between() {
        assert!(has_blank_line("a\n\nb", 1, 3));
        assert!(has_blank_line("a\n  \t\nb", 1, 6));
        assert!(!has_blank_line("a\nb", 1, 2));
        assert!(!has_blank_line("a\n b\n", 0, 5));
    }

    #[test]
    fn has_blank_line_on_an_invalid_range_is_false() {
        assert!(!has_blank_line("ab", 1, 99));
        assert!(!has_blank_line("é", 1, 2));
    }

    #[test]
    fn a_comment_between_declarations_stays_between_them() {
        assert_eq!(
            format("a ≔ 1\n// note\nb ≔ 2\n❧"),
            "a ≔ 1\n// note\nb ≔ 2\n❧\n"
        );
    }

    #[test]
    fn a_trailing_comment_stays_on_its_line() {
        assert_eq!(format("a ≔ 1 // one\nb ≔ 2\n❧"), "a ≔ 1 // one\nb ≔ 2\n❧\n");
    }

    #[test]
    fn a_comment_after_the_last_declaration_is_kept() {
        assert_eq!(format("a ≔ 1\n// end\n❧"), "a ≔ 1\n// end\n❧\n");
    }

    #[test]
    fn a_blank_line_is_kept_as_exactly_one() {
        assert_eq!(format("a ≔ 1\n\n\n\nb ≔ 2\n❧"), "a ≔ 1\n\nb ≔ 2\n❧\n");
    }

    #[test]
    fn no_blank_line_is_invented() {
        assert_eq!(format("a ≔ 1\nb ≔ 2\n❧"), "a ≔ 1\nb ≔ 2\n❧\n");
    }

    #[test]
    fn a_blank_line_before_a_leading_comment_is_kept() {
        assert_eq!(
            format("a ≔ 1\n\n// about b\nb ≔ 2\n❧"),
            "a ≔ 1\n\n// about b\nb ≔ 2\n❧\n"
        );
    }

    #[test]
    fn a_comment_inside_a_declaration_moves_above_it() {
        let out = format("a ≔ ⟡ 1 {\n  // arm\n  _ → 0\n}\n❧");
        assert!(out.starts_with("// arm\na ≔ ⟡ 1 {"), "{out}");
    }

    #[test]
    fn no_leading_blank_line_at_the_start_of_the_file() {
        assert_eq!(format("\n\n// top\na ≔ 1\n❧"), "// top\na ≔ 1\n❧\n");
    }

    #[test]
    fn an_empty_module_is_just_the_terminator() {
        assert_eq!(format("❧"), "❧\n");
    }
}
