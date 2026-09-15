//! `obfusku-lsp`: a Language Server Protocol server for Obfusku, speaking
//! JSON-RPC over stdio (`lsp-server`) and linking `obfusku-syntax` and
//! `obfusku-typecheck` directly rather than shelling out to the `obfusku`
//! CLI binary. See `spec/IMPLEMENTATION_ARCHITECTURE.md` §12 for the
//! pipeline this mirrors.

mod analyze;

use lsp_server::{Connection, Message, Notification};
use lsp_types::notification::{
    DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, Notification as _,
    PublishDiagnostics,
};
use lsp_types::{
    DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    InitializeParams, PublishDiagnosticsParams, ServerCapabilities, TextDocumentSyncCapability,
    TextDocumentSyncKind, Uri,
};
use std::collections::HashMap;
use std::path::PathBuf;

/// `lsp_types::Uri` (a `fluent_uri` wrapper) has no `to_file_path` like
/// `url::Url` did in earlier `lsp-types`, so `file://` URIs are decoded
/// by hand: strip the scheme, percent-decode the path component.
fn uri_to_path(uri: &Uri) -> Option<PathBuf> {
    let rest = uri.as_str().strip_prefix("file://")?;
    let decoded = percent_encoding::percent_decode_str(rest)
        .decode_utf8()
        .ok()?;
    Some(PathBuf::from(decoded.into_owned()))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (connection, io_threads) = Connection::stdio();

    let capabilities = ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        ..Default::default()
    };
    let init_params = connection.initialize(serde_json::to_value(capabilities)?)?;
    let _params: InitializeParams = serde_json::from_value(init_params)?;

    // `connection` must be dropped (releasing its `sender`) before this
    // join, or the writer thread blocks forever waiting for the channel
    // to close — so `run` takes ownership rather than a reference.
    run(connection)?;

    io_threads.join()?;
    Ok(())
}

/// `lsp_types::Uri` has interior mutability (it wraps `fluent_uri::Uri`,
/// which caches its parsed authority behind a `Cell`), so it cannot key a
/// `HashMap` under Clippy's `mutable_key_type` lint. Documents are keyed
/// by the URI's string form instead; the `Uri` itself travels alongside
/// the text so `publish_diagnostics` can echo it back verbatim.
struct Document {
    uri: Uri,
    text: String,
}

fn run(connection: Connection) -> Result<(), Box<dyn std::error::Error>> {
    let mut documents: HashMap<String, Document> = HashMap::new();

    for msg in &connection.receiver {
        match msg {
            Message::Request(req) => {
                if connection.handle_shutdown(&req)? {
                    return Ok(());
                }
            }
            Message::Notification(not) => {
                if let Some(key) = handle_notification(&mut documents, not) {
                    publish_diagnostics(&connection, &documents, &key)?;
                }
            }
            Message::Response(_) => {}
        }
    }
    Ok(())
}

/// Applies a notification to the document store. Returns the store key
/// whose diagnostics should be republished, if any.
fn handle_notification(
    documents: &mut HashMap<String, Document>,
    not: Notification,
) -> Option<String> {
    match not.method.as_str() {
        DidOpenTextDocument::METHOD => {
            let params: DidOpenTextDocumentParams = serde_json::from_value(not.params).ok()?;
            let uri = params.text_document.uri;
            let key = uri.as_str().to_string();
            documents.insert(
                key.clone(),
                Document {
                    uri,
                    text: params.text_document.text,
                },
            );
            Some(key)
        }
        DidChangeTextDocument::METHOD => {
            let params: DidChangeTextDocumentParams = serde_json::from_value(not.params).ok()?;
            let uri = params.text_document.uri;
            let key = uri.as_str().to_string();
            // Full sync: the last change event carries the whole document.
            if let Some(change) = params.content_changes.into_iter().last() {
                documents.insert(
                    key.clone(),
                    Document {
                        uri,
                        text: change.text,
                    },
                );
            }
            Some(key)
        }
        DidCloseTextDocument::METHOD => {
            let params: DidCloseTextDocumentParams = serde_json::from_value(not.params).ok()?;
            documents.remove(params.text_document.uri.as_str());
            None
        }
        _ => None,
    }
}

fn publish_diagnostics(
    connection: &Connection,
    documents: &HashMap<String, Document>,
    key: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let Some(doc) = documents.get(key) else {
        return Ok(());
    };
    let Some(path) = uri_to_path(&doc.uri) else {
        return Ok(());
    };

    let (source_map, diagnostics) = analyze::analyze(&path, &doc.text);
    let lsp_diagnostics = analyze::to_lsp_diagnostics(&source_map, &diagnostics);

    let params = PublishDiagnosticsParams {
        uri: doc.uri.clone(),
        diagnostics: lsp_diagnostics,
        version: None,
    };
    connection
        .sender
        .send(Message::Notification(Notification::new(
            PublishDiagnostics::METHOD.to_string(),
            params,
        )))?;
    Ok(())
}
