//! The message loop: receive, update documents, publish diagnostics.

use crate::analyze::diagnostics_for;
use crate::documents::{Documents, Update};
use crate::uri;
use lsp_server::{Connection, Message, Notification};
use lsp_types::notification::{Notification as _, PublishDiagnostics};
use lsp_types::{Diagnostic, PublishDiagnosticsParams, Uri};
use std::error::Error;

type ServerResult<T> = Result<T, Box<dyn Error>>;

/// Serves `connection` until the client asks to shut down or hangs up.
pub(crate) fn run(connection: &Connection) -> ServerResult<()> {
    let mut documents = Documents::default();

    while let Ok(first) = connection.receiver.recv() {
        // Edits that queued up while the last analysis ran collapse into one
        // analysis per document, on its newest text: typing faster than the
        // checker must not build an ever-growing backlog.
        let mut pending = Pending::default();
        let mut next = Some(first);
        while let Some(message) = next.take().or_else(|| connection.receiver.try_recv().ok()) {
            match message {
                Message::Request(request) => {
                    if connection.handle_shutdown(&request)? {
                        return Ok(());
                    }
                }
                Message::Notification(notification) => {
                    if let Some(update) = documents.apply(notification) {
                        pending.record(update);
                    }
                }
                Message::Response(_) => {}
            }
        }
        for uri in pending.cleared {
            publish(connection, uri, Vec::new())?;
        }
        for key in pending.analyze {
            publish_analysis(connection, &documents, &key)?;
        }
    }
    Ok(())
}

/// The work one batch of messages asks for.
#[derive(Debug, Default)]
struct Pending {
    analyze: Vec<String>,
    cleared: Vec<Uri>,
}

impl Pending {
    /// Merges `update` in: a document is analyzed once however often it
    /// changed, and a closed one is not analyzed at all.
    fn record(&mut self, update: Update) {
        match update {
            Update::Analyze(key) => {
                if !self.analyze.contains(&key) {
                    self.analyze.push(key);
                }
            }
            Update::Clear(uri) => {
                self.analyze.retain(|key| key != uri.as_str());
                self.cleared.push(uri);
            }
        }
    }
}

fn publish_analysis(connection: &Connection, documents: &Documents, key: &str) -> ServerResult<()> {
    let Some(document) = documents.get(key) else {
        return Ok(());
    };
    let Some(path) = uri::to_path(&document.uri) else {
        return Ok(());
    };
    publish(
        connection,
        document.uri.clone(),
        diagnostics_for(&path, &document.text),
    )
}

fn publish(connection: &Connection, uri: Uri, diagnostics: Vec<Diagnostic>) -> ServerResult<()> {
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
    use lsp_server::Request;
    use lsp_types::notification::{
        DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument,
    };
    use serde_json::json;
    use std::time::Duration;

    /// A `file://` URI of a buffer that is not on disk, inside a directory
    /// that is (the temp dir), so the Project root resolves.
    fn buffer_uri() -> String {
        let dir = std::env::temp_dir().join("obk-lsp-server-test-does-not-exist.obk");
        let path = dir.to_string_lossy().replace('\\', "/");
        format!(
            "file://{}{path}",
            if path.starts_with('/') { "" } else { "/" }
        )
    }

    /// A client talking to a server running on its own thread.
    struct Client {
        connection: Connection,
        server: Option<std::thread::JoinHandle<Result<(), String>>>,
    }

    impl Client {
        fn start() -> Self {
            let (server_side, connection) = Connection::memory();
            let server = std::thread::Builder::new()
                .stack_size(64 << 20)
                .spawn(move || run(&server_side).map_err(|e| e.to_string()))
                .unwrap();
            Client {
                connection,
                server: Some(server),
            }
        }

        fn notify(&self, method: &str, params: serde_json::Value) {
            self.connection
                .sender
                .send(Message::Notification(Notification::new(
                    method.to_string(),
                    params,
                )))
                .unwrap();
        }

        fn open(&self, text: &str) {
            self.notify(
                DidOpenTextDocument::METHOD,
                json!({"textDocument": {"uri": buffer_uri(), "languageId": "obfusku", "version": 1, "text": text}}),
            );
        }

        fn next_diagnostics(&self) -> PublishDiagnosticsParams {
            let message = self
                .connection
                .receiver
                .recv_timeout(Duration::from_secs(20))
                .expect("the server should have published");
            let Message::Notification(n) = message else {
                panic!("expected a notification, got {message:?}");
            };
            assert_eq!(n.method, PublishDiagnostics::METHOD);
            serde_json::from_value(n.params).unwrap()
        }

        fn shutdown(mut self) {
            self.connection
                .sender
                .send(Message::Request(Request::new(
                    1.into(),
                    "shutdown".into(),
                    json!(null),
                )))
                .unwrap();
            self.notify("exit", json!(null));
            self.server.take().unwrap().join().unwrap().unwrap();
        }
    }

    #[test]
    fn opening_a_clean_document_publishes_no_diagnostics() {
        let client = Client::start();
        client.open("x ≔ 1\n❧\n");
        let published = client.next_diagnostics();
        assert_eq!(published.uri.as_str(), buffer_uri());
        assert!(published.diagnostics.is_empty());
        client.shutdown();
    }

    #[test]
    fn a_broken_edit_publishes_an_error_and_a_fix_clears_it() {
        let client = Client::start();
        client.open("x ≔ 1\n❧\n");
        client.next_diagnostics();

        client.notify(
            DidChangeTextDocument::METHOD,
            json!({"textDocument": {"uri": buffer_uri(), "version": 2},
                   "contentChanges": [{"text": "x ≔ 1 ✚ \"a\"\n❧\n"}]}),
        );
        assert_eq!(client.next_diagnostics().diagnostics.len(), 1);

        client.notify(
            DidChangeTextDocument::METHOD,
            json!({"textDocument": {"uri": buffer_uri(), "version": 3},
                   "contentChanges": [{"text": "x ≔ 1\n❧\n"}]}),
        );
        assert!(client.next_diagnostics().diagnostics.is_empty());
        client.shutdown();
    }

    #[test]
    fn closing_a_document_clears_its_diagnostics() {
        let client = Client::start();
        client.open("x ≔ 1 ✚ \"a\"\n❧\n");
        assert_eq!(client.next_diagnostics().diagnostics.len(), 1);
        client.notify(
            DidCloseTextDocument::METHOD,
            json!({"textDocument": {"uri": buffer_uri()}}),
        );
        let cleared = client.next_diagnostics();
        assert_eq!(cleared.uri.as_str(), buffer_uri());
        assert!(cleared.diagnostics.is_empty());
        client.shutdown();
    }

    #[test]
    fn a_document_with_no_file_path_is_not_analyzed() {
        let client = Client::start();
        client.notify(
            DidOpenTextDocument::METHOD,
            json!({"textDocument": {"uri": "untitled:Untitled-1", "languageId": "obfusku",
                                    "version": 1, "text": "x ≔ 1 ✚ \"a\""}}),
        );
        client.open("x ≔ 1\n❧\n");
        // The untitled buffer publishes nothing, so the first message is ours.
        assert_eq!(client.next_diagnostics().uri.as_str(), buffer_uri());
        client.shutdown();
    }

    #[test]
    fn pending_analyzes_a_document_once_and_drops_closed_ones() {
        let uri: Uri = buffer_uri().parse().unwrap();
        let mut pending = Pending::default();
        pending.record(Update::Analyze(buffer_uri()));
        pending.record(Update::Analyze(buffer_uri()));
        assert_eq!(pending.analyze, [buffer_uri()]);
        pending.record(Update::Clear(uri));
        assert!(pending.analyze.is_empty());
        assert_eq!(pending.cleared.len(), 1);
    }
}
