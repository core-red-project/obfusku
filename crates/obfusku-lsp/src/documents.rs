//! The server's copy of every open document.

use lsp_server::Notification;
use lsp_types::notification::{
    DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, Notification as _,
};
use lsp_types::{
    DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams, Uri,
};
use std::collections::HashMap;

/// An open document: the URI exactly as the client sent it, and its text.
///
/// The URI is kept whole so diagnostics can be published back under the very
/// spelling the client used.
#[derive(Debug)]
pub(crate) struct Document {
    pub(crate) uri: Uri,
    pub(crate) text: String,
}

/// What a notification asks the server to do next.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Update {
    /// Re-analyze the document stored under this key.
    Analyze(String),
    /// The document closed: remove its diagnostics from the client.
    Clear(Uri),
}

/// All open documents, keyed by the URI's string form.
///
/// **Why a string key.** `lsp_types::Uri` has interior mutability (it wraps
/// `fluent_uri::Uri`, which caches its parsed authority behind a `Cell`), so
/// it cannot key a `HashMap` without tripping Clippy's `mutable_key_type`.
#[derive(Debug, Default)]
pub(crate) struct Documents {
    open: HashMap<String, Document>,
}

impl Documents {
    /// The document stored under `key`.
    pub(crate) fn get(&self, key: &str) -> Option<&Document> {
        self.open.get(key)
    }

    /// Applies a text-synchronization notification, returning what to publish
    /// next. Any other notification, and malformed parameters, are ignored: a
    /// bad message from a client must not stop the server.
    ///
    /// Sync is *full* (see `main`), so a change event carries the whole text
    /// and only the last one of a batch matters.
    pub(crate) fn apply(&mut self, notification: Notification) -> Option<Update> {
        match notification.method.as_str() {
            DidOpenTextDocument::METHOD => {
                let params: DidOpenTextDocumentParams =
                    serde_json::from_value(notification.params).ok()?;
                let document = params.text_document;
                Some(self.store(document.uri, document.text))
            }
            DidChangeTextDocument::METHOD => {
                let params: DidChangeTextDocumentParams =
                    serde_json::from_value(notification.params).ok()?;
                let uri = params.text_document.uri;
                match params.content_changes.into_iter().last() {
                    Some(change) => Some(self.store(uri, change.text)),
                    None => Some(Update::Analyze(uri.as_str().to_string())),
                }
            }
            DidCloseTextDocument::METHOD => {
                let params: DidCloseTextDocumentParams =
                    serde_json::from_value(notification.params).ok()?;
                let uri = params.text_document.uri;
                self.open.remove(uri.as_str());
                Some(Update::Clear(uri))
            }
            _ => None,
        }
    }

    fn store(&mut self, uri: Uri, text: String) -> Update {
        let key = uri.as_str().to_string();
        self.open.insert(key.clone(), Document { uri, text });
        Update::Analyze(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const URI: &str = "file:///proj/main.obk";

    fn note(method: &str, params: serde_json::Value) -> Notification {
        Notification::new(method.to_string(), params)
    }

    fn open(text: &str) -> Notification {
        note(
            DidOpenTextDocument::METHOD,
            json!({"textDocument": {"uri": URI, "languageId": "obfusku", "version": 1, "text": text}}),
        )
    }

    fn change(texts: &[&str]) -> Notification {
        let changes: Vec<_> = texts.iter().map(|t| json!({"text": t})).collect();
        note(
            DidChangeTextDocument::METHOD,
            json!({"textDocument": {"uri": URI, "version": 2}, "contentChanges": changes}),
        )
    }

    fn close() -> Notification {
        note(
            DidCloseTextDocument::METHOD,
            json!({"textDocument": {"uri": URI}}),
        )
    }

    #[test]
    fn opening_stores_the_text_and_asks_for_analysis() {
        let mut docs = Documents::default();
        assert_eq!(
            docs.apply(open("a")),
            Some(Update::Analyze(URI.to_string()))
        );
        assert_eq!(docs.get(URI).map(|d| d.text.as_str()), Some("a"));
    }

    #[test]
    fn a_change_replaces_the_text_with_the_last_event() {
        let mut docs = Documents::default();
        docs.apply(open("a"));
        assert_eq!(
            docs.apply(change(&["b", "c"])),
            Some(Update::Analyze(URI.to_string()))
        );
        assert_eq!(docs.get(URI).map(|d| d.text.as_str()), Some("c"));
    }

    #[test]
    fn a_change_without_events_keeps_the_text() {
        let mut docs = Documents::default();
        docs.apply(open("a"));
        docs.apply(change(&[]));
        assert_eq!(docs.get(URI).map(|d| d.text.as_str()), Some("a"));
    }

    #[test]
    fn closing_forgets_the_document_and_asks_to_clear() {
        let mut docs = Documents::default();
        docs.apply(open("a"));
        let update = docs.apply(close());
        assert!(matches!(update, Some(Update::Clear(u)) if u.as_str() == URI));
        assert!(docs.get(URI).is_none());
    }

    #[test]
    fn unrelated_notifications_are_ignored() {
        assert_eq!(
            Documents::default().apply(note("$/setTrace", json!({}))),
            None
        );
    }

    #[test]
    fn malformed_parameters_are_ignored() {
        let mut docs = Documents::default();
        assert_eq!(
            docs.apply(note(DidOpenTextDocument::METHOD, json!({"nope": 1}))),
            None
        );
    }
}
