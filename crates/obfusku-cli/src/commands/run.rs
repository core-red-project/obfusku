//! `run`, `check` and `build`: the three commands that take a program
//! through the whole pipeline and differ only in how far they go.

use super::report::{entry_file_of, pipeline_failure, print_warnings, usage_error};
use obfusku_cli::project::require_project_entry_point;
use obfusku_cli::report::render;
use obfusku_diagnostics::SourceMap;
use std::path::Path;
use std::process::ExitCode;

/// `run <file|project> [-- args]`. Warnings never fail a run: they go to
/// stderr before the value, so stdout stays exactly the program's result.
pub(super) fn run(path: &Path, program_args: &[String]) -> ExitCode {
    match obfusku_cli::run_file_with_args(path, program_args) {
        Ok((value, warnings)) => {
            print_warnings(path, &warnings);
            println!("{}", obfusku_runtime::show(&value.0));
            ExitCode::SUCCESS
        }
        Err(failure) => pipeline_failure(failure),
    }
}

/// `check <file|project> [--json]`: type-check without executing.
///
/// With `--json` the outcome is one machine-readable object on stdout in
/// both cases, so a tool never has to parse stderr.
pub(super) fn check(path: &Path, json: bool) -> ExitCode {
    match obfusku_cli::check_file(path) {
        Ok(warnings) if json => {
            // Warnings can only point into the entry file (see
            // `print_warnings`), so a one-file map is enough.
            let mut map = SourceMap::new();
            map.add_file(std::fs::read_to_string(entry_file_of(path)).unwrap_or_default());
            println!("{}", render::json(&map, true, &warnings));
            ExitCode::SUCCESS
        }
        Ok(warnings) => {
            print_warnings(path, &warnings);
            println!("ok");
            ExitCode::SUCCESS
        }
        Err((map, diagnostics)) if json => {
            println!("{}", render::json(&map, false, &diagnostics));
            ExitCode::from(1)
        }
        Err(failure) => pipeline_failure(failure),
    }
}

/// `build <project>`: certifies a Project as a distributable Source
/// Artifact under the 1.0 model; it copies, packages and writes nothing.
///
/// "Not a Project at all" is checked up front so it gets its own exit code
/// (`2`), distinct from a Project whose sources fail to resolve, parse or
/// type-check (`1`).
pub(super) fn build(path: &Path) -> ExitCode {
    if let Err(d) = require_project_entry_point(path) {
        return usage_error(d);
    }
    match obfusku_cli::build_project(path) {
        Ok(warnings) => {
            print_warnings(path, &warnings);
            println!(
                "Project at '{}' is a valid Source Artifact.",
                path.display()
            );
            ExitCode::SUCCESS
        }
        Err(failure) => pipeline_failure(failure),
    }
}
