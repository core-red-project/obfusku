//! Presentation of diagnostics: human text and machine JSON.
//!
//! Pipeline stages return [`Diagnostic`]s and know nothing about rendering
//! (`IMPLEMENTATION_ARCHITECTURE.md` §12): turning a span into `line:col`
//! needs the [`SourceMap`] every file was registered in, and that is the
//! caller's concern. A diagnostic can originate in an *imported* file, so
//! rendering always takes the map the pipeline returned, never just the
//! entry text.

use obfusku_diagnostics::{Diagnostic, SourceMap};
use std::fmt::Write as _;

/// Renders every diagnostic against a fresh [`SourceMap`] holding only
/// `source`. Suitable when the diagnostics cannot reference another file.
#[must_use]
pub fn diagnostics(source: &str, diagnostics: &[Diagnostic]) -> String {
    let mut map = SourceMap::new();
    map.add_file(source);
    diagnostics_map(&map, diagnostics)
}

/// Renders every diagnostic against an already-built [`SourceMap`] (the one
/// a pipeline call returned, which may hold several files).
#[must_use]
pub fn diagnostics_map(map: &SourceMap, diagnostics: &[Diagnostic]) -> String {
    diagnostics
        .iter()
        .map(|d| map.render(d))
        .collect::<Vec<_>>()
        .join("\n")
}

fn escape_json(text: &str, out: &mut String) {
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if u32::from(c) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", u32::from(c));
            }
            c => out.push(c),
        }
    }
}

/// Hand-written rather than via a JSON library: the shape is one flat object
/// and the CLI should not take a dependency for it.
///
/// `{"ok":…,"diagnostics":[{"severity","message","source","line","column",
/// "end_line","end_column"}]}` on one line; `source` is the id of the file
/// the span belongs to, positions are 1-based and the end is exclusive.
#[must_use]
pub fn json(map: &SourceMap, ok: bool, diagnostics: &[Diagnostic]) -> String {
    let mut out = format!("{{\"ok\":{ok},\"diagnostics\":[");
    for (i, d) in diagnostics.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let severity = d.severity.label();
        let (at, end) = map.span_location(d.primary);
        let _ = write!(out, "{{\"severity\":\"{severity}\",\"message\":\"");
        escape_json(&d.message, &mut out);
        let _ = write!(
            out,
            "\",\"source\":{},\"line\":{},\"column\":{},\"end_line\":{},\"end_column\":{}}}",
            d.primary.source.0, at.line, at.column, end.line, end.column
        );
    }
    out.push_str("]}");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use obfusku_diagnostics::{Severity, Span};

    #[test]
    fn json_escapes_and_locates() {
        let mut map = SourceMap::new();
        let id = map.add_file("ab\ncd");
        let d = Diagnostic {
            severity: Severity::Error,
            message: "say \"hi\"\n".to_string(),
            primary: Span {
                source: id,
                start: 4,
                end: 5,
            },
        };
        assert_eq!(
            json(&map, false, &[d]),
            "{\"ok\":false,\"diagnostics\":[{\"severity\":\"error\",\
             \"message\":\"say \\\"hi\\\"\\n\",\"source\":0,\"line\":2,\"column\":2,\
             \"end_line\":2,\"end_column\":3}]}"
        );
    }
}
