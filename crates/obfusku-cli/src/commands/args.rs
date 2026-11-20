//! Argument handling shared by the subcommands.

use super::report::usage_error;
use obfusku_cli::project::resolve_entry_point;
use std::path::Path;
use std::process::ExitCode;

/// Removes every occurrence of any of `names` from the arguments after the
/// command name and reports whether one was present. The command name
/// itself (`args[1]`) is never removed, so `obfusku --json` still errors as
/// an unknown command rather than silently becoming "no command".
pub(super) fn take_flag(args: &mut Vec<String>, names: &[&str]) -> bool {
    let before = args.len();
    let mut seen = 0;
    args.retain(|a| {
        seen += 1;
        seen <= 2 || !names.contains(&a.as_str())
    });
    args.len() != before
}

/// `args[2]`, or a usage error naming the command it belongs to.
fn path_argument(args: &[String]) -> Result<&String, ExitCode> {
    args.get(2).ok_or_else(|| {
        eprintln!(
            "expected a file path, e.g. 'obfusku {} program.obk'",
            args[1]
        );
        ExitCode::from(2)
    })
}

/// Hands `command` the path in `args[2]` after checking it exists.
/// `run`, `check` and `build` need the path itself (not just its text) to
/// resolve imports against the Project it belongs to.
pub(super) fn with_path(args: &[String], command: impl FnOnce(&Path) -> ExitCode) -> ExitCode {
    let path = match path_argument(args) {
        Ok(p) => Path::new(p),
        Err(code) => return code,
    };
    if let Err(e) = std::fs::metadata(path) {
        eprintln!("could not read '{}': {e}", path.display());
        return ExitCode::from(2);
    }
    command(path)
}

/// Hands `command` the source text of the entry file `args[2]` names.
/// `fmt` and `inspect` work on one file's own text (no import resolution),
/// so a Project directory only needs its entry module discovered.
pub(super) fn with_entry_source(
    args: &[String],
    command: impl FnOnce(&str) -> ExitCode,
) -> ExitCode {
    let path = match path_argument(args) {
        Ok(p) => p,
        Err(code) => return code,
    };
    match read_entry(path) {
        Ok(source) => command(&source),
        Err(code) => code,
    }
}

/// The text of the entry file that `path` (a file or Project) resolves to.
pub(super) fn read_entry(path: &str) -> Result<String, ExitCode> {
    let entry = resolve_entry_point(Path::new(path)).map_err(usage_error)?;
    std::fs::read_to_string(&entry.entry_file).map_err(|e| {
        eprintln!("could not read '{}': {e}", entry.entry_file.display());
        ExitCode::from(2)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(words: &[&str]) -> Vec<String> {
        words.iter().map(|w| (*w).to_string()).collect()
    }

    #[test]
    fn a_flag_is_removed_and_reported() {
        let mut args = argv(&["obfusku", "check", "--json", "p"]);
        assert!(take_flag(&mut args, &["--json"]));
        assert_eq!(args, ["obfusku", "check", "p"]);
    }

    #[test]
    fn every_occurrence_and_alias_is_removed() {
        let mut args = argv(&["obfusku", "run", "-q", "p", "--quiet"]);
        assert!(take_flag(&mut args, &["-q", "--quiet"]));
        assert_eq!(args, ["obfusku", "run", "p"]);
    }

    #[test]
    fn the_command_name_is_never_taken_for_a_flag() {
        let mut args = argv(&["obfusku", "--json"]);
        assert!(!take_flag(&mut args, &["--json"]));
        assert_eq!(args, ["obfusku", "--json"]);
    }

    #[test]
    fn an_absent_flag_changes_nothing() {
        let mut args = argv(&["obfusku", "run", "p"]);
        assert!(!take_flag(&mut args, &["--json"]));
        assert_eq!(args.len(), 3);
    }

    #[test]
    fn a_missing_path_is_a_usage_error() {
        let code = with_path(&argv(&["obfusku", "run"]), |_| ExitCode::SUCCESS);
        assert_eq!(format!("{code:?}"), format!("{:?}", ExitCode::from(2)));
    }

    #[test]
    fn an_unreadable_path_never_reaches_the_command() {
        let code = with_path(&argv(&["obfusku", "run", "/no/such/file.obk"]), |_| {
            panic!("the command must not run")
        });
        assert_eq!(format!("{code:?}"), format!("{:?}", ExitCode::from(2)));
    }
}
