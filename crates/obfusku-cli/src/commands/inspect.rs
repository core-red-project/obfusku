//! `inspect`: shows an intermediate representation of a program.

use super::args::read_entry;
use obfusku_cli::{InspectKind, inspect_source, render_diagnostics};
use std::process::ExitCode;

/// `inspect <file|project> --tokens|--ast|--core`.
///
/// The mode flag and the path may come in either order — exactly one
/// argument starts with `--` and whatever remains is the path, so there is
/// nothing ambiguous to resolve. Without a flag the Core AST is shown.
pub(super) fn run(args: &[String]) -> ExitCode {
    let mut kind: Option<InspectKind> = None;
    let mut path: Option<&str> = None;
    for a in &args[2.min(args.len())..] {
        match a.as_str() {
            "--tokens" if kind.is_none() => kind = Some(InspectKind::Tokens),
            "--ast" if kind.is_none() => kind = Some(InspectKind::Ast),
            "--core" if kind.is_none() => kind = Some(InspectKind::Core),
            other if other.starts_with("--") => {
                eprintln!("unknown inspect flag: {other} (expected --tokens, --ast, or --core)");
                return ExitCode::from(2);
            }
            other if path.is_none() => path = Some(other),
            other => {
                eprintln!("unexpected extra argument: {other}");
                return ExitCode::from(2);
            }
        }
    }
    let Some(path) = path else {
        eprintln!("expected a file path, e.g. 'obfusku inspect program.obk --ast'");
        return ExitCode::from(2);
    };
    let source = match read_entry(path) {
        Ok(source) => source,
        Err(code) => return code,
    };
    match inspect_source(&source, kind.unwrap_or(InspectKind::Core)) {
        Ok(repr) => {
            println!("{repr}");
            ExitCode::SUCCESS
        }
        Err(diagnostics) => {
            eprintln!("{}", render_diagnostics(&source, &diagnostics));
            ExitCode::from(1)
        }
    }
}
