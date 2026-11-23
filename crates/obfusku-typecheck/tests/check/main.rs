//! Conformance tests for `obfusku-typecheck`, built directly against
//! `obfusku_core::ast` so they prove the checker works independent of the parser.

#![allow(
    clippy::match_wildcard_for_single_variants,
    clippy::match_wild_err_arm,
    clippy::manual_let_else
)]

mod adts;
mod collections;
mod declarations;
mod generics;
mod inference;
mod matching;
mod operators;
mod prelude;
mod support;
