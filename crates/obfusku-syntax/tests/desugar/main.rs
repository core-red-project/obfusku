//! Surface-to-Core lowering conformance tests (`SEMANTIC_CORE.md` §16), grouped by what they exercise.

// Tests destructure one expected shape and fail on anything else.
#![allow(clippy::match_wildcard_for_single_variants)]

mod expressions;
mod groups;
mod locals_and_collections;
mod mutability;
mod operators;
mod records_and_exceptions;
mod support;
mod types;
