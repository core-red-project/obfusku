//! Obfusku CLI — orchestration and presentation only, no language logic
//! of its own (§12). Pipeline orchestration lives in `src/lib.rs`
//! (`run_source`/`check_source`/`fmt_source`/`inspect_source`).
//!
//! Exit codes (not spec-mandated, this crate's own convention): `0` on
//! success, `1` when the pipeline reports diagnostics, `2` on a CLI
//! usage error (bad arguments, unreadable file).

use obfusku_cli::{fmt_source, render_diagnostics, InspectKind};
use std::io::Write;
use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();

    // `-h`/`--help` and `-V`/`--version` short-circuit from *any*
    // position, not only immediately after the binary name — matching
    // standard CLI convention. Checked once up front before subcommand
    // dispatch.
    if args.iter().skip(1).any(|a| a == "--help" || a == "-h") {
        println!("{}", USAGE);
        return ExitCode::SUCCESS;
    }
    if args.iter().skip(1).any(|a| a == "--version" || a == "-V") {
        println!("obfusku {}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }

    match args.get(1).map(String::as_str) {
        Some("version") => {
            println!("obfusku {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Some("run") => with_path_arg(&args, cmd_run),
        Some("check") => with_path_arg(&args, cmd_check),
        Some("fmt") => with_file_arg(&args, cmd_fmt),
        Some("inspect") => cmd_inspect(&args),
        Some("repl") => {
            repl();
            ExitCode::SUCCESS
        }
        Some("build") | Some("test") => {
            eprintln!(
                "'{}' is not yet supported: it depends on the artifact/module distribution \
                 model, which spec/LANGUAGE_SPEC.md §5 leaves undecided — not invented here",
                args[1]
            );
            ExitCode::from(2)
        }
        Some(cmd) => {
            eprintln!("unknown command: {cmd}");
            print_usage();
            ExitCode::from(2)
        }
        None => {
            print_usage();
            ExitCode::from(2)
        }
    }
}

const USAGE: &str = "usage: obfusku <command> [args]\n\
     commands:\n  \
     run <file>       run a program\n  \
     check <file>     type-check without executing\n  \
     fmt <file>       print the canonically formatted source\n  \
     inspect <file> --tokens|--ast|--core\n  \
     repl             interactive read-eval-print loop\n  \
     version          print the CLI version\n\n\
     options:\n  \
     -h, --help       print this usage text and exit\n  \
     -V, --version    print the CLI version and exit (same as 'version')";

fn print_usage() {
    eprintln!("{}", USAGE);
}

/// Reads `args[2]` as a file path, reports a usage error if missing or
/// unreadable, and hands the source text to `cmd`.
fn with_file_arg(args: &[String], cmd: fn(&str) -> ExitCode) -> ExitCode {
    let Some(path) = args.get(2) else {
        eprintln!(
            "expected a file path, e.g. 'obfusku {} program.obk'",
            args[1]
        );
        return ExitCode::from(2);
    };
    match std::fs::read_to_string(path) {
        Ok(source) => cmd(&source),
        Err(e) => {
            eprintln!("could not read '{path}': {e}");
            ExitCode::from(2)
        }
    }
}

/// Reads `args[2]` as a file path and hands the `Path` itself (not just
/// its text) to `cmd` — `run`/`check` need the path to resolve
/// same-directory `ImportDeclaration`s against.
fn with_path_arg(args: &[String], cmd: fn(&Path) -> ExitCode) -> ExitCode {
    let Some(path) = args.get(2) else {
        eprintln!(
            "expected a file path, e.g. 'obfusku {} program.obk'",
            args[1]
        );
        return ExitCode::from(2);
    };
    let path = Path::new(path);
    if let Err(e) = std::fs::metadata(path) {
        eprintln!("could not read '{}': {e}", path.display());
        return ExitCode::from(2);
    }
    cmd(path)
}

fn cmd_run(path: &Path) -> ExitCode {
    match obfusku_cli::run_file(path) {
        Ok((value, warnings)) => {
            if !warnings.is_empty() {
                // Warnings never fail `run` — printed to stderr, before
                // the executed value (stdout stays just the result).
                eprintln!("{}", render_warnings(path, &warnings));
            }
            println!("{:?}", value.0);
            ExitCode::SUCCESS
        }
        Err((map, diags)) => {
            eprintln!("{}", obfusku_cli::render_diagnostics_map(&map, &diags));
            ExitCode::from(1)
        }
    }
}

fn cmd_check(path: &Path) -> ExitCode {
    match obfusku_cli::check_file(path) {
        Ok(warnings) => {
            if !warnings.is_empty() {
                eprintln!("{}", render_warnings(path, &warnings));
            }
            println!("ok");
            ExitCode::SUCCESS
        }
        Err((map, diags)) => {
            eprintln!("{}", obfusku_cli::render_diagnostics_map(&map, &diags));
            ExitCode::from(1)
        }
    }
}

/// Renders `run`/`check`'s non-fatal warnings against `path`'s own
/// text — a fresh `SourceMap` built just for this, since a warning's
/// span always belongs to `path` itself (unreachable-arm detection
/// never looks at an imported module's own body).
fn render_warnings(path: &Path, warnings: &[obfusku_diagnostics::Diagnostic]) -> String {
    let source = std::fs::read_to_string(path).unwrap_or_default();
    obfusku_cli::render_diagnostics(&source, warnings)
}

fn cmd_fmt(source: &str) -> ExitCode {
    match fmt_source(source) {
        Ok(formatted) => {
            print!("{formatted}");
            ExitCode::SUCCESS
        }
        Err(diags) => {
            eprintln!("{}", render_diagnostics(source, &diags));
            ExitCode::from(1)
        }
    }
}

/// Accepts the mode flag (`--tokens`/`--ast`/`--core`) and the file path
/// in either order — `inspect --ast file.obk` and
/// `inspect file.obk --ast` are both valid — since there's no ambiguity
/// to resolve: exactly one of `args[2..]` starts with `--`, and whatever
/// remains is the path.
fn cmd_inspect(args: &[String]) -> ExitCode {
    let rest = &args[2.min(args.len())..];
    let mut path: Option<&str> = None;
    let mut kind: Option<InspectKind> = None;
    for a in rest {
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
    let kind = kind.unwrap_or(InspectKind::Core);
    let source = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("could not read '{path}': {e}");
            return ExitCode::from(2);
        }
    };
    match obfusku_cli::inspect_source(&source, kind) {
        Ok(repr) => {
            println!("{repr}");
            ExitCode::SUCCESS
        }
        Err(diags) => {
            eprintln!("{}", render_diagnostics(&source, &diags));
            ExitCode::from(1)
        }
    }
}

/// Interactive loop, driven by [`obfusku_cli::ReplSession`]. Thin wrapper
/// reading lines, displaying prompts, and printing session output.
fn repl() {
    println!("obfusku repl — ':quit' or Ctrl-D to exit");
    let mut session = match obfusku_cli::ReplSession::new() {
        Ok(s) => s,
        Err(diags) => {
            // Only reachable if the bundled stdlib itself failed to
            // load — not a user-reachable condition, but still reported
            // rather than silently aborting.
            eprintln!("{}", render_diagnostics("", &diags));
            return;
        }
    };
    let stdin = std::io::stdin();
    loop {
        print!("> ");
        let _ = std::io::stdout().flush();
        let mut line = String::new();
        if stdin.read_line(&mut line).unwrap_or(0) == 0 {
            println!();
            break;
        }
        let line = line.trim_end();
        if line == ":quit" {
            break;
        }
        if line.is_empty() {
            continue;
        }
        match session.submit(line) {
            Ok((value, warnings)) => {
                if !warnings.is_empty() {
                    eprintln!("{}", session.render(&warnings));
                }
                println!("{:?}", value);
            }
            Err(diags) => {
                eprintln!("{}", session.render(&diags));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    /// Smoke test: the CLI links against every implementation crate and
    /// the dependency graph compiles end to end.
    #[test]
    fn links_against_all_implementation_crates() {
        let core_module = obfusku_core::ast::Module::default();
        assert!(obfusku_typecheck::check(&core_module).is_ok());
        assert!(obfusku_runtime::evaluate(&core_module).is_ok());

        let mut source_map = obfusku_diagnostics::SourceMap::new();
        let source_id = source_map.add_file("❧");
        let tokens = obfusku_syntax::lexer::tokenize("❧", source_id)
            .expect("lexing the smallest legal program must succeed");
        let parsed = obfusku_syntax::parser::parse(&tokens, source_id)
            .expect("parsing the smallest legal program must succeed");
        let _desugared = obfusku_syntax::desugar::desugar(&parsed)
            .expect("desugaring the smallest legal program must succeed");

        let _ = obfusku_fmt::format(&parsed);
    }
}
