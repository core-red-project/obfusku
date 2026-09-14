//! tokens → surface AST — `spec/CONCRETE_SYMBOLIC_GRAMMAR.md`.
//!
//! §15's declaration-boundary rule (newlines insignificant; a
//! declaration ends when the next token can't continue the current
//! expression) falls out for free from ordinary precedence-climbing
//! expression parsing — no distinct algorithm needed.

use crate::ast::{
    Argument, BaseType, BinOp, Declaration, Expression, FnParameter, FunctionDeclaration,
    ImportDeclaration, MatchArm, Module, Parameter, Pattern, Stage, TypeDeclaration, TypeRef, UnOp,
    ValueDeclaration, VariantDecl,
};
use crate::lexer::{Token, TokenKind};
use obfusku_diagnostics::{Diagnostic, Severity, SourceId, Span};

/// §4/§8.3's disambiguation: decidable from the first character alone.
fn starts_uppercase(name: &str) -> bool {
    name.chars().next().is_some_and(|c| c.is_uppercase())
}

/// The reserved-invalid combination `CONCRETE_SYMBOLIC_GRAMMAR.md`
/// §12.1/§16.1 names explicitly: `◈` and `•` occurring in the same
/// pipeline stage. A nested `Pipe`'s own `Stage` is a separate binding
/// region (§12's scoping rule) — this walker doesn't descend into one,
/// only into its `subject`, which is still lexically part of the
/// current stage.
fn stage_has_pipe_ref_and_hole_conflict(expr: &Expression) -> bool {
    let mut has_ref = false;
    let mut has_hole = false;
    scan_stage_conflict(expr, &mut has_ref, &mut has_hole);
    has_ref && has_hole
}

fn scan_argument_conflict(arg: &Argument, has_ref: &mut bool, has_hole: &mut bool) {
    match arg {
        Argument::Expr(e) => scan_stage_conflict(e, has_ref, has_hole),
        Argument::Hole(_) => *has_hole = true,
    }
}

fn scan_stage_conflict(expr: &Expression, has_ref: &mut bool, has_hole: &mut bool) {
    use Expression::*;
    match expr {
        PipeRef(_) => *has_ref = true,
        Int(..) | Real(..) | Str(..) | Bool(..) | Unit(..) | Reference(..) | ExplicitRef(..) => {}
        Lambda { body, .. } => scan_stage_conflict(body, has_ref, has_hole),
        Application { callable, args, .. } => {
            scan_stage_conflict(callable, has_ref, has_hole);
            for a in args {
                scan_argument_conflict(a, has_ref, has_hole);
            }
        }
        // A nested `▷`'s own `Stage` is its own region — only its
        // `subject` is still lexically part of the current one.
        Pipe { subject, .. } => scan_stage_conflict(subject, has_ref, has_hole),
        Rebind { value, .. } => scan_stage_conflict(value, has_ref, has_hole),
        Match {
            scrutinee, arms, ..
        } => {
            scan_stage_conflict(scrutinee, has_ref, has_hole);
            for arm in arms {
                scan_stage_conflict(&arm.result, has_ref, has_hole);
            }
        }
        BinaryOp { lhs, rhs, .. } => {
            scan_stage_conflict(lhs, has_ref, has_hole);
            scan_stage_conflict(rhs, has_ref, has_hole);
        }
        UnaryOp { operand, .. } => scan_stage_conflict(operand, has_ref, has_hole),
        Raise(value, _) => scan_stage_conflict(value, has_ref, has_hole),
        Catch { body, handler, .. } => {
            scan_stage_conflict(body, has_ref, has_hole);
            scan_stage_conflict(handler, has_ref, has_hole);
        }
        NamedConstruction { fields, .. } => {
            for (_, v) in fields {
                scan_stage_conflict(v, has_ref, has_hole);
            }
        }
        LocalValue { decl, body, .. } => {
            scan_stage_conflict(&decl.value, has_ref, has_hole);
            scan_stage_conflict(body, has_ref, has_hole);
        }
        LocalFunctions { decls, body, .. } => {
            for fd in decls {
                scan_stage_conflict(&fd.body, has_ref, has_hole);
            }
            scan_stage_conflict(body, has_ref, has_hole);
        }
        ArrayLiteral { elements, .. } => {
            for e in elements {
                scan_stage_conflict(e, has_ref, has_hole);
            }
        }
        Index { array, index, .. } => {
            scan_stage_conflict(array, has_ref, has_hole);
            scan_stage_conflict(index, has_ref, has_hole);
        }
    }
}

pub fn parse(tokens: &[Token], source_id: SourceId) -> Result<Module, Vec<Diagnostic>> {
    Parser {
        tokens,
        pos: 0,
        source_id,
    }
    .parse_module()
}

type PResult<T> = Result<T, Vec<Diagnostic>>;

struct Parser<'a> {
    tokens: &'a [Token],
    pos: usize,
    #[allow(dead_code)] // will anchor future multi-file diagnostics
    source_id: SourceId,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> &TokenKind {
        &self.tokens[self.pos].kind
    }

    fn peek_at(&self, offset: usize) -> &TokenKind {
        let i = (self.pos + offset).min(self.tokens.len() - 1);
        &self.tokens[i].kind
    }

    fn peek_span(&self) -> Span {
        self.tokens[self.pos].span
    }

    fn advance(&mut self) -> &Token {
        let tok = &self.tokens[self.pos];
        if self.pos + 1 < self.tokens.len() {
            self.pos += 1;
        }
        tok
    }

    fn error(&self, message: impl Into<String>) -> Vec<Diagnostic> {
        vec![Diagnostic {
            severity: Severity::Error,
            message: message.into(),
            primary: self.peek_span(),
        }]
    }

    fn expect(&mut self, kind: &TokenKind, what: &str) -> PResult<Span> {
        if self.peek() == kind {
            Ok(self.advance().span)
        } else {
            Err(self.error(format!("expected {what}, found {:?}", self.peek())))
        }
    }

    fn parse_module(&mut self) -> PResult<Module> {
        let mut declarations = Vec::new();
        loop {
            if matches!(self.peek(), TokenKind::Seal) {
                let seal = self.advance().span;
                return Ok(Module { declarations, seal });
            }
            if matches!(self.peek(), TokenKind::Eof) {
                return Err(self.error("expected '❧' to close the module, found end of input"));
            }
            declarations.push(self.parse_declaration()?);
        }
    }

    fn parse_declaration(&mut self) -> PResult<Declaration> {
        // §8.2: a declaration-position 'λ' is always a FunctionDeclaration.
        if matches!(self.peek(), TokenKind::Lambda) {
            return self.parse_function_declaration();
        }
        // `ImportDeclaration ::= "⟲" ModuleName` — only ever reachable
        // from declaration position (never `LocalBinding`), the same
        // way `parse_declaration` itself is never called from
        // expression parsing.
        if matches!(self.peek(), TokenKind::Import) {
            return self.parse_import_declaration();
        }

        let name_span = self.peek_span();
        let name = match self.peek().clone() {
            TokenKind::Ident(name) => {
                self.advance();
                name
            }
            other => {
                return Err(self.error(format!(
                    "expected a declaration (identifier followed by '≔', 'λname(...)', or '❧'), \
                     found {other:?}"
                )))
            }
        };

        if starts_uppercase(&name) {
            return self.parse_type_declaration(name, name_span);
        }

        // §8.1: `ValueDeclaration ::= name (":" Type)? "≔" ...` — the
        // annotation is module-boundary-only, so it's only ever
        // attempted here, never in `parse_local_value_binding`.
        let ty = if matches!(self.peek(), TokenKind::Colon) {
            self.advance();
            Some(self.parse_type_ref()?)
        } else {
            None
        };

        self.expect(&TokenKind::Bind, "'≔'")?;

        // Fixed order (§8.1): Mutability, then Export.
        let mutable = matches!(self.peek(), TokenKind::Mut);
        if mutable {
            self.advance();
        }
        let exported = matches!(self.peek(), TokenKind::Export);
        if exported {
            self.advance();
        }

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
        let name_span = self.peek_span();
        let module_name = match self.peek().clone() {
            TokenKind::Ident(name) => {
                self.advance();
                name
            }
            other => {
                return Err(self.error(format!("expected a module name after '⟲', found {other:?}")))
            }
        };
        let span = start.merge(name_span);
        Ok(Declaration::Import(ImportDeclaration {
            module_name,
            name_span,
            span,
        }))
    }

    /// §8.3: `TypeBody ::= SumBody | RecordBody | TupleBody`. `TupleBody`
    /// is unambiguous from the opening `(`. `SumBody`/`RecordBody` both
    /// open with `{`, disambiguated by the case of the first inner token.
    fn parse_type_declaration(&mut self, tag: String, tag_span: Span) -> PResult<Declaration> {
        let mut type_params = Vec::new();
        while let TokenKind::Ident(p) = self.peek().clone() {
            if starts_uppercase(&p) {
                break; // not a TypeParameter — let '≔' expectation below fail clearly
            }
            self.advance();
            type_params.push(p);
        }
        self.expect(&TokenKind::Bind, "'≔'")?;

        if matches!(self.peek(), TokenKind::LParen) {
            return self.parse_tuple_body(tag, tag_span, type_params);
        }
        self.expect(&TokenKind::LBrace, "'{' or '(' to start the type's body")?;
        if matches!(self.peek(), TokenKind::Ident(ref n) if !starts_uppercase(n)) {
            return self.parse_record_body(tag, tag_span, type_params);
        }
        let mut variants = vec![self.parse_variant()?];
        while matches!(self.peek(), TokenKind::ArmSep) {
            self.advance();
            variants.push(self.parse_variant()?);
        }
        let end = self.expect(&TokenKind::RBrace, "'}'")?;
        let span = tag_span.merge(end);
        Ok(Declaration::Type(TypeDeclaration {
            tag,
            type_params,
            variants,
            span,
        }))
    }

    /// `RecordBody ::= "{" name ":" Type ("⟢" ...)* "}"` — past the
    /// opening `{` already. Desugars to one self-tagged `VariantDecl`
    /// carrying `field_names` in declaration order.
    fn parse_record_body(
        &mut self,
        tag: String,
        tag_span: Span,
        type_params: Vec<String>,
    ) -> PResult<Declaration> {
        let mut field_names = Vec::new();
        let mut fields = Vec::new();
        loop {
            let name = match self.peek().clone() {
                TokenKind::Ident(n) if !starts_uppercase(&n) => {
                    self.advance();
                    n
                }
                other => return Err(self.error(format!("expected a field name, found {other:?}"))),
            };
            self.expect(&TokenKind::Colon, "':'")?;
            let ty = self.parse_type_ref()?;
            field_names.push(name);
            fields.push(ty);
            if matches!(self.peek(), TokenKind::ArmSep) {
                self.advance();
                continue;
            }
            break;
        }
        let end = self.expect(&TokenKind::RBrace, "'}'")?;
        let span = tag_span.merge(end);
        Ok(Declaration::Type(TypeDeclaration {
            tag: tag.clone(),
            type_params,
            variants: vec![VariantDecl {
                tag,
                fields,
                field_names: Some(field_names),
                span,
            }],
            span,
        }))
    }

    /// `TupleBody ::= "(" Type ("," Type)* ")"` — `(` only peeked, not
    /// consumed, when called. Desugars to one self-tagged `VariantDecl`,
    /// purely positional (no `field_names`).
    fn parse_tuple_body(
        &mut self,
        tag: String,
        tag_span: Span,
        type_params: Vec<String>,
    ) -> PResult<Declaration> {
        self.advance(); // consume '('
        let mut fields = vec![self.parse_type_ref()?];
        while matches!(self.peek(), TokenKind::Comma) {
            self.advance();
            fields.push(self.parse_type_ref()?);
        }
        let end = self.expect(&TokenKind::RParen, "')'")?;
        let span = tag_span.merge(end);
        Ok(Declaration::Type(TypeDeclaration {
            tag: tag.clone(),
            type_params,
            variants: vec![VariantDecl {
                tag,
                fields,
                field_names: None,
                span,
            }],
            span,
        }))
    }

    /// `Variant ::= Tag ( "(" Type? ("," Type)* ")" )?` — the whole
    /// parenthesized field list is optional (a bare `None` is legal).
    fn parse_variant(&mut self) -> PResult<VariantDecl> {
        let span = self.peek_span();
        let tag = match self.peek().clone() {
            TokenKind::Ident(name) if starts_uppercase(&name) => {
                self.advance();
                name
            }
            other => {
                return Err(self.error(format!("expected a variant name (Tag), found {other:?}")))
            }
        };
        let mut fields = Vec::new();
        let mut end = span;
        if matches!(self.peek(), TokenKind::LParen) {
            self.advance();
            if !matches!(self.peek(), TokenKind::RParen) {
                fields.push(self.parse_type_ref()?);
                while matches!(self.peek(), TokenKind::Comma) {
                    self.advance();
                    fields.push(self.parse_type_ref()?);
                }
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

    /// §9: `Type ::= BaseType | Tag | FunctionType | TypeApplication |
    /// "(" Type ")"`. Descends `→` first (loosest), then `▷` (tighter).
    fn parse_type_ref(&mut self) -> PResult<TypeRef> {
        self.parse_function_type()
    }

    /// Right-associative: `Int → Int → Int` = `Int → (Int → Int)`.
    fn parse_function_type(&mut self) -> PResult<TypeRef> {
        let lhs = self.parse_type_application()?;
        if matches!(self.peek(), TokenKind::Arrow) {
            self.advance();
            let rhs = self.parse_function_type()?;
            let span = lhs.span().merge(rhs.span());
            Ok(TypeRef::Function(Box::new(lhs), Box::new(rhs), span))
        } else {
            Ok(lhs)
        }
    }

    /// Left-associative: `Array ▷ Int ▷ Real` == `(Array ▷ Int) ▷ Real`.
    fn parse_type_application(&mut self) -> PResult<TypeRef> {
        let mut expr = self.parse_type_primary()?;
        while matches!(self.peek(), TokenKind::Pipe) {
            self.advance();
            let rhs = self.parse_type_primary()?;
            let span = expr.span().merge(rhs.span());
            expr = TypeRef::Apply(Box::new(expr), Box::new(rhs), span);
        }
        Ok(expr)
    }

    fn parse_type_primary(&mut self) -> PResult<TypeRef> {
        let span = self.peek_span();
        match self.peek().clone() {
            TokenKind::TyInt => {
                self.advance();
                Ok(TypeRef::Base(BaseType::Int, span))
            }
            TokenKind::TyReal => {
                self.advance();
                Ok(TypeRef::Base(BaseType::Real, span))
            }
            TokenKind::TyStr => {
                self.advance();
                Ok(TypeRef::Base(BaseType::Str, span))
            }
            TokenKind::TyBool => {
                self.advance();
                Ok(TypeRef::Base(BaseType::Bool, span))
            }
            TokenKind::TyUnit => {
                self.advance();
                Ok(TypeRef::Base(BaseType::Unit, span))
            }
            TokenKind::Ident(name) if starts_uppercase(&name) => {
                self.advance();
                Ok(TypeRef::Named(name, span))
            }
            TokenKind::Ident(name) => {
                self.advance();
                Ok(TypeRef::Var(name, span))
            }
            TokenKind::LParen => {
                self.advance();
                let inner = self.parse_type_ref()?;
                self.expect(&TokenKind::RParen, "')'")?;
                Ok(inner)
            }
            other => Err(self.error(format!("expected a type, found {other:?}"))),
        }
    }

    /// §7.4: `LocalBinding` is one of the `Expression` alternatives, so
    /// this checks (one token of lookahead) whether the upcoming tokens
    /// are a local declaration before falling through to the operator
    /// ladder. `λ(a) → …` (anonymous Lambda) vs. `λf(a: T): T → …`
    /// (local `FunctionDeclaration`) differ only in whether a name
    /// follows `λ` directly.
    fn parse_expression(&mut self) -> PResult<Expression> {
        if let TokenKind::Ident(name) = self.peek().clone() {
            if !starts_uppercase(&name) && matches!(self.peek_at(1), TokenKind::Bind) {
                return self.parse_local_value_binding();
            }
            // §8.1: the `(":" Type)?` annotation is module-boundary-only;
            // a local `name : Type ≔ ...` attempt gets a clear static
            // error here rather than falling through to the operator
            // ladder and failing on an unrelated token.
            if !starts_uppercase(&name) && matches!(self.peek_at(1), TokenKind::Colon) {
                return Err(self.error(
                    "a type annotation (':') is not valid on a local binding — \
                     CONCRETE_SYMBOLIC_GRAMMAR.md §8.1's annotation is module-boundary-only \
                     (§9); locally the type is always inferred",
                ));
            }
        }
        if matches!(self.peek(), TokenKind::Lambda)
            && matches!(self.peek_at(1), TokenKind::Ident(_))
        {
            return self.parse_local_function_bindings();
        }
        self.parse_pipe()
    }

    /// `LocalBinding ::= ValueDeclaration Expression`. Nested
    /// `LocalBinding`s fall out of recursing into `parse_expression` for
    /// the continuation.
    fn parse_local_value_binding(&mut self) -> PResult<Expression> {
        let name_span = self.peek_span();
        let name = match self.peek().clone() {
            TokenKind::Ident(name) => {
                self.advance();
                name
            }
            other => unreachable!("caller already confirmed an Ident, found {other:?}"),
        };
        self.expect(&TokenKind::Bind, "'≔'")?;
        let mutable = matches!(self.peek(), TokenKind::Mut);
        if mutable {
            self.advance();
        }
        if matches!(self.peek(), TokenKind::Export) {
            return Err(self.error(
                "'⟳' (export) is not valid on a local binding — CONCRETE_SYMBOLIC_GRAMMAR.md \
                 §7.4's LocalBinding has no export modifier; only module-level ValueDeclaration \
                 does (§8.1)",
            ));
        }
        let value = self.parse_expression()?;
        let decl_span = name_span.merge(value.span());
        let decl = ValueDeclaration {
            name,
            name_span,
            ty: None,
            mutable,
            exported: false,
            value,
            span: decl_span,
        };
        let body = self.parse_expression()?;
        let span = decl_span.merge(body.span());
        Ok(Expression::LocalValue {
            decl: Box::new(decl),
            body: Box::new(body),
            span,
        })
    }

    /// `LocalBinding ::= FunctionDeclaration+ Expression` — collects one
    /// maximal run of adjacent local `FunctionDeclaration`s (must know
    /// where the run ends before parsing the trailing body).
    fn parse_local_function_bindings(&mut self) -> PResult<Expression> {
        let mut decls = Vec::new();
        loop {
            let Declaration::Function(fd) = self.parse_function_declaration()? else {
                unreachable!("parse_function_declaration always returns Declaration::Function")
            };
            if fd.exported {
                return Err(self.error(
                    "'⟳' (export) is not valid on a local function declaration — \
                     CONCRETE_SYMBOLIC_GRAMMAR.md §8.4 restricts export to top-level Module \
                     declarations; only its position (top-level vs. nested), not its grammar \
                     shape, makes this invalid",
                ));
            }
            decls.push(fd);
            if !(matches!(self.peek(), TokenKind::Lambda)
                && matches!(self.peek_at(1), TokenKind::Ident(_)))
            {
                break;
            }
        }
        let body = self.parse_expression()?;
        let span = decls[0].span.merge(body.span());
        Ok(Expression::LocalFunctions {
            decls,
            body: Box::new(body),
            span,
        })
    }

    fn parse_pipe(&mut self) -> PResult<Expression> {
        let mut expr = self.parse_or()?;
        while matches!(self.peek(), TokenKind::Pipe) {
            self.advance();
            let stage = self.parse_stage()?;
            let span = expr.span().merge(stage.span());
            expr = Expression::Pipe {
                subject: Box::new(expr),
                stage: Box::new(stage),
                span,
            };
        }
        Ok(expr)
    }

    // ---- §13's 9-level arithmetic/comparison/logical ladder ----
    // Comparison/equality (levels 5/6) are non-associative: parsed as
    // "at most one" operator, chaining is a syntax error.

    fn parse_or(&mut self) -> PResult<Expression> {
        let mut expr = self.parse_xor()?;
        while matches!(self.peek(), TokenKind::Or) {
            self.advance();
            let rhs = self.parse_xor()?;
            let span = expr.span().merge(rhs.span());
            expr = Expression::BinaryOp {
                op: BinOp::Or,
                lhs: Box::new(expr),
                rhs: Box::new(rhs),
                span,
            };
        }
        Ok(expr)
    }

    fn parse_xor(&mut self) -> PResult<Expression> {
        let mut expr = self.parse_and()?;
        while matches!(self.peek(), TokenKind::Xor) {
            self.advance();
            let rhs = self.parse_and()?;
            let span = expr.span().merge(rhs.span());
            expr = Expression::BinaryOp {
                op: BinOp::Xor,
                lhs: Box::new(expr),
                rhs: Box::new(rhs),
                span,
            };
        }
        Ok(expr)
    }

    fn parse_and(&mut self) -> PResult<Expression> {
        let mut expr = self.parse_equality()?;
        while matches!(self.peek(), TokenKind::And) {
            self.advance();
            let rhs = self.parse_equality()?;
            let span = expr.span().merge(rhs.span());
            expr = Expression::BinaryOp {
                op: BinOp::And,
                lhs: Box::new(expr),
                rhs: Box::new(rhs),
                span,
            };
        }
        Ok(expr)
    }

    /// Level 6, non-associative: `==`/`!=`.
    fn parse_equality(&mut self) -> PResult<Expression> {
        let lhs = self.parse_comparison()?;
        let op = match self.peek() {
            TokenKind::EqEq => BinOp::Eq,
            TokenKind::NotEq => BinOp::NotEq,
            _ => return Ok(lhs),
        };
        self.advance();
        let rhs = self.parse_comparison()?;
        let span = lhs.span().merge(rhs.span());
        let node = Expression::BinaryOp {
            op,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
            span,
        };
        if matches!(self.peek(), TokenKind::EqEq | TokenKind::NotEq) {
            return Err(self.error(
                "equality operators do not chain — 'a == b == c' is not valid Obfusku \
                 (CONCRETE_SYMBOLIC_GRAMMAR.md §13: non-associative, not sugar for 'a==b ∧ b==c')",
            ));
        }
        Ok(node)
    }

    /// Level 5, non-associative: `< > <= >=`.
    fn parse_comparison(&mut self) -> PResult<Expression> {
        let lhs = self.parse_additive()?;
        let op = match self.peek() {
            TokenKind::Lt => BinOp::Lt,
            TokenKind::Gt => BinOp::Gt,
            TokenKind::Le => BinOp::Le,
            TokenKind::Ge => BinOp::Ge,
            _ => return Ok(lhs),
        };
        self.advance();
        let rhs = self.parse_additive()?;
        let span = lhs.span().merge(rhs.span());
        let node = Expression::BinaryOp {
            op,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
            span,
        };
        if matches!(
            self.peek(),
            TokenKind::Lt | TokenKind::Gt | TokenKind::Le | TokenKind::Ge
        ) {
            return Err(self.error(
                "comparison operators do not chain — 'a < b < c' is not valid Obfusku \
                 (CONCRETE_SYMBOLIC_GRAMMAR.md §13: non-associative, not sugar for 'a<b ∧ b<c')",
            ));
        }
        Ok(node)
    }

    fn parse_additive(&mut self) -> PResult<Expression> {
        let mut expr = self.parse_multiplicative()?;
        loop {
            let op = match self.peek() {
                TokenKind::Plus => BinOp::Add,
                TokenKind::Minus => BinOp::Sub,
                _ => break,
            };
            self.advance();
            let rhs = self.parse_multiplicative()?;
            let span = expr.span().merge(rhs.span());
            expr = Expression::BinaryOp {
                op,
                lhs: Box::new(expr),
                rhs: Box::new(rhs),
                span,
            };
        }
        Ok(expr)
    }

    fn parse_multiplicative(&mut self) -> PResult<Expression> {
        let mut expr = self.parse_unary()?;
        loop {
            let op = match self.peek() {
                TokenKind::Star => BinOp::Mul,
                TokenKind::Slash => BinOp::Div,
                TokenKind::Percent => BinOp::Mod,
                _ => break,
            };
            self.advance();
            let rhs = self.parse_unary()?;
            let span = expr.span().merge(rhs.span());
            expr = Expression::BinaryOp {
                op,
                lhs: Box::new(expr),
                rhs: Box::new(rhs),
                span,
            };
        }
        Ok(expr)
    }

    /// Level 2: prefix `¬`/`−`, right-to-left (i.e. `¬¬x` = `¬(¬x)`,
    /// via ordinary recursive descent). No ambiguity with binary `☠︎`
    /// (subtract) to resolve — distinct glyphs, distinct tokens.
    fn parse_unary(&mut self) -> PResult<Expression> {
        let op = match self.peek() {
            TokenKind::Not => UnOp::Not,
            TokenKind::Neg => UnOp::Neg,
            _ => return self.parse_application(),
        };
        let start = self.advance().span;
        let operand = self.parse_unary()?;
        let span = start.merge(operand.span());
        Ok(Expression::UnaryOp {
            op,
            operand: Box::new(operand),
            span,
        })
    }

    /// §12: bare-callable, or a parenthesized expression (which may
    /// contain `◈`). Classification is purely syntactic — parenthesized
    /// always means expression-stage, regardless of whether `◈`
    /// actually occurs inside.
    fn parse_stage(&mut self) -> PResult<Stage> {
        if matches!(self.peek(), TokenKind::LParen) {
            self.advance();
            let inner = self.parse_expression()?;
            self.expect(&TokenKind::RParen, "')'")?;
            if stage_has_pipe_ref_and_hole_conflict(&inner) {
                return Err(self.error(
                    "'◈' and '•' cannot appear in the same pipeline stage — presence of '◈' \
                     forces expression-stage interpretation, and '•' has no meaning there \
                     (CONCRETE_SYMBOLIC_GRAMMAR.md §12.1/§16.1)",
                ));
            }
            Ok(Stage::Expr(inner))
        } else {
            let callable = self.parse_application()?;
            Ok(Stage::Callable(callable))
        }
    }

    fn parse_application(&mut self) -> PResult<Expression> {
        let mut expr = self.parse_primary()?;
        // Greedy (§7.2): `(` continues Application, `[` continues
        // IndexExpr (§7.9) — same loop, so `f(x)[i]`/`arr[i][j]` chain.
        loop {
            if matches!(self.peek(), TokenKind::LParen) {
                self.advance();
                if matches!(self.peek(), TokenKind::RParen) {
                    return Err(self.error(
                        "expected at least one argument: zero-argument application ('f()') is not \
                         valid Obfusku — SEMANTIC_CORE.md's Apply is always single-argument and \
                         spec defines no Unit value to stand in for a zero-arity call",
                    ));
                }
                let mut args = vec![self.parse_argument()?];
                while matches!(self.peek(), TokenKind::Comma) {
                    self.advance();
                    args.push(self.parse_argument()?);
                }
                let end = self.expect(&TokenKind::RParen, "')'")?;
                let span = expr.span().merge(end);
                expr = Expression::Application {
                    callable: Box::new(expr),
                    args,
                    span,
                };
            } else if matches!(self.peek(), TokenKind::LBracket) {
                self.advance();
                let index = self.parse_expression()?;
                let end = self.expect(&TokenKind::RBracket, "']'")?;
                let span = expr.span().merge(end);
                expr = Expression::Index {
                    array: Box::new(expr),
                    index: Box::new(index),
                    span,
                };
            } else {
                break;
            }
        }
        Ok(expr)
    }

    /// `Argument ::= Expression | "•"` (§7.2).
    fn parse_argument(&mut self) -> PResult<Argument> {
        if matches!(self.peek(), TokenKind::Hole) {
            let span = self.advance().span;
            Ok(Argument::Hole(span))
        } else {
            Ok(Argument::Expr(self.parse_expression()?))
        }
    }

    fn parse_primary(&mut self) -> PResult<Expression> {
        let span = self.peek_span();
        match self.peek().clone() {
            TokenKind::Int(v) => {
                self.advance();
                Ok(Expression::Int(v, span))
            }
            TokenKind::Real(v) => {
                self.advance();
                Ok(Expression::Real(v, span))
            }
            TokenKind::Str(v) => {
                self.advance();
                Ok(Expression::Str(v, span))
            }
            TokenKind::Bool(v) => {
                self.advance();
                Ok(Expression::Bool(v, span))
            }
            // `∅` in expression position is the one Unit value —
            // disambiguated from `∅` the type purely by occurring here
            // (a type position never reaches `parse_primary`).
            TokenKind::TyUnit => {
                self.advance();
                Ok(Expression::Unit(span))
            }
            TokenKind::PipeRef => {
                self.advance();
                Ok(Expression::PipeRef(span))
            }
            // §7.7 ReferenceForm ("˚name"), or Rebind ("˚name ⚙︎ value")
            // when '˚' is immediately followed by '⚙︎'.
            TokenKind::Mut => {
                self.advance();
                let name_span = self.peek_span();
                match self.peek().clone() {
                    TokenKind::Ident(name) => {
                        self.advance();
                        if matches!(self.peek(), TokenKind::Rebind) {
                            self.advance();
                            let value = self.parse_expression()?;
                            let full_span = span.merge(value.span());
                            Ok(Expression::Rebind {
                                name,
                                name_span,
                                value: Box::new(value),
                                explicit: true,
                                span: full_span,
                            })
                        } else {
                            Ok(Expression::ExplicitRef(name, span.merge(name_span)))
                        }
                    }
                    other => Err(self.error(format!("expected a name after '˚', found {other:?}"))),
                }
            }
            TokenKind::Ident(name) => {
                self.advance();
                // §7.6 NamedConstruction: only recognized right after
                // an uppercase name; a Tag with no "{" falls through to
                // a bare Reference (nullary constructors).
                if starts_uppercase(&name) && matches!(self.peek(), TokenKind::LBrace) {
                    return self.parse_named_construction(name, span);
                }
                // §7.7 RebindForm, only right after a bare name.
                if matches!(self.peek(), TokenKind::Rebind) {
                    self.advance();
                    let value = self.parse_expression()?;
                    let full_span = span.merge(value.span());
                    Ok(Expression::Rebind {
                        name,
                        name_span: span,
                        value: Box::new(value),
                        explicit: false,
                        span: full_span,
                    })
                } else {
                    Ok(Expression::Reference(name, span))
                }
            }
            TokenKind::Lambda => self.parse_lambda(),
            TokenKind::Match => self.parse_match(),
            TokenKind::Raise => self.parse_raise(),
            TokenKind::Catch => self.parse_catch(),
            TokenKind::LParen => {
                self.advance();
                let inner = self.parse_expression()?;
                self.expect(&TokenKind::RParen, "')'")?;
                Ok(inner)
            }
            // §7.9. Empty (`[]`) is legal, unlike Application/Lambda.
            TokenKind::LBracket => {
                self.advance();
                let mut elements = Vec::new();
                if !matches!(self.peek(), TokenKind::RBracket) {
                    elements.push(self.parse_expression()?);
                    while matches!(self.peek(), TokenKind::Comma) {
                        self.advance();
                        elements.push(self.parse_expression()?);
                    }
                }
                let end = self.expect(&TokenKind::RBracket, "']'")?;
                let full_span = span.merge(end);
                Ok(Expression::ArrayLiteral {
                    elements,
                    span: full_span,
                })
            }
            other => Err(self.error(format!("expected an expression, found {other:?}"))),
        }
    }

    fn parse_lambda(&mut self) -> PResult<Expression> {
        let start = self.advance().span; // consume 'λ'
        self.expect(&TokenKind::LParen, "'(' after 'λ'")?;
        // §7.3: at least one Parameter is mandatory — zero-parameter
        // lambdas are excluded from the grammar for the same reason as
        // zero-argument calls (§7.2).
        if matches!(self.peek(), TokenKind::RParen) {
            return Err(self.error(
                "expected at least one parameter: zero-parameter lambdas ('λ() → …') are not \
                 valid Obfusku — SEMANTIC_CORE.md's Lambda is always single-parameter and spec \
                 defines no Unit value to stand in for a zero-arity parameter list",
            ));
        }
        let mut params = vec![self.parse_parameter()?];
        while matches!(self.peek(), TokenKind::Comma) {
            self.advance();
            params.push(self.parse_parameter()?);
        }
        self.expect(&TokenKind::RParen, "')'")?;
        self.expect(&TokenKind::Arrow, "'→'")?;
        let body = self.parse_expression()?;
        let span = start.merge(body.span());
        Ok(Expression::Lambda {
            params,
            body: Box::new(body),
            span,
        })
    }

    /// §8.2: `"λ" name "(" FnParameter,* ")" ":" Type "→" Expression`.
    fn parse_function_declaration(&mut self) -> PResult<Declaration> {
        let start = self.advance().span; // consume 'λ'
        let name_span = self.peek_span();
        let name = match self.peek().clone() {
            TokenKind::Ident(name) => {
                self.advance();
                name
            }
            other => {
                return Err(self.error(format!(
                    "expected a function name after 'λ' (a bare 'λ(...)' is an anonymous Lambda, \
                     not valid as a top-level declaration), found {other:?}"
                )))
            }
        };
        self.expect(&TokenKind::LParen, "'(' after the function name")?;
        if matches!(self.peek(), TokenKind::RParen) {
            return Err(self.error(
                "expected at least one parameter: zero-parameter function declarations \
                 ('λf() → …') are not valid Obfusku, for the same reason zero-parameter \
                 anonymous lambdas aren't (SEMANTIC_CORE.md's Lambda is always single-parameter \
                 and spec defines no Unit value to stand in for a zero-arity parameter list)",
            ));
        }
        let mut params = vec![self.parse_fn_parameter()?];
        while matches!(self.peek(), TokenKind::Comma) {
            self.advance();
            params.push(self.parse_fn_parameter()?);
        }
        self.expect(&TokenKind::RParen, "')'")?;
        self.expect(
            &TokenKind::Colon,
            "':' and a return type (mandatory on a FunctionDeclaration, CONCRETE_SYMBOLIC_GRAMMAR.md §8.2)",
        )?;
        // `parse_type_application`, not the full `parse_type_ref`: a
        // function-typed return's own `→` would otherwise swallow this
        // declaration's body-introducing `→` (no backtracking here) —
        // needs explicit parens: `λf(...): (⟁ → ⟁) → body`.
        let return_type = self.parse_type_application()?;
        self.expect(&TokenKind::Arrow, "'→'")?;
        // §8.4: export mark follows the declaration's own '≔' or '→'; a
        // FunctionDeclaration has no '≔', so it's '→⟳'. Accepted here
        // regardless of top-level vs. local position — §8.4 makes the
        // local restriction a static check, not a parse-time one, since
        // the grammar shape is identical; see parse_local_function_bindings.
        let exported = matches!(self.peek(), TokenKind::Export);
        if exported {
            self.advance();
        }
        let body = self.parse_expression()?;
        let span = start.merge(body.span());
        Ok(Declaration::Function(FunctionDeclaration {
            name,
            name_span,
            params,
            return_type,
            exported,
            body,
            span,
        }))
    }

    /// `FnParameter ::= name ":" Type` — annotation mandatory, unlike
    /// anonymous `Lambda`'s `Parameter` (§7.3).
    fn parse_fn_parameter(&mut self) -> PResult<FnParameter> {
        let span = self.peek_span();
        let name = match self.peek().clone() {
            TokenKind::Ident(name) => {
                self.advance();
                name
            }
            other => return Err(self.error(format!("expected a parameter name, found {other:?}"))),
        };
        self.expect(
            &TokenKind::Colon,
            "':' and a type (mandatory on a FunctionDeclaration parameter, CONCRETE_SYMBOLIC_GRAMMAR.md §8.2)",
        )?;
        let ty = self.parse_type_ref()?;
        let full_span = span.merge(ty.span());
        Ok(FnParameter {
            name,
            ty,
            span: full_span,
        })
    }

    /// §11: `"⟡" Expression "{" MatchArm ("⟢" MatchArm)* "}"`. Closed by
    /// the *generic* list-closer `}` (§6) — no bespoke "end of match"
    /// glyph, matching `GLYPH_SYSTEM_DESIGN.md` §7's finding.
    fn parse_match(&mut self) -> PResult<Expression> {
        let start = self.advance().span; // consume '⟡'
        let scrutinee = self.parse_expression()?;
        self.expect(&TokenKind::LBrace, "'{' to start the match arms")?;
        let mut arms = vec![self.parse_match_arm()?];
        while matches!(self.peek(), TokenKind::ArmSep) {
            self.advance();
            arms.push(self.parse_match_arm()?);
        }
        let end = self.expect(&TokenKind::RBrace, "'}'")?;
        let span = start.merge(end);
        Ok(Expression::Match {
            scrutinee: Box::new(scrutinee),
            arms,
            span,
        })
    }

    /// `NamedConstruction ::= Tag "{" FieldName ":" Expression ("⟢" FieldName ":" Expression)* "}"`
    /// (§7.6) — the opening `{` has already been peeked, not consumed,
    /// when called.
    fn parse_named_construction(&mut self, tag: String, tag_span: Span) -> PResult<Expression> {
        self.advance(); // consume '{'
        let mut fields = Vec::new();
        loop {
            let name = match self.peek().clone() {
                TokenKind::Ident(n) if !starts_uppercase(&n) => {
                    self.advance();
                    n
                }
                other => return Err(self.error(format!("expected a field name, found {other:?}"))),
            };
            self.expect(&TokenKind::Colon, "':'")?;
            let value = self.parse_expression()?;
            fields.push((name, value));
            if matches!(self.peek(), TokenKind::ArmSep) {
                self.advance();
                continue;
            }
            break;
        }
        let end = self.expect(&TokenKind::RBrace, "'}'")?;
        let span = tag_span.merge(end);
        Ok(Expression::NamedConstruction { tag, fields, span })
    }

    /// `CONCRETE_SYMBOLIC_GRAMMAR.md` §7.8: `"☄" Expression`.
    fn parse_raise(&mut self) -> PResult<Expression> {
        let start = self.advance().span; // consume '☄'
        let value = self.parse_expression()?;
        let span = start.merge(value.span());
        Ok(Expression::Raise(Box::new(value), span))
    }

    /// §7.8: `"☊" Expression Lambda`. `body` stops cleanly at `λ` (not a
    /// valid continuation of any expression), so `Lambda` parsing picks
    /// up exactly the handler.
    fn parse_catch(&mut self) -> PResult<Expression> {
        let start = self.advance().span; // consume '☊'
        let body = self.parse_expression()?;
        let handler = self.parse_expression()?;
        let span = start.merge(handler.span());
        Ok(Expression::Catch {
            body: Box::new(body),
            handler: Box::new(handler),
            span,
        })
    }

    /// `MatchArm ::= Pattern "→" Expression`.
    fn parse_match_arm(&mut self) -> PResult<MatchArm> {
        let pattern = self.parse_pattern()?;
        self.expect(&TokenKind::Arrow, "'→'")?;
        let result = self.parse_expression()?;
        let span = pattern.span().merge(result.span());
        Ok(MatchArm {
            pattern,
            result,
            span,
        })
    }

    /// `PNamedConstructor ::= Tag "{" name ":" Pattern ... "}"` (§10) —
    /// `{` only peeked, not consumed, when called.
    fn parse_named_constructor_pattern(&mut self, tag: String, tag_span: Span) -> PResult<Pattern> {
        self.advance(); // consume '{'
        let mut fields = Vec::new();
        loop {
            let name = match self.peek().clone() {
                TokenKind::Ident(n) if !starts_uppercase(&n) => {
                    self.advance();
                    n
                }
                other => return Err(self.error(format!("expected a field name, found {other:?}"))),
            };
            self.expect(&TokenKind::Colon, "':'")?;
            let pattern = self.parse_pattern()?;
            fields.push((name, pattern));
            if matches!(self.peek(), TokenKind::ArmSep) {
                self.advance();
                continue;
            }
            break;
        }
        let end = self.expect(&TokenKind::RBrace, "'}'")?;
        let span = tag_span.merge(end);
        Ok(Pattern::NamedConstructor { tag, fields, span })
    }

    fn parse_pattern(&mut self) -> PResult<Pattern> {
        let span = self.peek_span();
        match self.peek().clone() {
            TokenKind::Wildcard => {
                self.advance();
                Ok(Pattern::Wildcard(span))
            }
            TokenKind::Int(v) => {
                self.advance();
                Ok(Pattern::Int(v, span))
            }
            TokenKind::Real(v) => {
                self.advance();
                Ok(Pattern::Real(v, span))
            }
            TokenKind::Str(v) => {
                self.advance();
                Ok(Pattern::Str(v, span))
            }
            TokenKind::Bool(v) => {
                self.advance();
                Ok(Pattern::Bool(v, span))
            }
            TokenKind::Ident(name) if starts_uppercase(&name) => {
                self.advance();
                if matches!(self.peek(), TokenKind::LBrace) {
                    return self.parse_named_constructor_pattern(name, span);
                }
                let mut args = Vec::new();
                let mut end = span;
                if matches!(self.peek(), TokenKind::LParen) {
                    self.advance();
                    if !matches!(self.peek(), TokenKind::RParen) {
                        args.push(self.parse_pattern()?);
                        while matches!(self.peek(), TokenKind::Comma) {
                            self.advance();
                            args.push(self.parse_pattern()?);
                        }
                    }
                    end = self.expect(&TokenKind::RParen, "')'")?;
                }
                Ok(Pattern::Constructor {
                    tag: name,
                    args,
                    span: span.merge(end),
                })
            }
            TokenKind::Ident(name) => {
                self.advance();
                Ok(Pattern::Var(name, span))
            }
            other => Err(self.error(format!("expected a pattern, found {other:?}"))),
        }
    }

    /// `Parameter ::= name (":" Type)?` (§7.3) — annotation optional,
    /// unlike `FnParameter`'s mandatory one.
    fn parse_parameter(&mut self) -> PResult<Parameter> {
        let span = self.peek_span();
        match self.peek().clone() {
            TokenKind::Ident(name) => {
                self.advance();
                let ty = if matches!(self.peek(), TokenKind::Colon) {
                    self.advance();
                    Some(self.parse_type_ref()?)
                } else {
                    None
                };
                Ok(Parameter { name, ty, span })
            }
            other => Err(self.error(format!("expected a parameter name, found {other:?}"))),
        }
    }
}
