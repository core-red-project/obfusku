//! Throwaway directory trees for unit tests, deleted on drop.
//!
//! Tests across `project`, `natives` and `pipeline` all need real files
//! (path resolution and symlinks cannot be faked), so they share one helper
//! instead of each rolling a temp-dir counter. The root is canonicalized
//! because macOS temp paths go through a symlink and root comparisons would
//! otherwise fail for the wrong reason.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

pub(crate) struct Scratch(PathBuf);

impl Scratch {
    pub(crate) fn new() -> Self {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("obfusku-unit-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create scratch root");
        Scratch(dir.canonicalize().expect("canonicalize scratch root"))
    }

    pub(crate) fn root(&self) -> &Path {
        &self.0
    }

    /// Writes `content` at `rel` under the root, creating parent directories.
    pub(crate) fn write(&self, rel: &str, content: &str) -> PathBuf {
        let path = self.0.join(rel);
        std::fs::create_dir_all(path.parent().expect("has parent")).expect("create parents");
        std::fs::write(&path, content).expect("write file");
        path
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
