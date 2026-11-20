//! The `obfusku` executable.
//!
//! Exit codes are this crate's own convention, not spec-mandated: `0` on
//! success, `1` when the pipeline reports diagnostics, `2` on a usage error
//! (bad arguments, unreadable file), `101` if the interpreter thread panics.
//!
//! A *bundled* program (`obfusku bundle`, `ADR-022`) is this same binary with
//! sources appended; it takes over the whole command line, so none of the
//! subcommands below are reachable from it.

mod commands;

use obfusku_cli::project::bundle;
use std::process::ExitCode;

/// Interpreter stack, sized so [`RECURSION_LIMIT`] nested `eval` frames fit;
/// an unoptimized build's frames are several times larger. Deep recursion is
/// a language feature (programs loop by recursing), so the OS-default main
/// thread stack would turn it into a segfault.
const STACK_BYTES: usize = 1 << 30;
const RECURSION_LIMIT: usize = if cfg!(debug_assertions) {
    100_000
} else {
    1_000_000
};

fn main() -> ExitCode {
    let spawned = std::thread::Builder::new()
        .stack_size(STACK_BYTES)
        .spawn(|| {
            obfusku_runtime::eval::set_recursion_limit(RECURSION_LIMIT);
            run()
        });
    match spawned.map(std::thread::JoinHandle::join) {
        Ok(Ok(code)) => code,
        _ => ExitCode::from(101),
    }
}

fn run() -> ExitCode {
    if let Some(bundle) = bundle::embedded() {
        let program_args: Vec<String> = std::env::args().skip(1).collect();
        return bundle::run_embedded(&bundle, &program_args);
    }
    commands::dispatch(std::env::args().collect())
}

#[cfg(test)]
mod tests {
    /// Smoke test: the CLI links against every implementation crate and
    /// the dependency graph compiles end to end.
    #[test]
    fn links_against_all_implementation_crates() {
        let core_module = obfusku_core::ast::Module::default();
        assert!(obfusku_typecheck::check(&core_module).is_ok());
        assert!(obfusku_runtime::evaluate(&core_module).is_ok());

        let mut source_map = obfusku_diagnostics::SourceMap::new();
        let source_id = source_map.add_file("❧");
        let tokens = obfusku_syntax::lexer::tokenize("❧", source_id)
            .expect("lexing the smallest legal program must succeed");
        let parsed = obfusku_syntax::parser::parse(&tokens, source_id)
            .expect("parsing the smallest legal program must succeed");
        let _desugared = obfusku_syntax::desugar::desugar(&parsed)
            .expect("desugaring the smallest legal program must succeed");

        let _ = obfusku_fmt::format(&parsed);
    }
}
