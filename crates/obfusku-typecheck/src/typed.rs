//! What a successful check hands back.

use crate::types::Scheme;
use obfusku_diagnostics::Diagnostic;

/// A module that has passed type-checking.
///
/// Every binding's type is either a fully resolved
/// [`obfusku_core::types::Type`] or a quantified [`Scheme`], never a bare
/// variable: an unresolved variable is a type error `check` reports, not
/// something carried into a "successful" result.
#[derive(Debug)]
pub struct TypedModule {
    /// One entry per top-level binding, in source order.
    pub bindings: Vec<TypedBinding>,
    /// Non-fatal findings, currently unreachable `Match` arms.
    pub warnings: Vec<Diagnostic>,
}

/// A checked top-level binding.
#[derive(Debug)]
pub struct TypedBinding {
    /// The bound name.
    pub name: String,
    /// Its type.
    pub ty: TypeResult,
    /// Whether importers can see it.
    pub exported: bool,
}

/// The type of a binding: fixed, or generalized over some variables.
#[derive(Debug)]
pub enum TypeResult {
    /// No quantified variables; the type is fully known.
    Monomorphic(obfusku_core::types::Type),
    /// Generalized by the value restriction (`SEMANTIC_CORE.md` §19.1).
    Polymorphic(Scheme),
}
