# ADR-029 — Manifest format 2: project identity, explicit entry, bundle output, capabilities and git dependencies

## Status
Accepted

## Context
`ADR-020`'s manifest has three fields and only `entry` does anything: `name` is informational, unknown keys are ignored (so a misspelt key is silently dropped), the entry is a bare module name that is ambiguous as soon as two folders hold a `main.obk`, and a program cannot say what it needs from the host. The host natives (`ADR-022` to `ADR-027`) already give a program arguments, environment, clocks, randomness, streams and a filesystem; a language with that surface needs a place to declare and bound it. Programs also had no way to share a module across projects.

## Decision
`obfusku.toml` accepts `format = 2` next to `format = 1`. Format 1 is unchanged, including tolerance of unknown keys. Format 2 **rejects every key it does not define**, with a "did you mean" suggestion.

```toml
format = 2

[project]                 # all optional
name = "ouroboros"        # default name of a bundled executable
version = "0.1.0"
description = "…"
license = "MIT"
authors = ["…"]
obfusku = ">=1.1"         # minimum interpreter; a newer requirement refuses to run

[entry]                   # or: entry = "main"
module = "main"           # bare name, searched in the tree (ADR-018)
# path = "src/main.obk"   # or an explicit file inside the project

[bundle]
output = "ouroboros"      # default: project.name, then the entry's stem

[test]
include = ["tests/**/*.obk"]   # files `obfusku test` runs
timeout = 60                   # seconds per file (ADR-030)

[capabilities]            # absent: everything allowed, as before
console = true            # ⌁↑ ⌁↓ ⌁↑⍉ ⌁↑⎙ ⌁↓⎙ ⌁⍰⎙ ⌁↑⎚ ⌁↓⎚ ⌁⍰⌖
fs = "project"            # "none" | "project" | "cwd": the ⌬ ⌂ ⌭ file natives and their root
env = false               # ⌁↓⎈
args = true               # ⌁↓⌥
clock = false             # ⌁↓⧗ ⌁↓⧖ ⌁↑⧗
random = false            # ⌁↓⚀ ⌁↓⚁
exit = false              # ⌁↯

[dependencies]
utils  = { path = "../utils" }
parser = { github = "owner/repo", tag = "v1.2.0" }
legacy = { gitlab = "group/repo", branch = "main" }
other  = { git = "https://host/x.git", rev = "<40-hex commit>" }
```

### Capabilities
When `[capabilities]` is present, every key not set is `false` (`fs`: `"none"`). A denied native keeps its type, so the program still type-checks. Calling one from the **entry module** is a static error at the use site (`'⌁↓⎈' needs the 'env' capability`), so `check` and the language server report it; calling one from an imported module raises `Failure("PermissionDenied", …)` at run time, the same failure `ADR-019` uses for paths outside the root. A module that declares a top-level name equal to a native (`ADR-028`) is not the native. `fs = "project"` is the project root (`ADR-019`), `"cwd"` the working directory; a bundled program's `"project"` is its working directory (`ADR-022`). A project with no manifest, or a format 1 manifest, has every capability.

### Dependencies
Each entry has exactly one location (`path`, `git`, `github = "owner/repo"`, `gitlab = "group/repo"`) and, for git, exactly one of `tag`, `rev` (a full commit hash) or `branch`.

- `obfusku fetch [--update] [project]` resolves each git dependency (transitively) to a commit, downloads it with `git` into `$OBFUSKU_HOME` (default `~/.obfusku`), strips `.git`, and writes `obfusku.lock` (`source`, `ref`, `rev` per package). The lock is reused unless `--update`.
- `run`, `check`, `build` and the language server never use the network: a git dependency that is not locked or not cached fails with a pointer to `obfusku fetch`.
- An import `⟲ name` is searched in the importing package's own tree first and only then in its **direct** dependencies; two dependencies offering the same name are ambiguous. Inside a dependency, imports resolve against that dependency and its own dependencies. `tests/`, `examples/` and `benches/` of a dependency are not importable.
- `bundle` embeds dependency sources and the package graph, so the executable needs neither the cache nor the network.
- A dependency runs with the **root** project's capabilities; there is no per-dependency sandbox.

## Alternatives Considered
Keeping capabilities informational only was rejected: a declaration that nothing enforces is the situation `name` is already in. Per-dependency capabilities were left out: they need a capability-passing model the language does not have. Resolving dependencies by hash of contents instead of commit was not needed: a git commit already names its tree. A hosted registry was rejected for now; git URLs reach GitHub, GitLab and self-hosted servers with no service to run.

## Consequences
`project.rs` delegates to `manifest.rs`, `deps.rs` and `capability.rs`. The bundle payload gains a trailing package graph (`ADR-022`'s payload without it still decodes). `obfusku fetch` shells out to `git`, which must be installed only for that command. `examples/project` moves to format 2.

## Related Specification
`spec/adr/ADR-017-obfusku-project-model.md`; `ADR-018`; `ADR-019`; `ADR-020`; `ADR-022`; `ADR-028`.
