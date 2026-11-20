//! `fetch`: pin and download git dependencies (`ADR-029`).

use super::report::{host_failure, usage_error};
use obfusku_cli::project::{deps, resolve_entry_point};
use std::path::PathBuf;
use std::process::ExitCode;

/// `fetch [--update] [project]`. Network access happens only here; `run`
/// and `check` never touch the network and point back at this command when
/// something is missing.
pub(super) fn run(args: &[String]) -> ExitCode {
    let update = args.iter().skip(2).any(|a| a == "--update");
    let target = args
        .iter()
        .skip(2)
        .find(|a| !a.starts_with("--"))
        .map_or_else(|| PathBuf::from("."), PathBuf::from);
    let root = match resolve_entry_point(&target) {
        Ok(entry) => entry.project_root,
        Err(d) => return usage_error(d),
    };
    match deps::fetch(&root, update) {
        Ok(report) => {
            for line in report {
                println!("{line}");
            }
            ExitCode::SUCCESS
        }
        Err(d) => host_failure(d),
    }
}
