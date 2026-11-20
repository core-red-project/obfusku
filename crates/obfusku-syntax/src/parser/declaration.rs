//! Top-level forms (`CONCRETE_SYMBOLIC_GRAMMAR.md` §8).

use super::{PResult, Parser, starts_uppercase};
use crate::ast::{
    Declaration, FnParameter, FunctionDeclaration, ImportDeclaration, Module, TypeDeclaration,
    ValueDeclaration, VariantDecl,
};
use crate::lexer::TokenKind;
use obfusku_diagnostics::Span;

impl Parser<'_> {
    /// `Declaration* "❧"`.
    pub(super) fn parse_module(&mut self) -> PResult<Module> {
        let mut declarations = Vec::new();
        loop {
            if matches!(self.peek(), TokenKind::Seal) {
                let seal = self.advance().span;
                return Ok(Module { declarations, seal });
            }
            if matches!(self.peek(), TokenKind::Eof) {
                return Err(self.error("expected '❧' to close the module, found end of input"));
            }
            self.links = 0;
            declarations.push(self.parse_declaration()?);
        }
    }

    fn parse_declaration(&mut self) -> PResult<Declaration> {
        // §8.2: a declaration-position `λ` is always a FunctionDeclaration.
        if matches!(self.peek(), TokenKind::Lambda) {
            return self.parse_function_declaration().map(Declaration::Function);
        }
        // Imports and exceptions are reachable only from declaration
        // position, never from a local binding, the same way this function
        // is never called from expression parsing.
        if matches!(self.peek(), TokenKind::Import) {
            return self.parse_import_declaration();
        }
        // `ExceptionDeclaration ::= "☈" Variant` (`ADR-026`).
        if self.eat(&TokenKind::ExceptionDecl) {
            return Ok(Declaration::Exception(self.parse_variant()?));
        }

        let (name, name_span) =
            self.ident("a declaration (identifier followed by '≔', 'λname(...)', or '❧')")?;

        if starts_uppercase(&name) {
            return self.parse_type_declaration(name, name_span);
        }

        // §8.1: the `(":" Type)?` annotation is module-boundary-only, so it
        // is only ever attempted here, never on a local binding.
        let ty = if self.eat(&TokenKind::Colon) {
            Some(self.parse_type_ref()?)
        } else {
            None
        };

        self.expect(&TokenKind::Bind, "'≔'")?;

        // Fixed order (§8.1): Mutability, then Export.
        let mutable = self.eat(&TokenKind::Mut);
        let exported = self.eat(&TokenKind::Export);

        let value = self.parse_expression()?;
        let span = name_span.merge(value.span());
        Ok(Declaration::Value(ValueDeclaration {
            name,
            name_span,
            ty,
            mutable,
            exported,
            value,
            span,
        }))
    }

    fn parse_import_declaration(&mut self) -> PResult<Declaration> {
        let start = self.advance().span; // consume '⟲'
        let (module_name, name_span) = self.ident("a module name after '⟲'")?;
        Ok(Declaration::Import(ImportDeclaration {
            module_name,
            name_span,
            span: start.merge(name_span),
        }))
    }

    /// §8.3: `TypeBody ::= SumBody | RecordBody | TupleBody`.
    ///
    /// A tuple body is unambiguous from its opening `(`. Sum and record
    /// bodies both open with `{` and are told apart by the case of the first
    /// inner token: a lowercase name starts a record field, an uppercase one
    /// a variant.
    fn parse_type_declaration(&mut self, tag: String, tag_span: Span) -> PResult<Declaration> {
        let mut type_params = Vec::new();
        while let TokenKind::Ident(p) = self.peek().clone() {
            if starts_uppercase(&p) {
                break; // not a type parameter: let the '≔' expectation fail clearly
            }
            self.advance();
            type_params.push(p);
        }
        self.expect(&TokenKind::Bind, "'≔'")?;

        if matches!(self.peek(), TokenKind::LParen) {
            return self.parse_tuple_body(tag, tag_span, type_params);
        }
        self.expect(&TokenKind::LBrace, "'{' or '(' to start the type's body")?;
        if matches!(self.peek(), TokenKind::Ident(n) if !starts_uppercase(n)) {
            return self.parse_record_body(tag, tag_span, type_params);
        }
        let mut variants = vec![self.parse_variant()?];
        while self.eat(&TokenKind::ArmSep) {
            variants.push(self.parse_variant()?);
        }
        let end = self.expect(&TokenKind::RBrace, "'}'")?;
        Ok(Declaration::Type(TypeDeclaration {
            tag,
            type_params,
            variants,
            span: tag_span.merge(end),
        }))
    }

    /// `RecordBody ::= "{" name ":" Type (⟢ …)* "}"`, past the opening `{`.
    /// One self-tagged variant carrying `field_names` in declaration order.
    fn parse_record_body(
        &mut self,
        tag: String,
        tag_span: Span,
        type_params: Vec<String>,
    ) -> PResult<Declaration> {
        let named = self.named_fields(Self::parse_type_ref)?;
        let end = self.expect(&TokenKind::RBrace, "'}'")?;
        let (field_names, fields) = named.into_iter().unzip();
        Ok(Self::single_variant_type(
            tag,
            tag_span,
            type_params,
            fields,
            Some(field_names),
            end,
        ))
    }

    /// `TupleBody ::= "(" Type ("," Type)* ")"`; the `(` is peeked, not yet
    /// consumed. One self-tagged variant, purely positional.
    fn parse_tuple_body(
        &mut self,
        tag: String,
        tag_span: Span,
        type_params: Vec<String>,
    ) -> PResult<Declaration> {
        self.advance(); // consume '('
        let fields = self.comma_list(Self::parse_type_ref)?;
        let end = self.expect(&TokenKind::RParen, "')'")?;
        Ok(Self::single_variant_type(
            tag,
            tag_span,
            type_params,
            fields,
            None,
            end,
        ))
    }

    /// A record or tuple: a type whose only variant is named like the type.
    fn single_variant_type(
        tag: String,
        tag_span: Span,
        type_params: Vec<String>,
        fields: Vec<crate::ast::TypeRef>,
        field_names: Option<Vec<String>>,
        end: Span,
    ) -> Declaration {
        let span = tag_span.merge(end);
        Declaration::Type(TypeDeclaration {
            tag: tag.clone(),
            type_params,
            variants: vec![VariantDecl {
                tag,
                fields,
                field_names,
                span,
            }],
            span,
        })
    }

    /// `Variant ::= Tag ( "(" Type? ("," Type)* ")" )?`: the whole
    /// parenthesized list is optional, so a bare `None` is legal.
    pub(super) fn parse_variant(&mut self) -> PResult<VariantDecl> {
        let span = self.peek_span();
        let tag = match self.peek().clone() {
            TokenKind::Ident(name) if starts_uppercase(&name) => {
                self.advance();
                name
            }
            other => {
                return Err(self.error(format!("expected a variant name (Tag), found {other:?}")));
            }
        };
        let mut fields = Vec::new();
        let mut end = span;
        if self.eat(&TokenKind::LParen) {
            if !matches!(self.peek(), TokenKind::RParen) {
                fields = self.comma_list(Self::parse_type_ref)?;
            }
            end = self.expect(&TokenKind::RParen, "')'")?;
        }
        Ok(VariantDecl {
            tag,
            fields,
            field_names: None,
            span: span.merge(end),
        })
    }

    /// §8.2: `"λ" name "(" FnParameter,* ")" ":" Type "→" Expression`.
    pub(super) fn parse_function_declaration(&mut self) -> PResult<FunctionDeclaration> {
        let start = self.advance().span; // consume 'λ'
        let (name, name_span) = self.ident(
            "a function name after 'λ' (a bare 'λ(...)' is an anonymous Lambda, \
             not valid as a top-level declaration)",
        )?;
        self.expect(&TokenKind::LParen, "'(' after the function name")?;
        if matches!(self.peek(), TokenKind::RParen) {
            return Err(self.error(
                "expected at least one parameter: zero-parameter function declarations \
                 ('λf() → …') are not valid Obfusku, for the same reason zero-parameter \
                 anonymous lambdas aren't (SEMANTIC_CORE.md's Lambda is always single-parameter \
                 and spec defines no Unit value to stand in for a zero-arity parameter list)",
            ));
        }
        let params = self.comma_list(Self::parse_fn_parameter)?;
        self.expect(&TokenKind::RParen, "')'")?;
        self.expect(
            &TokenKind::Colon,
            "':' and a return type (mandatory on a FunctionDeclaration, CONCRETE_SYMBOLIC_GRAMMAR.md §8.2)",
        )?;
        let return_type = self.parse_type_application()?;
        self.expect(&TokenKind::Arrow, "'→'")?;
        // §8.4: the export mark follows the declaration's own `≔` or `→`; a
        // function has no `≔`, so it is `→⟳`. Accepted at any position:
        // §8.4 makes the local restriction a static check, since the grammar
        // shape is identical (see `parse_local_function_bindings`).
        let exported = self.eat(&TokenKind::Export);
        let body = self.parse_expression()?;
        Ok(FunctionDeclaration {
            name,
            name_span,
            params,
            return_type,
            exported,
            span: start.merge(body.span()),
            body,
        })
    }

    /// `FnParameter ::= name ":" Type`, annotation mandatory (unlike an
    /// anonymous lambda's `Parameter`, §7.3).
    fn parse_fn_parameter(&mut self) -> PResult<FnParameter> {
        let (name, span) = self.ident("a parameter name")?;
        self.expect(
            &TokenKind::Colon,
            "':' and a type (mandatory on a FunctionDeclaration parameter, CONCRETE_SYMBOLIC_GRAMMAR.md §8.2)",
        )?;
        let ty = self.parse_type_ref()?;
        let span = span.merge(ty.span());
        Ok(FnParameter { name, ty, span })
    }
}

#[cfg(test)]
mod tests {
    use crate::ast::{Declaration, TypeDeclaration};
    use crate::parser::testing::{error, module};

    fn only(source: &str) -> Declaration {
        module(source).declarations.remove(0)
    }

    fn type_decl(source: &str) -> TypeDeclaration {
        match only(source) {
            Declaration::Type(t) => t,
            other => panic!("expected a type, got {other:?}"),
        }
    }

    #[test]
    fn a_value_declaration_records_annotation_and_markers() {
        let Declaration::Value(v) = only("x: ⟁ ≔ ˚⟳ 1\n❧") else {
            panic!("expected a value");
        };
        assert_eq!(v.name, "x");
        assert!(v.ty.is_some() && v.mutable && v.exported);
    }

    #[test]
    fn mutability_must_precede_export() {
        assert!(error("x ≔ ⟳˚ 1\n❧").contains("expected a name after '˚'"));
    }

    #[test]
    fn a_function_declaration_requires_annotated_parameters_and_a_return_type() {
        let Declaration::Function(f) = only("λf(a: ⟁, b: ⟁): ⟁ →⟳ a\n❧") else {
            panic!("expected a function");
        };
        assert_eq!(f.params.len(), 2);
        assert!(f.exported);
        assert!(error("λf(a): ⟁ → a\n❧").contains("':' and a type"));
        assert!(error("λf(a: ⟁) → a\n❧").contains("':' and a return type"));
    }

    #[test]
    fn zero_parameter_functions_are_rejected() {
        assert!(error("λf(): ⟁ → 1\n❧").contains("at least one parameter"));
    }

    #[test]
    fn a_function_typed_return_needs_parentheses() {
        let Declaration::Function(f) = only("λf(a: ⟁): (⟁ → ⟁) → λ(x) → x\n❧")
        else {
            panic!("expected a function");
        };
        assert!(matches!(f.return_type, crate::ast::TypeRef::Function(..)));
    }

    #[test]
    fn imports_and_exceptions_parse() {
        let m = module("⟲ Foo\n☈ Boom(⌘)\n❧");
        assert!(matches!(&m.declarations[0], Declaration::Import(i) if i.module_name == "Foo"));
        assert!(matches!(&m.declarations[1], Declaration::Exception(v) if v.tag == "Boom"));
        assert!(error("⟲ 1\n❧").contains("expected a module name after '⟲'"));
    }

    #[test]
    fn a_sum_type_lists_its_variants() {
        let t = type_decl("Opt a ≔ { None ⟢ Some(a) }\n❧");
        assert_eq!(t.type_params, ["a"]);
        assert_eq!(t.variants.len(), 2);
        assert!(t.variants[1].field_names.is_none());
    }

    #[test]
    fn a_record_type_is_one_self_tagged_variant_with_field_names() {
        let t = type_decl("Point ≔ { x: ⟁ ⟢ y: ⟁ }\n❧");
        assert_eq!(t.variants.len(), 1);
        assert_eq!(t.variants[0].tag, "Point");
        assert_eq!(
            t.variants[0].field_names.as_deref(),
            Some(&["x".to_string(), "y".to_string()][..])
        );
    }

    #[test]
    fn a_tuple_type_is_one_self_tagged_positional_variant() {
        let t = type_decl("Pair ≔ (⟁, ⌘)\n❧");
        assert_eq!(t.variants[0].fields.len(), 2);
        assert!(t.variants[0].field_names.is_none());
    }

    #[test]
    fn a_declaration_must_start_with_a_name() {
        assert!(error("1 ≔ 2\n❧").contains("expected a declaration"));
    }

    #[test]
    fn a_type_body_must_open_with_a_brace_or_paren() {
        assert!(error("T ≔ 1\n❧").contains("'{' or '(' to start the type's body"));
    }

    #[test]
    fn a_variant_needs_a_tag() {
        assert!(error("T ≔ { 1 }\n❧").contains("expected a variant name"));
    }
}
