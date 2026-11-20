//! `fmt`: the canonical form of a source file (`ADR-009`).

use obfusku_cli::{fmt_source, render_diagnostics};
use std::process::ExitCode;

/// `fmt [--check] <file|project>`: prints the canonical form. With
/// `--check` it prints nothing and exits `1` when the file differs from it,
/// for use in CI and hooks.
pub(super) fn print(source: &str, check: bool) -> ExitCode {
    match fmt_source(source) {
        Ok(formatted) if check => {
            if formatted == source {
                ExitCode::SUCCESS
            } else {
                eprintln!(
                    "error: the file is not formatted; run 'obfusku fmt' to see the canonical form"
                );
                ExitCode::from(1)
            }
        }
        Ok(formatted) => {
            print!("{formatted}");
            ExitCode::SUCCESS
        }
        Err(diagnostics) => {
            eprintln!("{}", render_diagnostics(source, &diagnostics));
            ExitCode::from(1)
        }
    }
}

/// `fmt --write <file>`: rewrites the file in canonical form, and only when
/// it differs, so an already-formatted file keeps its modification time.
pub(super) fn write(args: &[String]) -> ExitCode {
    let Some(path) = args.get(2) else {
        eprintln!("expected a file path, e.g. 'obfusku fmt --write program.obk'");
        return ExitCode::from(2);
    };
    let source = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("could not read '{path}': {e}");
            return ExitCode::from(2);
        }
    };
    match fmt_source(&source) {
        Ok(formatted) => {
            if formatted != source
                && let Err(e) = std::fs::write(path, formatted)
            {
                eprintln!("could not write '{path}': {e}");
                return ExitCode::from(2);
            }
            ExitCode::SUCCESS
        }
        Err(diagnostics) => {
            eprintln!("{}", render_diagnostics(&source, &diagnostics));
            ExitCode::from(1)
        }
    }
}
