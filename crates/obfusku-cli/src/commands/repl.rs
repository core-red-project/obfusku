//! `repl`: the terminal front end of [`obfusku_cli::ReplSession`].

use obfusku_cli::{ReplSession, render_diagnostics};
use std::io::Write;
use std::process::ExitCode;

/// Reads lines, prompts, and prints each result. All language behavior is in
/// the session; this loop is only the terminal.
pub(super) fn run() -> ExitCode {
    println!("obfusku repl — ':quit' or Ctrl-D to exit");
    let mut session = match ReplSession::new() {
        Ok(s) => s,
        Err(diagnostics) => {
            // Reachable only if the bundled stdlib itself failed to load —
            // not a user condition, but reported rather than silently dropped.
            eprintln!("{}", render_diagnostics("", &diagnostics));
            return ExitCode::SUCCESS;
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
                println!("{}", obfusku_runtime::show(&value));
            }
            Err(diagnostics) => eprintln!("{}", session.render(&diagnostics)),
        }
    }
    ExitCode::SUCCESS
}
