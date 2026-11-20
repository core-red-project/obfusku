//! `obfusku-lsp`: a Language Server Protocol server for Obfusku.
//!
//! It speaks JSON-RPC over stdio (`lsp-server`) and reports diagnostics for
//! every open document as it is edited. Analysis is the CLI's own pipeline
//! (`obfusku_cli::analyze_text`), so the editor and `obfusku check` can never
//! disagree about what is an error. There is no language logic in this crate.
//!
//! **Why full-document sync.** Obfusku files are small and the checker
//! re-reads the whole module anyway, so incremental edits would add range
//! bookkeeping for no saving.
//!
//! # Map
//!
//! | Module | Role |
//! |---|---|
//! | `server` | the message loop, batching edits and publishing |
//! | `documents` | the open documents and text-sync notifications |
//! | `analyze` | checking a buffer, isolating checker panics |
//! | `convert` | Obfusku diagnostics to LSP diagnostics, byte to UTF-16 positions |
//! | `uri` | `file://` URIs to filesystem paths |

mod analyze;
mod convert;
mod documents;
mod server;
mod uri;

use lsp_server::Connection;
use lsp_types::{
    InitializeParams, ServerCapabilities, TextDocumentSyncCapability, TextDocumentSyncKind,
};

/// The checker recurses over the syntax tree, so analysis runs on a thread
/// with the same large stack the CLI gives it.
const ANALYSIS_STACK_BYTES: usize = 256 << 20;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (connection, io_threads) = Connection::stdio();

    let capabilities = ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        ..Default::default()
    };
    let init_params = connection.initialize(serde_json::to_value(capabilities)?)?;
    let _params: InitializeParams = serde_json::from_value(init_params)?;

    // The worker takes ownership of `connection` and drops it before
    // returning: its sender must be released before `io_threads.join()`, or
    // the writer thread waits forever for a channel that never closes.
    let worker = std::thread::Builder::new()
        .stack_size(ANALYSIS_STACK_BYTES)
        .spawn(move || {
            let result = server::run(&connection).map_err(|e| e.to_string());
            drop(connection);
            result
        })?;
    worker
        .join()
        .map_err(|_| "the language server thread panicked")??;

    io_threads.join()?;
    Ok(())
}
