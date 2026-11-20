//! The language through the library API: real source text through the actual lexer, parser, desugarer, checker and evaluator, never hand-built Core.

mod support;

mod collections;
mod data;
mod diagnostics;
mod numbers_text;
mod operators;
mod recursion;
mod typing;
mod values;
