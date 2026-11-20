//! Helpers shared by every integration-test crate.
//!
//! Each `tests/<area>/` folder is its own test binary, and they all drive
//! the same compiled `obfusku` executable against throwaway files. The
//! helpers live here once instead of being copied into each. Cargo only
//! treats `tests/<dir>/main.rs` as a test target, so this directory is
//! compiled solely through the `#[path]` module in each crate's `main.rs`.
//!
//! A given crate uses only some of them, hence the `dead_code` allowance.

#![allow(dead_code)]

use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

pub fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_obfusku"))
}

/// A `.obk` file under the system temp directory, deleted when dropped.
/// No external crate needed for something this small.
pub struct TempObkFile(pub std::path::PathBuf);

impl TempObkFile {
    pub fn new(source: &str) -> Self {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("obfusku-cli-test-{}-{n}.obk", std::process::id()));
        std::fs::write(&path, source).expect("write temp .obk file");
        TempObkFile(path)
    }

    pub fn path(&self) -> &std::path::Path {
        &self.0
    }
}

impl Drop for TempObkFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

pub fn write_temp(source: &str) -> TempObkFile {
    TempObkFile::new(source)
}

/// A directory of named `.obk` files, for import-resolution tests —
/// `⟲ mathutil` must resolve to a real `mathutil.obk` sitting next to
/// the importing file, not an arbitrary temp filename.
pub struct TempObkDir(pub std::path::PathBuf);

impl TempObkDir {
    pub fn new() -> Self {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("obfusku-cli-test-dir-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        TempObkDir(dir)
    }

    pub fn write(&self, name: &str, source: &str) -> std::path::PathBuf {
        let path = self.0.join(format!("{name}.obk"));
        std::fs::write(&path, source).expect("write module file");
        path
    }
}

impl Drop for TempObkDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A scratch directory tree, deleted on drop. Every helper takes a
/// path relative to the tree's own root, creating parent directories
/// as needed — used to build both flat and nested Project layouts.
pub struct TempTree(pub std::path::PathBuf);

impl TempTree {
    pub fn new() -> Self {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("obfusku-project-test-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create temp tree root");
        TempTree(dir)
    }

    pub fn root(&self) -> &std::path::Path {
        &self.0
    }

    pub fn write(&self, rel_path: &str, content: &str) -> std::path::PathBuf {
        let path = self.0.join(rel_path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create parent dirs");
        }
        std::fs::write(&path, content).expect("write file");
        path
    }

    pub fn manifest(&self, rel_dir: &str, entry: &str) -> std::path::PathBuf {
        self.write(
            &format!("{rel_dir}/obfusku.toml"),
            &format!("format = 1\nentry = \"{entry}\"\n"),
        )
    }
}

impl Drop for TempTree {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub const ARGS_PROGRAM: &str = "\u{3BB}first(xs: List \u{25B7} \u{2318}): \u{2318} \u{2192} \
    \u{27E1} xs {\n  \u{2298} \u{2192} \u{22}none\u{22}\n  \u{27E2} \u{205D}(h, tl) \u{2192} h\n}\n\
    shown \u{2254} \u{2301}\u{2191}(first(\u{2301}\u{2193}\u{2325}(\u{2205})))\n\
    result \u{2254} 1\n\u{2767}\n";

pub fn run_in(dir: &std::path::Path, src: &str) -> std::process::Output {
    let file = dir.join("prog.obk");
    std::fs::write(&file, src).unwrap();
    bin().arg("run").arg(&file).output().unwrap()
}
