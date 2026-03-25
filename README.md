# SOTS — Super Optimized Tactical Shooter

A work-in-progress tactical shooter with an authoritative Rust server (Dockerized) and a Windows desktop client built with wgpu + winit.

---

## Architecture

```
[Windows Client]  ──UDP 7777──▶  [Linux Server (Docker)]
  winit window                     tokio async loop
  wgpu renderer                    hecs ECS world
  64 Hz input send                 64 Hz tick broadcast
```

The server is the authority on all game state. The client sends inputs and renders whatever the server says.

---

## Prerequisites

| Tool | Purpose | Install |
|---|---|---|
| Rust stable | Build everything | `rustup` |
| Docker Desktop | Run the server container | [docker.com](https://docker.com) |
| `just` | Task runner | `cargo install just` |
| `cross` | Windows cross-compilation | `cargo install cross --locked` |

---

## Quick Start

### 1 — Start the server

**Option A: locally (no Docker)**
```bash
just server-run
# or: cargo run -p server
```

**Option B: Docker**
```bash
just server-docker
# or: docker-compose -f docker/docker-compose.yml up --build
```

The server listens on **UDP 7777** by default.

---

### 2 — Run the client

**On Linux / macOS (development):**
```bash
cargo run -p client
# Connect to a remote server:
cargo run -p client -- 192.168.1.10:7777
```

**On Windows — build and run the native .exe:**
```bash
# Cross-compile from any OS (requires cross + Docker)
just client-windows
# → target/x86_64-pc-windows-gnu/release/client.exe

# Debug build (console window visible for log output)
just client-windows-debug
# → target/x86_64-pc-windows-gnu/debug/client.exe
```

Copy the `.exe` to your Windows machine and run it:
```
client.exe                        # connects to 127.0.0.1:7777
client.exe 192.168.1.10:7777      # connects to a remote server
```

---

## Controls

| Key | Action |
|---|---|
| `W` / `↑` | Move forward (north) |
| `S` / `↓` | Move backward (south) |
| `A` / `←` | Strafe left |
| `D` / `→` | Strafe right |
| `Alt+F4` / window ✕ | Quit |

The camera is top-down and follows your player (cyan square). Other players appear as orange squares.

---

## Development Commands

```bash
just build          # build all crates
just test           # run all tests
just lint           # clippy -D warnings
just fmt            # format all code
just check          # fmt + lint + test (full pre-PR gate)
just clean          # cargo clean
```

---

## Project Layout

```
/
├── Cargo.toml          # workspace root (pinned dep versions)
├── CLAUDE.md           # project guide and tech decisions
├── ARCHITECTURE.md     # runtime thread/data-flow diagrams
├── CHANGELOG.md        # one entry per session
├── justfile            # dev task runner
├── server/             # authoritative game server (Dockerized)
│   └── src/
│       ├── main.rs     # tokio entry point
│       ├── network/    # UDP recv/send loop
│       └── game/       # hecs ECS world, tick, movement
├── client/             # Windows desktop client
│   └── src/
│       ├── main.rs     # winit event loop entry
│       ├── app.rs      # ApplicationHandler (input + render dispatch)
│       ├── network/    # UDP client task (background tokio thread)
│       ├── input/      # WASD key state
│       ├── renderer/   # wgpu pipeline, floor tiles, player quads
│       └── state.rs    # shared state between threads
├── shared/             # types imported by both server and client
│   └── src/
│       ├── protocol.rs # ClientPacket / ServerPacket enums
│       ├── types.rs    # PlayerState, InputFrame, QuantizedPosition
│       ├── tick.rs     # TickNum, TICK_RATE (64 Hz)
│       └── transport.rs# Transport trait (swappable later)
└── docker/
    ├── Dockerfile      # multi-stage: rust builder → bookworm-slim
    └── docker-compose.yml  # host networking + sysctl UDP tuning
```

---

## Roadmap

| Phase | Status | Description |
|---|---|---|
| 0 | ✅ Done | Workspace skeleton, Docker, docs |
| 1 | ✅ Done | UDP connect/disconnect, 64 Hz tick, flat map |
| 2 | 🔲 Next | Client-side prediction, server reconciliation |
| 3 | 🔲 | Hit detection, health, respawn |
| 4 | 🔲 | 3D rendering (first/third person camera) |
| 5 | 🔲 | Weapons, game mode, scoreboard |
| 6 | 🔲 | Auth, map format, production hardening |
