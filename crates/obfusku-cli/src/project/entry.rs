//! Locating the entry module of a run (`ADR-017`).
//!
//! A Project is a directory containing `obfusku.toml`. A `.obk` file with
//! no manifest anywhere in its ancestry still runs exactly as it did before
//! the Project model existed (`ADR-017`'s mandatory bare-file fallback);
//! nothing here changes that path.

use super::manifest::{EntryRef, MANIFEST_FILE_NAME, invalid_manifest, load_manifest};
use super::search::{ModuleSearch, find_module};
use crate::report::host_error::{host_error, read_error};
use obfusku_diagnostics::Diagnostic;
use std::path::{Path, PathBuf};

/// The result of locating an entry module from whatever the CLI was
/// pointed at (`ADR-017`): either a bare `.obk` file (`entry_file` is
/// that file, `project_root` is the nearest ancestor manifest's
/// directory if one exists, else `entry_file`'s own parent — bare-file
/// mode) or a Project directory (`entry_file` is resolved from its
/// manifest's `entry` field, `project_root` is that directory itself).
pub struct EntryPoint {
    /// Directory that anchors imports and capabilities.
    pub project_root: PathBuf,
    /// The module to run.
    pub entry_file: PathBuf,
}

/// Walks upward from `start` (a file or an already-existing directory)
/// looking for the nearest ancestor directory containing
/// `obfusku.toml`. `None` means no manifest exists anywhere in the
/// ancestry — the caller falls back to bare-file mode unchanged.
///
/// The nearest-ancestor rule is load-bearing, not incidental: a deeper
/// Project nested inside an outer one must claim files under it before
/// the outer Project's own manifest is ever consulted.
pub fn discover_project_root(start: &Path) -> std::io::Result<Option<PathBuf>> {
    let canonical = start.canonicalize()?;
    let mut dir = if canonical.is_dir() {
        canonical
    } else {
        canonical
            .parent()
            .map_or_else(|| canonical.clone(), Path::to_path_buf)
    };
    loop {
        if dir.join(MANIFEST_FILE_NAME).is_file() {
            return Ok(Some(dir));
        }
        match dir.parent() {
            Some(parent) => dir = parent.to_path_buf(),
            None => return Ok(None),
        }
    }
}

/// Resolves what `run`/`check`/`fmt`/`inspect` were pointed at into a
/// concrete entry file and Project root. A directory argument is
/// required to carry its own `obfusku.toml` directly (`ADR-017`: "its
/// own marker names the entry module") — this does not walk further
/// upward looking for one, since that would silently pick a different
/// Project than the one the user explicitly named. A file argument
/// keeps working exactly as before this model existed, gaining
/// project-relative resolution only when a manifest happens to be found
/// above it.
pub fn resolve_entry_point(path: &Path) -> Result<EntryPoint, Diagnostic> {
    if path.is_dir() {
        let manifest = load_manifest(path)?;
        let manifest_path = path.join(MANIFEST_FILE_NAME);
        let entry_file = match &manifest.entry {
            EntryRef::Path(rel) => {
                let file = path.join(rel);
                let inside = file
                    .canonicalize()
                    .ok()
                    .zip(path.canonicalize().ok())
                    .is_some_and(|(f, r)| f.starts_with(r));
                if !file.is_file() || !inside || file.extension().is_none_or(|e| e != "obk") {
                    return Err(invalid_manifest(
                        &manifest_path,
                        format!("entry path '{rel}' is not a '.obk' file inside this Project"),
                    ));
                }
                file
            }
            EntryRef::Module(name) => match find_module(path, name) {
                ModuleSearch::Found(f) => f,
                ModuleSearch::NotFound => {
                    return Err(invalid_manifest(
                        &manifest_path,
                        format!(
                            "entry module '{name}' does not resolve to an existing file under \
                             this Project"
                        ),
                    ));
                }
                ModuleSearch::Ambiguous(paths) => {
                    return Err(invalid_manifest(
                        &manifest_path,
                        format!(
                            "entry module '{name}' is ambiguous — matches: {}; use [entry] \
                             path = \"…\" in a format 2 manifest",
                            paths
                                .iter()
                                .map(|p| p.display().to_string())
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                    ));
                }
            },
        };
        Ok(EntryPoint {
            project_root: path.to_path_buf(),
            entry_file,
        })
    } else {
        let project_root = discover_project_root(path)
            .map_err(|e| read_error(path, &e))?
            .unwrap_or_else(|| {
                // A bare `main.obk` has the empty path as its parent.
                match path.parent() {
                    Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
                    _ => PathBuf::from("."),
                }
            });
        Ok(EntryPoint {
            project_root,
            entry_file: path.to_path_buf(),
        })
    }
}

/// Like [`resolve_entry_point`], but for `build`: a bare `.obk`
/// file with no manifest anywhere in its ancestry has nothing to
/// certify as a distributable unit — `run`/`check`'s bare-file
/// fallback does not apply here, and this is reported as a distinct,
/// clear usage error rather than silently degrading to it.
pub fn require_project_entry_point(path: &Path) -> Result<EntryPoint, Diagnostic> {
    let entry = resolve_entry_point(path)?;
    if entry.project_root.join(MANIFEST_FILE_NAME).is_file() {
        Ok(entry)
    } else {
        Err(host_error(format!(
            "'{}' is not part of an Obfusku Project — 'build' requires an \
                 '{MANIFEST_FILE_NAME}' at or above it; a bare '.obk' file with no \
                 Project has nothing to certify as a distributable Source Artifact \
                 (run it with 'run'/'check' instead)",
            path.display()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::scratch::Scratch;

    const MANIFEST: &str = "format = 2\nentry = \"main\"\n";

    #[test]
    fn a_project_directory_resolves_its_entry_module() {
        let s = Scratch::new();
        s.write("obfusku.toml", MANIFEST);
        let main = s.write("src/main.obk", "❧");
        let e = resolve_entry_point(s.root()).unwrap();
        assert_eq!(
            (e.project_root, e.entry_file),
            (s.root().to_path_buf(), main)
        );
    }

    #[test]
    fn a_directory_without_a_manifest_is_an_error_not_a_guess() {
        let s = Scratch::new();
        assert!(resolve_entry_point(s.root()).is_err());
    }

    #[test]
    fn a_missing_entry_module_is_reported_against_the_manifest() {
        let s = Scratch::new();
        s.write("obfusku.toml", MANIFEST);
        let err = resolve_entry_point(s.root()).err().expect("no main.obk");
        assert!(
            err.message.contains("entry module 'main'"),
            "{}",
            err.message
        );
    }

    #[test]
    fn an_ambiguous_entry_module_lists_every_match() {
        let s = Scratch::new();
        s.write("obfusku.toml", MANIFEST);
        s.write("a/main.obk", "❧");
        s.write("b/main.obk", "❧");
        let err = resolve_entry_point(s.root()).err().expect("ambiguous");
        assert!(err.message.contains("ambiguous"), "{}", err.message);
    }

    #[test]
    fn a_bare_file_without_a_manifest_roots_at_its_own_directory() {
        let s = Scratch::new();
        let f = s.write("lone.obk", "❧");
        let e = resolve_entry_point(&f).unwrap();
        assert_eq!(
            e.project_root,
            s.root().canonicalize().unwrap_or(s.root().to_path_buf())
        );
        assert_eq!(e.entry_file, f);
    }

    #[test]
    fn a_file_inside_a_project_takes_the_projects_root() {
        let s = Scratch::new();
        s.write("obfusku.toml", MANIFEST);
        let f = s.write("src/other.obk", "❧");
        assert_eq!(resolve_entry_point(&f).unwrap().project_root, s.root());
    }

    #[test]
    fn the_nearest_manifest_wins_over_an_outer_one() {
        let s = Scratch::new();
        s.write("obfusku.toml", MANIFEST);
        s.write("inner/obfusku.toml", MANIFEST);
        let f = s.write("inner/x.obk", "❧");
        assert_eq!(
            discover_project_root(&f).unwrap(),
            Some(s.root().join("inner"))
        );
    }

    #[test]
    fn build_requires_a_real_project() {
        let s = Scratch::new();
        let f = s.write("lone.obk", "❧");
        let err = require_project_entry_point(&f).err().expect("no manifest");
        assert!(
            err.message.contains("not part of an Obfusku Project"),
            "{}",
            err.message
        );
        s.write("obfusku.toml", MANIFEST);
        assert!(require_project_entry_point(&f).is_ok());
    }
}
