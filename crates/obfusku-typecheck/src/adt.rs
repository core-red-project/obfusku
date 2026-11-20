//! What the checker knows about declared types.
//!
//! [`AdtRegistry`] is built once per module from its type declarations (plus
//! imported ones) and consulted wherever a constructor, a record field or a
//! match needs to be resolved. Arity validation of written types lives next
//! to it because both answer "does this name mean what the program says it
//! means?".

mod arity;
mod registry;

pub(crate) use arity::{check_type_arity, free_params};
pub(crate) use registry::{ARRAY, AdtRegistry, EXCEPTION, VariantInfo};
