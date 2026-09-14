# Task runner configuration

default:
    @just --list

# Bootstrap toolchain components
install:
    rustup component add rustfmt clippy

# Start interactive REPL for local development
dev:
    cargo run -p obfusku-cli -- repl

# Produce build artifacts for all workspace crates
build:
    cargo build --workspace

# Run complete test suite across all workspace crates
test:
    cargo test --workspace

# Run correctness and type checking
typecheck:
    cargo check --workspace --all-targets

# Run static analysis and linting
lint:
    cargo clippy --workspace --all-targets -- -D warnings

# Apply automatic formatting
format:
    cargo fmt --all

# Run the complete quality gate
check: format lint typecheck test

# Remove build artifacts and caches
clean:
    cargo clean
