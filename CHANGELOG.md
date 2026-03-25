# Changelog

All notable changes to SOTS are documented here.
Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Versions follow [Semantic Versioning](https://semver.org/).

---

## [Unreleased]

---

## [0.2.0] — 2026-03-25

### Added
- **Server `game/` module** — full hecs ECS game state: `WorldPos`, `WorldYaw`, `PlayerTag`
  components; `Session` struct tracking entity, pending input, last-active tick; `GameState`
  driving `tick()`, `handle_client_packet()`, and `build_snapshots()`.
- **Server `network/` module** — UDP recv/send loop on `tokio::select!`; `Connect` spawns
  player entity and replies with `ConnectAck`; `Input` stores `InputFrame`; `Disconnect`
  despawns entity; `Heartbeat` refreshes timestamp. Idle sessions pruned after 10 s.
- **Server movement** — WASD `InputFrame` bits applied each tick: yaw rotation, forward/right
  vectors from `sin/cos(yaw)`, `MOVE_SPEED = 5.0 / TICK_RATE`, position clamped to ±95 units.
- **`shared/types.rs`** — `QuantizedPosition` OFFSET=1024.0 fix (negative world coords encode
  correctly to u16); `movement` constants module (`FORWARD/BACKWARD/LEFT/RIGHT` bitfield).
- **Client `input/` module** — `InputState` mapping W/↑ S/↓ A/← D/→ to movement bitfield.
- **Client `state.rs`** — `SharedState` with parking_lot `Mutex<InputSnapshot>` and
  `Mutex<GameView>`; `GameView` derives `Clone` for lock-free snapshot copy to renderer.
- **Client `network/` module** — UDP client task: binds `0.0.0.0:0`, sends `Connect`,
  waits up to 5 s for `ConnectAck`, then main loop: `tokio::select!` recv → update `GameView`;
  tick → send `InputFrame` at 64 Hz.
- **Client `renderer/` module** — full wgpu 29.x pipeline: vertex buffer (up to 8192 verts),
  uniform buffer (view-projection matrix), WGSL shader. Draws 20×20 checkerboard floor tiles
  (±100 world units, 10-unit tiles) and 2×2 player quads (cyan = local, orange = remote).
- **Client `renderer/shader.wgsl`** — WGSL vertex/fragment shader with `mat4x4<f32>` uniform.
- **Client `app.rs`** — winit 0.30.x `ApplicationHandler`: `resumed()` creates 1280×720 window
  and initialises wgpu renderer via `pollster::block_on`; `about_to_wait()` drives continuous
  rendering; `RedrawRequested` renders frame and calls `reconfigure()` when surface is lost/outdated.
- **Client `main.rs`** — spawns network thread with its own tokio runtime; winit event loop on
  main thread (required by Windows/macOS); server address from optional CLI argument.
- **`README.md`** — full run instructions: server (local + Docker), client (Linux + Windows exe),
  controls table (WASD + arrows), project layout tree, prerequisites table, roadmap.

### Changed
- `Cargo.toml` workspace deps: added `hecs`, `winit 0.30`, `wgpu 29`, `pollster 0.3`,
  `bytemuck { derive }`, updated `bincode 2` to `features = ["serde"]`, `glam` serde feature.
- `server/src/main.rs`: switched to `#[tokio::main]`, calls `network::run_server(config).await`.

### Fixed
- `QuantizedPosition` underflow for negative coordinates — added OFFSET=1024.0 to encode
  the full [-1024, 1024) world range safely into u16.
- wgpu 29.x API changes: `Instance::default()`, `request_adapter().await?` (now `Result`),
  `request_device` 1-arg form, `PipelineLayoutDescriptor::immediate_size` (replaces
  `push_constant_ranges`), `RenderPipelineDescriptor::multiview_mask` (replaces `multiview`),
  `CurrentSurfaceTexture` enum variants (replaces `Result<SurfaceTexture, SurfaceError>`),
  `RenderPassColorAttachment::depth_slice`, `RenderPassDescriptor::multiview_mask`,
  `set_bind_group(n, Some(&bg), &[])`.

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
