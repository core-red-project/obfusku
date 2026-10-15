# Task runner configuration

set windows-shell := ["bash", "-cu"]

default:
    @just --list

# Bootstrap toolchain components and install the git hooks
install:
    rustup component add rustfmt clippy
    git config core.hooksPath .githooks

# Start interactive REPL for local development
dev:
    cargo run -p obfusku-cli -- repl

# Produce build artifacts for all workspace crates
build:
    cargo build --workspace --locked

# Run complete test suite across all workspace crates
test:
    cargo test --workspace --locked

# Run correctness and type checking
typecheck:
    cargo check --workspace --all-targets --locked

# Run static analysis and linting
lint:
    cargo clippy --workspace --all-targets --locked -- -D warnings

# Audit dependencies: advisories, licenses, sources.
# DENY_OFFLINE=1 reuses the cached advisory database instead of fetching it.
deny:
    @command -v cargo-deny >/dev/null || { echo "cargo-deny is missing: cargo install cargo-deny --locked" >&2; exit 1; }
    cargo deny check {{ if env("DENY_OFFLINE", "") != "" { "--disable-fetch" } else { "" } }}

# Apply automatic formatting
format:
    cargo fmt --all

# Verify formatting without modifying files
format-check:
    cargo fmt --all --check

# Run the complete quality gate (non-mutating)
check: format-check lint typecheck test deny

# Remove build artifacts and caches
clean:
    cargo clean

# Startup and core-operation timings; not part of `check` (timing is noisy)
bench:
    @command -v hyperfine >/dev/null || { echo "hyperfine is missing: cargo install hyperfine --locked" >&2; exit 1; }
    cargo build --release --locked -p obfusku-cli
    hyperfine --warmup 3 'target/release/obfusku --version' 'target/release/obfusku run benches/fixture.obk'

# Archive release binaries for TARGET into dist/ (asset names: install.sh, install.ps1)
package:
    bash scripts/package.sh

# Type-check everything for Windows from any host (compiles; does not run)
check-windows:
    rustup target add x86_64-pc-windows-msvc
    cargo check --target x86_64-pc-windows-msvc --workspace --all-targets

# Publish the crates in dependency order (re-run after a rate limit)
publish *ARGS:
    bash scripts/publish-crates.sh {{ARGS}}

# Write RELEASE_NOTES.md for VERSION from Conventional Commits
changelog:
    #!/usr/bin/env bash
    set -euo pipefail
    : "${VERSION:?set VERSION, e.g. 1.2.3}"
    git cliff --latest --strip all -o RELEASE_NOTES.md
