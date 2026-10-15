# ADR-030 — `obfusku test` runs a Project's test files as ordinary programs

## Status
Accepted

## Context
`ADR-017` and `LANGUAGE_SPEC.md` §5 left `test` unimplemented because it seemed to need a language-level testing construct. A real program (Ouroboros) needs tests now, and everything a test needs already exists: modules to import the code under test, exceptions to signal failure, and a process to run in.

## Decision
`obfusku test [project]` requires a Project (`ADR-017`). It selects the `.obk` files matching `[test] include` of the manifest (`ADR-029`; default `tests/**/*.obk`; `*` within a path segment, `**` across segments; hidden directories and `target` never searched), in path order, and runs each as its own `obfusku run <file>` with the Project root as working directory and no standard input. A file that runs longer than `[test] timeout` seconds (default 60) is killed and counted as failed.

A file passes when its program finishes with exit status 0; any uncaught exception, type error or non-zero exit fails it, and its standard error is shown indented under `FAIL <path>`. The command prints `<n> passed, <m> failed` and exits `0` when none failed, `1` otherwise, and `2` when no file matches or the target is not a Project.

A test file is an ordinary module: it imports the code under test by name (`ADR-018`) and fails by raising. Helpers live in a module that the pattern does not select (for example `tests/support.obk` next to `*_test.obk` files). Test modules share the Project's module namespace, so their names must not collide with `src` modules.

Tests run with the Project's capabilities (`ADR-029`) and, being separate processes, cannot affect one another through state, `⌁↯` or output.

## Alternatives Considered
A dedicated assertion form (message plus source location) is the better end state and stays on the roadmap; making it a prerequisite blocked a useful command for a convention that costs nothing. Running tests in-process was rejected: the runtime keeps per-thread tables and `⌁↯` terminates the process. A per-test-function model (collect `test…` bindings) needs reflection over bindings that the language does not expose.

## Consequences
`obfusku test` replaces the "deferred" stub and its two tests. A project must keep test file names distinct from module names. No timing limit is imposed on a test.

## Related Specification
`ADR-017`, `ADR-018`, `ADR-029`.
