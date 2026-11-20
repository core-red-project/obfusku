//! Rejecting duplicate names before any inference runs.
//!
//! A flat binding list has no way to give a reused name real lexical
//! shadowing, and a reused constructor tag would make pattern and
//! constructor resolution ambiguous, so both are static errors found up
//! front.

use super::Prelude;
use obfusku_core::ast::{Module, TypeDecl};
use obfusku_diagnostics::{Diagnostic, Span};
use std::collections::HashMap;

/// Fails if any top-level binding reuses a prelude name or another binding's
/// name.
///
/// A repeated binding is reported as an error at the repeat plus a `Note` at
/// the first occurrence. This crate has no `SourceMap`, so it cannot render
/// the first span as `line:col` itself; a separate note lets the renderer,
/// which does, resolve it exactly as it resolves the primary location.
pub(super) fn reject_binding_clashes(
    module: &Module,
    prelude: &Prelude,
) -> Result<(), Vec<Diagnostic>> {
    let mut first_seen: HashMap<&str, Span> = HashMap::new();
    let mut errors = Vec::new();
    for binding in module.all_bindings() {
        let name = binding.name.as_str();
        if prelude.contains_key(name) {
            // The prelude is one flat merge of imports, the ambient stdlib
            // and ambient natives, and this crate cannot tell which a name
            // came from, so the message must be accurate for all three.
            errors.push(Diagnostic::error(
                format!(
                    "'{name}' is already bound by the ambient prelude (an import, or a \
                     stdlib/native name)"
                ),
                binding.span,
            ));
        } else if let Some(&first) = first_seen.get(name) {
            errors.push(Diagnostic::error(
                format!("'{name}' is already bound in this module"),
                binding.span,
            ));
            errors.push(Diagnostic::note(
                format!("'{name}' first bound here"),
                first,
            ));
        } else {
            first_seen.insert(name, binding.span);
        }
    }
    into_result(errors)
}

/// Fails if two declarations (local or imported) share a type name or a
/// constructor tag.
///
/// Type names and constructor tags are separate namespaces, so a record or
/// tuple may share its single variant's name with its type. `Exception` is
/// exempt as a type name because declaring it extends the built-in one
/// (`ADR-026`); its variant tags are still checked.
pub(super) fn reject_type_clashes(type_decls: &[TypeDecl]) -> Result<(), Vec<Diagnostic>> {
    let mut type_names: HashMap<&str, Span> = HashMap::new();
    let mut variant_tags: HashMap<&str, Span> = HashMap::new();
    let mut errors = Vec::new();
    for decl in type_decls {
        if decl.tag != crate::adt::EXCEPTION {
            record(&mut type_names, &decl.tag, decl.span, &mut errors);
        }
        for variant in &decl.variants {
            record(&mut variant_tags, &variant.tag, decl.span, &mut errors);
        }
    }
    into_result(errors)
}

/// Notes `name` as declared at `span`, or reports the clash with the first
/// declaration.
fn record<'a>(
    seen: &mut HashMap<&'a str, Span>,
    name: &'a str,
    span: Span,
    errors: &mut Vec<Diagnostic>,
) {
    match seen.get(name) {
        Some(&first) => {
            errors.push(Diagnostic::error(
                format!("'{name}' is declared by more than one type (locally or via an import)"),
                span,
            ));
            errors.push(Diagnostic::note(
                format!("'{name}' first declared here"),
                first,
            ));
        }
        None => {
            seen.insert(name, span);
        }
    }
}

fn into_result(errors: Vec<Diagnostic>) -> Result<(), Vec<Diagnostic>> {
    if errors.is_empty() { Ok(()) } else { Err(errors) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Scheme, Type};
    use obfusku_core::ast::{Binding, BindingGroup, Expr, Literal, VariantDecl};
    use obfusku_diagnostics::{Severity, SourceId};

    fn at(offset: u32) -> Span {
        Span::point(SourceId(0), offset)
    }

    fn binding(name: &str, offset: u32) -> BindingGroup {
        BindingGroup::Let(Binding {
            name: name.to_string(),
            value: Expr::Lit(Literal::Int(0), at(offset)),
            exported: false,
            span: at(offset),
            declared_type: None,
        })
    }

    fn module(bindings: Vec<BindingGroup>) -> Module {
        Module {
            bindings,
            ..Module::default()
        }
    }

    fn decl(tag: &str, variants: &[&str], offset: u32) -> TypeDecl {
        TypeDecl {
            tag: tag.to_string(),
            type_params: vec![],
            variants: variants
                .iter()
                .map(|v| VariantDecl {
                    tag: (*v).to_string(),
                    fields: vec![],
                    field_names: None,
                    span: at(offset),
                })
                .collect(),
            span: at(offset),
        }
    }

    #[test]
    fn distinct_names_pass() {
        let m = module(vec![binding("a", 0), binding("b", 1)]);
        assert!(reject_binding_clashes(&m, &Prelude::new()).is_ok());
    }

    #[test]
    fn a_repeat_is_an_error_at_the_repeat_and_a_note_at_the_first() {
        let m = module(vec![binding("a", 0), binding("a", 9)]);
        let errors = reject_binding_clashes(&m, &Prelude::new()).expect_err("duplicate");
        assert_eq!(errors.len(), 2);
        assert_eq!(errors[0].severity, Severity::Error);
        assert_eq!(errors[0].primary, at(9));
        assert_eq!(errors[1].severity, Severity::Note);
        assert_eq!(errors[1].primary, at(0));
    }

    #[test]
    fn a_name_in_the_prelude_is_rejected_without_a_note() {
        let mut prelude = Prelude::new();
        prelude.insert("print".into(), Scheme::monomorphic(Type::Unit));
        let errors = reject_binding_clashes(&module(vec![binding("print", 0)]), &prelude)
            .expect_err("clash");
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("ambient prelude"));
    }

    #[test]
    fn a_type_declared_twice_is_reported_with_its_first_site() {
        let decls = [decl("T", &["A"], 0), decl("T", &["B"], 5)];
        let errors = reject_type_clashes(&decls).expect_err("duplicate");
        assert_eq!(errors.len(), 2);
        assert_eq!(errors[1].primary, at(0));
    }

    #[test]
    fn a_constructor_tag_reused_across_types_is_reported() {
        let decls = [decl("T", &["A"], 0), decl("U", &["A"], 5)];
        assert_eq!(reject_type_clashes(&decls).expect_err("clash").len(), 2);
    }

    #[test]
    fn a_record_may_share_its_name_with_its_only_constructor() {
        assert!(reject_type_clashes(&[decl("Point", &["Point"], 0)]).is_ok());
    }

    #[test]
    fn exception_may_be_extended_but_not_its_variants_redeclared() {
        let extend = [decl("Exception", &["Timeout"], 0), decl("Exception", &["Disk"], 5)];
        assert!(reject_type_clashes(&extend).is_ok());
        let clash = [decl("Exception", &["Timeout"], 0), decl("Exception", &["Timeout"], 5)];
        assert!(reject_type_clashes(&clash).is_err());
    }
}
