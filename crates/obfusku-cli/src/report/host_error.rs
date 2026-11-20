//! Errors raised by the host, outside any program text.
//!
//! Source diagnostics carry a span into a `SourceMap`; a missing file or a
//! malformed manifest has none. They share one constructor so every such
//! failure renders alike and the span-less convention (`Span::default()`)
//! lives in one place.

use obfusku_diagnostics::{Diagnostic, Severity, Span};
use std::path::Path;

/// An error diagnostic with no source location.
pub(crate) fn host_error(message: impl Into<String>) -> Diagnostic {
    Diagnostic {
        severity: Severity::Error,
        message: message.into(),
        primary: Span::default(),
    }
}

/// `could not read '<path>': <cause>` — the one phrasing every read
/// failure uses, so tests and users can rely on it.
pub(crate) fn read_error(path: &Path, cause: &std::io::Error) -> Diagnostic {
    host_error(format!("could not read '{}': {cause}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use obfusku_diagnostics::{Severity, Span};

    #[test]
    fn a_host_error_is_a_span_less_error() {
        let d = host_error("boom");
        assert_eq!(d.severity, Severity::Error);
        assert_eq!(d.message, "boom");
        assert_eq!(d.primary, Span::default());
    }

    #[test]
    fn a_read_error_names_the_path_and_the_cause() {
        let d = read_error(Path::new("a/b.obk"), &std::io::Error::other("denied"));
        assert_eq!(d.message, "could not read 'a/b.obk': denied");
    }
}
