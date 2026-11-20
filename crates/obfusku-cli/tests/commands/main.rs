//! The `obfusku` executable as a user drives it: argv, exit codes, stdout and stderr. One module per subcommand, so a failing test names the command at fault.

#[path = "../support/mod.rs"]
mod support;

mod build;
mod bundle;
mod dispatch;
mod format;
mod inspect;
mod programs;
mod repl;
mod run;
mod test_runner;
