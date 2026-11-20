//! Typing of data: constructors, arrays and record fields.

use super::annotation::instantiate_core_type;
use super::{Checker, Infer, TypeEnv, reject};
use crate::adt::VariantInfo;
use crate::types::Type;
use obfusku_core::ast::Expr;
use obfusku_diagnostics::{Diagnostic, Span};
use std::collections::HashMap;

/// A constructor seen at one use: its declaration, and the fresh variables
/// standing for its type's parameters at *this* use.
///
/// Fresh per use is what makes constructors generic: `Some(1)` and
/// `Some("a")` instantiate `Option<t>` independently.
pub(super) struct VariantInstance {
    pub(super) info: VariantInfo,
    pub(super) params: HashMap<String, Type>,
    /// The constructed type, e.g. `Option<?3>`.
    pub(super) adt_ty: Type,
}

impl VariantInstance {
    /// The declared type of field `index`, with this use's variables.
    pub(super) fn field_type(&self, index: usize) -> Type {
        instantiate_core_type(&self.info.field_types[index], &self.params)
    }
}

impl Checker {
    /// Looks `tag` up and instantiates its type parameters, or `None` if no
    /// such constructor is declared.
    pub(super) fn instantiate_variant(&mut self, tag: &str) -> Option<VariantInstance> {
        let info = self.adt_registry.variants.get(tag)?.clone();
        let params: HashMap<String, Type> = info
            .type_params
            .iter()
            .map(|p| (p.clone(), self.fresh()))
            .collect();
        let adt_ty = Type::Adt(
            info.adt_name.clone(),
            info.type_params.iter().map(|p| params[p].clone()).collect(),
        );
        Some(VariantInstance {
            info,
            params,
            adt_ty,
        })
    }

    pub(super) fn infer_constructor(
        &mut self,
        tag: &str,
        args: &[Expr],
        span: Span,
        env: &TypeEnv,
    ) -> Infer<Type> {
        let Some(variant) = self.instantiate_variant(tag) else {
            return reject(span, format!("unknown constructor '{tag}'"));
        };
        if args.len() != variant.info.field_types.len() {
            return reject(
                span,
                format!(
                    "constructor '{tag}' expects {} argument(s), found {}",
                    variant.info.field_types.len(),
                    args.len()
                ),
            );
        }
        for (index, arg) in args.iter().enumerate() {
            let arg_ty = self.infer(arg, env)?;
            let expected = variant.field_type(index);
            self.expect_same_or(&arg_ty, &expected, span, |c| {
                format!(
                    "constructor '{tag}' field type mismatch: expected {}, found {}",
                    c.show(&expected),
                    c.show(&arg_ty)
                )
            })?;
        }
        Ok(variant.adt_ty)
    }

    /// Homogeneous: every element unifies with one shared type, and an empty
    /// literal leaves it an unconstrained variable for context to settle.
    pub(super) fn infer_array(
        &mut self,
        elements: &[Expr],
        span: Span,
        env: &TypeEnv,
    ) -> Infer<Type> {
        let elem_ty = self.fresh();
        for element in elements {
            let ty = self.infer(element, env)?;
            self.expect_same_or(&elem_ty, &ty, span, |c| {
                format!(
                    "array elements must all share one type: expected {}, found {}",
                    c.show(&elem_ty),
                    c.show(&ty)
                )
            })?;
        }
        Ok(Type::array(elem_ty))
    }

    /// Bounds are a runtime condition and are never checked here.
    pub(super) fn infer_index(
        &mut self,
        array: &Expr,
        index: &Expr,
        span: Span,
        env: &TypeEnv,
    ) -> Infer<Type> {
        let array_ty = self.infer(array, env)?;
        let index_ty = self.infer(index, env)?;
        self.expect_same_or(&index_ty, &Type::Int, span, |c| {
            format!("array index must be Int, found {}", c.show(&index_ty))
        })?;
        let elem_ty = self.fresh();
        self.expect_same_or(&array_ty, &Type::array(elem_ty.clone()), span, |c| {
            format!("cannot index: {} is not an Array", c.show(&array_ty))
        })?;
        Ok(self.resolve(&elem_ty))
    }

    pub(super) fn infer_field(
        &mut self,
        base: &Expr,
        name: &str,
        span: Span,
        env: &TypeEnv,
    ) -> Infer<Type> {
        let base_ty = self.infer(base, env)?;
        self.record_field(&base_ty, name, span)
            .map_err(|d| vec![d])
    }

    /// `base ⟨ name ≔ value ⟩`: every field keeps its declared type, and the
    /// result has the base's type.
    pub(super) fn infer_update(
        &mut self,
        base: &Expr,
        fields: &[(String, Expr)],
        span: Span,
        env: &TypeEnv,
    ) -> Infer<Type> {
        let base_ty = self.infer(base, env)?;
        for (name, value) in fields {
            let value_ty = self.infer(value, env)?;
            let field_ty = self
                .record_field(&base_ty, name, span)
                .map_err(|d| vec![d])?;
            self.expect_same_or(&field_ty, &value_ty, span, |c| {
                format!(
                    "field '{name}' has type {}, but the new value has type {}",
                    c.show(&field_ty),
                    c.show(&value_ty)
                )
            })?;
        }
        Ok(self.resolve(&base_ty))
    }

    /// The type of `base_ty.name`.
    ///
    /// A known record type is searched directly. A still-unknown one (an
    /// unannotated parameter) is taken to be the single record that declares
    /// a field of that name; if several do, the program must annotate which.
    fn record_field(&mut self, base_ty: &Type, name: &str, span: Span) -> Result<Type, Diagnostic> {
        let owner = match self.resolve(base_ty) {
            Type::Adt(adt, _) => self.owner_within(&adt, name).ok_or_else(|| {
                Diagnostic::error(
                    format!("'{adt}' is not a record with a field named '{name}'"),
                    span,
                )
            })?,
            Type::Var(_) => self.owner_by_field_name(name, span)?,
            other => {
                return Err(Diagnostic::error(
                    format!(
                        "cannot read field '{name}' of {}: it is not a record",
                        other.describe()
                    ),
                    span,
                ));
            }
        };
        let record = self
            .instantiate_variant(&owner)
            .ok_or_else(|| Diagnostic::error(format!("unknown record '{owner}'"), span))?;
        self.unify(base_ty, &record.adt_ty, span)?;
        let index = record
            .info
            .field_names
            .as_ref()
            .and_then(|names| names.iter().position(|n| n == name))
            .ok_or_else(|| Diagnostic::error(format!("no field '{name}' on '{owner}'"), span))?;
        Ok(record.field_type(index))
    }

    /// The constructor of type `adt` that declares field `name`.
    fn owner_within(&self, adt: &str, name: &str) -> Option<String> {
        let info = self.adt_registry.adts.get(adt)?;
        info.variants
            .iter()
            .find(|tag| {
                self.adt_registry
                    .variants
                    .get(*tag)
                    .and_then(|v| v.field_names.as_ref())
                    .is_some_and(|names| names.iter().any(|n| n == name))
            })
            .cloned()
    }

    /// The only constructor declaring field `name`, when the record type is
    /// not yet known.
    fn owner_by_field_name(&self, name: &str, span: Span) -> Result<String, Diagnostic> {
        match self.adt_registry.field_owners.get(name).map(Vec::as_slice) {
            Some([only]) => Ok(only.clone()),
            Some(many) if many.len() > 1 => Err(Diagnostic::error(
                format!(
                    "field '{name}' is declared by more than one record ({}); annotate the \
                     value's type to say which",
                    many.join(", ")
                ),
                span,
            )),
            _ => Err(Diagnostic::error(
                format!("no record declares a field '{name}'"),
                span,
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::testing::{boolean, checker, int, span, string, var};
    use super::*;
    use crate::adt::AdtRegistry;
    use obfusku_core::ast::{TypeDecl, VariantDecl};
    use obfusku_core::types::Type as CoreType;

    fn record(tag: &str, fields: &[(&str, CoreType)]) -> TypeDecl {
        TypeDecl {
            tag: tag.to_string(),
            type_params: vec![],
            variants: vec![VariantDecl {
                tag: tag.to_string(),
                fields: fields.iter().map(|(_, t)| t.clone()).collect(),
                field_names: Some(fields.iter().map(|(n, _)| (*n).to_string()).collect()),
                span: span(),
            }],
            span: span(),
        }
    }

    fn checker_with(decls: &[TypeDecl]) -> Checker {
        Checker::new(AdtRegistry::build(decls).expect("declarations are valid"))
    }

    fn point() -> Expr {
        Expr::Constructor {
            tag: "Point".into(),
            args: vec![int(1), int(2)],
            span: span(),
        }
    }

    fn field(base: Expr, name: &str) -> Expr {
        Expr::Field {
            base: Box::new(base),
            name: name.to_string(),
            span: span(),
        }
    }

    fn point_decl() -> TypeDecl {
        record("Point", &[("x", CoreType::Int), ("y", CoreType::Int)])
    }

    fn run(c: &mut Checker, expr: &Expr) -> Infer<Type> {
        let ty = c.infer(expr, &TypeEnv::new())?;
        Ok(c.resolve(&ty))
    }

    fn array(elements: Vec<Expr>) -> Expr {
        Expr::ArrayLiteral {
            elements,
            span: span(),
        }
    }

    #[test]
    fn a_constructor_builds_its_declared_type() {
        let mut c = checker_with(&[point_decl()]);
        assert_eq!(
            run(&mut c, &point()).expect("infers"),
            Type::Adt("Point".into(), vec![])
        );
    }

    #[test]
    fn an_unknown_constructor_is_named() {
        let mut c = checker();
        let ghost = Expr::Constructor {
            tag: "Ghost".into(),
            args: vec![],
            span: span(),
        };
        let errors = run(&mut c, &ghost).expect_err("unknown");
        assert_eq!(errors[0].message, "unknown constructor 'Ghost'");
    }

    #[test]
    fn a_constructor_with_the_wrong_argument_count_says_how_many() {
        let mut c = checker_with(&[point_decl()]);
        let short = Expr::Constructor {
            tag: "Point".into(),
            args: vec![int(1)],
            span: span(),
        };
        let errors = run(&mut c, &short).expect_err("arity");
        assert_eq!(
            errors[0].message,
            "constructor 'Point' expects 2 argument(s), found 1"
        );
    }

    #[test]
    fn a_constructor_field_of_the_wrong_type_names_both_types() {
        let mut c = checker_with(&[point_decl()]);
        let bad = Expr::Constructor {
            tag: "Point".into(),
            args: vec![int(1), string("y")],
            span: span(),
        };
        let errors = run(&mut c, &bad).expect_err("mismatch");
        assert!(errors[0].message.ends_with("expected Int, found String"));
    }

    #[test]
    fn array_elements_must_agree() {
        let mut c = checker();
        let ok = array(vec![int(1), int(2)]);
        assert_eq!(run(&mut c, &ok).expect("infers"), Type::array(Type::Int));
        let mixed = array(vec![int(1), boolean(true)]);
        assert!(run(&mut c, &mixed).is_err());
    }

    #[test]
    fn indexing_yields_the_element_type_and_demands_an_int_index() {
        let mut c = checker();
        let index = |i| Expr::Index {
            array: Box::new(array(vec![string("a")])),
            index: Box::new(i),
            span: span(),
        };
        assert_eq!(run(&mut c, &index(int(0))).expect("infers"), Type::Str);
        let errors = run(&mut c, &index(boolean(true))).expect_err("index type");
        assert_eq!(errors[0].message, "array index must be Int, found Bool");
    }

    #[test]
    fn indexing_something_that_is_not_an_array_is_rejected() {
        let mut c = checker();
        let bad = Expr::Index {
            array: Box::new(int(1)),
            index: Box::new(int(0)),
            span: span(),
        };
        let errors = run(&mut c, &bad).expect_err("not an array");
        assert_eq!(errors[0].message, "cannot index: Int is not an Array");
    }

    #[test]
    fn a_field_read_has_the_declared_field_type() {
        let mut c = checker_with(&[point_decl()]);
        assert_eq!(run(&mut c, &field(point(), "x")).expect("infers"), Type::Int);
    }

    #[test]
    fn reading_an_unknown_field_of_a_known_record_names_the_record() {
        let mut c = checker_with(&[point_decl()]);
        let errors = run(&mut c, &field(point(), "z")).expect_err("no such field");
        assert_eq!(
            errors[0].message,
            "'Point' is not a record with a field named 'z'"
        );
    }

    #[test]
    fn reading_a_field_of_a_non_record_is_rejected() {
        let mut c = checker_with(&[point_decl()]);
        let errors = run(&mut c, &field(int(1), "x")).expect_err("not a record");
        assert!(errors[0].message.contains("it is not a record"));
    }

    #[test]
    fn an_unannotated_base_finds_the_one_record_declaring_the_field() {
        let mut c = checker_with(&[point_decl()]);
        let mut env = TypeEnv::new();
        let base = c.fresh();
        env.insert("p".into(), crate::types::Scheme::monomorphic(base.clone()));
        let ty = c.infer(&field(var("p"), "y"), &env).expect("infers");
        assert_eq!(c.resolve(&ty), Type::Int);
        assert_eq!(c.resolve(&base), Type::Adt("Point".into(), vec![]));
    }

    #[test]
    fn a_field_declared_by_two_records_is_ambiguous_without_an_annotation() {
        let other = record("Pixel", &[("x", CoreType::Int)]);
        let mut c = checker_with(&[point_decl(), other]);
        let base = c.fresh();
        let mut env = TypeEnv::new();
        env.insert("p".into(), crate::types::Scheme::monomorphic(base));
        let errors = c
            .infer(&field(var("p"), "x"), &env)
            .expect_err("ambiguous");
        assert!(errors[0].message.contains("declared by more than one record"));
    }

    #[test]
    fn an_update_checks_each_new_value_against_its_field() {
        let mut c = checker_with(&[point_decl()]);
        let update = |value| Expr::Update {
            base: Box::new(point()),
            fields: vec![("x".to_string(), value)],
            span: span(),
        };
        assert_eq!(
            run(&mut c, &update(int(9))).expect("infers"),
            Type::Adt("Point".into(), vec![])
        );
        let errors = run(&mut c, &update(string("no"))).expect_err("mismatch");
        assert_eq!(
            errors[0].message,
            "field 'x' has type Int, but the new value has type String"
        );
    }
}
