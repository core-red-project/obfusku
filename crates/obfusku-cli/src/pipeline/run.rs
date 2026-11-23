//! Evaluating a checked entry module and reporting how it ended.

use super::RunResult;
use super::check::{Failure, check_entry};
use super::host::Host;
use crate::project::deps::Packages;
use crate::project::entry::EntryPoint;
use crate::project::manifest::Capabilities;
use crate::project::modules::ModuleSource;
use obfusku_diagnostics::Diagnostic;
use obfusku_runtime::Value;
use std::path::Path;

/// Runs a bundle (`project::bundle::run_embedded`): modules come from
/// `sources` under `project_root`, and the filesystem natives are rooted at
/// `fs_root` — the caller's directory, not the bundle's virtual one.
pub(crate) fn run_bundled(
    project_root: &Path,
    entry_file: &Path,
    fs_root: &Path,
    args: &[String],
    sources: ModuleSource,
    capabilities: Option<Capabilities>,
    packages: Packages,
) -> Result<(RunResult, Vec<Diagnostic>), Failure> {
    run_entry(
        Ok(EntryPoint {
            project_root: project_root.to_path_buf(),
            entry_file: entry_file.to_path_buf(),
        }),
        &Host {
            args,
            fs_root: Some(fs_root),
            sources,
            capabilities,
            packages: Some(packages),
        },
    )
}

/// Checks `entry`, then evaluates it. `run`'s reported value is the last
/// top-level binding (`ADR-004`), whether or not it is exported.
pub(super) fn run_entry(
    entry: Result<EntryPoint, Diagnostic>,
    host: &Host,
) -> Result<(RunResult, Vec<Diagnostic>), Failure> {
    let checked = check_entry(entry, host)?;
    match obfusku_runtime::evaluate_with_prelude_traced(&checked.core_module, checked.values) {
        Ok(bindings) => {
            let last = bindings
                .into_iter()
                .last()
                .map_or(Value::Unit, |(_, _, v)| v);
            Ok((RunResult(last), checked.warnings))
        }
        Err(fault) => {
            let mut diagnostics = vec![fault.diagnostic];
            diagnostics.extend(trace_notes(&fault.trace));
            Err((checked.source_map, diagnostics))
        }
    }
}

/// A runtime trace as notes anchored at each call site, innermost first, so
/// the renderer resolves `line:col` like for any other diagnostic. Runs of
/// the same call (deep recursion) collapse into one note with a count,
/// otherwise a stack overflow would print a hundred thousand lines.
fn trace_notes(trace: &[obfusku_runtime::eval::TraceFrame]) -> Vec<Diagnostic> {
    let mut notes = Vec::new();
    let mut frames = trace.iter().peekable();
    while let Some(frame) = frames.next() {
        let mut times = 1;
        while frames
            .next_if(|n| n.function == frame.function && n.call_site == frame.call_site)
            .is_some()
        {
            times += 1;
        }
        let repeat = if times > 1 {
            format!(" ({times} times in a row)")
        } else {
            String::new()
        };
        notes.push(Diagnostic::note(
            format!("called '{}' from here{repeat}", frame.function),
            frame.call_site,
        ));
    }
    notes
}
