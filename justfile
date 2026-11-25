# TotalFloat developer commands. See AGENTS.md "Commands".

# List available recipes
default:
    @just --list

# fmt check, clippy (-D warnings), tests. Must pass before every PR.
check:
    cargo fmt --all -- --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test --workspace
    cargo xtask rules
    @just ui-check

# The web app's type check, lint and tests, when pnpm is installed
ui-check:
    @if command -v pnpm >/dev/null 2>&1; then cargo xtask ui --check; else echo "skip: pnpm isn't on the PATH, so the ui/ checks didn't run (CI runs them)"; fi

# Tests only
test:
    cargo test --workspace

# Format
fmt:
    cargo fmt --all

