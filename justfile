# SOTS — justfile
# Install: cargo install just
# Usage:   just <recipe>   (run `just` with no args to list all recipes)

# Shell configuration — use cmd.exe on Windows, sh everywhere else.
set windows-shell := ["cmd.exe", "/c"]

# Windows cross-compilation target.
# GNU toolchain works for all phases up to and including Phase 2.
# Re-evaluate for x86_64-pc-windows-msvc when wgpu/winit land in Phase 3
# (use `cargo install cargo-xwin` if MSVC is needed from Linux).
windows_target := "x86_64-pc-windows-gnu"

# ── Default: list all recipes ─────────────────────────────────────────────────

[private]
default:
    @just --list

# ── Build ─────────────────────────────────────────────────────────────────────

# Build the entire workspace (debug)
build:
    cargo build --workspace

# Build the entire workspace (release)
build-release:
    cargo build --workspace --release

# Run the client locally (any platform)
client-run:
    cargo run -p client

# Cross-compile the client to a Windows .exe (Linux/macOS only — requires: cargo install cross)
# Output: target/x86_64-pc-windows-gnu/release/client.exe
[unix]
client-windows:
    cross build --target {{windows_target}} --release -p client
    @echo
    @echo "Windows executable: target/{{windows_target}}/release/client.exe"

# Cross-compile the client to a Windows .exe (debug, with console logging)
[unix]
client-windows-debug:
    cross build --target {{windows_target}} -p client
    @echo
    @echo "Windows executable (debug): target/{{windows_target}}/debug/client.exe"

# ── Quality gates ─────────────────────────────────────────────────────────────

# Run all workspace tests
test:
    cargo test --workspace

# Run Clippy (warnings = errors, matches CI)
lint:
    cargo clippy --workspace -- -D warnings

# Check formatting without modifying files
fmt-check:
    cargo fmt --all -- --check

# Apply formatting
fmt:
    cargo fmt --all

# Full pre-PR check: format + clippy + tests
check: fmt-check lint test

# ── Server ────────────────────────────────────────────────────────────────────

# Run the server locally (no Docker, debug build)
server-run:
    cargo run -p server

# Run the server locally (release build)
server-run-release:
    cargo run -p server --release

# Build and run the server in Docker
server-docker:
    docker compose -f docker/docker-compose.yml up --build

# Build the Docker image without running
server-docker-build:
    docker compose -f docker/docker-compose.yml build

# Stop and remove the Docker container
server-docker-down:
    docker compose -f docker/docker-compose.yml down

# ── Convenience ───────────────────────────────────────────────────────────────

# Watch server for file changes and rebuild (requires: cargo install cargo-watch)
watch:
    cargo watch -x "run -p server"

# Clean all build artefacts
clean:
    cargo clean
