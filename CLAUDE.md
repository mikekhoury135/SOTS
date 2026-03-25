# CLAUDE.md — Tactical Shooter Project (SOTS)

## Project Overview

A tactical shooter game with a containerized server and a Windows desktop client,
communicating over a network. Primary language is Rust. All new tech decisions are
documented here before code is written that depends on them.

**Repo structure:**
```
/
├── Cargo.toml          # workspace root
├── CHANGELOG.md        # one entry per task/session
├── CLAUDE.md           # this file
├── ARCHITECTURE.md     # dependency flow and runtime diagrams
├── server/             # game server binary (Dockerized)
├── client/             # Windows desktop client binary
├── shared/             # shared types, protocol, serialization (workspace crate)
├── docker/             # Dockerfile + docker-compose
└── docs/               # design docs, protocol specs
```

Use a Cargo workspace (`Cargo.toml` at root) so server, client, and shared are
workspace members. Dependency versions are pinned at the workspace level.

---

## Tech Stack

| Layer | Technology | Version | Notes |
|---|---|---|---|
| Language | Rust (stable) | 1.85+ | Default for all new code |
| Async runtime | tokio | 1.47.x (LTS) | LTS support through Sep 2026; IO send side only |
| Networking | Raw UDP — `tokio::net::UdpSocket` + `socket2` | — | See Networking section |
| Serialization | `serde` + `bincode` 2.x | 1.x / 2.x | Fast binary wire format; serde feature-gated in bincode 2 |
| Math | `glam` | 0.32.x | SIMD Vec3/Mat4; used in shared types |
| Game state (ECS) | `hecs` | 0.10.x | Lightweight archetype ECS; no proc-macro overhead |
| Tick timing | `spin_sleep` | 0.3.x | Sub-millisecond accurate fixed-rate loop |
| Socket tuning | `socket2` | 0.5.x | SO_REUSEPORT, buffer sizes, DSCP/IP_TOS marking |
| Buffer mgmt | `bytes` | 1.x | Zero-malloc hot path; BytesMut pool |
| IO↔game comms | `crossbeam-channel` | 0.5.x | Lock-free SPSC between IO thread and game loop |
| Fast hash maps | `ahash` | 0.8.x | 2–10× faster than SipHash for session table |
| Fast mutexes | `parking_lot` | 0.12.x | 5× faster under contention vs std::sync::Mutex |
| Lib errors | `thiserror` | 2.x | Derive macros for error enums |
| Binary errors | `anyhow` | 1.x | Ergonomic error propagation in binaries |
| Logging | `tracing` + `tracing-subscriber` | 0.1.x / 0.3.x | Structured, async-aware, env-filter |
| Windowing | `winit` | 0.30.x **(pinned)** | Pre-1.0 — breaking changes land in minor versions; pin this |
| GPU rendering | `wgpu` | 29.x | Vulkan/D3D12/Metal; WebGPU-aligned; actively maintained |
| Server packaging | Docker | — | Multi-stage build; `debian:bookworm-slim` runtime |

**Explicitly avoided (deprecated / unmaintained):**
- `laminar` — UDP networking lib, unmaintained since Oct 2023
- `legion` ECS — deprecated
- `glium` — deprecated OpenGL wrapper
- `Bevy` — breaking changes every ~3 months, too unstable for a custom game loop

**Deferred (revisit after profiling):**
- `rkyv` — zero-copy deserialization; API still evolving; add if bincode becomes a hotspot
- `crossfire` — faster channels than crossbeam; newer, less battle-tested
- `tokio-uring` — io_uring on Linux; v0.1.0, UDP support immature; consider Phase 6+

When a new technology is added, **update this table first** before writing code.

---

## Networking Model

**Transport: Raw UDP with a minimal custom reliability layer.**

Rationale:
- QUIC (`quinn`) mandates TLS 1.3 — adds ~5–10% CPU overhead and latency at 64 Hz tick
  windows (15.6 ms). Unacceptable when ping is the primary bottleneck.
- Competitive shooters (CS2, Valorant, Quake) all use raw UDP.
- `laminar` is dead; we own a thin layer — not much code:
  sequence numbers (`u16` wrapping), ACK bitfield (32 packets in one `u32`),
  retransmit for critical packets (connect/disconnect), best-effort for per-tick data.
- The transport is hidden behind a `Transport` trait in `shared/transport.rs`
  so QUIC can be swapped in later without touching game logic.

**Game port: UDP 7777**

### Socket tuning (applied at server startup via `socket2`)
```rust
socket.set_recv_buffer_size(8 * 1024 * 1024)?; // 8 MB
socket.set_send_buffer_size(8 * 1024 * 1024)?;
socket.set_reuse_port(true)?;                   // one socket per recv thread
socket.set_tos(0b10111000)?;                    // DSCP EF = high-priority QoS
```

### Docker networking
Use `network_mode: "host"` — eliminates Docker bridge NAT entirely.
Kernel sysctl values are set in `docker-compose.yml`.

---

## Thread Architecture

The game loop is **fully synchronous and isolated from the Tokio scheduler**.
This prevents async scheduler jitter from ever affecting tick timing.

```
┌─────────────────┐   crossbeam    ┌──────────────────┐   crossbeam    ┌──────────────────┐
│  IO Recv Thread │ ─────────────▶ │  Game Loop Thread │ ─────────────▶ │  IO Send (tokio) │
│  socket2 UDP    │   Datagram     │  spin_sleep timer  │   OutPacket    │  fire-and-forget │
│  SO_REUSEPORT   │   bounded ch.  │  hecs World        │   bounded ch.  │  workers         │
└─────────────────┘                │  64 Hz tick        │                └──────────────────┘
                                   └──────────────────┘
```

- **IO Recv**: one or more threads, each owning a `socket2` UDP socket with `SO_REUSEPORT`.
  Reads datagrams, decodes packet headers, pushes to game loop via bounded crossbeam channel.
- **Game Loop**: single synchronous thread. Reads inputs, runs tick, builds delta snapshots,
  pushes outbound packets to send queue. Uses `spin_sleep::Interval` for accurate 64 Hz cadence.
- **IO Send**: tokio async workers drain the send queue and fire packets.

---

## Protocol Design (latency-first)

All types live in `shared/` and are imported by both server and client.

| Decision | Value | Rationale |
|---|---|---|
| Position encoding | `u16` fixed-point (×32 scale) | 50% smaller than `f32`; <1/32 unit error |
| Tick number | `u16` wrapping | Smaller than `u32`; half-space comparison handles wrap |
| State updates | Delta compressed | Only changed entities sent per tick |
| Player flags | `u8` bitfield | Alive/crouching/shooting/reloading packed in one byte |
| ACK scheme | `u16` sequence + `u32` bitfield | 32-packet history in 6 bytes total |
| Max packet size | 1400 bytes | Under MTU (1500) minus IP(20) + UDP(8) headers |

---

## Tick Rate

| Setting | Value | Notes |
|---|---|---|
| Target tick rate | 64 Hz | 15.625 ms windows |
| Tick number type | `u16` wrapping | ~17 min before wrap; safe with half-space comparison |
| Timer | `spin_sleep::Interval` | Avoids ~1 ms OS timer jitter from tokio::time::interval |
| Ceiling | 128 Hz | Architecture supports it; bump after profiling confirms IO headroom |

Tick rate is a single constant in `shared/src/tick.rs: TICK_RATE`.

---

## Git Workflow

### Branch Strategy
```
main                  ← production-stable; PRs only, manually approved
└── dev               ← integration branch; auto-push OK
    └── feature/<name>
    └── fix/<name>
    └── chore/<name>
```

### Commit Discipline
- Commit after every request/task — one logical unit of work per commit.
- Format: `<type>(<scope>): <short description>`
  - Types: `feat`, `fix`, `refactor`, `chore`, `docs`, `test`
  - Example: `feat(server): add player spawn packet handler`
- Push to feature branch immediately after committing.
- Open a PR to `main` when a feature is complete. PRs are manually reviewed — do not merge without approval.

### PR Checklist
- [ ] `cargo build --workspace` passes
- [ ] `cargo test --workspace` passes
- [ ] `cargo clippy --workspace -- -D warnings` clean
- [ ] Docker build succeeds if server code changed
- [ ] `CLAUDE.md` updated if new tech or conventions added
- [ ] `CHANGELOG.md` updated
- [ ] Feature documented in `docs/` if non-trivial

---

## Coding Conventions

- **No `unwrap()` in production paths** — use `?` or explicit error handling. `unwrap()` is
  acceptable in tests and one-off tooling only.
- **Clippy is law** — `cargo clippy -- -D warnings` must pass before any commit touching logic.
- **Format on save** — `cargo fmt --all` before committing. CI will reject unformatted code.
- **`thiserror`** for library/crate errors; **`anyhow`** for binary entry points.
- **Keep game logic pure** — no I/O in game logic modules. Pure functions can be unit-tested
  without a running server.
- **No `unsafe` without justification** — every `unsafe` block must have a `// SAFETY:` comment.
- **No allocations in hot path** — pre-allocate buffers at startup; reuse `BytesMut` pools in
  the tick loop. No `Vec::new()` or `Box::new()` per tick.
- **`AHashMap` everywhere** — replace `HashMap` with `ahash::AHashMap` for all in-memory maps.

---

## Testing Strategy

| Type | Tool | Location |
|---|---|---|
| Unit tests | `cargo test` | Inline `#[cfg(test)]` in each crate |
| Integration | `cargo test` | `tests/` dir per crate |
| Network/sim | Custom harness | `server/tests/` — spin up server, connect mock client |
| Manual QA | Local docker + client | Primary end-to-end validation |

Run before any PR:
```
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --all -- --check
```

---

## Docker

Multi-stage build in `docker/Dockerfile`:
1. **Build stage** — `rust:1.85-bookworm`, compiles server binary in release mode.
2. **Runtime stage** — `debian:bookworm-slim`, copies binary only. No Rust toolchain.

`docker/docker-compose.yml` uses `network_mode: "host"` and sets kernel sysctl values for
UDP performance. Port 7777/UDP is bound directly on the host interface.

---

## Development Environment

### Prerequisites
- Rust stable toolchain (`rustup`)
- Docker Desktop (server container)
- `cargo-watch` (optional, hot reload)

### Useful Commands
```bash
# Full workspace build
cargo build --workspace

# Run tests
cargo test --workspace

# Lint (strict)
cargo clippy --workspace -- -D warnings

# Format
cargo fmt --all

# Check formatting without changing files
cargo fmt --all -- --check

# Run server locally (no Docker)
cargo run -p server

# Run server in Docker
docker-compose -f docker/docker-compose.yml up --build

# Run client (Windows)
cargo run -p client

# Watch for changes (requires cargo-watch)
cargo watch -x "run -p server"
```

---

## Decisions Log

| Date | Decision | Rationale |
|---|---|---|
| — | Rust for all core code | Performance, memory safety, single binary deploys |
| — | Client Windows-only for now | Simplifies rendering/input stack during early dev |
| — | Authoritative server model | Prevents client-side cheating; standard for tac shooters |
| 2026-03-25 | Raw UDP over QUIC | QUIC TLS overhead unacceptable at 64 Hz; ping is the bottleneck |
| 2026-03-25 | wgpu + winit over Bevy | Bevy breaks every ~3 months; custom loop needed for perf |
| 2026-03-25 | hecs over legion/shipyard | Lightweight, no proc-macro magic, easy to unit-test |
| 2026-03-25 | Synchronous game loop thread | Prevents tokio scheduler jitter from affecting tick timing |
| 2026-03-25 | Delta compression + u16 fixed-point | Minimises bytes-on-wire; less serialisation CPU per tick |
| 2026-03-25 | Docker network_mode: host | Eliminates NAT overhead; direct NIC access for UDP |
| 2026-03-25 | Game port UDP 7777 | Common game server port |
| 2026-03-25 | Tick rate 64 Hz (ceiling 128 Hz) | Industry standard; bump after profiling |

---

## Open Questions

- [ ] Rendering stack finalised: `wgpu 29.x + winit 0.30.x` — monitor winit 1.0 release
- [ ] Authentication: stub token in handshake for now; real auth in Phase 6+
- [ ] Map/level format: custom binary (Phase 6); defer until rendering is solid
- [ ] Audio: `rodio` or `kira`? Add when Phase 5 begins
- [ ] `rkyv` vs `bincode 2.x`: profile deserialization in Phase 2, switch if warranted
- [ ] `tokio-uring`: revisit in Phase 6 when targeting high player counts on Linux
