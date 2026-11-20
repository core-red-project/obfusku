//! The subcommands of `obfusku` and how the command line reaches them.
//!
//! Each subcommand is one module that turns arguments into a call on
//! [`obfusku_cli`]'s pipeline and the result into output and an exit code.
//! They hold *presentation* only; if a command needs to know what a program
//! means, that logic belongs in the library.
//!
//! Shared mechanics live in two places so no command repeats them:
//! `args` (flags and path arguments) and `report` (warnings, and turning a
//! failure into the right exit code).

mod args;
mod bundle;
mod completions;
mod fetch;
mod format;
mod inspect;
mod repl;
mod report;
mod run;
mod test;

use args::take_flag;
use std::process::ExitCode;

const USAGE: &str = "usage: obfusku <command> [args]\n\
     commands:\n  \
     run <file|project> [-- args]  run a program or Project directory\n  \
     check <file|project>     type-check without executing\n  \
     test [project]           run the Project's test files (tests/**/*.obk, or [test] include)\n  \
     fmt [--check|--write] <file|project>  print the canonical source; --check exits 1 if it differs, --write rewrites the file\n  \
     inspect <file|project> --tokens|--ast|--core\n  \
     build <project>          certify a Project as a distributable Source Artifact\n  \
     bundle <file|project> [-o out]  write a standalone executable of the program\n  \
     fetch [--update] [project]  pin git dependencies in obfusku.lock and download them\n  \
     repl                     interactive read-eval-print loop\n  \
     completions <shell>      print a completion script (bash, zsh, fish, powershell)\n  \
     version                  print the CLI version\n\n\
     options:\n  \
     -h, --help               print this usage text and exit\n  \
     -V, --version            print the CLI version and exit (same as 'version')\n  \
     -q, --quiet              suppress warnings\n  \
     --json                   'check': one JSON object of diagnostics on stdout";

/// Every subcommand, in the order the usage text lists them. The single
/// source for shell completions, so a new command cannot be forgotten there.
pub(crate) const COMMANDS: &[&str] = &[
    "run",
    "check",
    "fmt",
    "inspect",
    "build",
    "bundle",
    "fetch",
    "test",
    "repl",
    "completions",
    "version",
];

/// Flags accepted before any subcommand, for completions.
pub(crate) const GLOBAL_FLAGS: &[&str] =
    &["-h", "--help", "-V", "--version", "-q", "--quiet", "--json"];

fn print_usage() {
    eprintln!("{USAGE}");
}

fn print_version() {
    println!("obfusku {}", env!("CARGO_PKG_VERSION"));
}

/// Runs the command named by `args` (`args[0]` is the program name).
pub(crate) fn dispatch(mut args: Vec<String>) -> ExitCode {
    // Everything after `--` belongs to the program being run (`ADR-022`),
    // including would-be `--help`/`--version`.
    let program_args = match args.iter().position(|a| a == "--") {
        Some(i) => {
            let rest = args.split_off(i + 1);
            args.pop();
            rest
        }
        None => Vec::new(),
    };

    // `--help` and `--version` short-circuit from *any* position, matching
    // standard CLI convention, so they are checked before dispatch.
    if args.iter().skip(1).any(|a| a == "--help" || a == "-h") {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    if args.iter().skip(1).any(|a| a == "--version" || a == "-V") {
        print_version();
        return ExitCode::SUCCESS;
    }

    let command = args.get(1).cloned();
    let command = command.as_deref();
    let json = take_flag(&mut args, &["--json"]);
    let check_format = take_flag(&mut args, &["--check"]);
    let write_format = take_flag(&mut args, &["--write"]);
    let quiet = take_flag(&mut args, &["-q", "--quiet"]);
    if json && command != Some("check") {
        eprintln!("error: '--json' is only supported by 'check'");
        return ExitCode::from(2);
    }
    if (check_format || write_format) && command != Some("fmt") {
        eprintln!("error: '--check' and '--write' are only supported by 'fmt'");
        return ExitCode::from(2);
    }
    if check_format && write_format {
        eprintln!("error: use '--check' or '--write', not both");
        return ExitCode::from(2);
    }
    report::set_quiet(quiet);

    match command {
        Some("completions") => completions::run(&args),
        Some("version") => {
            print_version();
            ExitCode::SUCCESS
        }
        Some("run") => args::with_path(&args, |p| run::run(p, &program_args)),
        Some("check") => args::with_path(&args, |p| run::check(p, json)),
        Some("build") => args::with_path(&args, run::build),
        Some("fmt") if write_format => format::write(&args),
        Some("fmt") => args::with_entry_source(&args, |source| format::print(source, check_format)),
        Some("inspect") => inspect::run(&args),
        Some("repl") => repl::run(),
        Some("bundle") => bundle::run(&args),
        Some("fetch") => fetch::run(&args),
        Some("test") => test::run(&args),
        Some(unknown) => {
            eprintln!("unknown command: {unknown}");
            print_usage();
            ExitCode::from(2)
        }
        None => {
            print_usage();
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn code(words: &[&str]) -> String {
        let args = words.iter().map(|w| (*w).to_string()).collect();
        format!("{:?}", dispatch(args))
    }

    fn exit(n: u8) -> String {
        format!("{:?}", ExitCode::from(n))
    }

    #[test]
    fn version_and_help_succeed_from_any_position() {
        let ok = format!("{:?}", ExitCode::SUCCESS);
        assert_eq!(code(&["obfusku", "version"]), ok);
        assert_eq!(code(&["obfusku", "run", "--help"]), ok);
        assert_eq!(code(&["obfusku", "-V"]), ok);
    }

    #[test]
    fn no_command_and_unknown_commands_are_usage_errors() {
        assert_eq!(code(&["obfusku"]), exit(2));
        assert_eq!(code(&["obfusku", "frobnicate"]), exit(2));
    }

    #[test]
    fn mode_flags_are_only_valid_on_their_own_command() {
        assert_eq!(code(&["obfusku", "run", "--json", "x"]), exit(2));
        assert_eq!(code(&["obfusku", "check", "--write", "x"]), exit(2));
        assert_eq!(
            code(&["obfusku", "fmt", "--check", "--write", "x"]),
            exit(2)
        );
    }

    #[test]
    fn everything_after_a_double_dash_belongs_to_the_program() {
        // `--help` after `--` must not short-circuit; the unknown file then
        // fails as a usage error instead.
        assert_eq!(
            code(&["obfusku", "run", "/no/such.obk", "--", "--help"]),
            exit(2)
        );
    }

    #[test]
    fn every_listed_command_has_a_handler() {
        // A listed-but-unhandled command would fall through to
        // "unknown command"; none of these may be reported that way.
        for c in COMMANDS {
            assert!(
                USAGE.contains(c) || *c == "version",
                "{c} missing from usage"
            );
        }
    }
}
