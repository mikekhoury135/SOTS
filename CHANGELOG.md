# Changelog

All notable changes to SOTS are documented here.
Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Versions follow [Semantic Versioning](https://semver.org/).

---

## [Unreleased]

---

## [0.1.1] — 2026-03-25

### Added
- `justfile` — task runner with recipes for build, test, lint, fmt, server (local + Docker),
  and Windows cross-compilation. Run `just` with no args to list all recipes.
- Windows cross-compilation via `cross`: `just client-windows` produces
  `target/x86_64-pc-windows-gnu/release/client.exe` from any OS using Docker.
  Debug variant (`just client-windows-debug`) retains console window for log visibility.
- `#![cfg_attr(all(target_os = "windows", not(debug_assertions)), windows_subsystem = "windows")]`
  in `client/src/main.rs` — release builds run without a console window on Windows;
  debug builds keep the console so `tracing` output is visible during development.

### Changed
- `CLAUDE.md`: updated Prerequisites section (added `just`, `cross`, `cargo-watch`),
  replaced raw cargo commands with `just` equivalents, added Windows Cross-Compilation
  section with target rationale and Phase 3 MSVC migration note.
- `CLAUDE.md`: Decisions Log updated with `cross`, GNU target, `just`, and
  `windows_subsystem` decisions.

---

## [0.1.0] — 2026-03-25

### Added
- Cargo workspace root (`Cargo.toml`) with three members: `server`, `client`, `shared`.
  All dependency versions pinned at workspace level to prevent drift.
- **`shared` crate** — pure data, no I/O, importable by both server and client:
  - `types.rs`: `PlayerState`, `InputFrame`, `QuantizedPosition` (u16 fixed-point ×32),
    `PlayerFlags` (u8 bitfield), `PlayerId`
  - `protocol.rs`: `ClientPacket`, `ServerPacket`, `PacketHeader` (seq + ACK bitfield),
    `MAX_PACKET_SIZE` (1400), `DEFAULT_PORT` (7777)
  - `transport.rs`: `Transport` trait + `TransportError` — swappable transport abstraction
  - `tick.rs`: `TickNum` (u16 wrapping with half-space comparison), `TICK_RATE` (64),
    `TICK_DURATION`
- **`server` crate** stub — tokio entry point, `ServerConfig` defaults, module stubs for
  `network/` and `game/` with Phase 1/2 intent comments.
- **`client` crate** stub — entry point with architecture comment, module stubs for
  `network/`, `renderer/`, and `input/`.
- **`docker/Dockerfile`** — multi-stage build: `rust:1.85-bookworm` builder →
  `debian:bookworm-slim` runtime. Copies only the server binary.
- **`docker/docker-compose.yml`** — `network_mode: host` (eliminates NAT), sysctl tuning
  for high-frequency UDP (`rmem_max`/`wmem_max` 128 MB, `netdev_max_backlog` 50000).
- **`CLAUDE.md`** — full project guide: finalised tech stack table, networking rationale,
  thread architecture, protocol design, git workflow, coding conventions, decisions log,
  open questions.
- **`ARCHITECTURE.md`** — crate dependency graph, server/client thread model diagrams,
  per-tick data flow, wire protocol layout, Docker layer map, phase roadmap.
- **`CHANGELOG.md`** — this file.

### Decisions recorded
- Raw UDP (tokio + socket2) chosen over QUIC (quinn): QUIC TLS overhead incompatible
  with latency goals at 64 Hz tick rate.
- `wgpu 29.x + winit 0.30.x` (pinned) chosen over Bevy: Bevy's ~3-month breaking-change
  cycle is incompatible with a stable custom game loop.
- `hecs` chosen as ECS: lightweight, no proc-macro overhead, easy to unit-test.
- Game loop runs on a dedicated synchronous thread (no tokio); spin_sleep for tick accuracy.
- Delta compression + u16 fixed-point positions adopted from day one.
- Docker `network_mode: host` for direct NIC access.
- Game port: **UDP 7777**.
- Tick rate: **64 Hz** (ceiling 128 Hz, bump after profiling).
