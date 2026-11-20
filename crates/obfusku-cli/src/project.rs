//! Everything about an Obfusku *Project*: what it is, where it is, what it
//! declares and what ships with it (`ADR-017`).
//!
//! | Module | Concern |
//! |---|---|
//! | `entry` | which file to run, and from which root (`ADR-017`) |
//! | `search` | finding `.obk` files and test files under a root (`ADR-018`) |
//! | `manifest` | the validated `obfusku.toml` (`ADR-020`, `ADR-029`) |
//! | `deps` | `[dependencies]`, `obfusku.lock` and `obfusku fetch` (`ADR-029`) |
//! | `modules` | resolving `⟲` imports against the Project and its dependencies |
//! | `exports` | what a checked module offers its importers (shared with stdlib and REPL) |
//! | `bundle` | packaging a Project into one executable (`ADR-022`) |
//!
//! The Project root is also the boundary for imports (`ADR-018`) and for
//! filesystem natives (`ADR-019`), so the same root answers all three
//! questions "what may this import", "what may this read" and "what does
//! this declare".

pub mod bundle;
pub mod deps;
pub mod entry;
pub(crate) mod exports;
pub mod manifest;
pub mod modules;
#[cfg(test)]
pub(crate) mod scratch;
pub mod search;

pub use entry::{
    EntryPoint, discover_project_root, require_project_entry_point, resolve_entry_point,
};
pub use manifest::{MANIFEST_FILE_NAME, Manifest, load_manifest};
pub use search::{ModuleSearch, find_module, matching_files};
