//! Module resolver: resolves `ImportDeclaration`s to `.obk` files
//! anywhere under the Project root, by bare-name search
//! (`ADR-017`/`ADR-018`), recursively, with cycle detection.
//!
//! Each imported module is independently parsed, desugared, type-checked,
//! and evaluated. Only exported (`⟳`) bindings are provided to the importer.

use super::deps::{AUXILIARY_DIRS, Packages};
use super::search::{ModuleSearch, find_module_skipping};
use crate::project::exports::{export_types, export_values};
use crate::report::host_error::host_error;
use obfusku_diagnostics::{Diagnostic, SourceMap, Span};
use obfusku_runtime::Value;
use obfusku_syntax::ast::Declaration;
use obfusku_typecheck::Prelude;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Where module text comes from: the filesystem, or a bundle's embedded
/// sources (`ADR-022`) keyed by virtual path.
#[derive(Clone, Copy, Default)]
pub enum ModuleSource<'a> {
    /// Read from the filesystem.
    #[default]
    Disk,
    /// Look up in an in-memory map.
    Memory(&'a HashMap<PathBuf, String>),
}

/// Everything the resolver needs from a source, so it is written once and
/// works for disk and bundles alike.
impl ModuleSource<'_> {
    /// `skip` names directories that are not searched (hidden ones never
    /// are): a dependency's tests and examples.
    fn find(self, root: &Path, name: &str, skip: &[&str]) -> ModuleSearch {
        match self {
            ModuleSource::Disk => find_module_skipping(root, name, skip),
            ModuleSource::Memory(files) => {
                let visible = |p: &Path| {
                    p.strip_prefix(root).is_ok_and(|rel| {
                        let dirs = rel.parent().into_iter().flat_map(Path::components);
                        dirs.map(|c| c.as_os_str().to_string_lossy().into_owned())
                            .all(|c| !c.starts_with('.') && !skip.contains(&c.as_str()))
                    })
                };
                let mut matches: Vec<PathBuf> = files
                    .keys()
                    .filter(|p| visible(p) && p.file_stem().is_some_and(|s| s == name))
                    .cloned()
                    .collect();
                matches.sort();
                match matches.len() {
                    0 => ModuleSearch::NotFound,
                    1 => ModuleSearch::Found(matches.remove(0)),
                    _ => ModuleSearch::Ambiguous(matches),
                }
            }
        }
    }

    pub(crate) fn read(self, path: &Path) -> std::io::Result<String> {
        match self {
            ModuleSource::Disk => std::fs::read_to_string(path),
            ModuleSource::Memory(files) => files
                .get(path)
                .cloned()
                .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::NotFound)),
        }
    }

    fn canonical(self, path: &Path) -> std::io::Result<PathBuf> {
        match self {
            ModuleSource::Disk => path.canonicalize(),
            ModuleSource::Memory(files) if files.contains_key(path) => Ok(path.to_path_buf()),
            ModuleSource::Memory(_) => Err(std::io::ErrorKind::NotFound.into()),
        }
    }
}

/// The names every module sees without importing: the stdlib, natives and
/// the type declarations behind them.
#[derive(Clone, Default)]
pub struct Ambient {
    /// Type schemes of ambient values.
    pub types: Prelude,
    /// Ambient runtime values.
    pub values: HashMap<String, Value>,
    /// ADTs declared by the stdlib.
    pub type_decls: Vec<obfusku_core::ast::TypeDecl>,
    /// Which package each import is searched in (`ADR-029`).
    pub packages: std::rc::Rc<Packages>,
}

/// What imports may shadow, and the one place that rule lives.
impl Ambient {
    /// Ambient value names a module may redefine locally (`ADR-028`):
    /// not re-bound by one of its imports, and not a constructor.
    #[must_use]
    pub fn shadowable(&self, imported: &Prelude) -> HashSet<String> {
        let constructors: HashSet<&str> = self
            .type_decls
            .iter()
            .flat_map(|td| td.variants.iter().map(|v| v.tag.as_str()))
            .collect();
        self.types
            .keys()
            .filter(|n| !imported.contains_key(*n) && !constructors.contains(n.as_str()))
            .cloned()
            .collect()
    }
}

/// A fully resolved module's exported surface — what an importer sees.
#[derive(Clone, Default)]
pub struct ResolvedModule {
    /// Type schemes of exported values.
    pub types: Prelude,
    /// Exported runtime values.
    pub values: HashMap<String, Value>,
    /// Every `TypeDecl` declared by this module. All types are exported
    /// so that constructor tags and patterns resolve in importing modules.
    pub type_decls: Vec<obfusku_core::ast::TypeDecl>,
}

/// An error anchored at the import that caused it.
fn error_at(span: Span, message: String) -> Diagnostic {
    Diagnostic::error(message, span)
}

fn io_error(path: &Path, e: &std::io::Error) -> Diagnostic {
    host_error(format!(
        "could not read imported module '{}': {e}",
        path.display()
    ))
}

fn circular_import_error(path: &Path) -> Diagnostic {
    host_error(format!(
        "circular import detected: '{}' is already being resolved",
        path.display()
    ))
}

fn not_found_error(name: &str, span: Span) -> Diagnostic {
    error_at(
        span,
        format!(
            "'{name}' does not resolve to any '.obk' file under this Project (\u{2192} \
             {name}.obk not found anywhere in the Project's own module tree)"
        ),
    )
}

fn ambiguous_error(name: &str, matches: &[PathBuf], span: Span) -> Diagnostic {
    let listed: Vec<String> = matches.iter().map(|p| p.display().to_string()).collect();
    error_at(
        span,
        format!(
            "'{name}' is ambiguous — more than one module named '{name}.obk' exists under this \
             Project: {}",
            listed.join(", ")
        ),
    )
}

fn collision_error(name: &str, span: Span) -> Diagnostic {
    error_at(
        span,
        format!("'{name}' is exported by more than one imported module"),
    )
}

/// A module is looked up in its own package first; only when it is absent
/// there are the package's direct dependencies searched, and two
/// dependencies offering the same name are ambiguous. Returns the root of
/// the package the module belongs to, which scopes its own imports.
fn search_packages(
    sources: ModuleSource,
    root: &Path,
    name: &str,
    packages: &Packages,
) -> (ModuleSearch, PathBuf) {
    match sources.find(root, name, &[]) {
        ModuleSearch::NotFound => {}
        found => return (found, root.to_path_buf()),
    }
    let mut hits: Vec<(PathBuf, PathBuf)> = Vec::new();
    for dep_root in packages.dependency_roots(root) {
        match sources.find(dep_root, name, &AUXILIARY_DIRS) {
            ModuleSearch::Found(path) => hits.push((path, dep_root.clone())),
            ModuleSearch::Ambiguous(paths) => {
                return (ModuleSearch::Ambiguous(paths), dep_root.clone());
            }
            ModuleSearch::NotFound => {}
        }
    }
    match hits.len() {
        0 => (ModuleSearch::NotFound, root.to_path_buf()),
        1 => {
            let (path, owner) = hits.remove(0);
            (ModuleSearch::Found(path), owner)
        }
        _ => (
            ModuleSearch::Ambiguous(hits.into_iter().map(|(p, _)| p).collect()),
            root.to_path_buf(),
        ),
    }
}

/// Resolves `surface`'s `ImportDeclaration`s, returning imported types,
/// values, and type declarations. Caches modules by canonical path to
/// ensure each module is resolved once. `project_root` is the boundary
/// every import is searched under (`ADR-018`) — the same root used for
/// filesystem natives (`ADR-019`).
#[allow(clippy::type_complexity)]
pub fn resolve_imports(
    surface: &obfusku_syntax::ast::Module,
    project_root: &Path,
    sources: ModuleSource,
    ambient: &Ambient,
    source_map: &mut SourceMap,
    cache: &mut HashMap<PathBuf, ResolvedModule>,
    resolving: &mut HashSet<PathBuf>,
) -> Result<
    (
        Prelude,
        HashMap<String, Value>,
        Vec<obfusku_core::ast::TypeDecl>,
    ),
    Vec<Diagnostic>,
> {
    let mut types = Prelude::new();
    let mut values = HashMap::new();
    let mut type_decls = Vec::new();
    let mut errors = Vec::new();

    for decl in &surface.declarations {
        let Declaration::Import(import) = decl else {
            continue;
        };
        let (found, owner_root) = search_packages(
            sources,
            project_root,
            &import.module_name,
            &ambient.packages,
        );
        let imported_path = match found {
            ModuleSearch::Found(path) => path,
            ModuleSearch::NotFound => {
                errors.push(not_found_error(&import.module_name, import.span));
                continue;
            }
            ModuleSearch::Ambiguous(matches) => {
                errors.push(ambiguous_error(&import.module_name, &matches, import.span));
                continue;
            }
        };
        match resolve_module(
            &imported_path,
            &owner_root,
            sources,
            ambient,
            source_map,
            cache,
            resolving,
        ) {
            Ok(resolved) => {
                for (name, scheme) in &resolved.types {
                    if types.contains_key(name) {
                        errors.push(collision_error(name, import.span));
                        continue;
                    }
                    types.insert(name.clone(), scheme.clone());
                }
                for (name, v) in &resolved.values {
                    values.insert(name.clone(), v.clone());
                }
                type_decls.extend(resolved.type_decls.iter().cloned());
            }
            Err(mut ds) => errors.append(&mut ds),
        }
    }

    if errors.is_empty() {
        Ok((types, values, type_decls))
    } else {
        Err(errors)
    }
}

/// Fully resolves one `.obk` file: parse → resolve its own imports
/// (recursively, against the same `project_root`) → desugar →
/// type-check → evaluate, returning only its exported surface. Cached
/// by canonical path; cycle-checked via `resolving`. `path`'s text is
/// registered in `source_map` even on failure, so any diagnostic
/// returned can still be rendered.
pub fn resolve_module(
    path: &Path,
    project_root: &Path,
    sources: ModuleSource,
    ambient: &Ambient,
    source_map: &mut SourceMap,
    cache: &mut HashMap<PathBuf, ResolvedModule>,
    resolving: &mut HashSet<PathBuf>,
) -> Result<ResolvedModule, Vec<Diagnostic>> {
    let canonical = sources
        .canonical(path)
        .map_err(|e| vec![io_error(path, &e)])?;
    if let Some(cached) = cache.get(&canonical) {
        return Ok(cached.clone());
    }
    if !resolving.insert(canonical.clone()) {
        return Err(vec![circular_import_error(path)]);
    }

    let result = resolve_module_uncached(
        path,
        project_root,
        sources,
        ambient,
        source_map,
        cache,
        resolving,
    );
    resolving.remove(&canonical);

    let resolved = result?;
    cache.insert(canonical, resolved.clone());
    Ok(resolved)
}

/// The body of [`resolve_module`], after the cache and cycle checks. Kept
/// separate so those checks wrap every exit path, including `?`.
fn resolve_module_uncached(
    path: &Path,
    project_root: &Path,
    sources: ModuleSource,
    ambient: &Ambient,
    source_map: &mut SourceMap,
    cache: &mut HashMap<PathBuf, ResolvedModule>,
    resolving: &mut HashSet<PathBuf>,
) -> Result<ResolvedModule, Vec<Diagnostic>> {
    let source = sources.read(path).map_err(|e| vec![io_error(path, &e)])?;

    let source_id = source_map.add_file(&source);
    let tokens = obfusku_syntax::lexer::tokenize(&source, source_id).map_err(|d| vec![d])?;
    let surface = obfusku_syntax::parser::parse(&tokens, source_id)?;

    let (import_types, import_values, import_type_decls) = resolve_imports(
        &surface,
        project_root,
        sources,
        ambient,
        source_map,
        cache,
        resolving,
    )?;
    let shadowable = ambient.shadowable(&import_types);
    let mut type_prelude = ambient.types.clone();
    type_prelude.extend(import_types);
    let mut value_prelude = ambient.values.clone();
    value_prelude.extend(import_values);
    let mut imported_type_decls = ambient.type_decls.clone();
    imported_type_decls.extend(import_type_decls);

    let core_module = obfusku_syntax::desugar::desugar_with_ambient(
        &surface,
        &HashSet::new(),
        &imported_type_decls,
    )?;
    let typed = obfusku_typecheck::check_shadowing(
        &core_module,
        type_prelude,
        &imported_type_decls,
        &shadowable,
    )?;
    let evaluated =
        obfusku_runtime::evaluate_with_prelude(&core_module, value_prelude).map_err(|d| vec![d])?;

    let types = export_types(&typed, true);
    let values = export_values(evaluated, true);

    // All type declarations are exported for pattern resolution.
    let type_decls = core_module.type_decls.clone();

    Ok(ResolvedModule {
        types,
        values,
        type_decls,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn memory(files: &[&str]) -> HashMap<PathBuf, String> {
        files
            .iter()
            .map(|p| (PathBuf::from(p), String::new()))
            .collect()
    }

    #[test]
    fn an_in_memory_module_is_found_by_stem_under_the_root() {
        let files = memory(&["/r/main.obk", "/r/lib/util.obk"]);
        let src = ModuleSource::Memory(&files);
        assert!(matches!(src.find(Path::new("/r"), "util", &[]),
            ModuleSearch::Found(p) if p == Path::new("/r/lib/util.obk")));
        assert!(matches!(
            src.find(Path::new("/r"), "nope", &[]),
            ModuleSearch::NotFound
        ));
    }

    #[test]
    fn in_memory_search_has_the_same_hiding_rules_as_disk() {
        let files = memory(&["/r/.deps/0/m.obk", "/r/tests/t.obk"]);
        let src = ModuleSource::Memory(&files);
        assert!(matches!(
            src.find(Path::new("/r"), "m", &[]),
            ModuleSearch::NotFound
        ));
        assert!(matches!(
            src.find(Path::new("/r"), "t", &["tests"]),
            ModuleSearch::NotFound
        ));
    }

    #[test]
    fn in_memory_ambiguity_is_reported_in_sorted_order() {
        let files = memory(&["/r/b/m.obk", "/r/a/m.obk"]);
        let ModuleSearch::Ambiguous(p) =
            ModuleSource::Memory(&files).find(Path::new("/r"), "m", &[])
        else {
            panic!("expected ambiguity");
        };
        assert_eq!(
            p,
            [PathBuf::from("/r/a/m.obk"), PathBuf::from("/r/b/m.obk")]
        );
    }

    #[test]
    fn reading_a_missing_in_memory_module_is_not_found() {
        let files = memory(&["/r/a.obk"]);
        let src = ModuleSource::Memory(&files);
        assert!(src.read(Path::new("/r/a.obk")).is_ok());
        assert_eq!(
            src.read(Path::new("/r/b.obk")).unwrap_err().kind(),
            std::io::ErrorKind::NotFound
        );
    }

    #[test]
    fn shadowable_excludes_imported_names_and_constructors() {
        use obfusku_typecheck::Scheme;
        let scheme = Scheme::monomorphic(obfusku_typecheck::Type::Int);
        let ambient = Ambient {
            types: Prelude::from([
                ("keep".to_string(), scheme.clone()),
                ("imported".to_string(), scheme.clone()),
            ]),
            ..Ambient::default()
        };
        let imported = Prelude::from([("imported".to_string(), scheme)]);
        let shadowable = ambient.shadowable(&imported);
        assert!(shadowable.contains("keep") && !shadowable.contains("imported"));
    }
}
