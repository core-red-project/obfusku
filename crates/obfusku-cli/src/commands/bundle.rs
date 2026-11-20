//! `bundle`: a program as one standalone executable (`ADR-022`).

use super::report::{pipeline_failure, usage_error};
use obfusku_cli::project::{bundle, load_manifest, resolve_entry_point};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// The output name a manifest asks for, with the platform's executable
/// suffix applied once (never doubled if the manifest already has it).
fn configured_output(path: &Path) -> Option<String> {
    let root = resolve_entry_point(path).map_or_else(|_| path.to_path_buf(), |e| e.project_root);
    let manifest = load_manifest(&root).ok()?;
    let name = manifest.bundle_output.or(manifest.name)?;
    let suffix = std::env::consts::EXE_SUFFIX;
    Some(if suffix.is_empty() || name.ends_with(suffix) {
        name
    } else {
        format!("{name}{suffix}")
    })
}

/// `bundle <file|project> [-o out]`: type-checks the program, then writes a
/// copy of this interpreter with the program's sources embedded. The output
/// defaults to the manifest's `[bundle] output`/`name`, else the entry
/// file's stem, in the current directory.
pub(super) fn run(args: &[String]) -> ExitCode {
    let mut path: Option<&str> = None;
    let mut out: Option<&str> = None;
    let mut rest = args.iter().skip(2);
    while let Some(a) = rest.next() {
        match a.as_str() {
            "-o" => {
                let Some(v) = rest.next() else {
                    eprintln!("'-o' expects an output path");
                    return ExitCode::from(2);
                };
                out = Some(v);
            }
            other if path.is_none() => path = Some(other),
            other => {
                eprintln!("unexpected extra argument: {other}");
                return ExitCode::from(2);
            }
        }
    }
    let Some(path) = path else {
        eprintln!("expected a file or project, e.g. 'obfusku bundle program.obk -o program'");
        return ExitCode::from(2);
    };
    let path = Path::new(path);
    // A program that does not check must not become an executable.
    if let Err(failure) = obfusku_cli::check_file(path) {
        return pipeline_failure(failure);
    }
    let collected = match bundle::collect(path) {
        Ok(b) => b,
        Err(d) => return usage_error(d),
    };
    let out = out
        .map(PathBuf::from)
        .or_else(|| configured_output(path).map(PathBuf::from))
        .unwrap_or_else(|| {
            let stem = Path::new(&collected.entry).file_stem().map_or_else(
                || "program".to_string(),
                |s| s.to_string_lossy().into_owned(),
            );
            PathBuf::from(format!("{stem}{}", std::env::consts::EXE_SUFFIX))
        });
    match bundle::write(&collected, &out) {
        Ok(()) => {
            println!("wrote {}", out.display());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("could not write '{}': {e}", out.display());
            ExitCode::from(2)
        }
    }
}
