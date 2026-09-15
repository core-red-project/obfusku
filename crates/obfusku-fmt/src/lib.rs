//! Formatter — consumes surface AST, before desugaring (§13). No
//! dependency on `obfusku-core`/`obfusku-typecheck`/`obfusku-runtime`,
//! so formatting ill-typed, in-progress code still works.
//!
//! Comments are not preserved: the lexer discards them as trivia, and
//! nothing in the surface AST carries them. One fixed canonical style,
//! not configurable — `IMPLEMENTATION_ARCHITECTURE.md` §13 mandates the
//! architecture, not the exact spacing/indentation.
//!
//! Correctness contract: re-parsing `format(m)` must produce a
//! structurally equivalent module (`tests/roundtrip.rs`). Every
//! construct is parenthesized whenever bare printing would be
//! ambiguous — an extra parenthesis is fine, a wrong re-parse isn't.

use obfusku_syntax::ast::{
    Argument, BaseType, BinOp, Declaration, Expression, FnParameter, FunctionDeclaration, Module,
    Parameter, Pattern, Stage, TypeDeclaration, TypeRef, UnOp, ValueDeclaration, VariantDecl,
};

const SUB: &str = "\u{2620}\u{FE0E}"; // ☠︎ (binary subtract)
const REBIND: &str = "\u{2699}\u{FE0E}"; // ⚙︎

/// Format a surface module back to canonical source text.
pub fn format(module: &Module) -> String {
    let mut out = String::new();
    for decl in &module.declarations {
        print_declaration(&mut out, decl, 0);
        out.push('\n');
    }
    out.push('\u{2767}'); // ❧
    out.push('\n');
    out
}

fn indent(out: &mut String, level: usize) {
    for _ in 0..level {
        out.push_str("  ");
    }
}

fn print_declaration(out: &mut String, decl: &Declaration, level: usize) {
    match decl {
        Declaration::Value(vd) => print_value_declaration(out, vd, level),
        Declaration::Function(fd) => print_function_declaration(out, fd, level),
        Declaration::Type(td) => print_type_declaration(out, td, level),
        Declaration::Import(id) => {
            indent(out, level);
            out.push('\u{27F2}'); // ⟲
            out.push_str(&id.module_name);
        }
    }
}

/// §8.1's own documented ambiguity: `≔` immediately followed by `˚` is
/// always parsed as the `Mutability` modifier, never as the start of a
/// `ReferenceForm`/explicit-capture `RebindForm` value — both of which
/// print with a literal leading `˚`. An *immutable* binding whose value
/// is one of these must be parenthesized, exactly as spec prescribes
/// (`x ≔ (˚y)`), or re-parsing silently produces a *mutable* binding
/// with a different value instead.
fn value_starts_with_mut_glyph(value: &Expression) -> bool {
    matches!(
        value,
        Expression::ExplicitRef(..) | Expression::Rebind { explicit: true, .. }
    )
}

/// §8.1: `name "≔" Mutability? Export? Expression` — `Mutability`
/// (`˚`) then `Export` (`⟳`), that fixed order.
fn print_value_declaration(out: &mut String, vd: &ValueDeclaration, level: usize) {
    indent(out, level);
    out.push_str(&vd.name);
    if let Some(ty) = &vd.ty {
        out.push_str(": ");
        print_type_ref(out, ty);
    }
    out.push_str(" \u{2254} "); // ≔
    if vd.mutable {
        out.push('˚');
    }
    if vd.exported {
        out.push('\u{27F3}'); // ⟳
    }
    if !vd.mutable && value_starts_with_mut_glyph(&vd.value) {
        out.push('(');
        print_expression(out, &vd.value, Prec::Bottom);
        out.push(')');
    } else {
        print_expression(out, &vd.value, Prec::Bottom);
    }
}

/// §8.2: `"λ" name "(" FnParameter,* ")" ":" Type "→" Expression`.
fn print_function_declaration(out: &mut String, fd: &FunctionDeclaration, level: usize) {
    indent(out, level);
    out.push('λ');
    out.push_str(&fd.name);
    out.push('(');
    for (i, p) in fd.params.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        print_fn_parameter(out, p);
    }
    out.push_str("): ");
    print_type_ref(out, &fd.return_type);
    out.push_str(" \u{2192}"); // →
    if fd.exported {
        out.push('\u{27F3}'); // ⟳
    }
    out.push(' ');
    print_expression(out, &fd.body, Prec::Bottom);
}

fn print_fn_parameter(out: &mut String, p: &FnParameter) {
    out.push_str(&p.name);
    out.push_str(": ");
    print_type_ref(out, &p.ty);
}

/// §8.3: `TypeBody ::= SumBody | RecordBody | TupleBody`. Distinguished
/// like the parser does: one variant with `field_names: Some` → record;
/// one variant, `field_names: None`, tag == declaration tag → tuple;
/// else sum.
fn print_type_declaration(out: &mut String, td: &TypeDeclaration, level: usize) {
    indent(out, level);
    out.push_str(&td.tag);
    for p in &td.type_params {
        out.push(' ');
        out.push_str(p);
    }
    out.push_str(" \u{2254} "); // ≔

    if td.variants.len() == 1 && td.variants[0].tag == td.tag {
        let v = &td.variants[0];
        if let Some(names) = &v.field_names {
            out.push_str("{ ");
            for (i, (name, ty)) in names.iter().zip(v.fields.iter()).enumerate() {
                if i > 0 {
                    out.push_str(" \u{27E2} "); // ⟢
                }
                out.push_str(name);
                out.push_str(": ");
                print_type_ref(out, ty);
            }
            out.push_str(" }");
            return;
        }
        out.push('(');
        for (i, ty) in v.fields.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            print_type_ref(out, ty);
        }
        out.push(')');
        return;
    }

    out.push_str("{ ");
    for (i, v) in td.variants.iter().enumerate() {
        if i > 0 {
            out.push_str(" \u{27E2} "); // ⟢
        }
        print_variant(out, v);
    }
    out.push_str(" }");
}

fn print_variant(out: &mut String, v: &VariantDecl) {
    out.push_str(&v.tag);
    if !v.fields.is_empty() {
        out.push('(');
        for (i, ty) in v.fields.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            print_type_ref(out, ty);
        }
        out.push(')');
    }
}

/// §9: right-assoc `FunctionType` (`→`), left-assoc `TypeApplication`
/// (`▷`, tighter). A function-typed operand always needs parens.
fn print_type_ref(out: &mut String, ty: &TypeRef) {
    match ty {
        TypeRef::Base(b, _) => out.push_str(base_type_glyph(*b)),
        TypeRef::Named(n, _) => out.push_str(n),
        TypeRef::Var(n, _) => out.push_str(n),
        TypeRef::Function(lhs, rhs, _) => {
            print_type_operand(out, lhs, true);
            out.push_str(" \u{2192} "); // →
            print_type_ref(out, rhs);
        }
        TypeRef::Apply(lhs, rhs, _) => {
            print_type_ref(out, lhs);
            out.push_str(" \u{25B7} "); // ▷
            print_type_operand(out, rhs, true);
        }
    }
}

fn print_type_operand(out: &mut String, ty: &TypeRef, parenthesize_function: bool) {
    if parenthesize_function && matches!(ty, TypeRef::Function(..)) {
        out.push('(');
        print_type_ref(out, ty);
        out.push(')');
    } else {
        print_type_ref(out, ty);
    }
}

fn base_type_glyph(b: BaseType) -> &'static str {
    match b {
        BaseType::Int => "\u{27C1}",  // ⟁
        BaseType::Real => "\u{29C6}", // ⧆
        BaseType::Str => "\u{2318}",  // ⌘
        BaseType::Bool => "\u{25CB}", // ○
        BaseType::Unit => "\u{2205}", // ∅
    }
}

/// §13's precedence ladder — higher binds tighter; a child needs
/// `precedence(child) >= min_prec` to print bare. `Bottom` is used at
/// every "owns the rest of the expression" call site (parsed via the
/// full `parse_expression`, so nothing needs protecting from a
/// following operator).
#[derive(Clone, Copy, PartialEq, PartialOrd)]
struct Prec(u8);

impl Prec {
    const BOTTOM: Prec = Prec(0);
    const OR: Prec = Prec(1);
    const XOR: Prec = Prec(2);
    const AND: Prec = Prec(3);
    const EQUALITY: Prec = Prec(4);
    const COMPARISON: Prec = Prec(5);
    const ADDITIVE: Prec = Prec(6);
    const MULTIPLICATIVE: Prec = Prec(7);
    const UNARY: Prec = Prec(8);
    const ATOM: Prec = Prec(9);

    #[allow(non_upper_case_globals)]
    const Bottom: Prec = Prec::BOTTOM;
}

fn binop_prec(op: BinOp) -> Prec {
    match op {
        BinOp::Or => Prec::OR,
        BinOp::Xor => Prec::XOR,
        BinOp::And => Prec::AND,
        BinOp::Eq | BinOp::NotEq => Prec::EQUALITY,
        BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge => Prec::COMPARISON,
        BinOp::Add | BinOp::Sub => Prec::ADDITIVE,
        BinOp::Mul | BinOp::Div | BinOp::Mod => Prec::MULTIPLICATIVE,
    }
}

fn is_left_associative(op: BinOp) -> bool {
    !matches!(
        op,
        BinOp::Eq | BinOp::NotEq | BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge
    )
}

/// `Lambda`/`Rebind`/`Pipe`/`LocalValue`/`LocalFunctions` get
/// `Prec::BOTTOM`: each parses its trailing piece via the full
/// expression grammar with no closing delimiter, so printing one bare
/// as an operand would let it swallow whatever should follow it.
/// Everything else is self-delimiting, safe at `Prec::ATOM`.
fn expr_prec(e: &Expression) -> Prec {
    match e {
        Expression::BinaryOp { op, .. } => binop_prec(*op),
        Expression::UnaryOp { .. } => Prec::UNARY,
        Expression::Lambda { .. }
        | Expression::Rebind { .. }
        | Expression::Pipe { .. }
        | Expression::LocalValue { .. }
        | Expression::LocalFunctions { .. } => Prec::BOTTOM,
        _ => Prec::ATOM,
    }
}

/// Prints `e` as an operand requiring at least `min_prec` to appear
/// bare, parenthesizing otherwise.
fn print_operand(out: &mut String, e: &Expression, min_prec: Prec) {
    if expr_prec(e) < min_prec {
        out.push('(');
        print_expression(out, e, Prec::Bottom);
        out.push(')');
    } else {
        print_expression(out, e, min_prec);
    }
}

fn print_expression(out: &mut String, e: &Expression, min_prec: Prec) {
    match e {
        Expression::Int(v, _) => out.push_str(&v.to_string()),
        Expression::Real(v, _) => out.push_str(&format_real(*v)),
        Expression::Str(v, _) => print_string_literal(out, v),
        Expression::Bool(true, _) => out.push('\u{25C9}'), // ◉
        Expression::Bool(false, _) => out.push('\u{25CE}'), // ◎
        Expression::Unit(_) => out.push('\u{2205}'),       // ∅
        Expression::Reference(name, _) => out.push_str(name),
        Expression::PipeRef(_) => out.push('\u{25C8}'), // ◈
        Expression::ExplicitRef(name, _) => {
            out.push('˚');
            out.push_str(name);
        }

        Expression::Lambda { params, body, .. } => {
            out.push('λ');
            out.push('(');
            for (i, p) in params.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                print_parameter(out, p);
            }
            out.push_str(") \u{2192} "); // →
            print_expression(out, body, Prec::Bottom);
        }

        Expression::Application { callable, args, .. } => {
            print_operand(out, callable, Prec::ATOM);
            out.push('(');
            for (i, a) in args.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                match a {
                    Argument::Expr(e) => print_expression(out, e, Prec::Bottom),
                    Argument::Hole(_) => out.push('\u{2022}'), // •
                }
            }
            out.push(')');
        }

        Expression::Index { array, index, .. } => {
            print_operand(out, array, Prec::ATOM);
            out.push('[');
            print_expression(out, index, Prec::Bottom);
            out.push(']');
        }

        Expression::ArrayLiteral { elements, .. } => {
            out.push('[');
            for (i, elem) in elements.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                print_expression(out, elem, Prec::Bottom);
            }
            out.push(']');
        }

        Expression::Pipe { subject, stage, .. } => {
            print_operand(out, subject, Prec::ATOM);
            out.push_str(" \u{25B7} "); // ▷
            match &**stage {
                Stage::Callable(c) => print_operand(out, c, Prec::ATOM),
                Stage::Expr(inner) => {
                    out.push('(');
                    print_expression(out, inner, Prec::Bottom);
                    out.push(')');
                }
            }
        }

        Expression::Rebind {
            name,
            value,
            explicit,
            ..
        } => {
            if *explicit {
                out.push('˚');
            }
            out.push_str(name);
            out.push_str(&format!(" {REBIND} "));
            print_expression(out, value, Prec::Bottom);
        }

        Expression::Match {
            scrutinee, arms, ..
        } => {
            out.push('\u{27E1}'); // ⟡
            out.push(' ');
            print_operand(out, scrutinee, Prec::ATOM);
            out.push_str(" {\n");
            for (i, arm) in arms.iter().enumerate() {
                if i > 0 {
                    out.push_str(" \u{27E2}\n"); // ⟢
                }
                out.push_str("  ");
                print_pattern(out, &arm.pattern);
                out.push_str(" \u{2192} "); // →
                print_expression(out, &arm.result, Prec::Bottom);
            }
            out.push_str("\n}");
        }

        Expression::BinaryOp { op, lhs, rhs, .. } => {
            let prec = binop_prec(*op);
            if prec < min_prec {
                out.push('(');
                print_binop(out, *op, lhs, rhs);
                out.push(')');
            } else {
                print_binop(out, *op, lhs, rhs);
            }
        }

        Expression::UnaryOp { op, operand, .. } => {
            if Prec::UNARY < min_prec {
                out.push('(');
                print_unop(out, *op, operand);
                out.push(')');
            } else {
                print_unop(out, *op, operand);
            }
        }

        Expression::Raise(value, _) => {
            out.push('\u{2604}'); // ☄
            out.push(' ');
            print_operand(out, value, Prec::ATOM);
        }

        Expression::Catch { body, handler, .. } => {
            out.push('\u{260A}'); // ☊
            out.push(' ');
            print_operand(out, body, Prec::ATOM);
            out.push(' ');
            // `handler` must stay a bare `λ`: `parse_catch` relies on an
            // unparenthesized `λ` to know where `body` ends. Wrapping it
            // in `(...)` would look like an Application continuing `body`.
            print_expression(out, handler, Prec::Bottom);
        }

        Expression::NamedConstruction { tag, fields, .. } => {
            out.push_str(tag);
            out.push_str(" { ");
            for (i, (name, value)) in fields.iter().enumerate() {
                if i > 0 {
                    out.push_str(" \u{27E2} "); // ⟢
                }
                out.push_str(name);
                out.push_str(": ");
                print_expression(out, value, Prec::Bottom);
            }
            out.push_str(" }");
        }

        Expression::LocalValue { decl, body, .. } => {
            print_value_declaration(out, decl, 0);
            out.push('\n');
            print_expression(out, body, Prec::Bottom);
        }

        Expression::LocalFunctions { decls, body, .. } => {
            for fd in decls {
                print_function_declaration(out, fd, 0);
                out.push('\n');
            }
            print_expression(out, body, Prec::Bottom);
        }
    }
}

fn print_binop(out: &mut String, op: BinOp, lhs: &Expression, rhs: &Expression) {
    let prec = binop_prec(op);
    let left_assoc = is_left_associative(op);
    // Left-associative: the left operand may be the same precedence
    // (groups naturally); the right may not, or `a - b - c` would
    // re-parse as `a - (b - c)`. Non-associative operators (comparison,
    // equality) require both sides strictly tighter, matching the
    // parser's own "chaining is a syntax error" rule.
    let lhs_min = if left_assoc { prec } else { Prec(prec.0 + 1) };
    let rhs_min = Prec(prec.0 + 1);
    print_operand(out, lhs, lhs_min);
    out.push(' ');
    out.push_str(binop_glyph(op));
    out.push(' ');
    print_operand(out, rhs, rhs_min);
}

fn print_unop(out: &mut String, op: UnOp, operand: &Expression) {
    out.push_str(unop_glyph(op));
    print_operand(out, operand, Prec::UNARY);
}

fn binop_glyph(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "\u{271A}",
        BinOp::Sub => SUB,
        BinOp::Mul => "\u{2731}",
        BinOp::Div => "\u{00F7}",
        BinOp::Mod => "\u{2317}",
        BinOp::Lt => "<",
        BinOp::Gt => ">",
        BinOp::Le => "\u{2264}",
        BinOp::Ge => "\u{2265}",
        BinOp::Eq => "\u{2261}",
        BinOp::NotEq => "\u{2260}",
        BinOp::And => "\u{2227}",
        BinOp::Or => "\u{2228}",
        BinOp::Xor => "\u{22BB}",
    }
}

fn unop_glyph(op: UnOp) -> &'static str {
    match op {
        UnOp::Not => "\u{00AC}", // ¬
        UnOp::Neg => "\u{2212}", // −
    }
}

fn print_parameter(out: &mut String, p: &Parameter) {
    out.push_str(&p.name);
    if let Some(ty) = &p.ty {
        out.push_str(": ");
        print_type_ref(out, ty);
    }
}

fn print_pattern(out: &mut String, p: &Pattern) {
    match p {
        Pattern::Var(name, _) => out.push_str(name),
        Pattern::Wildcard(_) => out.push('_'),
        Pattern::Int(v, _) => out.push_str(&v.to_string()),
        Pattern::Real(v, _) => out.push_str(&format_real(*v)),
        Pattern::Str(v, _) => print_string_literal(out, v),
        Pattern::Bool(true, _) => out.push('\u{25C9}'),
        Pattern::Bool(false, _) => out.push('\u{25CE}'),
        Pattern::Constructor { tag, args, .. } => {
            out.push_str(tag);
            if !args.is_empty() {
                out.push('(');
                for (i, a) in args.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    print_pattern(out, a);
                }
                out.push(')');
            }
        }
        Pattern::NamedConstructor { tag, fields, .. } => {
            out.push_str(tag);
            out.push_str(" { ");
            for (i, (name, pat)) in fields.iter().enumerate() {
                if i > 0 {
                    out.push_str(" \u{27E2} ");
                }
                out.push_str(name);
                out.push_str(": ");
                print_pattern(out, pat);
            }
            out.push_str(" }");
        }
    }
}

fn print_string_literal(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out.push('"');
}

fn format_real(v: f64) -> String {
    if v == v.trunc() && v.is_finite() {
        format!("{v:.1}")
    } else {
        format!("{v}")
    }
}
