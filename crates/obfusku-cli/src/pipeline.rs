//! Pipeline orchestration (`IMPLEMENTATION_ARCHITECTURE.md` §12).
//!
//! **No language logic lives here.** Every step is a call into the crate that
//! owns that stage — lexing and parsing in `obfusku-syntax`, inference in
//! `obfusku-typecheck`, evaluation in `obfusku-runtime`. This module decides
//! only *order* and *what the host adds* (files, imports, natives,
//! capabilities).
//!
//! | Module | Entry points |
//! |---|---|
//! | `source` | `run_source`, `check_source`, `fmt_source`, `inspect_source` — text only |
//! | this module | `run_file`, `check_file`, `build_project`, `analyze_text` — files and Projects |
//! | `check` | the shared text → `Checked` path |
//! | `run` | evaluation and runtime traces |
//! | `host` | what the embedder grants |
//! | `repl` | the incremental interactive session (`ADR-013`) |

mod check;
mod host;
mod repl;
mod run;
mod source;

pub use check::Failure;
pub use repl::ReplSession;
pub(crate) use run::run_bundled;
pub use source::{InspectKind, check_source, fmt_source, inspect_source, run_source};

use crate::project::entry::{require_project_entry_point, resolve_entry_point};
use host::Host;
use obfusku_diagnostics::{Diagnostic, SourceMap};
use obfusku_runtime::Value;
use std::path::{Path, PathBuf};

/// `run`'s reported value: the last top-level binding evaluated.
///
/// This is a CLI reporting convention (`ADR-004`), independent of export
/// visibility (`⟳`): a program whose last binding is private still reports it.
#[derive(Debug)]
pub struct RunResult(pub Value);

/// Runs `path` — a `.obk` file or a Project directory — resolving its own
/// imports against the Project root (`ADR-017`, `ADR-018`). On success also
/// returns the checker's non-fatal warnings.
pub fn run_file(path: &Path) -> Result<(RunResult, Vec<Diagnostic>), Failure> {
    run_file_with_args(path, &[])
}

/// [`run_file`] with program arguments, readable from Obfusku source via
/// `⌁↓⌥` (`ADR-022`).
pub fn run_file_with_args(
    path: &Path,
    args: &[String],
) -> Result<(RunResult, Vec<Diagnostic>), Failure> {
    run::run_entry(
        resolve_entry_point(path),
        &Host {
            args,
            ..Host::default()
        },
    )
}

/// Like [`check_source`], but resolves imports first — see [`run_file`].
pub fn check_file(path: &Path) -> Result<Vec<Diagnostic>, Failure> {
    check::check_entry(resolve_entry_point(path), &Host::default()).map(|c| c.warnings)
}

/// The `build` pipeline: identical to [`check_file`] except that the entry
/// point must belong to a real Project (no bare-file fallback). Success
/// certifies the Project as a valid, distributable Source Artifact
/// (`ADR-017`); nothing is written.
pub fn build_project(path: &Path) -> Result<Vec<Diagnostic>, Failure> {
    check::check_entry(require_project_entry_point(path), &Host::default()).map(|c| c.warnings)
}

/// Diagnostics for `source` as the entry module of the Project containing
/// `path` — the editor-buffer variant of [`check_file`], used by the
/// language server. Returns the [`SourceMap`] the spans are relative to and
/// every diagnostic (errors, or on success the warnings).
#[must_use]
pub fn analyze_text(path: &Path, source: &str) -> (SourceMap, Vec<Diagnostic>) {
    let mut source_map = SourceMap::new();
    let root = match resolve_entry_point(path) {
        Ok(e) => e.project_root,
        // A buffer not yet saved: check it against its would-be directory.
        Err(_) if !path.exists() => path
            .parent()
            .map_or_else(|| PathBuf::from("."), Path::to_path_buf),
        Err(d) => {
            source_map.add_file("");
            return (source_map, vec![d]);
        }
    };
    match check::check_text(source_map, source, &root, &Host::default()) {
        Ok(checked) => (checked.source_map, checked.warnings),
        Err((map, ds)) => (map, ds),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::scratch::Scratch;

    const MANIFEST: &str = "format = 2\nentry = \"main\"\n";

    #[test]
    fn run_file_resolves_imports_from_the_project() {
        let s = Scratch::new();
        s.write("obfusku.toml", MANIFEST);
        s.write("lib.obk", "one ≔⟳ 1\n❧\n");
        let main = s.write("main.obk", "⟲ lib\nr ≔ one ✚ 41\n❧\n");
        let (result, _) = run_file(&main).unwrap();
        assert!(matches!(result.0, Value::Int(42)));
    }

    #[test]
    fn a_missing_import_fails_with_a_renderable_map() {
        let s = Scratch::new();
        let main = s.write("main.obk", "⟲ ghost\nr ≔ 1\n❧\n");
        let (map, diags) = run_file(&main).unwrap_err();
        let text = crate::report::render::diagnostics_map(&map, &diags);
        assert!(text.contains("ghost"), "{text}");
    }

    #[test]
    fn program_arguments_reach_the_program() {
        let s = Scratch::new();
        let main = s.write("main.obk", "args ≔ ⌁↓⌥(∅)\n❧\n");
        let (result, _) = run_file_with_args(&main, &["x".to_string()]).unwrap();
        assert!(matches!(&result.0, Value::Adt { tag, .. } if &**tag == "⁝"));
    }

    #[test]
    fn a_denied_capability_blocks_the_check() {
        let s = Scratch::new();
        s.write(
            "obfusku.toml",
            "format = 2\nentry = \"main\"\n[capabilities]\nconsole = true\n",
        );
        s.write("main.obk", "e ≔ ⌁↓⎈(\"HOME\")\n❧\n");
        let (_, diags) = check_file(s.root()).unwrap_err();
        assert!(diags.iter().any(|d| d.message.contains("'env' capability")));
    }

    #[test]
    fn a_failure_with_no_source_still_has_a_file_to_render_against() {
        let s = Scratch::new();
        let (map, diags) = check_file(&s.root().join("absent.obk")).unwrap_err();
        // Must not panic: the span-less diagnostic needs *a* file in the map.
        assert!(!crate::report::render::diagnostics_map(&map, &diags).is_empty());
    }

    #[test]
    fn build_needs_a_manifest_and_check_does_not() {
        let s = Scratch::new();
        let f = s.write("lone.obk", "r ≔ 1\n❧\n");
        assert!(check_file(&f).is_ok());
        assert!(build_project(&f).is_err());
        s.write("obfusku.toml", MANIFEST);
        s.write("main.obk", "r ≔ 1\n❧\n");
        assert!(build_project(s.root()).is_ok());
    }

    #[test]
    fn analyze_text_checks_an_unsaved_buffer_against_its_directory() {
        let s = Scratch::new();
        s.write("lib.obk", "one ≔⟳ 1\n❧\n");
        let unsaved = s.root().join("new.obk");
        let (_, diags) = analyze_text(&unsaved, "⟲ lib\nr ≔ one\n❧\n");
        assert!(diags.is_empty(), "{diags:?}");
        let (_, diags) = analyze_text(&unsaved, "r ≔ 1 ✚ \"x\"\n❧\n");
        assert!(!diags.is_empty());
    }

    #[test]
    fn a_runtime_failure_carries_call_notes() {
        let s = Scratch::new();
        let main = s.write("main.obk", "λf(n: ⟁): ⟁ → n ÷ 0\nr ≔ f(1)\n❧\n");
        let (_, diags) = run_file(&main).unwrap_err();
        assert!(diags.len() >= 2, "{diags:?}");
        assert!(
            diags
                .iter()
                .any(|d| d.message.contains("called 'f' from here"))
        );
    }
}
