# SOTS — Super Optimized Tactical Shooter

A work-in-progress tactical shooter with an authoritative Rust server (Dockerized) and a Windows desktop client built with wgpu + winit.

---

## Architecture

```
[Windows Client]  ──UDP 7777──▶  [Linux Server (Docker)]
  winit window                     tokio async loop
  wgpu 3D renderer                 hecs ECS world
  128 Hz input send                128 Hz tick (spin_sleep)
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

> All commands below must be run from the **repo root** (`SOTS/`).

### 1 — Start the server

**Option A: locally (Windows, Linux, or macOS — no Docker)**
```
just server-run
```
or equivalently:
```
cargo run -p server
```

**Option B: Docker (Linux host or Docker Desktop + WSL2)**
```
just server-docker
```
or equivalently (Docker v2):
```
docker compose -f docker/docker-compose.yml up --build
```

The server listens on **UDP 7777** by default.

---

### 2 — Run the client

**On any platform (Windows, Linux, macOS):**
```
just client-run
```
or equivalently:
```
cargo run -p client                       # connects to 127.0.0.1:7777
cargo run -p client -- 192.168.1.10:7777  # connects to a remote server
```

**Cross-compile a Windows `.exe` for distribution (Linux/macOS only — requires `cross` + Docker):**
```
just client-windows
# → target/x86_64-pc-windows-gnu/release/client.exe

just client-windows-debug
# → target/x86_64-pc-windows-gnu/debug/client.exe  (console visible for logs)
```

Copy the `.exe` to any Windows machine and run it:
```
client.exe                        # connects to 127.0.0.1:7777
client.exe 192.168.1.10:7777      # connects to a remote server
```

---

## Controls

| Key | Action |
|---|---|
| `W` / `↑` | Move forward |
| `S` / `↓` | Move backward |
| `A` / `←` | Strafe left |
| `D` / `→` | Strafe right |
| Mouse | Look (yaw) |
| Left click | Shoot (hitscan) |
| `F3` | Toggle debug overlay |
| `F4` | Cycle simulated latency (0/50/100/200ms) |
| `Escape` | Quit |

The camera is first-person 3D. Mouse controls where you look. Walls and other players are rendered as 3D boxes. A white crosshair marks the screen center.

---

## Development Commands

Run from the **repo root**:

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
│       ├── input/      # WASD key + mouse state
│       ├── renderer/   # wgpu 3D pipeline, perspective camera, depth buffer
│       └── state.rs    # shared state between threads
├── shared/             # types imported by both server and client
│   └── src/
│       ├── protocol.rs # ClientPacket / ServerPacket enums
│       ├── types.rs    # PlayerState, InputFrame, QuantizedPosition
│       ├── tick.rs     # TickNum, TICK_RATE (128 Hz)
│       ├── physics.rs  # movement, collision, wall geometry
│       ├── combat.rs   # hitscan raycast, damage, respawn
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
| 2 | ✅ Done | CSP, server reconciliation, walls, debug overlay |
| 3 | ✅ Done | 128 Hz tick, dedicated game loop, hit detection, health, respawn |
| 4 | ✅ Done | First-person 3D rendering, mouse look, depth buffer |
| 5 | 🔲 Next | Weapons, game mode, scoreboard |
| 6 | 🔲 | Auth, map format, production hardening |
