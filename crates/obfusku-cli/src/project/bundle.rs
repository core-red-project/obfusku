//! `obfusku bundle` (`ADR-022`): a program as one self-contained executable.
//!
//! **Why this shape.** The interpreter is already a native binary, so a
//! bundle is *the interpreter itself* with the program's sources appended —
//! no compiler backend, no second runtime to keep in sync, and the sources
//! still go through the same checker at startup. [`embedded`] looks for the
//! trailer at the end of the running executable; if present, the binary runs
//! its payload and ignores the command line (see `main.rs`).
//!
//! # Layout
//!
//! `<interpreter bytes><payload><payload length u64 LE><MAGIC>`.
//!
//! Payload: `entry path`, then `count u32`, then `count` × (`path`,
//! `content`), where a path is `len u32` + UTF-8 bytes relative to the
//! Project root and content is `len u64` + bytes. Then the package graph
//! (`ADR-029`): `count u32` × (`owner path`, `n u32`, `n` × `dependency
//! path`); dependency sources sit under `.deps/<k>/`, the project itself is
//! `""`. A payload that ends after the files has no dependencies, which is
//! how bundles written before dependencies existed still decode.
//!
//! The magic number is the only integrity check: a bundle is trusted exactly
//! as much as the executable it lives in.

use super::deps::{Packages, load_packages};
use super::entry::resolve_entry_point;
use super::manifest::{MANIFEST_FILE_NAME, parse_manifest};
use super::modules::ModuleSource;
use super::search::walk_obk_files;
use crate::report::host_error::read_error;
use obfusku_diagnostics::Diagnostic;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// Marks the end of a bundle. Versioned (`…1`) so a future layout can be told
/// apart instead of misparsed.
const MAGIC: &[u8; 8] = b"OBKBNDL1";
/// Trailer size: payload length (8 bytes) plus [`MAGIC`] (8 bytes).
const TRAILER_LEN: u64 = 16;

/// A program's source tree, ready to embed or extract.
#[derive(Debug, PartialEq, Eq)]
pub struct Bundle {
    /// Entry module path, relative to the project root.
    pub entry: String,
    /// Every source as (relative path, content).
    pub files: Vec<(String, Vec<u8>)>,
    /// Owner package root → its direct dependency roots, as bundle paths.
    pub edges: Vec<(String, Vec<String>)>,
}

/// Length-prefixed (`u32`) UTF-8, the encoding of every path in the payload.
fn put_str(out: &mut Vec<u8>, s: &str) {
    out.extend_from_slice(&(s.len() as u32).to_le_bytes());
    out.extend_from_slice(s.as_bytes());
}

/// Lengths are fixed-width little-endian so a bundle is byte-identical across
/// platforms and `decode` can bounds-check before allocating.
impl Bundle {
    /// Serializes to the payload layout in the module docs.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        put_str(&mut out, &self.entry);
        out.extend_from_slice(&(self.files.len() as u32).to_le_bytes());
        for (path, content) in &self.files {
            put_str(&mut out, path);
            out.extend_from_slice(&(content.len() as u64).to_le_bytes());
            out.extend_from_slice(content);
        }
        out.extend_from_slice(&(self.edges.len() as u32).to_le_bytes());
        for (owner, deps) in &self.edges {
            put_str(&mut out, owner);
            out.extend_from_slice(&(deps.len() as u32).to_le_bytes());
            for d in deps {
                put_str(&mut out, d);
            }
        }
        out
    }

    /// Parses a payload; `None` if it is truncated or malformed.
    #[must_use]
    pub fn decode(mut bytes: &[u8]) -> Option<Bundle> {
        fn take<'a>(bytes: &mut &'a [u8], n: usize) -> Option<&'a [u8]> {
            if bytes.len() < n {
                return None;
            }
            let (head, tail) = bytes.split_at(n);
            *bytes = tail;
            Some(head)
        }
        fn take_u32(bytes: &mut &[u8]) -> Option<usize> {
            Some(u32::from_le_bytes(take(bytes, 4)?.try_into().ok()?) as usize)
        }
        fn take_str(bytes: &mut &[u8]) -> Option<String> {
            let n = take_u32(bytes)?;
            String::from_utf8(take(bytes, n)?.to_vec()).ok()
        }
        let entry = take_str(&mut bytes)?;
        let count = take_u32(&mut bytes)?;
        let mut files = Vec::new();
        for _ in 0..count {
            let path = take_str(&mut bytes)?;
            let len = u64::from_le_bytes(take(&mut bytes, 8)?.try_into().ok()?) as usize;
            files.push((path, take(&mut bytes, len)?.to_vec()));
        }
        let mut edges = Vec::new();
        if !bytes.is_empty() {
            for _ in 0..take_u32(&mut bytes)? {
                let owner = take_str(&mut bytes)?;
                let mut deps = Vec::new();
                for _ in 0..take_u32(&mut bytes)? {
                    deps.push(take_str(&mut bytes)?);
                }
                edges.push((owner, deps));
            }
        }
        Some(Bundle {
            entry,
            files,
            edges,
        })
    }
}

/// `path` relative to `root`, always `/`-separated: bundle paths must not
/// depend on the OS that wrote them.
fn bundle_path_of(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

/// Collects `path`'s Project (`ADR-017`): every `.obk` under its root plus
/// the manifest, with the entry file named relative to that root.
pub fn collect(path: &Path) -> Result<Bundle, Diagnostic> {
    let entry = resolve_entry_point(path)?;
    let root = entry
        .project_root
        .canonicalize()
        .map_err(|e| read_error(&entry.project_root, &e))?;
    let entry_file = entry.entry_file.canonicalize().unwrap_or(entry.entry_file);

    let mut paths: Vec<PathBuf> = Vec::new();
    walk_obk_files(&root, &mut |p| paths.push(p.to_path_buf()));
    let manifest = root.join(MANIFEST_FILE_NAME);
    if manifest.is_file() {
        paths.push(manifest);
    }
    paths.sort();

    let relative = |p: &Path| bundle_path_of(p, &root);
    let mut files = Vec::new();
    for p in &paths {
        let content = std::fs::read(p).map_err(|e| read_error(p, &e))?;
        files.push((relative(p), content));
    }
    // Dependencies travel inside the executable under `.deps/<k>/`.
    let packages = load_packages(&root)?;
    let mut dep_roots: Vec<PathBuf> = packages
        .edges()
        .values()
        .flatten()
        .filter(|r| **r != root)
        .cloned()
        .collect();
    dep_roots.sort();
    dep_roots.dedup();
    let bundle_path = |r: &Path| {
        if *r == root {
            String::new()
        } else {
            let k = dep_roots.iter().position(|d| d == r).unwrap_or(0);
            format!(".deps/{k}")
        }
    };
    for (k, dep_root) in dep_roots.iter().enumerate() {
        let mut found = Vec::new();
        walk_obk_files(dep_root, &mut |p| found.push(p.to_path_buf()));
        found.sort();
        for p in found {
            let content = std::fs::read(&p).map_err(|e| read_error(&p, &e))?;
            files.push((
                format!(".deps/{k}/{}", bundle_path_of(&p, dep_root)),
                content,
            ));
        }
    }
    let mut edges: Vec<(String, Vec<String>)> = packages
        .edges()
        .iter()
        .map(|(owner, deps)| {
            (
                bundle_path(owner),
                deps.iter().map(|d| bundle_path(d)).collect(),
            )
        })
        .collect();
    edges.sort();
    Ok(Bundle {
        entry: relative(&entry_file),
        files,
        edges,
    })
}

/// Reads this executable's own embedded payload, if it is a bundle.
#[must_use]
pub fn embedded() -> Option<Bundle> {
    let mut f = std::fs::File::open(std::env::current_exe().ok()?).ok()?;
    let len = f.metadata().ok()?.len();
    if len < TRAILER_LEN {
        return None;
    }
    f.seek(SeekFrom::Start(len - TRAILER_LEN)).ok()?;
    let mut trailer = [0u8; TRAILER_LEN as usize];
    f.read_exact(&mut trailer).ok()?;
    if &trailer[8..] != MAGIC {
        return None;
    }
    let payload_len = u64::from_le_bytes(trailer[..8].try_into().ok()?);
    f.seek(SeekFrom::Start(len.checked_sub(TRAILER_LEN + payload_len)?))
        .ok()?;
    let mut payload = vec![0u8; payload_len as usize];
    f.read_exact(&mut payload).ok()?;
    Bundle::decode(&payload)
}

/// Writes interpreter + `bundle` to `out`. The interpreter is the running
/// executable, minus any payload it already carries.
pub fn write(bundle: &Bundle, out: &Path) -> std::io::Result<()> {
    let exe = std::env::current_exe()?;
    let mut bytes = std::fs::read(&exe)?;
    if bytes.len() as u64 >= TRAILER_LEN && &bytes[bytes.len() - 8..] == MAGIC {
        let n = bytes.len();
        let payload_len = u64::from_le_bytes(bytes[n - 16..n - 8].try_into().unwrap());
        let keep = (n as u64)
            .checked_sub(TRAILER_LEN.saturating_add(payload_len))
            .ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "the running executable carries a corrupt bundle trailer",
                )
            })?;
        bytes.truncate(keep as usize);
    }
    let payload = bundle.encode();
    bytes.extend_from_slice(&payload);
    bytes.extend_from_slice(&(payload.len() as u64).to_le_bytes());
    bytes.extend_from_slice(MAGIC);
    std::fs::write(out, bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(out, std::fs::Permissions::from_mode(0o755))?;
    }
    Ok(())
}

/// Runs `bundle` with `args` against the caller's working directory,
/// resolving imports from its embedded sources — nothing is written to
/// disk. Returns the process exit code.
#[must_use]
pub fn run_embedded(bundle: &Bundle, args: &[String]) -> std::process::ExitCode {
    use std::process::ExitCode;
    let root = PathBuf::from("/obfusku-bundle");
    let mut sources = std::collections::HashMap::new();
    for (rel, content) in &bundle.files {
        if Path::new(rel).extension().is_none_or(|e| e != "obk") {
            continue;
        }
        if let Ok(text) = String::from_utf8(content.clone()) {
            sources.insert(root.join(rel), text);
        } else {
            eprintln!("bundled module '{rel}' is not valid UTF-8");
            return ExitCode::from(2);
        }
    }
    let capabilities = bundle
        .files
        .iter()
        .find(|(rel, _)| rel == MANIFEST_FILE_NAME)
        .and_then(|(_, content)| String::from_utf8(content.clone()).ok())
        .and_then(|text| parse_manifest(Path::new(MANIFEST_FILE_NAME), &text).ok())
        .and_then(|m| m.capabilities);
    let packages = Packages::new(
        bundle
            .edges
            .iter()
            .map(|(owner, deps)| {
                let at = |rel: &str| {
                    if rel.is_empty() {
                        root.clone()
                    } else {
                        root.join(rel)
                    }
                };
                (at(owner), deps.iter().map(|d| at(d)).collect())
            })
            .collect(),
    );
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let result = crate::pipeline::run_bundled(
        &root,
        &root.join(&bundle.entry),
        &cwd,
        args,
        ModuleSource::Memory(&sources),
        capabilities,
        packages,
    );
    match result {
        Ok(_) => ExitCode::SUCCESS,
        Err((map, diags)) => {
            eprintln!("{}", crate::report::render::diagnostics_map(&map, &diags));
            ExitCode::from(1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_decode_round_trips() {
        let b = Bundle {
            entry: "main.obk".to_string(),
            files: vec![
                ("main.obk".to_string(), b"x \xe2\x89\x94 1".to_vec()),
                ("utils/a.obk".to_string(), Vec::new()),
            ],
            edges: vec![(String::new(), vec![".deps/0".to_string()])],
        };
        assert_eq!(Bundle::decode(&b.encode()), Some(b));
    }

    #[test]
    fn truncated_payload_is_rejected_not_panicked_on() {
        let b = Bundle {
            entry: "main.obk".to_string(),
            files: vec![("main.obk".to_string(), b"abc".to_vec())],
            edges: vec![(String::new(), vec![".deps/0".to_string()])],
        };
        let bytes = b.encode();
        // A payload ending exactly after the files is the dependency-free form.
        let files_end = Bundle {
            entry: b.entry.clone(),
            files: b.files.clone(),
            edges: Vec::new(),
        }
        .encode()
        .len()
            - 4;
        for n in (0..bytes.len()).filter(|n| *n != files_end) {
            assert_eq!(Bundle::decode(&bytes[..n]), None, "prefix {n}");
        }
    }
}
