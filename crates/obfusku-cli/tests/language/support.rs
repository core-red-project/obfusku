//! Shared harness: run a program through the library and unwrap the outcome.

use obfusku_cli::run_source;
use obfusku_runtime::Value;

pub fn run_ok(source: &str) -> Value {
    // `.0.0` is deliberate, not an oversight: see `RunResult`'s
    // doc comment for why this crate refuses to let that convention hide
    // behind a bare `Value` at this call site. The second `.0` discards
    // `run_source`'s non-fatal warnings — this helper only cares about
    // the executed value; see `check_source_...warnings` tests for
    // warning-specific coverage.
    run_source(source)
        .unwrap_or_else(|diags| {
            panic!("expected {source:?} to run successfully, got diagnostics: {diags:?}")
        })
        .0
        .0
}

pub fn run_err(source: &str) -> Vec<obfusku_diagnostics::Diagnostic> {
    run_source(source).expect_err(&format!("expected {source:?} to fail"))
}

pub fn as_int(v: Value) -> i64 {
    match v {
        Value::Int(i) => i,
        other => panic!("expected Int, got {other:?}"),
    }
}

/// The full pipeline (`run_file`): stdlib and natives in scope, unlike
/// `run_source`.
pub fn run_full(source: &str) -> Result<Value, Vec<obfusku_diagnostics::Diagnostic>> {
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("obfusku-e2e-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("prog.obk");
    std::fs::write(&file, source).unwrap();
    let result = obfusku_cli::run_file(&file)
        .map(|(v, _)| v.0)
        .map_err(|(_, ds)| ds);
    let _ = std::fs::remove_dir_all(&dir);
    result
}

pub fn run_full_ok(source: &str) -> Value {
    run_full(source).unwrap_or_else(|ds| panic!("expected {source:?} to run, got {ds:?}"))
}

pub fn run_full_err(source: &str) -> Vec<obfusku_diagnostics::Diagnostic> {
    run_full(source).expect_err(&format!("expected {source:?} to fail"))
}

pub fn run_str(source: &str) -> String {
    match &run_full_ok(source) {
        Value::Str(s) => s.to_string(),
        other => panic!("expected Str, got {other:?}"),
    }
}
