# Justfile
# https://github.com/casey/just

[private]
default:
    @just --list

build:
    cargo build --release

wasm-build:
    @command -v trunk >/dev/null 2>&1 || cargo install trunk
    trunk build --release

wasm-dev:
    @command -v trunk >/dev/null 2>&1 || cargo install trunk
    trunk build

run *args:
    cargo run -- {{args}}

fmt:
    cargo fmt
    cargo clippy --all-targets --all-features -- -D warnings
    # @command -v shear >/dev/null 2>&1 || cargo install shear
    # cargo shear --fix

check:
    cargo fmt --check
    cargo clippy --all-targets --all-features -- -D warnings

install-hook:
    #!/usr/bin/env bash
    cat > .git/hooks/pre-commit << 'EOF'
    #!/bin/sh
    set -e
    echo "Running pre-commit quality checks..."
    just check
    EOF
    chmod +x .git/hooks/pre-commit
    echo "Pre-commit hook installation confirmed."

remove-hook:
    rm .git/hooks/pre-commit
    echo "Pre-commit hook uninstallation confirmed."

# Run unit tests
test: fmt
	cargo test
