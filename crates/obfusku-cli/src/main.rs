//! Obfusku CLI — orchestration and presentation only, no language logic
//! of its own (§12). Pipeline orchestration lives in `src/lib.rs`
//! (`run_source`/`check_source`/`fmt_source`/`inspect_source`).
//!
//! Exit codes (not spec-mandated, this crate's own convention): `0` on
//! success, `1` when the pipeline reports diagnostics, `2` on a CLI
//! usage error (bad arguments, unreadable file).

use obfusku_cli::{InspectKind, fmt_source, render_diagnostics};
use std::io::Write;
use std::path::Path;
use std::process::ExitCode;

/// Interpreter stack, sized so `RECURSION_LIMIT` nested `eval` frames fit;
/// an unoptimized build's frames are several times larger.
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
    // A bundled program (`obfusku bundle`, `ADR-022`) owns the whole
    // command line: no subcommands, no `--`.
    if let Some(bundle) = obfusku_cli::bundle::embedded() {
        let program_args: Vec<String> = std::env::args().skip(1).collect();
        return obfusku_cli::bundle::run_embedded(&bundle, &program_args);
    }
    let mut args: Vec<String> = std::env::args().collect();
    // Everything after `--` belongs to the program being run (`ADR-022`),
    // including would-be `--help`/`--version`.
    let program_args = match args.iter().position(|a| a == "--") {
        Some(i) => {
            let rest = args.split_off(i + 1);
            args.pop();
            rest
        }
        None => Vec::new(),
    };

    // `-h`/`--help` and `-V`/`--version` short-circuit from *any*
    // position, not only immediately after the binary name — matching
    // standard CLI convention. Checked once up front before subcommand
    // dispatch.
    if args.iter().skip(1).any(|a| a == "--help" || a == "-h") {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    if args.iter().skip(1).any(|a| a == "--version" || a == "-V") {
        println!("obfusku {}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }

    let json = take_flag(&mut args, &["--json"]);
    if json && args.get(1).map(String::as_str) != Some("check") {
        eprintln!("error: '--json' is only supported by 'check'");
        return ExitCode::from(2);
    }
    let check_format = take_flag(&mut args, &["--check"]);
    let write_format = take_flag(&mut args, &["--write"]);
    if (check_format || write_format) && args.get(1).map(String::as_str) != Some("fmt") {
        eprintln!("error: '--check' and '--write' are only supported by 'fmt'");
        return ExitCode::from(2);
    }
    if check_format && write_format {
        eprintln!("error: use '--check' or '--write', not both");
        return ExitCode::from(2);
    }
    let quiet = take_flag(&mut args, &["-q", "--quiet"]);
    QUIET.store(quiet, std::sync::atomic::Ordering::Relaxed);

    match args.get(1).map(String::as_str) {
        Some("completions") => cmd_completions(&args),
        Some("version") => {
            println!("obfusku {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Some("run") => with_path_arg(&args, |p| cmd_run(p, &program_args)),
        Some("check") => with_path_arg(&args, |p| cmd_check(p, json)),
        Some("fmt") if write_format => cmd_fmt_write(&args),
        Some("fmt") => with_file_arg(&args, |source| cmd_fmt(source, check_format)),
        Some("inspect") => cmd_inspect(&args),
        Some("repl") => {
            repl();
            ExitCode::SUCCESS
        }
        Some("build") => with_path_arg(&args, cmd_build),
        Some("bundle") => cmd_bundle(&args),
        Some("fetch") => cmd_fetch(&args),
        Some("test") => cmd_test(&args),
        Some(cmd) => {
            eprintln!("unknown command: {cmd}");
            print_usage();
            ExitCode::from(2)
        }
        None => {
            print_usage();
            ExitCode::from(2)
        }
    }
}

const USAGE: &str = "usage: obfusku <command> [args]\n\
     commands:\n  \
     run <file|project> [-- args]  run a program or Project directory\n  \
     check <file|project>     type-check without executing\n  \
     test [project]           run the Project's test files (tests/**/*.obk, or [test] include)\n  \
     fmt [--check|--write] <file|project>  print the canonical source; --check exits 1 if it differs, --write rewrites the file\n  \
     inspect <file|project> --tokens|--ast|--core\n  \
     build <project>          certify a Project as a distributable Source Artifact\n  \
     bundle <file|project> [-o out]  write a standalone executable of the program\n  \
     fetch [--update] [project]  pin git dependencies in obfusku.lock and download them\n  \
     repl                     interactive read-eval-print loop\n  \
     completions <shell>      print a completion script (bash, zsh, fish, powershell)\n  \
     version                  print the CLI version\n\n\
     options:\n  \
     -h, --help               print this usage text and exit\n  \
     -V, --version            print the CLI version and exit (same as 'version')\n  \
     -q, --quiet              suppress warnings\n  \
     --json                   'check': one JSON object of diagnostics on stdout";

fn print_usage() {
    eprintln!("{USAGE}");
}

/// Reads `args[2]` — a file path or a Project directory — reports a
/// usage error if missing or unreadable, and hands the resolved entry
/// file's source text to `cmd`. `fmt`/`inspect` operate on one file's
/// own text only (no import resolution), so a directory argument only
/// needs its entry module discovered, per `project::resolve_entry_point`.
fn with_file_arg(args: &[String], cmd: impl FnOnce(&str) -> ExitCode) -> ExitCode {
    let Some(path) = args.get(2) else {
        eprintln!(
            "expected a file path, e.g. 'obfusku {} program.obk'",
            args[1]
        );
        return ExitCode::from(2);
    };
    let entry_file = match obfusku_cli::project::resolve_entry_point(Path::new(path)) {
        Ok(entry) => entry.entry_file,
        Err(d) => {
            eprintln!("{}", obfusku_cli::render_diagnostics("", &[d]));
            return ExitCode::from(2);
        }
    };
    match std::fs::read_to_string(&entry_file) {
        Ok(source) => cmd(&source),
        Err(e) => {
            eprintln!("could not read '{}': {e}", entry_file.display());
            ExitCode::from(2)
        }
    }
}

/// Reads `args[2]` as a file path and hands the `Path` itself (not just
/// its text) to `cmd` — `run`/`check` need the path to resolve
/// same-directory `ImportDeclaration`s against.
fn with_path_arg(args: &[String], cmd: impl FnOnce(&Path) -> ExitCode) -> ExitCode {
    let Some(path) = args.get(2) else {
        eprintln!(
            "expected a file path, e.g. 'obfusku {} program.obk'",
            args[1]
        );
        return ExitCode::from(2);
    };
    let path = Path::new(path);
    if let Err(e) = std::fs::metadata(path) {
        eprintln!("could not read '{}': {e}", path.display());
        return ExitCode::from(2);
    }
    cmd(path)
}

fn cmd_run(path: &Path, program_args: &[String]) -> ExitCode {
    match obfusku_cli::run_file_with_args(path, program_args) {
        Ok((value, warnings)) => {
            // Warnings never fail `run`: stderr, before the value.
            print_warnings(path, &warnings);
            println!("{}", obfusku_runtime::show(&value.0));
            ExitCode::SUCCESS
        }
        Err((map, diags)) => {
            eprintln!("{}", obfusku_cli::render_diagnostics_map(&map, &diags));
            ExitCode::from(1)
        }
    }
}

fn cmd_check(path: &Path, json: bool) -> ExitCode {
    match obfusku_cli::check_file(path) {
        Ok(warnings) => {
            if json {
                let mut map = obfusku_diagnostics::SourceMap::new();
                let entry = obfusku_cli::project::resolve_entry_point(path)
                    .map_or_else(|_| path.to_path_buf(), |e| e.entry_file);
                map.add_file(std::fs::read_to_string(entry).unwrap_or_default());
                println!(
                    "{}",
                    obfusku_cli::surface::diagnostics_json(&map, true, &warnings)
                );
                return ExitCode::SUCCESS;
            }
            print_warnings(path, &warnings);
            println!("ok");
            ExitCode::SUCCESS
        }
        Err((map, diags)) => {
            if json {
                println!(
                    "{}",
                    obfusku_cli::surface::diagnostics_json(&map, false, &diags)
                );
            } else {
                eprintln!("{}", obfusku_cli::render_diagnostics_map(&map, &diags));
            }
            ExitCode::from(1)
        }
    }
}

/// `test [project]`: runs every test file of the Project, each as its own
/// `obfusku run` so one test cannot affect another (state, `⌁↯`, output).
/// A test passes when its program finishes without an uncaught exception.
fn cmd_test(args: &[String]) -> ExitCode {
    let target = args
        .get(2)
        .map_or_else(|| Path::new(".").to_path_buf(), std::path::PathBuf::from);
    let entry = match obfusku_cli::project::require_project_entry_point(&target) {
        Ok(e) => e,
        Err(d) => {
            eprintln!("{}", obfusku_cli::render_diagnostics("", &[d]));
            return ExitCode::from(2);
        }
    };
    let root = entry.project_root;
    let manifest = obfusku_cli::project::load_manifest(&root).ok();
    let patterns = manifest
        .as_ref()
        .map(|m| m.test_include.clone())
        .unwrap_or_default();
    let timeout = std::time::Duration::from_secs(
        manifest
            .as_ref()
            .and_then(|m| m.test_timeout_secs)
            .unwrap_or(DEFAULT_TEST_TIMEOUT_SECS),
    );
    let patterns = if patterns.is_empty() {
        vec!["tests/**/*.obk".to_string()]
    } else {
        patterns
    };
    let files = obfusku_cli::project::matching_files(&root, &patterns);
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
        let rel = file
            .strip_prefix(&root)
            .unwrap_or(file)
            .display()
            .to_string()
            .replace('\\', "/");
        let started = std::time::Instant::now();
        let out = run_with_timeout(&exe, file, &root, timeout);
        let ms = started.elapsed().as_millis();
        match out {
            Ok(None) => {
                failed += 1;
                println!("FAIL  {rel} ({ms} ms)");
                println!("      timed out after {} s", timeout.as_secs());
            }
            Ok(Some(o)) if o.status.success() => {
                passed += 1;
                println!("ok    {rel} ({ms} ms)");
            }
            Ok(Some(o)) => {
                failed += 1;
                println!("FAIL  {rel} ({ms} ms)");
                let text = String::from_utf8_lossy(&o.stderr);
                for line in text.lines() {
                    println!("      {line}");
                }
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

const DEFAULT_TEST_TIMEOUT_SECS: u64 = 60;

/// `Ok(None)` when the child outlived `timeout` and was killed. Output is
/// drained on threads so a chatty child cannot block on a full pipe.
fn run_with_timeout(
    exe: &Path,
    file: &Path,
    root: &Path,
    timeout: std::time::Duration,
) -> std::io::Result<Option<std::process::Output>> {
    use std::io::Read;
    use std::process::Stdio;
    let mut child = std::process::Command::new(exe)
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
    let deadline = std::time::Instant::now() + timeout;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break Some(status);
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    };
    let join = |h: Option<std::thread::JoinHandle<Vec<u8>>>| {
        h.and_then(|h| h.join().ok()).unwrap_or_default()
    };
    let (stdout, stderr) = (join(out), join(err));
    Ok(status.map(|status| std::process::Output {
        status,
        stdout,
        stderr,
    }))
}

/// `fetch [--update] [project]`: pin and download git dependencies.
fn cmd_fetch(args: &[String]) -> ExitCode {
    let update = args.iter().skip(2).any(|a| a == "--update");
    let target = args
        .iter()
        .skip(2)
        .find(|a| !a.starts_with("--"))
        .map_or_else(|| Path::new(".").to_path_buf(), std::path::PathBuf::from);
    let root = match obfusku_cli::project::resolve_entry_point(&target) {
        Ok(entry) => entry.project_root,
        Err(d) => {
            eprintln!("{}", obfusku_cli::render_diagnostics("", &[d]));
            return ExitCode::from(2);
        }
    };
    match obfusku_cli::deps::fetch(&root, update) {
        Ok(report) => {
            for line in report {
                println!("{line}");
            }
            ExitCode::SUCCESS
        }
        Err(d) => {
            eprintln!("{}", obfusku_cli::render_diagnostics("", &[d]));
            ExitCode::from(1)
        }
    }
}

/// `completions <shell>`: a completion script on stdout.
fn cmd_completions(args: &[String]) -> ExitCode {
    let Some(shell) = args.get(2) else {
        eprintln!(
            "expected a shell: bash, zsh, fish or powershell\nusage: obfusku completions <shell>"
        );
        return ExitCode::from(2);
    };
    if let Some(script) = obfusku_cli::surface::completions(shell) {
        print!("{script}");
        ExitCode::SUCCESS
    } else {
        eprintln!("error: unsupported shell '{shell}'\nexpected bash, zsh, fish or powershell");
        ExitCode::from(2)
    }
}

/// `--quiet` silences warnings; errors and program output are unaffected.
static QUIET: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn print_warnings(path: &Path, warnings: &[obfusku_diagnostics::Diagnostic]) {
    if !warnings.is_empty() && !QUIET.load(std::sync::atomic::Ordering::Relaxed) {
        eprintln!("{}", render_warnings(path, warnings));
    }
}

/// Removes every occurrence of any of `names` from the arguments after the
/// command name and reports whether one was present.
fn take_flag(args: &mut Vec<String>, names: &[&str]) -> bool {
    let before = args.len();
    let mut i = 0;
    args.retain(|a| {
        i += 1;
        i <= 2 || !names.contains(&a.as_str())
    });
    args.len() != before
}

/// `build`: requires a real Project (`project::
/// require_project_entry_point` — checked up front so "not a Project"
/// gets its own exit code, distinct from an ordinary validation
/// failure) and certifies it as a valid, distributable Source Artifact
/// under the 1.0 model — it does not copy, package, or write anything
/// to disk. `2` — not a Project at all (no manifest); `1` — a Project,
/// but its source graph fails to resolve/parse/type-check; `0` — valid.
fn cmd_build(path: &Path) -> ExitCode {
    if let Err(d) = obfusku_cli::project::require_project_entry_point(path) {
        eprintln!("{}", obfusku_cli::render_diagnostics("", &[d]));
        return ExitCode::from(2);
    }
    match obfusku_cli::build_project(path) {
        Ok(warnings) => {
            print_warnings(path, &warnings);
            println!(
                "Project at '{}' is a valid Source Artifact.",
                path.display()
            );
            ExitCode::SUCCESS
        }
        Err((map, diags)) => {
            eprintln!("{}", obfusku_cli::render_diagnostics_map(&map, &diags));
            ExitCode::from(1)
        }
    }
}

/// `bundle <file|project> [-o out]`: type-checks the program, then writes a
/// copy of this interpreter with the program's sources embedded
/// (`bundle::write`). Default output: `<name>` in the current directory.
fn cmd_bundle(args: &[String]) -> ExitCode {
    let mut path: Option<&str> = None;
    let mut out: Option<&str> = None;
    let mut rest = args.iter().skip(2);
    while let Some(a) = rest.next() {
        match a.as_str() {
            "-o" => {
                if let Some(v) = rest.next() {
                    out = Some(v);
                } else {
                    eprintln!("'-o' expects an output path");
                    return ExitCode::from(2);
                }
            }
            other if path.is_none() => path = Some(other),
            other => {
                eprintln!("unexpected extra argument: {other}");
                return ExitCode::from(2);
            }
        }
    }
    let Some(path) = path else {
        eprintln!("expected a file or project, e.g. 'obfusku bundle program.obk -o program'");
        return ExitCode::from(2);
    };
    let path = Path::new(path);
    if let Err((map, diags)) = obfusku_cli::check_file(path) {
        eprintln!("{}", obfusku_cli::render_diagnostics_map(&map, &diags));
        return ExitCode::from(1);
    }
    let bundle = match obfusku_cli::bundle::collect(path) {
        Ok(b) => b,
        Err(d) => {
            eprintln!("{}", obfusku_cli::render_diagnostics("", &[d]));
            return ExitCode::from(2);
        }
    };
    let configured = obfusku_cli::project::load_manifest(
        &obfusku_cli::project::resolve_entry_point(path)
            .map_or_else(|_| path.to_path_buf(), |e| e.project_root),
    )
    .ok()
    .and_then(|m| m.bundle_output.or(m.name))
    .map(|n| {
        let suffix = std::env::consts::EXE_SUFFIX;
        if suffix.is_empty() || n.ends_with(suffix) {
            n
        } else {
            format!("{n}{suffix}")
        }
    });
    let out = out.or(configured.as_deref()).map_or_else(
        || {
            let stem = Path::new(&bundle.entry).file_stem().map_or_else(
                || "program".to_string(),
                |s| s.to_string_lossy().into_owned(),
            );
            std::path::PathBuf::from(format!("{stem}{}", std::env::consts::EXE_SUFFIX))
        },
        std::path::PathBuf::from,
    );
    match obfusku_cli::bundle::write(&bundle, &out) {
        Ok(()) => {
            println!("wrote {}", out.display());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("could not write '{}': {e}", out.display());
            ExitCode::from(2)
        }
    }
}

/// Renders `run`/`check`'s non-fatal warnings against `path`'s own
/// text — a fresh `SourceMap` built just for this, since a warning's
/// span always belongs to `path` itself (unreachable-arm detection
/// never looks at an imported module's own body).
fn render_warnings(path: &Path, warnings: &[obfusku_diagnostics::Diagnostic]) -> String {
    let entry_file = obfusku_cli::project::resolve_entry_point(path)
        .map_or_else(|_| path.to_path_buf(), |e| e.entry_file);
    let source = std::fs::read_to_string(&entry_file).unwrap_or_default();
    obfusku_cli::render_diagnostics(&source, warnings)
}

/// `fmt --write <file>`: rewrites the file in canonical form, only when it differs.
fn cmd_fmt_write(args: &[String]) -> ExitCode {
    let Some(path) = args.get(2) else {
        eprintln!("expected a file path, e.g. 'obfusku fmt --write program.obk'");
        return ExitCode::from(2);
    };
    let source = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("could not read '{path}': {e}");
            return ExitCode::from(2);
        }
    };
    match fmt_source(&source) {
        Ok(formatted) => {
            if formatted != source
                && let Err(e) = std::fs::write(path, formatted)
            {
                eprintln!("could not write '{path}': {e}");
                return ExitCode::from(2);
            }
            ExitCode::SUCCESS
        }
        Err(diags) => {
            eprintln!("{}", render_diagnostics(&source, &diags));
            ExitCode::from(1)
        }
    }
}

/// `fmt` prints the canonical form; with `--check` it prints nothing and
/// exits 1 when the file differs from it.
fn cmd_fmt(source: &str, check: bool) -> ExitCode {
    match fmt_source(source) {
        Ok(formatted) if check => {
            if formatted == source {
                ExitCode::SUCCESS
            } else {
                eprintln!(
                    "error: the file is not formatted; run 'obfusku fmt' to see the canonical form"
                );
                ExitCode::from(1)
            }
        }
        Ok(formatted) => {
            print!("{formatted}");
            ExitCode::SUCCESS
        }
        Err(diags) => {
            eprintln!("{}", render_diagnostics(source, &diags));
            ExitCode::from(1)
        }
    }
}

/// Accepts the mode flag (`--tokens`/`--ast`/`--core`) and the file path
/// in either order — `inspect --ast file.obk` and
/// `inspect file.obk --ast` are both valid — since there's no ambiguity
/// to resolve: exactly one of `args[2..]` starts with `--`, and whatever
/// remains is the path.
fn cmd_inspect(args: &[String]) -> ExitCode {
    let rest = &args[2.min(args.len())..];
    let mut path: Option<&str> = None;
    let mut kind: Option<InspectKind> = None;
    for a in rest {
        match a.as_str() {
            "--tokens" if kind.is_none() => kind = Some(InspectKind::Tokens),
            "--ast" if kind.is_none() => kind = Some(InspectKind::Ast),
            "--core" if kind.is_none() => kind = Some(InspectKind::Core),
            other if other.starts_with("--") => {
                eprintln!("unknown inspect flag: {other} (expected --tokens, --ast, or --core)");
                return ExitCode::from(2);
            }
            other if path.is_none() => path = Some(other),
            other => {
                eprintln!("unexpected extra argument: {other}");
                return ExitCode::from(2);
            }
        }
    }
    let Some(path) = path else {
        eprintln!("expected a file path, e.g. 'obfusku inspect program.obk --ast'");
        return ExitCode::from(2);
    };
    let kind = kind.unwrap_or(InspectKind::Core);
    let entry_file = match obfusku_cli::project::resolve_entry_point(Path::new(path)) {
        Ok(entry) => entry.entry_file,
        Err(d) => {
            eprintln!("{}", obfusku_cli::render_diagnostics("", &[d]));
            return ExitCode::from(2);
        }
    };
    let source = match std::fs::read_to_string(&entry_file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("could not read '{}': {e}", entry_file.display());
            return ExitCode::from(2);
        }
    };
    match obfusku_cli::inspect_source(&source, kind) {
        Ok(repr) => {
            println!("{repr}");
            ExitCode::SUCCESS
        }
        Err(diags) => {
            eprintln!("{}", render_diagnostics(&source, &diags));
            ExitCode::from(1)
        }
    }
}

/// Interactive loop, driven by [`obfusku_cli::ReplSession`]. Thin wrapper
/// reading lines, displaying prompts, and printing session output.
fn repl() {
    println!("obfusku repl — ':quit' or Ctrl-D to exit");
    let mut session = match obfusku_cli::ReplSession::new() {
        Ok(s) => s,
        Err(diags) => {
            // Only reachable if the bundled stdlib itself failed to
            // load — not a user-reachable condition, but still reported
            // rather than silently aborting.
            eprintln!("{}", render_diagnostics("", &diags));
            return;
        }
    };
    let stdin = std::io::stdin();
    loop {
        print!("> ");
        let _ = std::io::stdout().flush();
        let mut line = String::new();
        if stdin.read_line(&mut line).unwrap_or(0) == 0 {
            println!();
            break;
        }
        let line = line.trim_end();
        if line == ":quit" {
            break;
        }
        if line.is_empty() {
            continue;
        }
        match session.submit(line) {
            Ok((value, warnings)) => {
                if !warnings.is_empty() {
                    eprintln!("{}", session.render(&warnings));
                }
                println!("{}", obfusku_runtime::show(&value));
            }
            Err(diags) => {
                eprintln!("{}", session.render(&diags));
            }
        }
    }
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
