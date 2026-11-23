//! Lowering a whole module: bindings, recursion groups, types, exceptions.

use super::constructor::constructor_binding;
use super::lowerer::{Lowered, Lowerer};
use super::types::{lower_type_decl, lower_type_ref, lower_variant};
use crate::ast::{self as surface, Declaration, ValueDeclaration, VariantDecl};
use obfusku_core::ast::{self as core, BindingGroup};
use obfusku_core::exception::BUILTIN_EXCEPTIONS;
use obfusku_core::types::Type;
use obfusku_diagnostics::{Diagnostic, Span};
use std::collections::{HashMap, HashSet};

/// The module being built.
#[derive(Default)]
struct Output {
    type_decls: Vec<core::TypeDecl>,
    bindings: Vec<BindingGroup>,
    errors: Vec<Diagnostic>,
    /// The `LetRec` group being collected: one maximal run of adjacent
    /// function declarations (§8.2), decided purely from declaration kind.
    run: Vec<core::Binding>,
}

impl Output {
    /// Closes the current function run into a `LetRec` group.
    fn flush_run(&mut self) {
        if !self.run.is_empty() {
            self.bindings
                .push(BindingGroup::LetRec(std::mem::take(&mut self.run)));
        }
    }

    /// Adds a single (non-recursive) binding.
    fn push_let(&mut self, binding: core::Binding) {
        self.bindings.push(BindingGroup::Let(binding));
    }

    /// Keeps a lowering's value, or records its errors and keeps going.
    fn keep<T>(&mut self, result: Lowered<T>) -> Option<T> {
        match result {
            Ok(value) => Some(value),
            Err(mut errors) => {
                self.errors.append(&mut errors);
                None
            }
        }
    }
}

pub(super) fn lower_module(
    surface: &surface::Module,
    ambient_mut_names: &HashSet<String>,
    ambient_types: &[core::TypeDecl],
) -> Result<core::Module, Vec<Diagnostic>> {
    let mut record_fields = HashMap::new();
    for variant in ambient_types.iter().flat_map(|t| &t.variants) {
        if let Some(names) = &variant.field_names {
            record_fields.insert(variant.tag.clone(), names.clone());
        }
    }
    let mut lowerer = Lowerer::new(ambient_mut_names.clone(), record_fields);
    let mut out = Output::default();

    // §15.2: `Exception`'s built-in constructors are synthesized here rather
    // than from a surface declaration. The table is shared with the checker
    // and the runtime (`obfusku_core::exception`).
    for (tag, arity) in BUILTIN_EXCEPTIONS {
        out.push_let(constructor_binding(tag, arity, Span::default()));
    }

    for declaration in &surface.declarations {
        if !matches!(declaration, Declaration::Function(_)) {
            out.flush_run();
        }
        match declaration {
            Declaration::Function(function) => {
                let binding = lowerer.lower_function_binding(function, function.is_exported);
                if let Some(binding) = out.keep(binding) {
                    out.run.push(binding);
                }
            }
            // Imports resolve to nothing in Core: the module resolver acts on
            // `surface.declarations` before desugaring and splices the
            // imported module's exports in as ordinary pre-existing names.
            Declaration::Import(_) => {}
            Declaration::Exception(variant) => lower_exception(&mut out, variant),
            Declaration::Type(decl) => lower_type_declaration(&mut lowerer, &mut out, decl),
            Declaration::Value(decl) => lower_value_declaration(&mut lowerer, &mut out, decl),
        }
    }
    out.flush_run();

    if out.errors.is_empty() {
        Ok(core::Module {
            type_decls: out.type_decls,
            bindings: out.bindings,
        })
    } else {
        Err(out.errors)
    }
}

/// `ADR-026`: one more variant of the open `Exception` type, carried as a
/// Core `TypeDecl` named `Exception` that the checker merges into the
/// built-in one, plus its constructor.
fn lower_exception(out: &mut Output, variant: &VariantDecl) {
    out.type_decls.push(core::TypeDecl {
        tag: "Exception".to_string(),
        type_params: Vec::new(),
        variants: vec![lower_variant(variant)],
        span: variant.span,
    });
    out.push_let(constructor_binding(
        &variant.tag,
        variant.fields.len(),
        variant.span,
    ));
}

/// §9: each variant becomes one constructor binding. Record field names are
/// registered *before* anything after this declaration is lowered, so a
/// record is usable by name from the next declaration on.
fn lower_type_declaration(
    lowerer: &mut Lowerer,
    out: &mut Output,
    decl: &surface::TypeDeclaration,
) {
    for variant in &decl.variants {
        if let Some(field_names) = &variant.field_names {
            lowerer
                .record_fields
                .insert(variant.tag.clone(), field_names.clone());
        }
    }
    out.type_decls.push(lower_type_decl(decl));
    for variant in &decl.variants {
        out.push_let(constructor_binding(
            &variant.tag,
            variant.fields.len(),
            variant.span,
        ));
    }
}

fn lower_value_declaration(lowerer: &mut Lowerer, out: &mut Output, decl: &ValueDeclaration) {
    let Some(value) = out.keep(lowerer.lower_binding_value(decl, None)) else {
        return;
    };
    out.push_let(core::Binding {
        name: decl.name.clone(),
        value,
        is_exported: decl.is_exported,
        span: decl.span,
        declared_type: declared_value_type(decl),
    });
    if decl.is_mutable {
        lowerer.mut_names.insert(decl.name.clone());
    }
}

/// The checker's constraint for a value's annotation.
///
/// A `˚` binding's expression is wrapped in `MutCell`, so its inferred type
/// is `Cell<T>`, while the annotation names the *element* type
/// (`x : ⟁ ≔˚ 0`, not `x : Cell<⟁> ≔˚ 0`). It is wrapped the same way, or it
/// would be a guaranteed mismatch instead of a real constraint.
fn declared_value_type(decl: &ValueDeclaration) -> Option<Type> {
    decl.ty.as_ref().map(|annotation| {
        let ty = lower_type_ref(annotation);
        if decl.is_mutable { Type::cell(ty) } else { ty }
    })
}

#[cfg(test)]
mod tests {
    use crate::desugar::{desugar, desugar_with_ambient};
    use crate::lexer::tokenize;
    use crate::parser::parse;
    use obfusku_core::ast::{self as core, BindingGroup};
    use obfusku_core::exception::BUILTIN_EXCEPTIONS;
    use obfusku_core::types::Type;
    use obfusku_diagnostics::{Diagnostic, SourceId};
    use std::collections::HashSet;

    fn surface(source: &str) -> crate::ast::Module {
        let tokens = tokenize(source, SourceId(0)).unwrap();
        parse(&tokens, SourceId(0)).unwrap()
    }

    fn lower(source: &str) -> Result<core::Module, Vec<Diagnostic>> {
        desugar(&surface(source))
    }

    /// The module's own bindings, without the built-in exception constructors.
    fn own(module: &core::Module) -> &[BindingGroup] {
        &module.bindings[BUILTIN_EXCEPTIONS.len()..]
    }

    #[test]
    fn every_module_starts_with_the_builtin_exception_constructors() {
        let m = lower("❧").unwrap();
        let names: Vec<_> = m.bound_names().collect();
        let expected: Vec<_> = BUILTIN_EXCEPTIONS.iter().map(|(t, _)| *t).collect();
        assert_eq!(names, expected);
        assert!(own(&m).is_empty());
    }

    #[test]
    fn a_value_declaration_is_a_single_let() {
        let m = lower("x ≔⟳ 1\n❧").unwrap();
        let [BindingGroup::Let(b)] = own(&m) else {
            panic!("expected one let");
        };
        assert!(b.is_exported && b.name == "x");
    }

    #[test]
    fn adjacent_functions_form_one_letrec_and_a_value_splits_the_run() {
        let src = "λf(a: ⟁): ⟁ → a λg(a: ⟁): ⟁ → a x ≔ 1 λh(a: ⟁): ⟁ → a\n❧";
        let m = lower(src).unwrap();
        let groups = own(&m);
        assert!(matches!(&groups[0], BindingGroup::LetRec(b) if b.len() == 2));
        assert!(matches!(&groups[1], BindingGroup::Let(_)));
        assert!(matches!(&groups[2], BindingGroup::LetRec(b) if b.len() == 1));
    }

    #[test]
    fn a_function_binding_carries_its_curried_declared_type() {
        let m = lower("λf(a: ⟁, b: ⌘): ○ → ◉\n❧").unwrap();
        let BindingGroup::LetRec(bindings) = &own(&m)[0] else {
            panic!("expected a letrec");
        };
        assert_eq!(
            bindings[0].declared_type,
            Some(Type::curried([Type::Int, Type::Str], Type::Bool))
        );
    }

    #[test]
    fn a_mutable_annotation_is_wrapped_in_a_cell() {
        let m = lower("x: ⟁ ≔ ˚ 0\n❧").unwrap();
        let BindingGroup::Let(b) = &own(&m)[0] else {
            panic!("expected a let");
        };
        assert_eq!(b.declared_type, Some(Type::cell(Type::Int)));
    }

    #[test]
    fn a_type_declaration_adds_a_decl_and_one_constructor_per_variant() {
        let m = lower("Opt a ≔ { None ⟢ Some(a) }\n❧").unwrap();
        assert_eq!(m.type_decls.len(), 1);
        let names: Vec<_> = m.bound_names().skip(BUILTIN_EXCEPTIONS.len()).collect();
        assert_eq!(names, ["None", "Some"]);
    }

    #[test]
    fn an_exception_declaration_extends_the_open_exception_type() {
        let m = lower("☈ Boom(⌘)\n❧").unwrap();
        assert_eq!(m.type_decls[0].tag, "Exception");
        assert_eq!(m.type_decls[0].variants[0].tag, "Boom");
        assert!(m.bound_names().any(|n| n == "Boom"));
    }

    #[test]
    fn an_import_lowers_to_nothing() {
        let m = lower("⟲ Foo\n❧").unwrap();
        assert!(own(&m).is_empty());
    }

    #[test]
    fn a_record_must_be_declared_before_it_is_constructed() {
        let before = "v ≔ P { x: 1 }\nP ≔ { x: ⟁ }\n❧";
        assert!(
            lower(before).unwrap_err()[0]
                .message
                .contains("not a known record type")
        );
        assert!(lower("P ≔ { x: ⟁ }\nv ≔ P { x: 1 }\n❧").is_ok());
    }

    #[test]
    fn errors_from_different_declarations_are_all_reported() {
        let errs = lower("a ≔ ◈\nb ≔ ◈\n❧").unwrap_err();
        assert_eq!(errs.len(), 2);
    }

    #[test]
    fn ambient_mutable_names_make_later_references_reads() {
        let ambient: HashSet<String> = ["c".to_string()].into();
        let m = desugar_with_ambient(&surface("v ≔ c\n❧"), &ambient, &[]).unwrap();
        let BindingGroup::Let(b) = &own(&m)[0] else {
            panic!("expected a let");
        };
        assert!(matches!(b.value, core::Expr::MutRead { .. }));
    }

    #[test]
    fn ambient_records_can_be_constructed_by_name() {
        let first = lower("P ≔ { x: ⟁ ⟢ y: ⟁ }\n❧").unwrap();
        let m = desugar_with_ambient(
            &surface("v ≔ P { y: 2 ⟢ x: 1 }\n❧"),
            &HashSet::new(),
            &first.type_decls,
        );
        assert!(m.is_ok());
    }
}
