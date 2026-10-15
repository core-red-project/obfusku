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
    path_from_file_uri(uri.as_str())
}

fn path_from_file_uri(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    let decoded = percent_encoding::percent_decode_str(rest)
        .decode_utf8()
        .ok()?;
    // `file:///C:/x` carries a leading slash before the drive letter,
    // which is not part of a Windows path.
    let bytes = decoded.as_bytes();
    let windows_drive =
        bytes.len() >= 3 && bytes[0] == b'/' && bytes[1].is_ascii_alphabetic() && bytes[2] == b':';
    let path = if windows_drive {
        &decoded[1..]
    } else {
        &decoded[..]
    };
    Some(PathBuf::from(path))
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
    // The checker recurses over the syntax tree; the CLI's big stack applies here too.
    let worker = std::thread::Builder::new()
        .stack_size(ANALYSIS_STACK_BYTES)
        .spawn(move || {
            let out = run(&connection).map_err(|e| e.to_string());
            drop(connection);
            out
        })?;
    worker
        .join()
        .map_err(|_| "the language server thread panicked")??;

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

const ANALYSIS_STACK_BYTES: usize = 256 << 20;

fn run(connection: &Connection) -> Result<(), Box<dyn std::error::Error>> {
    let mut documents: HashMap<String, Document> = HashMap::new();

    while let Ok(first) = connection.receiver.recv() {
        // Edits that queued up while the last analysis ran collapse to one
        // analysis per document, on its newest text.
        let mut to_analyze: Vec<String> = Vec::new();
        let mut cleared: Vec<Uri> = Vec::new();
        let mut next = Some(first);
        while let Some(msg) = next.take().or_else(|| connection.receiver.try_recv().ok()) {
            match msg {
                Message::Request(req) => {
                    if connection.handle_shutdown(&req)? {
                        return Ok(());
                    }
                }
                Message::Notification(not) => match handle_notification(&mut documents, not) {
                    Some(Update::Analyze(key)) => {
                        if !to_analyze.contains(&key) {
                            to_analyze.push(key);
                        }
                    }
                    Some(Update::Clear(uri)) => {
                        to_analyze.retain(|k| k != uri.as_str());
                        cleared.push(uri);
                    }
                    None => {}
                },
                Message::Response(_) => {}
            }
        }
        for uri in cleared {
            publish(connection, uri, Vec::new())?;
        }
        for key in to_analyze {
            publish_diagnostics(connection, &documents, &key)?;
        }
    }
    Ok(())
}

/// What a notification asks the server to publish.
enum Update {
    /// Re-analyze the document with this store key.
    Analyze(String),
    /// The document closed: remove its diagnostics from the client.
    Clear(Uri),
}

/// Applies a notification to the document store.
fn handle_notification(
    documents: &mut HashMap<String, Document>,
    not: Notification,
) -> Option<Update> {
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
            Some(Update::Analyze(key))
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
            Some(Update::Analyze(key))
        }
        DidCloseTextDocument::METHOD => {
            let params: DidCloseTextDocumentParams = serde_json::from_value(not.params).ok()?;
            let uri = params.text_document.uri;
            documents.remove(uri.as_str());
            Some(Update::Clear(uri))
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

    // A bug in one analysis must not take the server (and every open file) down.
    let text = doc.text.as_str();
    let analysis = std::panic::catch_unwind(|| {
        let (source_map, diagnostics) = analyze::analyze(&path, text);
        analyze::to_lsp_diagnostics(&source_map, &diagnostics)
    });
    let lsp_diagnostics = analysis.unwrap_or_else(|_| {
        vec![lsp_types::Diagnostic {
            message: "internal error: the checker panicked on this file; please report it"
                .to_string(),
            severity: Some(lsp_types::DiagnosticSeverity::ERROR),
            source: Some("obfusku".to_string()),
            ..Default::default()
        }]
    });

    publish(connection, doc.uri.clone(), lsp_diagnostics)
}

fn publish(
    connection: &Connection,
    uri: Uri,
    diagnostics: Vec<lsp_types::Diagnostic>,
) -> Result<(), Box<dyn std::error::Error>> {
    let params = PublishDiagnosticsParams {
        uri,
        diagnostics,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unix_and_windows_file_uris_become_native_paths() {
        assert_eq!(
            path_from_file_uri("file:///home/u/my%20proj/main.obk"),
            Some(PathBuf::from("/home/u/my proj/main.obk"))
        );
        assert_eq!(
            path_from_file_uri("file:///C:/Users/u/main.obk"),
            Some(PathBuf::from("C:/Users/u/main.obk"))
        );
        assert_eq!(
            path_from_file_uri("file:///c%3A/Users/u/main.obk"),
            Some(PathBuf::from("c:/Users/u/main.obk"))
        );
        assert_eq!(path_from_file_uri("untitled:Untitled-1"), None);
    }
}
