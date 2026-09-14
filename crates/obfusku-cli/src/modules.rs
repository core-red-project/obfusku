//! Module resolver: resolves `ImportDeclaration`s to same-directory
//! `<module_name>.obk` files, recursively, with cycle detection.
//!
//! Each imported module is independently parsed, desugared, type-checked,
//! and evaluated. Only exported (`⟳`) bindings are provided to the importer.

use obfusku_diagnostics::{Diagnostic, Severity, SourceMap, Span};
use obfusku_runtime::Value;
use obfusku_syntax::ast::Declaration;
use obfusku_typecheck::{Prelude, Scheme, TypeResult};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// A fully resolved module's exported surface — what an importer sees.
#[derive(Clone, Default)]
pub struct ResolvedModule {
    pub types: Prelude,
    pub values: HashMap<String, Value>,
    /// Every `TypeDecl` declared by this module. All types are exported
    /// so that constructor tags and patterns resolve in importing modules.
    pub type_decls: Vec<obfusku_core::ast::TypeDecl>,
}

fn io_error(path: &Path, e: std::io::Error) -> Diagnostic {
    Diagnostic {
        severity: Severity::Error,
        message: format!("could not read imported module '{}': {e}", path.display()),
        primary: Span::default(),
    }
}

fn circular_import_error(path: &Path) -> Diagnostic {
    Diagnostic {
        severity: Severity::Error,
        message: format!(
            "circular import detected: '{}' is already being resolved",
            path.display()
        ),
        primary: Span::default(),
    }
}

/// Built-in exception constructors synthesized into every module,
/// excluded from export to avoid collision on import.
pub(crate) const BUILTIN_EXPORTS: [&str; 5] = [
    "DivisionByZero",
    "NonExhaustiveMatch",
    "InvalidOperation",
    "Failure",
    "IntegerOverflow",
];

fn collision_error(name: &str, span: Span) -> Diagnostic {
    Diagnostic {
        severity: Severity::Error,
        message: format!("'{name}' is exported by more than one imported module"),
        primary: span,
    }
}

/// Resolves `surface`'s `ImportDeclaration`s, returning imported types,
/// values, and type declarations. Caches modules by canonical path to
/// ensure each module is resolved once.
#[allow(clippy::type_complexity)]
pub fn resolve_imports(
    surface: &obfusku_syntax::ast::Module,
    base_dir: &Path,
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
        let imported_path = base_dir.join(format!("{}.obk", import.module_name));
        match resolve_module(&imported_path, source_map, cache, resolving) {
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
/// (recursively) → desugar → type-check → evaluate, returning only its
/// exported surface. Cached by canonical path; cycle-checked via
/// `resolving`. `path`'s text is registered in `source_map` even on
/// failure, so any diagnostic returned can still be rendered.
pub fn resolve_module(
    path: &Path,
    source_map: &mut SourceMap,
    cache: &mut HashMap<PathBuf, ResolvedModule>,
    resolving: &mut HashSet<PathBuf>,
) -> Result<ResolvedModule, Vec<Diagnostic>> {
    let canonical = path.canonicalize().map_err(|e| vec![io_error(path, e)])?;
    if let Some(cached) = cache.get(&canonical) {
        return Ok(cached.clone());
    }
    if !resolving.insert(canonical.clone()) {
        return Err(vec![circular_import_error(path)]);
    }

    let result = resolve_module_uncached(path, source_map, cache, resolving);
    resolving.remove(&canonical);

    let resolved = result?;
    cache.insert(canonical, resolved.clone());
    Ok(resolved)
}

fn resolve_module_uncached(
    path: &Path,
    source_map: &mut SourceMap,
    cache: &mut HashMap<PathBuf, ResolvedModule>,
    resolving: &mut HashSet<PathBuf>,
) -> Result<ResolvedModule, Vec<Diagnostic>> {
    let source = std::fs::read_to_string(path).map_err(|e| vec![io_error(path, e)])?;
    let base_dir = path.parent().unwrap_or_else(|| Path::new("."));

    let source_id = source_map.add_file(&source);
    let tokens = obfusku_syntax::lexer::tokenize(&source, source_id).map_err(|d| vec![d])?;
    let surface = obfusku_syntax::parser::parse(&tokens, source_id)?;

    let (type_prelude, value_prelude, imported_type_decls) =
        resolve_imports(&surface, base_dir, source_map, cache, resolving)?;

    let core_module = obfusku_syntax::desugar::desugar(&surface)?;
    let typed = obfusku_typecheck::check_with_prelude_and_types(
        &core_module,
        type_prelude,
        imported_type_decls,
    )?;
    let evaluated =
        obfusku_runtime::evaluate_with_prelude(&core_module, value_prelude).map_err(|d| vec![d])?;

    let mut types = Prelude::new();
    for tb in &typed.bindings {
        if !tb.exported || BUILTIN_EXPORTS.contains(&tb.name.as_str()) {
            continue;
        }
        let scheme = match &tb.ty {
            TypeResult::Monomorphic(core_ty) => Scheme::monomorphic(
                obfusku_typecheck::instantiate_core_type(core_ty, &HashMap::new()),
            ),
            TypeResult::Polymorphic(scheme) => scheme.clone(),
        };
        types.insert(tb.name.clone(), scheme);
    }

    let mut values = HashMap::new();
    for (name, exported, value) in evaluated {
        if exported && !BUILTIN_EXPORTS.contains(&name.as_str()) {
            values.insert(name, value);
        }
    }

    // All type declarations are exported for pattern resolution.
    let type_decls = core_module.type_decls.clone();

    Ok(ResolvedModule {
        types,
        values,
        type_decls,
    })
}
