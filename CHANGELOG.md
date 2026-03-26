# Changelog

All notable changes to SOTS are documented here.
Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Versions follow [Semantic Versioning](https://semver.org/).

---

## [Unreleased]

---

## [0.5.0] — 2026-03-26

### Added
- **First-person 3D camera** — perspective projection (90° FOV) from eye height (1.5 units).
  Camera follows the player's predicted position and yaw, providing a true FPS view.
- **Mouse look** — cursor is captured on startup (`CursorGrabMode::Locked` with `Confined`
  fallback). Raw `DeviceEvent::MouseMotion` delta drives yaw rotation via `yaw_delta` in
  `InputFrame`. Accumulated between ticks and drained per network tick for smooth turning.
- **Depth buffer** — `Depth32Float` render attachment enables correct 3D occlusion. Depth
  texture is recreated on window resize.
- **3D wall geometry** — walls rendered as shaded boxes (floor to 3.0 units high) with darker
  side faces for visual depth. Uses back-face culling (`wgpu::Face::Back`).
- **3D player models** — remote players rendered as orange boxes (1.0 × 2.0 units). Local
  player hidden in first person (no self-rendering).
- **Crosshair** — white cross overlay rendered in world space at a fixed distance in front
  of the camera.
- **Sky color** — clear color changed from near-black to light blue (0.4, 0.6, 0.9) for an
  outdoor sky feel.
- **Escape to quit** — `Escape` key exits the game cleanly.
- **Focus re-grab** — cursor is re-captured when the window regains focus.
- `shared::physics` constants: `WALL_HEIGHT` (3.0), `EYE_HEIGHT` (1.5), `PLAYER_HEIGHT` (2.0).
- `GameView::predicted_yaw` — renderer reads camera direction from prediction state.
- `InputSnapshot::accumulate_yaw()` / `take_yaw_delta()` — thread-safe mouse delta
  accumulation between the winit event loop and the network tick.

### Changed
- **Controls** — shooting moved from `Space` to left mouse click for better FPS feel.
  W/S movement direction corrected (W = screen-up = −Z, S = screen-down = +Z).
- **Renderer** — complete rewrite from top-down orthographic to first-person perspective.
  Floor tiles, wall boxes, player boxes, crosshair, and debug overlay all rendered in 3D.
  Pipeline now includes depth-stencil state and back-face culling.
- **Shoot direction** — server hitscan direction updated to match physics forward convention
  (`sin_y, 0, -cos_y`).
- **Wall layout** — north barrier and far walls repositioned to align with corrected forward
  direction. East pillar unchanged.
- **Debug overlay** — server ghost rendered as a red 3D box at server-confirmed position.
  HUD bars (RTT, pending inputs, simulated latency) rendered as small world-space quads
  anchored to the camera view.

---

## [0.4.0] — 2026-03-25

### Added
- **128 Hz tick rate** — doubled from 64 Hz. `TICK_RATE` constant in `shared/tick.rs` drives
  both server and client; `TICK_DURATION`, `MOVE_SPEED` auto-derive from it.
- **Dedicated game loop thread** — server game loop now runs on a dedicated OS thread
  (`std::thread`) isolated from tokio's async scheduler. Uses `spin_sleep` for
  sub-millisecond tick accuracy (<100µs jitter vs ~1ms from tokio::time::interval).
  IO (recv/send) stays on tokio; communication via bounded crossbeam channels.
- **`shared/combat.rs`** — pure hitscan raycast: `ray_vs_aabb` for wall occlusion,
  `ray_vs_circle` for player hit detection. Constants: `HITSCAN_RANGE` (100 units),
  `HITSCAN_DAMAGE` (25 per hit), `MAX_HEALTH` (100), `RESPAWN_TICKS` (384 = 3s at 128Hz).
- **Health system** — `Health(u8)` ECS component. Hitscan shots subtract 25 HP; at 0 HP
  the player dies (ALIVE flag cleared, movement disabled).
- **Respawn system** — `RespawnTimer(u16)` component counts down from `RESPAWN_TICKS`.
  On reaching 0, player respawns at a cycled spawn point with full health.
- **Shoot action** — `Space` key sets `movement::SHOOT` bit in `InputFrame`. Server
  processes hitscan on the tick the bit is set. Rays are blocked by walls.
- **Spawn points** — 4 spawn positions cycling for new connections and respawns.
- 3 new unit tests in `shared::combat`: hitscan hit, wall-blocked, miss.

### Changed
- **Server architecture** — `server/network/mod.rs` rewritten: IO loop (tokio) sends/receives
  via crossbeam channels to/from the game loop thread. Bounded channels (4096 cap) provide
  backpressure and overload shedding.
- **`server/Cargo.toml`** — added `spin_sleep`, `crossbeam-channel` dependencies.
- **`shared/types.rs`** — added `movement::SHOOT` bit (1 << 4).
- **Client input** — `Space` key mapped to SHOOT.

---

## [0.3.0] — 2026-03-25

### Added
- **Client-side prediction (CSP)** — client immediately applies movement inputs locally
  so the player feels instant response regardless of network latency.
- **Server reconciliation** — on receiving a server snapshot, the client rewinds to the
  server-confirmed position and replays all unacknowledged inputs. If prediction matched
  the server exactly, no visible correction occurs.
- **Input sequence numbers** — each `InputFrame` carries a monotonically increasing `u32`
  sequence. The server echoes `last_processed_input` per client in `StateUpdate` packets
  so the client can discard acknowledged inputs from its prediction buffer.
- **`shared/physics.rs`** — pure movement and collision logic shared between server and
  client. Single source of truth: `apply_input()` applies one tick of WASD movement with
  wall collision and map-boundary clamping. Diagonal movement now normalised (no speed boost).
- **Static wall obstacles** — 4 axis-aligned walls defined in `shared::physics::WALLS`:
  central L-shaped barrier, top-left box, bottom-right box. Both server and client use the
  same wall data for deterministic collision.
- **Wall collision with wall-sliding** — X and Z axes tested independently so players slide
  along walls instead of stopping dead.
- **F3 debug overlay** — toggles a visual overlay showing:
  - Red ghost square at server-confirmed position (shows prediction divergence)
  - Color-coded RTT bar (green <30ms, yellow 30-100ms, red >100ms)
  - Pending-inputs bar (blue, length = unacknowledged input count)
  - Simulated-latency indicator (purple, visible when F4 latency > 0)
- **F4 simulated latency** — cycles through 0 → 50 → 100 → 200 → 0 ms of artificial
  outbound packet delay. Combined with F3 overlay, makes CSP/reconciliation directly
  observable during testing.
- 3 new unit tests in `shared::physics`: wall overlap, movement blocked by wall, diagonal
  normalisation.

### Changed
- **Server `game/mod.rs`** — now calls `shared::physics::apply_input()` instead of inline
  movement logic. Tracks `last_processed_seq` per session. `build_snapshots()` returns
  per-client packets with `last_processed_input`.
- **`ServerPacket::StateUpdate`** — added `last_processed_input: u32` field.
- **`InputFrame`** — added `sequence: u32` field.
- **Client renderer** — camera now follows predicted position; draws walls as brown quads;
  vertex buffer increased to 16384 for additional geometry.
- **Client network** — complete rewrite with prediction buffer (`VecDeque<InputFrame>`),
  reconciliation loop, delayed-send queue for simulated latency, RTT tracking.

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
