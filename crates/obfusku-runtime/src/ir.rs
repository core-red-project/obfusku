//! Resolved form of the Core AST that the evaluator runs: every local
//! variable is a `(frame depth, slot)` pair, every top-level name a
//! shared cell, every constructor tag an interned `Rc<str>`. Produced
//! once per top-level binding, so evaluation never hashes or compares
//! identifier text.

use crate::Text;
use crate::types;
use crate::value::Value;
use obfusku_core::ast::{BinOp, Expr, Literal, Pattern, UnOp};
use obfusku_diagnostics::Span;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// A top-level (or prelude) name's storage. Filled when its binding
/// evaluates; a closure compiled earlier in the same recursive group
/// sees the value once it exists.
#[derive(Debug)]
pub struct GlobalCell {
    pub name: Rc<str>,
    pub value: RefCell<Option<Value>>,
}

pub type Globals = HashMap<String, Rc<GlobalCell>>;

pub enum Node {
    Local {
        depth: u32,
        slot: u32,
    },
    Global(Rc<GlobalCell>, Span),
    Unbound(Rc<str>, Span),
    Lit(Value),
    Lambda {
        body: Rc<Node>,
        name: Rc<str>,
    },
    Apply {
        func: Box<Node>,
        arg: Box<Node>,
        span: Span,
    },
    MutCell(Box<Node>),
    MutRead(Box<Node>, Span),
    MutRebind {
        cell: Box<Node>,
        value: Box<Node>,
        span: Span,
    },
    Constructor {
        tag: Rc<str>,
        args: Vec<Node>,
    },
    Match {
        scrutinee: Box<Node>,
        arms: Vec<Arm>,
    },
    Raise(Box<Node>, Span),
    Catch {
        body: Box<Node>,
        handler: Rc<Node>,
    },
    Binary {
        op: BinOp,
        lhs: Box<Node>,
        rhs: Box<Node>,
        span: Span,
    },
    Unary {
        op: UnOp,
        operand: Box<Node>,
        span: Span,
    },
    Let {
        value: Box<Node>,
        body: Rc<Node>,
    },
    LetRec {
        values: Vec<Node>,
        body: Rc<Node>,
    },
    Array(Vec<Node>),
    Index {
        array: Box<Node>,
        index: Box<Node>,
        span: Span,
    },
    Field {
        base: Box<Node>,
        name: String,
        span: Span,
    },
    Update {
        base: Box<Node>,
        fields: Vec<(String, Node)>,
        span: Span,
    },
}

pub struct Arm {
    pub pattern: Pat,
    /// Slots the pattern binds; a frame exists only when this is non-zero.
    pub vars: u32,
    pub result: Rc<Node>,
}

pub enum Pat {
    Wild,
    Var,
    Lit(Value),
    Con { tag: Rc<str>, args: Vec<Pat> },
}

thread_local! {
    static TAGS: RefCell<HashMap<String, Rc<str>>> = RefCell::new(HashMap::new());
}

/// One shared `Rc<str>` per distinct tag, so tag equality is usually a
/// pointer comparison.
#[must_use]
pub fn intern(tag: &str) -> Rc<str> {
    TAGS.with(|t| {
        let mut t = t.borrow_mut();
        if let Some(rc) = t.get(tag) {
            return Rc::clone(rc);
        }
        let rc: Rc<str> = Rc::from(tag);
        t.insert(tag.to_string(), Rc::clone(&rc));
        rc
    })
}

#[must_use]
pub fn literal_value(lit: &Literal) -> Value {
    match lit {
        Literal::Int(v) => Value::Int(*v),
        Literal::Real(v) => Value::Real(*v),
        Literal::Str(v) => Value::Str(Text::from(v.as_str())),
        Literal::Bool(v) => Value::Bool(*v),
        Literal::Unit => Value::Unit,
    }
}

pub struct Compiler<'a> {
    pub globals: &'a mut Globals,
    scopes: Vec<Vec<String>>,
}

impl<'a> Compiler<'a> {
    pub fn new(globals: &'a mut Globals) -> Self {
        Compiler {
            globals,
            scopes: Vec::new(),
        }
    }

    fn resolve(&self, name: &str) -> Option<(u32, u32)> {
        for (depth, frame) in self.scopes.iter().rev().enumerate() {
            if let Some(slot) = frame.iter().rposition(|n| n == name) {
                return Some((depth as u32, slot as u32));
            }
        }
        None
    }

    fn scoped<T>(&mut self, names: Vec<String>, f: impl FnOnce(&mut Self) -> T) -> T {
        self.scopes.push(names);
        let out = f(self);
        self.scopes.pop();
        out
    }

    /// `hint` names a `Lambda` bound directly to a name, for call traces.
    pub fn expr(&mut self, e: &Expr, hint: Option<&str>) -> Node {
        match e {
            Expr::Var(name, span) => match self.resolve(name) {
                Some((depth, slot)) => Node::Local { depth, slot },
                None => match self.globals.get(name) {
                    Some(cell) => Node::Global(Rc::clone(cell), *span),
                    None => Node::Unbound(Rc::from(name.as_str()), *span),
                },
            },
            Expr::Lit(lit, _) => Node::Lit(literal_value(lit)),
            Expr::Lambda { param, body, .. } => {
                // A curried declaration is a lambda chain: every link
                // carries the declared name.
                let inner = matches!(**body, Expr::Lambda { .. })
                    .then_some(hint)
                    .flatten();
                let body = self.scoped(vec![param.clone()], |c| c.expr(body, inner));
                Node::Lambda {
                    body: Rc::new(body),
                    name: Rc::from(hint.unwrap_or("λ")),
                }
            }
            Expr::Apply { func, arg, span } => Node::Apply {
                func: Box::new(self.expr(func, None)),
                arg: Box::new(self.expr(arg, None)),
                span: *span,
            },
            Expr::MutCell { initial, .. } => Node::MutCell(Box::new(self.expr(initial, None))),
            Expr::MutRead { cell, span } => Node::MutRead(Box::new(self.expr(cell, None)), *span),
            Expr::MutRebind {
                cell,
                new_value,
                span,
            } => Node::MutRebind {
                cell: Box::new(self.expr(cell, None)),
                value: Box::new(self.expr(new_value, None)),
                span: *span,
            },
            Expr::Constructor { tag, args, .. } => Node::Constructor {
                tag: intern(tag),
                args: args.iter().map(|a| self.expr(a, None)).collect(),
            },
            Expr::Match {
                scrutinee, arms, ..
            } => {
                let scrutinee = Box::new(self.expr(scrutinee, None));
                let arms = arms
                    .iter()
                    .map(|arm| {
                        let mut names = Vec::new();
                        let pattern = pattern(&arm.pattern, &mut names);
                        let vars = names.len() as u32;
                        let result = if vars == 0 {
                            self.expr(&arm.result, None)
                        } else {
                            self.scoped(names, |c| c.expr(&arm.result, None))
                        };
                        Arm {
                            pattern,
                            vars,
                            result: Rc::new(result),
                        }
                    })
                    .collect();
                Node::Match { scrutinee, arms }
            }
            Expr::Raise { value, span } => Node::Raise(Box::new(self.expr(value, None)), *span),
            Expr::Catch {
                body,
                handler_param,
                handler_body,
                ..
            } => Node::Catch {
                body: Box::new(self.expr(body, None)),
                handler: Rc::new(
                    self.scoped(vec![handler_param.clone()], |c| c.expr(handler_body, None)),
                ),
            },
            Expr::BinaryOp { op, lhs, rhs, span } => Node::Binary {
                op: *op,
                lhs: Box::new(self.expr(lhs, None)),
                rhs: Box::new(self.expr(rhs, None)),
                span: *span,
            },
            Expr::UnaryOp { op, operand, span } => Node::Unary {
                op: *op,
                operand: Box::new(self.expr(operand, None)),
                span: *span,
            },
            Expr::Let {
                name, value, body, ..
            } => {
                let value = Box::new(self.expr(value, Some(name)));
                let body = self.scoped(vec![name.clone()], |c| c.expr(body, None));
                Node::Let {
                    value,
                    body: Rc::new(body),
                }
            }
            Expr::LetRec { bindings, body, .. } => {
                let names: Vec<String> = bindings.iter().map(|b| b.name.clone()).collect();
                self.scoped(names, |c| {
                    let values = bindings
                        .iter()
                        .map(|b| c.expr(&b.value, Some(&b.name)))
                        .collect();
                    let body = c.expr(body, None);
                    Node::LetRec {
                        values,
                        body: Rc::new(body),
                    }
                })
            }
            Expr::ArrayLiteral { elements, .. } => {
                Node::Array(elements.iter().map(|x| self.expr(x, None)).collect())
            }
            Expr::Index { array, index, span } => Node::Index {
                array: Box::new(self.expr(array, None)),
                index: Box::new(self.expr(index, None)),
                span: *span,
            },
            Expr::Field { base, name, span } => Node::Field {
                base: Box::new(self.expr(base, None)),
                name: name.clone(),
                span: *span,
            },
            Expr::Update { base, fields, span } => Node::Update {
                base: Box::new(self.expr(base, None)),
                fields: fields
                    .iter()
                    .map(|(n, v)| (n.clone(), self.expr(v, None)))
                    .collect(),
                span: *span,
            },
        }
    }
}

fn pattern(p: &Pattern, names: &mut Vec<String>) -> Pat {
    match p {
        Pattern::Wildcard(_) => Pat::Wild,
        Pattern::Var(name, _) => {
            names.push(name.clone());
            Pat::Var
        }
        Pattern::Lit(lit, _) => Pat::Lit(literal_value(lit)),
        Pattern::Constructor { tag, args, .. } => Pat::Con {
            tag: intern(tag),
            args: args.iter().map(|a| pattern(a, names)).collect(),
        },
    }
}

/// Registers a module's type metadata; kept here so `lib.rs` has one
/// entry point for "prepare to run this module".
pub fn register_types(decls: &[obfusku_core::ast::TypeDecl]) {
    types::register(decls);
}
