//! State shared by the lowering of one module.

use obfusku_diagnostics::{Diagnostic, Span};
use std::collections::{HashMap, HashSet};

/// The result of lowering one piece of the tree.
pub(super) type Lowered<T> = Result<T, Vec<Diagnostic>>;

/// A single error at `span`, ready to return from a lowering function.
pub(super) fn fail<T>(span: Span, message: impl Into<String>) -> Lowered<T> {
    Err(vec![Diagnostic::error(message, span)])
}

/// Lowers every item, keeping all the errors if any failed.
///
/// Independent siblings (array elements, record fields, match arms) are all
/// attempted so one pass reports every problem, not just the first.
pub(super) fn collect_all<T>(results: impl IntoIterator<Item = Lowered<T>>) -> Lowered<Vec<T>> {
    let mut values = Vec::new();
    let mut errors = Vec::new();
    for result in results {
        match result {
            Ok(value) => values.push(value),
            Err(mut found) => errors.append(&mut found),
        }
    }
    if errors.is_empty() {
        Ok(values)
    } else {
        Err(errors)
    }
}

/// Both results, or the errors of whichever failed (first's before second's).
pub(super) fn both<A, B>(a: Lowered<A>, b: Lowered<B>) -> Lowered<(A, B)> {
    match (a, b) {
        (Ok(a), Ok(b)) => Ok((a, b)),
        (Err(mut ea), Err(mut eb)) => {
            ea.append(&mut eb);
            Err(ea)
        }
        (Err(e), Ok(_)) | (Ok(_), Err(e)) => Err(e),
    }
}

/// What lowering carries from declaration to declaration.
pub(super) struct Lowerer {
    fresh_counter: u32,
    /// Names mutable so far. Module-level bindings are sequential (§11): a
    /// later binding sees earlier ones, never the reverse.
    pub(super) mut_names: HashSet<String>,
    /// Declared field order per record `Tag`, for reordering named
    /// construction and named patterns. A record must be declared before it
    /// is used.
    pub(super) record_fields: HashMap<String, Vec<String>>,
}

impl Lowerer {
    pub(super) fn new(
        mut_names: HashSet<String>,
        record_fields: HashMap<String, Vec<String>>,
    ) -> Self {
        Self {
            fresh_counter: 0,
            mut_names,
            record_fields,
        }
    }

    /// A synthetic pipe or hole name no source identifier can produce: `$`
    /// is not an identifier character, so there is no collision risk with
    /// real names.
    pub(super) fn fresh(&mut self) -> String {
        self.numbered("$pipe")
    }

    /// A module-unique type-variable name, for the rename an anonymous
    /// lambda's own type variables get (see `expression::lower_lambda`).
    pub(super) fn fresh_type_var(&mut self) -> String {
        self.numbered("$tv")
    }

    fn numbered(&mut self, prefix: &str) -> String {
        let name = format!("{prefix}{}", self.fresh_counter);
        self.fresh_counter += 1;
        name
    }

    /// Runs `lower` with each `(name, mutable)` in effect, restoring what the
    /// names were before.
    ///
    /// `mut_names` is one flat set for the whole pass, so a local binding's
    /// mutability must neither leak past its scope nor erase a same-named
    /// outer mutable binding it merely shadows.
    pub(super) fn with_mutability<T>(
        &mut self,
        names: impl IntoIterator<Item = (String, bool)>,
        lower: impl FnOnce(&mut Self) -> T,
    ) -> T {
        let mut saved: Vec<(String, bool)> = Vec::new();
        for (name, mutable) in names {
            if !saved.iter().any(|(n, _)| *n == name) {
                saved.push((name.clone(), self.mut_names.contains(&name)));
            }
            self.set_mutable(&name, mutable);
        }
        let out = lower(self);
        for (name, was_mutable) in saved {
            self.set_mutable(&name, was_mutable);
        }
        out
    }

    fn set_mutable(&mut self, name: &str, is_mutable: bool) {
        if is_mutable {
            self.mut_names.insert(name.to_string());
        } else {
            self.mut_names.remove(name);
        }
    }

    /// `values` in the record's declared field order.
    ///
    /// Shared by named construction (expressions) and named patterns:
    /// `fields` are `(name, value)` as written, and the result is each
    /// value in the order `tag` was declared with. It is an error if `tag` is
    /// not a known record, a field is missing, or one is not declared.
    pub(super) fn reorder_named_fields<'a, T>(
        &self,
        tag: &str,
        fields: &'a [(String, T)],
        span: Span,
    ) -> Lowered<Vec<&'a T>> {
        let Some(declared_order) = self.record_fields.get(tag) else {
            return fail(
                span,
                format!("'{tag}' is not a known record type (no matching RecordBody declaration)"),
            );
        };
        let mut by_name: HashMap<&str, &T> = fields.iter().map(|(n, v)| (n.as_str(), v)).collect();
        let mut ordered = Vec::with_capacity(declared_order.len());
        for field_name in declared_order {
            match by_name.remove(field_name.as_str()) {
                Some(value) => ordered.push(value),
                None => {
                    return fail(
                        span,
                        format!("missing field '{field_name}' in construction of '{tag}'"),
                    );
                }
            }
        }
        if let Some((extra_name, _)) = by_name.into_iter().next() {
            return fail(span, format!("'{tag}' has no field named '{extra_name}'"));
        }
        Ok(ordered)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lowerer() -> Lowerer {
        Lowerer::new(HashSet::new(), HashMap::new())
    }

    fn errors(r: Lowered<()>) -> Vec<String> {
        r.unwrap_err().into_iter().map(|d| d.message).collect()
    }

    #[test]
    fn collect_all_keeps_every_error_and_skips_values_on_failure() {
        let r = collect_all([
            Ok(1),
            fail(Span::default(), "a"),
            Ok(2),
            fail(Span::default(), "b"),
        ]);
        assert_eq!(errors(r.map(|_| ())), ["a", "b"]);
        assert_eq!(collect_all([Ok(1), Ok(2)]).unwrap(), [1, 2]);
    }

    #[test]
    fn both_merges_errors_in_order() {
        let r = both::<(), ()>(fail(Span::default(), "x"), fail(Span::default(), "y"));
        assert_eq!(errors(r.map(|_| ())), ["x", "y"]);
        assert!(both(Ok(1), Ok(2)).is_ok());
        assert_eq!(
            errors(both::<(), i32>(fail(Span::default(), "x"), Ok(1)).map(|_| ())),
            ["x"]
        );
    }

    #[test]
    fn fresh_names_are_unique_and_cannot_be_source_identifiers() {
        let mut l = lowerer();
        let names = [l.fresh(), l.fresh_type_var(), l.fresh()];
        assert!(names.iter().all(|n| n.starts_with('$')));
        let mut sorted = names.to_vec();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), 3);
    }

    #[test]
    fn mutability_is_restored_after_the_scope() {
        let mut l = lowerer();
        l.mut_names.insert("outer".into());
        l.with_mutability([("x".into(), true), ("outer".into(), false)], |l| {
            assert!(l.mut_names.contains("x"));
            assert!(!l.mut_names.contains("outer"));
        });
        assert!(!l.mut_names.contains("x"));
        assert!(l.mut_names.contains("outer"));
    }

    #[test]
    fn a_name_listed_twice_is_restored_to_its_original_state() {
        let mut l = lowerer();
        l.mut_names.insert("f".into());
        l.with_mutability([("f".into(), false), ("f".into(), false)], |_| {});
        assert!(l.mut_names.contains("f"));
    }

    fn records() -> Lowerer {
        let mut fields = HashMap::new();
        fields.insert("P".to_string(), vec!["x".to_string(), "y".to_string()]);
        Lowerer::new(HashSet::new(), fields)
    }

    fn given(pairs: &[(&str, i32)]) -> Vec<(String, i32)> {
        pairs.iter().map(|(n, v)| ((*n).to_string(), *v)).collect()
    }

    #[test]
    fn named_fields_come_back_in_declared_order() {
        let fields = given(&[("y", 2), ("x", 1)]);
        let ordered = records()
            .reorder_named_fields("P", &fields, Span::default())
            .unwrap();
        assert_eq!(ordered, [&1, &2]);
    }

    #[test]
    fn unknown_records_missing_and_extra_fields_are_errors() {
        let l = records();
        let at = Span::default();
        let one = given(&[("x", 1)]);
        let extra = given(&[("x", 1), ("y", 2), ("z", 3)]);
        let msg = |r: Lowered<Vec<&i32>>| r.unwrap_err().remove(0).message;
        assert!(msg(l.reorder_named_fields("Q", &one, at)).contains("not a known record type"));
        assert!(msg(l.reorder_named_fields("P", &one, at)).contains("missing field 'y'"));
        assert!(msg(l.reorder_named_fields("P", &extra, at)).contains("no field named 'z'"));
    }
}
