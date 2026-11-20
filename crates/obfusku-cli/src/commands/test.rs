//! `test`: runs a Project's test files (`ADR-030`).
//!
//! **Each test file is its own `obfusku run` process.** A test can call
//! `⌁↯`, loop forever or fill the output pipe; isolating it in a child means
//! one bad test cannot take the others down, and "passes" has one meaning:
//! the program finished without an uncaught exception.

use super::report::usage_error;
use obfusku_cli::project::{load_manifest, matching_files, require_project_entry_point};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Output, Stdio};
use std::time::{Duration, Instant};

/// A runaway test must not hang CI; a minute is generous for a program
/// that is only expected to finish.
const DEFAULT_TEST_TIMEOUT_SECS: u64 = 60;
/// Where tests live when the manifest has no `[test] include`.
const DEFAULT_TEST_PATTERN: &str = "tests/**/*.obk";

/// `test [project]`.
pub(super) fn run(args: &[String]) -> ExitCode {
    let target = args
        .get(2)
        .map_or_else(|| PathBuf::from("."), PathBuf::from);
    let root = match require_project_entry_point(&target) {
        Ok(entry) => entry.project_root,
        Err(d) => return usage_error(d),
    };
    let manifest = load_manifest(&root).ok();
    let mut patterns = manifest
        .as_ref()
        .map(|m| m.test_include.clone())
        .unwrap_or_default();
    if patterns.is_empty() {
        patterns.push(DEFAULT_TEST_PATTERN.to_string());
    }
    let timeout = Duration::from_secs(
        manifest
            .as_ref()
            .and_then(|m| m.test_timeout_secs)
            .unwrap_or(DEFAULT_TEST_TIMEOUT_SECS),
    );
    let files = matching_files(&root, &patterns);
    if files.is_empty() {
        eprintln!("error: no test files match {}", patterns.join(", "));
        return ExitCode::from(2);
    }
    let Ok(exe) = std::env::current_exe() else {
        eprintln!("error: cannot locate the obfusku executable");
        return ExitCode::from(2);
    };

    let (mut passed, mut failed) = (0usize, 0usize);
    for file in &files {
        // `/`-separated on every OS, so output is comparable across them.
        let rel = file
            .strip_prefix(&root)
            .unwrap_or(file)
            .display()
            .to_string()
            .replace('\\', "/");
        let started = Instant::now();
        let outcome = run_with_timeout(&exe, file, &root, timeout);
        let ms = started.elapsed().as_millis();
        match outcome {
            Ok(Some(o)) if o.status.success() => {
                passed += 1;
                println!("ok    {rel} ({ms} ms)");
            }
            Ok(Some(o)) => {
                failed += 1;
                println!("FAIL  {rel} ({ms} ms)");
                for line in String::from_utf8_lossy(&o.stderr).lines() {
                    println!("      {line}");
                }
            }
            Ok(None) => {
                failed += 1;
                println!("FAIL  {rel} ({ms} ms)");
                println!("      timed out after {} s", timeout.as_secs());
            }
            Err(e) => {
                failed += 1;
                println!("FAIL  {rel}: could not start: {e}");
            }
        }
    }
    println!("{passed} passed, {failed} failed");
    if failed == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

/// Runs `exe run file` in `root`; `Ok(None)` when the child outlived
/// `timeout` and was killed. Output is drained on threads so a chatty child
/// cannot block on a full pipe and then be mistaken for a hang.
fn run_with_timeout(
    exe: &Path,
    file: &Path,
    root: &Path,
    timeout: Duration,
) -> std::io::Result<Option<Output>> {
    let mut child = Command::new(exe)
        .arg("run")
        .arg(file)
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let drain = |mut r: Box<dyn Read + Send>| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = r.read_to_end(&mut buf);
            buf
        })
    };
    let out = child.stdout.take().map(|r| drain(Box::new(r)));
    let err = child.stderr.take().map(|r| drain(Box::new(r)));
    let deadline = Instant::now() + timeout;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break Some(status);
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let join = |h: Option<std::thread::JoinHandle<Vec<u8>>>| {
        h.and_then(|h| h.join().ok()).unwrap_or_default()
    };
    let (stdout, stderr) = (join(out), join(err));
    Ok(status.map(|status| Output {
        status,
        stdout,
        stderr,
    }))
}
