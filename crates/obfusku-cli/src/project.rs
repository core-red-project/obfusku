//! Obfusku Project discovery and manifest (`ADR-017`/`ADR-018`/`ADR-019`/`ADR-020`).
//!
//! A Project is a directory containing `obfusku.toml`, discovered by
//! walking upward from a given file or directory to the nearest ancestor
//! that has one. A `.obk` file with no manifest anywhere in its ancestry
//! runs exactly as before this model existed (`ADR-017`'s mandatory
//! bare-file fallback) — nothing here changes that path's behavior.

use obfusku_diagnostics::{Diagnostic, Severity, Span};
use std::path::{Path, PathBuf};

use crate::manifest::EntryRef;
pub use crate::manifest::{MANIFEST_FILE_NAME, Manifest, load_manifest};

fn manifest_error(manifest_path: &Path, message: impl std::fmt::Display) -> Diagnostic {
    Diagnostic {
        severity: Severity::Error,
        message: format!("invalid manifest '{}': {message}", manifest_path.display()),
        primary: Span::default(),
    }
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

/// The outcome of searching a Project tree for a module by bare name
/// (`ADR-018`): a whole-tree search by filename, not a same-directory
/// lookup — `ModuleName` stays a single `Ident`, so ambiguity (the same
/// base name present under two different subdirectories) is a real
/// possibility this must report, not silently resolve by search order.
pub enum ModuleSearch {
    Found(PathBuf),
    NotFound,
    Ambiguous(Vec<PathBuf>),
}

/// Searches every `.obk` file reachable under `root` for one whose
/// filename (minus `.obk`) equals `name`.
#[must_use]
pub fn find_module(root: &Path, name: &str) -> ModuleSearch {
    find_module_skipping(root, name, &[])
}

/// [`find_module`] that does not descend into directories named in `skip`
/// (a dependency's tests and examples are not importable modules).
#[must_use]
pub fn find_module_skipping(root: &Path, name: &str, skip: &[&str]) -> ModuleSearch {
    let mut matches = Vec::new();
    walk_obk_files_skipping(root, skip, &mut |path| {
        if path.file_stem().and_then(|s| s.to_str()) == Some(name) {
            matches.push(path.to_path_buf());
        }
    });
    match matches.len() {
        0 => ModuleSearch::NotFound,
        1 => ModuleSearch::Found(matches.into_iter().next().expect("len checked above")),
        _ => ModuleSearch::Ambiguous(matches),
    }
}

/// Does not follow symlinks (no cycles, no escaping the root) and skips
/// hidden directories and Cargo's `target`.
pub(crate) fn walk_obk_files(dir: &Path, visit: &mut impl FnMut(&Path)) {
    walk_obk_files_skipping(dir, &[], visit);
}

fn walk_obk_files_skipping(dir: &Path, skip: &[&str], visit: &mut impl FnMut(&Path)) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let path = entry.path();
        if kind.is_dir() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if !name.starts_with('.') && name != "target" && !skip.contains(&&*name) {
                walk_obk_files_skipping(&path, skip, visit);
            }
        } else if kind.is_file() && path.extension().and_then(|e| e.to_str()) == Some("obk") {
            visit(&path);
        }
    }
}

/// The result of locating an entry module from whatever the CLI was
/// pointed at (`ADR-017`): either a bare `.obk` file (`entry_file` is
/// that file, `project_root` is the nearest ancestor manifest's
/// directory if one exists, else `entry_file`'s own parent — bare-file
/// mode) or a Project directory (`entry_file` is resolved from its
/// manifest's `entry` field, `project_root` is that directory itself).
pub struct EntryPoint {
    pub project_root: PathBuf,
    pub entry_file: PathBuf,
}

fn io_diagnostic(path: &Path, e: &std::io::Error) -> Diagnostic {
    Diagnostic {
        severity: Severity::Error,
        message: format!("could not read '{}': {e}", path.display()),
        primary: Span::default(),
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
                    return Err(manifest_error(
                        &manifest_path,
                        format!("entry path '{rel}' is not a '.obk' file inside this Project"),
                    ));
                }
                file
            }
            EntryRef::Module(name) => match find_module(path, name) {
                ModuleSearch::Found(f) => f,
                ModuleSearch::NotFound => {
                    return Err(manifest_error(
                        &manifest_path,
                        format!(
                            "entry module '{name}' does not resolve to an existing file under \
                             this Project"
                        ),
                    ));
                }
                ModuleSearch::Ambiguous(paths) => {
                    return Err(manifest_error(
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
            .map_err(|e| io_diagnostic(path, &e))?
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
        Err(Diagnostic {
            severity: Severity::Error,
            message: format!(
                "'{}' is not part of an Obfusku Project — 'build' requires an \
                 '{MANIFEST_FILE_NAME}' at or above it; a bare '.obk' file with no \
                 Project has nothing to certify as a distributable Source Artifact \
                 (run it with 'run'/'check' instead)",
                path.display()
            ),
            primary: Span::default(),
        })
    }
}

/// `ADR-019`'s symlink-escape check: the deepest existing ancestor of
/// `resolved` (inclusive) is canonicalized and must stay within
/// `root_canonical`. Nonexistent components below that ancestor (a
/// `writeFile` target that doesn't exist yet) cannot themselves be
/// symlinks, so checking the nearest real ancestor is sufficient —
/// this is not merely "check the target if it exists," it walks up
/// until it finds something real to check.
#[must_use]
pub fn escapes_root_via_symlink(root_canonical: &Path, resolved: &Path) -> bool {
    let mut probe = resolved.to_path_buf();
    loop {
        if let Ok(real) = probe.canonicalize() {
            return !real.starts_with(root_canonical);
        }
        if !probe.pop() {
            return false;
        }
    }
}

/// `*` matches within one path segment, `**` any number of segments.
fn glob_match(pattern: &[&str], path: &[&str]) -> bool {
    fn segment(p: &str, s: &str) -> bool {
        match p.split_once('*') {
            None => p == s,
            Some((head, tail)) => {
                let Some(rest) = s.strip_prefix(head) else {
                    return false;
                };
                (0..=rest.len())
                    .filter(|i| rest.is_char_boundary(*i))
                    .any(|i| segment(tail, &rest[i..]))
            }
        }
    }
    match pattern.split_first() {
        None => path.is_empty(),
        Some((&"**", rest)) => (0..=path.len()).any(|i| glob_match(rest, &path[i..])),
        Some((p, rest)) => path
            .split_first()
            .is_some_and(|(s, tail)| segment(p, s) && glob_match(rest, tail)),
    }
}

/// The `.obk` files under `root` matching any of `patterns` (relative,
/// `/`-separated, with `*` and `**`), in path order. Hidden directories and
/// `target` are never searched.
#[must_use]
pub fn matching_files(root: &Path, patterns: &[String]) -> Vec<PathBuf> {
    let mut found = Vec::new();
    walk_obk_files(root, &mut |p| {
        let Ok(rel) = p.strip_prefix(root) else {
            return;
        };
        let parts: Vec<String> = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect();
        let parts: Vec<&str> = parts.iter().map(String::as_str).collect();
        let hit = patterns.iter().any(|pat| {
            let pat: Vec<&str> = pat.split('/').collect();
            glob_match(&pat, &parts)
        });
        if hit {
            found.push(p.to_path_buf());
        }
    });
    found.sort();
    found
}

#[cfg(test)]
mod glob_tests {
    use super::glob_match;

    fn m(p: &str, s: &str) -> bool {
        glob_match(
            &p.split('/').collect::<Vec<_>>(),
            &s.split('/').collect::<Vec<_>>(),
        )
    }

    #[test]
    fn star_stays_in_a_segment_and_double_star_crosses() {
        assert!(m("tests/*.obk", "tests/a.obk"));
        assert!(!m("tests/*.obk", "tests/sub/a.obk"));
        assert!(m("tests/**/*.obk", "tests/a.obk"));
        assert!(m("tests/**/*.obk", "tests/x/y/a.obk"));
        assert!(!m("tests/**/*.obk", "src/a.obk"));
        assert!(m("**/*_test.obk", "a/b/c_test.obk"));
        assert!(m("t*t.obk", "test.obk"));
    }
}
