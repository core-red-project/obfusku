//! Telling the user what went wrong.
//!
//! Two halves, kept apart because they have different callers:
//!
//! - `render` turns the pipeline's [`Diagnostic`](obfusku_diagnostics::Diagnostic)s
//!   into text or JSON — presentation, which the pipeline stages themselves
//!   know nothing about;
//! - `host_error` builds the span-less diagnostics the *host* raises for
//!   things outside any program text (an unreadable file, a bad manifest).

pub(crate) mod host_error;
pub mod render;
