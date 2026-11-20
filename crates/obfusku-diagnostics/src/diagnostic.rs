//! What a stage reports: [`Severity`] and [`Diagnostic`].

use crate::span::Span;
use std::fmt;

/// How serious a [`Diagnostic`] is.
///
/// Only [`Error`](Severity::Error) stops a pipeline stage; the others ride
/// alongside a successful result. `Note` also carries information that is not
/// a fault at all, such as the trace appended to a runtime failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Severity {
    /// The program is rejected.
    Error,
    /// The program is accepted but likely not what was meant.
    Warning,
    /// Context for another diagnostic, or information with no fault.
    Note,
}

impl Severity {
    /// The lowercase word every renderer prints for this severity.
    ///
    /// One definition on purpose: the text renderer, the JSON report and the
    /// language server must agree on this vocabulary, and a hand-written
    /// `match` in each would let them drift.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Note => "note",
        }
    }
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// One message anchored to a primary [`Span`].
///
/// This is the *only* error type that crosses a crate boundary. Each stage
/// (lexer, parser, desugarer, checker, evaluator) is free to model its
/// failures as a rich enum internally, then converts to this flat shape at its
/// public edge (`IMPLEMENTATION_ARCHITECTURE.md` §17). That keeps every
/// consumer (terminal, JSON, LSP) rendering one type and no stage depending on
/// another's error vocabulary.
///
/// The fields are public: a diagnostic is plain data with no invariant to
/// protect, and the constructors below are shorthand for the common case.
#[derive(Debug, Clone)]
pub struct Diagnostic {
    /// How serious the report is.
    pub severity: Severity,
    /// The human-readable text, without severity or position.
    pub message: String,
    /// Where it applies; the position a renderer reports.
    pub primary: Span,
}

impl Diagnostic {
    /// A diagnostic of any `severity`.
    #[must_use]
    pub fn new(severity: Severity, message: impl Into<String>, primary: Span) -> Self {
        Self {
            severity,
            message: message.into(),
            primary,
        }
    }

    /// An [`Error`](Severity::Error) at `primary`.
    #[must_use]
    pub fn error(message: impl Into<String>, primary: Span) -> Self {
        Self::new(Severity::Error, message, primary)
    }

    /// A [`Warning`](Severity::Warning) at `primary`.
    #[must_use]
    pub fn warning(message: impl Into<String>, primary: Span) -> Self {
        Self::new(Severity::Warning, message, primary)
    }

    /// A [`Note`](Severity::Note) at `primary`.
    #[must_use]
    pub fn note(message: impl Into<String>, primary: Span) -> Self {
        Self::new(Severity::Note, message, primary)
    }

    /// Whether this diagnostic rejects the program.
    #[must_use]
    pub const fn is_error(&self) -> bool {
        matches!(self.severity, Severity::Error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::span::SourceId;

    const AT: Span = Span::new(SourceId(0), 1, 2);

    #[test]
    fn labels_are_the_lowercase_words() {
        assert_eq!(Severity::Error.label(), "error");
        assert_eq!(Severity::Warning.label(), "warning");
        assert_eq!(Severity::Note.label(), "note");
    }

    #[test]
    fn display_matches_label() {
        for severity in [Severity::Error, Severity::Warning, Severity::Note] {
            assert_eq!(severity.to_string(), severity.label());
        }
    }

    #[test]
    fn constructors_set_the_severity() {
        assert_eq!(Diagnostic::error("e", AT).severity, Severity::Error);
        assert_eq!(Diagnostic::warning("w", AT).severity, Severity::Warning);
        assert_eq!(Diagnostic::note("n", AT).severity, Severity::Note);
    }

    #[test]
    fn constructors_keep_message_and_span() {
        let d = Diagnostic::new(Severity::Note, String::from("hello"), AT);
        assert_eq!(d.message, "hello");
        assert_eq!(d.primary, AT);
    }

    #[test]
    fn only_errors_reject() {
        assert!(Diagnostic::error("", AT).is_error());
        assert!(!Diagnostic::warning("", AT).is_error());
        assert!(!Diagnostic::note("", AT).is_error());
    }
}
