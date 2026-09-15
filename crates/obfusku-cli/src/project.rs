//! Obfusku Project discovery and manifest (`ADR-017`/`ADR-018`/`ADR-019`/`ADR-020`).
//!
//! A Project is a directory containing `obfusku.toml`, discovered by
//! walking upward from a given file or directory to the nearest ancestor
//! that has one. A `.obk` file with no manifest anywhere in its ancestry
//! runs exactly as before this model existed (`ADR-017`'s mandatory
//! bare-file fallback) — nothing here changes that path's behavior.

use obfusku_diagnostics::{Diagnostic, Severity, Span};
use std::path::{Path, PathBuf};

pub const MANIFEST_FILE_NAME: &str = "obfusku.toml";
const SUPPORTED_FORMAT: i64 = 1;

/// A validated `obfusku.toml` (`ADR-020`). Unknown fields are tolerated
/// silently at a supported `format` — this struct only ever reads the
/// fields it knows about.
#[derive(Debug, Clone)]
pub struct Manifest {
    pub format: i64,
    pub entry: String,
    pub name: Option<String>,
}

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
            .map(Path::to_path_buf)
            .unwrap_or_else(|| canonical.clone())
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

/// Reads and validates `<root>/obfusku.toml`. Malformed TOML, a missing
/// or non-integer `format`, an unsupported `format` value, or a missing
/// `entry` are all the same class of error (`ADR-020`) — never silently
/// treated as "no manifest."
pub fn load_manifest(root: &Path) -> Result<Manifest, Diagnostic> {
    let manifest_path = root.join(MANIFEST_FILE_NAME);
    let text = std::fs::read_to_string(&manifest_path)
        .map_err(|e| manifest_error(&manifest_path, format!("could not read: {e}")))?;
    let value: toml::Value = text
        .parse()
        .map_err(|e| manifest_error(&manifest_path, format!("malformed TOML: {e}")))?;
    let table = value
        .as_table()
        .ok_or_else(|| manifest_error(&manifest_path, "must be a TOML table at the top level"))?;

    let format = table
        .get("format")
        .and_then(toml::Value::as_integer)
        .ok_or_else(|| manifest_error(&manifest_path, "missing or non-integer 'format' field"))?;
    if format != SUPPORTED_FORMAT {
        return Err(manifest_error(
            &manifest_path,
            format!(
                "unsupported manifest format {format} (this obfusku binary supports format \
                 {SUPPORTED_FORMAT})"
            ),
        ));
    }

    let entry = table
        .get("entry")
        .and_then(toml::Value::as_str)
        .ok_or_else(|| manifest_error(&manifest_path, "missing or non-string 'entry' field"))?
        .to_string();

    let name = table
        .get("name")
        .and_then(toml::Value::as_str)
        .map(str::to_string);

    Ok(Manifest {
        format,
        entry,
        name,
    })
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
pub fn find_module(root: &Path, name: &str) -> ModuleSearch {
    let mut matches = Vec::new();
    walk_obk_files(root, &mut |path| {
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

fn walk_obk_files(dir: &Path, visit: &mut impl FnMut(&Path)) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_obk_files(&path, visit);
        } else if path.extension().and_then(|e| e.to_str()) == Some("obk") {
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

fn io_diagnostic(path: &Path, e: std::io::Error) -> Diagnostic {
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
        match find_module(path, &manifest.entry) {
            ModuleSearch::Found(entry_file) => Ok(EntryPoint {
                project_root: path.to_path_buf(),
                entry_file,
            }),
            ModuleSearch::NotFound => Err(manifest_error(
                &path.join(MANIFEST_FILE_NAME),
                format!(
                    "entry module '{}' does not resolve to an existing file under this Project",
                    manifest.entry
                ),
            )),
            ModuleSearch::Ambiguous(paths) => Err(manifest_error(
                &path.join(MANIFEST_FILE_NAME),
                format!(
                    "entry module '{}' is ambiguous — matches: {}",
                    manifest.entry,
                    paths
                        .iter()
                        .map(|p| p.display().to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            )),
        }
    } else {
        let project_root = discover_project_root(path)
            .map_err(|e| io_diagnostic(path, e))?
            .unwrap_or_else(|| {
                path.parent()
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| PathBuf::from("."))
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
