//! Top-level forms (`CONCRETE_SYMBOLIC_GRAMMAR.md` §8).

use super::{Expression, TypeRef};
use obfusku_diagnostics::Span;

/// A whole source file: its declarations, then the closing `❧`.
#[derive(Debug, Clone, Default)]
pub struct Module {
    /// The declarations, in source order.
    pub declarations: Vec<Declaration>,
    /// The closing `❧`. A module without one does not parse, so a program
    /// cut short is an error instead of a silently shorter program.
    pub seal: Span,
}

/// One top-level form.
#[derive(Debug, Clone)]
pub enum Declaration {
    /// `name ≔ value`.
    Value(ValueDeclaration),
    /// `λname(…) : T → body` (§8.2).
    ///
    /// The only surface route to self- and mutual recursion: never a
    /// [`ValueDeclaration`] whose value happens to be a lambda. A run of
    /// adjacent functions becomes one `LetRec` group.
    Function(FunctionDeclaration),
    /// A sum, record or tuple type.
    Type(TypeDeclaration),
    /// `"⟲" ModuleName`: brings another module's exported (`⟳`) bindings
    /// into scope.
    ///
    /// The syntax crate does no file I/O: the CLI resolves the name to a
    /// file and splices the result in, and the desugarer lowers an import to
    /// nothing.
    Import(ImportDeclaration),
    /// `"☈" Variant`: adds one variant to the open `Exception` type
    /// (`ADR-026`).
    Exception(VariantDecl),
}

impl Declaration {
    /// The whole declaration.
    #[must_use]
    pub fn span(&self) -> Span {
        match self {
            Declaration::Value(v) => v.span,
            Declaration::Function(f) => f.span,
            Declaration::Type(t) => t.span,
            Declaration::Import(i) => i.span,
            Declaration::Exception(v) => v.span,
        }
    }
}

/// `"⟲" ModuleName`.
#[derive(Debug, Clone)]
pub struct ImportDeclaration {
    /// The module name as written; resolves to a same-directory
    /// `<module_name>.obk` (`ADR-018`).
    pub module_name: String,
    /// Just the name, for errors about an unresolved import.
    pub name_span: Span,
    /// `⟲` through the name.
    pub span: Span,
}

/// §8.1: `name (":" Type)? "≔" Mutability? Export? Expression`.
#[derive(Debug, Clone)]
pub struct ValueDeclaration {
    /// The bound name.
    pub name: String,
    /// Just the name, for errors about the binding itself.
    pub name_span: Span,
    /// The annotation.
    ///
    /// Module-boundary only (§9): always `None` on a local binding, where
    /// the type is inferred.
    pub ty: Option<TypeRef>,
    /// `˚`: the binding is a mutable cell.
    pub mutable: bool,
    /// `⟳`: visible to importers.
    pub exported: bool,
    /// The bound expression.
    pub value: Expression,
    /// The whole declaration.
    pub span: Span,
}

/// §8.2: `"λ" name "(" FnParameter,* ")" ":" Type "→" Expression`.
///
/// At least one parameter is mandatory, and every parameter and the return
/// type are annotated through [`FnParameter`] (never the optional
/// annotation of an anonymous lambda's `Parameter`). Annotations are what let
/// a recursive group be checked without inferring through the recursion.
#[derive(Debug, Clone)]
pub struct FunctionDeclaration {
    /// The function's name.
    pub name: String,
    /// Just the name.
    pub name_span: Span,
    /// The parameters, in order; never empty.
    pub params: Vec<FnParameter>,
    /// The declared result type.
    pub return_type: TypeRef,
    /// `→⟳`: visible to importers. Rejected on a local function.
    pub exported: bool,
    /// The body.
    pub body: Expression,
    /// The whole declaration.
    pub span: Span,
}

/// `name ":" Type`, annotation mandatory.
#[derive(Debug, Clone)]
pub struct FnParameter {
    /// The parameter name.
    pub name: String,
    /// Its type.
    pub ty: TypeRef,
    /// The whole parameter.
    pub span: Span,
}

/// `Tag TypeParameter* "≔" TypeBody` (§8.3).
#[derive(Debug, Clone)]
pub struct TypeDeclaration {
    /// The type's name; also the namespace of its constructors.
    pub tag: String,
    /// Type parameter names, in order.
    pub type_params: Vec<String>,
    /// The variants. A record or tuple has exactly one, named like the type.
    pub variants: Vec<VariantDecl>,
    /// The whole declaration.
    pub span: Span,
}

/// `Tag ( "(" Type? ("," Type)* ")" )?`.
///
/// Also the single self-tagged "variant" a record or tuple body produces
/// (`ABSTRACT_GRAMMAR.md` §2.3), so all three body forms share one shape.
#[derive(Debug, Clone)]
pub struct VariantDecl {
    /// The constructor name.
    pub tag: String,
    /// One type per field, in order.
    pub fields: Vec<TypeRef>,
    /// `Some(names)` only for a record body's variant.
    ///
    /// The desugarer needs them to reorder a later named construction or
    /// pattern into declared positional order.
    pub field_names: Option<Vec<String>>,
    /// The whole variant.
    pub span: Span,
}

#[cfg(test)]
mod tests {
    use super::*;
    use obfusku_diagnostics::SourceId;

    fn at(start: u32, end: u32) -> Span {
        Span::new(SourceId(0), start, end)
    }

    #[test]
    fn a_default_module_is_empty() {
        let m = Module::default();
        assert!(m.declarations.is_empty());
        assert_eq!(m.seal, Span::default());
    }

    #[test]
    fn every_declaration_reports_its_span() {
        let variant = VariantDecl {
            tag: "T".into(),
            fields: vec![],
            field_names: None,
            span: at(1, 2),
        };
        let import = ImportDeclaration {
            module_name: "m".into(),
            name_span: at(3, 4),
            span: at(2, 4),
        };
        assert_eq!(Declaration::Exception(variant).span(), at(1, 2));
        assert_eq!(Declaration::Import(import).span(), at(2, 4));
    }
}
