//! Turning outcomes into terminal output and exit codes.
//!
//! The three outcomes every command distinguishes: a *usage* problem (exit
//! `2`: bad arguments, no such Project), a *program* problem found by the
//! pipeline (exit `1`), and success (`0`). Centralized so each command picks
//! the outcome and this module owns how it looks.

use obfusku_cli::{Failure, render_diagnostics, render_diagnostics_map};
use obfusku_diagnostics::Diagnostic;
use std::path::Path;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};

/// `--quiet` silences warnings; errors and program output are unaffected.
static QUIET: AtomicBool = AtomicBool::new(false);

pub(super) fn set_quiet(is_quiet: bool) {
    QUIET.store(is_quiet, Ordering::Relaxed);
}

/// A host-level error (no source span): printed, exit `2`.
pub(super) fn usage_error(d: Diagnostic) -> ExitCode {
    eprintln!("{}", render_diagnostics("", &[d]));
    ExitCode::from(2)
}

/// A host-level error that is not a usage mistake: printed, exit `1`.
pub(super) fn host_failure(d: Diagnostic) -> ExitCode {
    eprintln!("{}", render_diagnostics("", &[d]));
    ExitCode::from(1)
}

/// A pipeline failure: rendered against its own `SourceMap`, exit `1`.
pub(super) fn pipeline_failure((map, diagnostics): Failure) -> ExitCode {
    eprintln!("{}", render_diagnostics_map(&map, &diagnostics));
    ExitCode::from(1)
}

/// The entry file of `path`, or `path` itself when it cannot be resolved.
pub(super) fn entry_file_of(path: &Path) -> std::path::PathBuf {
    obfusku_cli::project::resolve_entry_point(path)
        .map_or_else(|_| path.to_path_buf(), |e| e.entry_file)
}

/// Prints non-fatal diagnostics to stderr unless `--quiet`.
///
/// A warning's span always belongs to the entry file itself (unreachable-arm
/// detection never looks inside an imported module), so a fresh single-file
/// map built from that text renders it correctly.
pub(super) fn print_warnings(path: &Path, warnings: &[Diagnostic]) {
    if warnings.is_empty() || QUIET.load(Ordering::Relaxed) {
        return;
    }
    let source = std::fs::read_to_string(entry_file_of(path)).unwrap_or_default();
    eprintln!("{}", render_diagnostics(&source, warnings));
}
