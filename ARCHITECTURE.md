# ARCHITECTURE.md — SOTS Dependency & Runtime Flow

This document describes how all crates, threads, and data flows connect at runtime.
Update this file whenever a new crate is introduced or a significant structural change is made.

---

## 1. Cargo Workspace Structure

```
Cargo.toml (workspace root)
├── shared/          — library crate; no I/O, no threads
├── server/          — binary crate; imports shared
└── client/          — binary crate; imports shared
```

Workspace-level `[workspace.dependencies]` pins all crate versions in one place.
Both `server` and `client` inherit versions from there — no version drift.

---

## 2. Crate Dependency Graph

```
           ┌──────────────────────────────────────────────┐
           │                   shared                      │
           │                                               │
           │  serde ──▶ bincode   (wire serialisation)     │
           │  glam                (Vec3, quantised pos)     │
           │  thiserror           (TransportError enum)     │
           │  bytes               (Datagram buffer type)    │
           │                                               │
           │  Modules:                                      │
           │    types.rs       PlayerState, InputFrame      │
           │    protocol.rs    ClientPacket, ServerPacket   │
           │    transport.rs   Transport trait              │
           │    tick.rs        TickNum, TICK_RATE, TICK_DUR │
           └──────────┬──────────────────────┬─────────────┘
                      │                      │
          ┌───────────▼──────────┐  ┌────────▼─────────────┐
          │       server         │  │        client         │
          │                      │  │                       │
          │  tokio (async rt)    │  │  tokio (async rt)     │
          │  socket2 (raw UDP)   │  │  socket2 (UDP)        │
          │  crossbeam-channel   │  │  crossbeam-channel    │
          │  ahash (session map) │  │  ahash                │
          │  parking_lot (locks) │  │  parking_lot          │
          │  spin_sleep (ticker) │  │  tracing              │
          │  hecs (ECS world)    │  │  anyhow               │
          │  tracing             │  │                       │
          │  anyhow              │  │  [Phase 3]            │
          │                      │  │  winit 0.30.x (window)│
          └──────────────────────┘  │  wgpu 29.x (GPU)      │
                                    └───────────────────────┘
```

---

## 3. Server Runtime Thread Model

```
  ┌──────────────────────────────────────────────────────────────────────┐
  │                          server process                              │
  │                                                                      │
  │  ┌─────────────────────┐          ┌──────────────────────────────┐  │
  │  │   IO Recv Thread(s) │          │      Game Loop Thread        │  │
  │  │                     │          │                              │  │
  │  │  socket2::UdpSocket │          │  spin_sleep::Interval        │  │
  │  │  SO_REUSEPORT       │─ inbound─▶  64 Hz fixed timestep       │  │
  │  │  8 MB recv buffer   │  channel │                              │  │
  │  │                     │          │  hecs::World                 │  │
  │  │  decode header      │          │  ├─ Position components      │  │
  │  │  push Datagram      │          │  ├─ Health components        │  │
  │  │                     │          │  ├─ InputBuffer components   │  │
  │  └─────────────────────┘          │  └─ Flags components         │  │
  │                                   │                              │  │
  │  ┌─────────────────────┐          │  per tick:                   │  │
  │  │   IO Send           │          │  1. drain inbound channel    │  │
  │  │   (tokio workers)   │          │  2. apply inputs to world    │  │
  │  │                     │◀outbound─│  3. run game logic           │  │
  │  │  fire-and-forget    │  channel │  4. build delta snapshot     │  │
  │  │  8 MB send buffer   │          │  5. push to outbound channel │  │
  │  └─────────────────────┘          └──────────────────────────────┘  │
  │                                                                      │
  │  ┌───────────────────────────────────────────────┐                  │
  │  │  Session Table (AHashMap<SocketAddr, Session>) │                  │
  │  │  parking_lot::RwLock — read-heavy, write-rare  │                  │
  │  └───────────────────────────────────────────────┘                  │
  └──────────────────────────────────────────────────────────────────────┘
```

**Key invariant:** The game loop thread never blocks on I/O and never awaits a future.
All communication is through bounded `crossbeam-channel` queues. Back-pressure from a
full queue signals an overloaded server.

---

## 4. Client Runtime Flow (Phase 3+)

```
  ┌──────────────────────────────────────────────────────────────────────┐
  │                          client process                              │
  │                                                                      │
  │  ┌──────────────────────────────────────────────────────────────┐   │
  │  │  winit Event Loop (main thread — Windows requires this)       │   │
  │  │                                                               │   │
  │  │  WindowEvent::KeyboardInput  ──▶  input module               │   │
  │  │  WindowEvent::MouseMotion    ──▶  input module               │   │
  │  │  WindowEvent::RedrawRequested──▶  renderer::render_frame()   │   │
  │  └──────────────────────────────────────────────────────────────┘   │
  │                      │                        │                      │
  │                ┌─────▼──────┐        ┌────────▼───────┐             │
  │                │   input    │        │    renderer    │             │
  │                │            │        │                │             │
  │                │ pack into  │        │  wgpu device   │             │
  │                │ InputFrame │        │  wgpu surface  │             │
  │                └─────┬──────┘        │  wgpu queue    │             │
  │                      │               │                │             │
  │                      │ crossbeam     │  reads latest  │             │
  │                      ▼               │  interpolated  │             │
  │  ┌───────────────────────────┐       │  game state    │             │
  │  │   network task (tokio)    │       └────────▲───────┘             │
  │  │                           │                │                      │
  │  │  tokio UdpSocket          │       ┌────────┴───────┐             │
  │  │  send InputFrame → server │       │   prediction   │             │
  │  │  recv StateUpdate         │──────▶│                │             │
  │  │                           │       │  apply input   │             │
  │  └───────────────────────────┘       │  locally       │             │
  │                                      │  interpolate   │             │
  │                                      │  reconcile     │             │
  │                                      └────────────────┘             │
  └──────────────────────────────────────────────────────────────────────┘
```

**Key invariant:** `winit` event loop owns the main thread (Windows requirement).
The tokio runtime runs in a background thread. State is exchanged via shared
`parking_lot::RwLock` or crossbeam channels — never blocking the event loop.

---

## 5. Data Flow: One Server Tick

```
  [Client A] ──UDP──▶ [IO Recv]
                           │ Datagram { data, addr }
                           ▼ (crossbeam bounded)
                      [Game Loop] ◀── spin_sleep tick fires
                           │
                           ├─ 1. Decode ClientPacket::Input { frames }
                           ├─ 2. Apply InputFrame to hecs entity
                           ├─ 3. Run movement, collision, hit detection
                           ├─ 4. Diff world vs last ACKed state per client
                           ├─ 5. Build ServerPacket::StateUpdate { delta }
                           │
                           ▼ (crossbeam bounded)
                      [IO Send]
                           │ bincode-encode + PacketHeader
                           ▼
  [Client A] ◀──UDP──
  [Client B] ◀──UDP──
  [Client N] ◀──UDP──
```

---

## 6. Shared Crate Module Map

```
shared/src/
├── lib.rs          pub mod declarations
├── types.rs        PlayerState, InputFrame, QuantizedPosition, PlayerFlags
├── protocol.rs     ClientPacket, ServerPacket, PacketHeader, MAX_PACKET_SIZE, DEFAULT_PORT
├── transport.rs    Transport trait, TransportError, Datagram
└── tick.rs         TickNum (u16 wrapping), TICK_RATE, TICK_DURATION
```

`shared` has **no I/O and no async**. Every type in it is pure data or a pure trait.
This makes the entire game-logic surface unit-testable without spinning up a server.

---

## 7. Wire Protocol Summary

```
UDP Datagram layout (max 1400 bytes):
┌─────────────────────────────────────────────────────┐
│  PacketHeader (6 bytes)                              │
│    sequence : u16   — sender's sequence number       │
│    ack      : u16   — last received remote sequence  │
│    ack_bits : u32   — bitfield of prior 32 sequences │
├─────────────────────────────────────────────────────┤
│  Payload (bincode-encoded ClientPacket/ServerPacket) │
│                                                      │
│  StateUpdate player entry (11 bytes each):           │
│    id       : u16                                    │
│    x, y, z  : u16 × 3  (fixed-point ×32)            │
│    yaw      : u16                                    │
│    pitch    : i16                                    │
│    health   : u8                                     │
│    flags    : u8   (bitfield)                        │
└─────────────────────────────────────────────────────┘
```

---

## 8. Docker Layer Map

```
docker/Dockerfile
  Stage 1 (builder): rust:1.85-bookworm
    └── cargo build --release -p server
          └── /build/target/release/server  (~5 MB stripped binary)

  Stage 2 (runtime): debian:bookworm-slim
    └── COPY binary only
    └── EXPOSE 7777/udp
    └── ENTRYPOINT ["sots-server"]

docker/docker-compose.yml
  network_mode: host          ← no NAT, direct NIC
  sysctls:
    net.core.rmem_max       = 128 MB
    net.core.wmem_max       = 128 MB
    net.core.netdev_max_backlog = 50000
```

---

## 9. Phase Roadmap

| Phase | Focus | Key deliverables |
|---|---|---|
| **0** ✅ | Foundation | Workspace, stubs, Docker, CLAUDE.md, ARCHITECTURE.md |
| **1** | Network skeleton | UDP IO threads, session table, handshake, heartbeat |
| **2** | Tick loop + game state | hecs world, 64 Hz loop, input→state, delta snapshots |
| **3** | Client rendering | winit window, wgpu pipeline, placeholder geometry, input capture |
| **4** | Prediction & reconciliation | Client-side prediction, server corrections |
| **5** | Core game logic | Movement, collision, hitscan, health, respawn |
| **6** | Polish & infra | Auth stub, config (TOML), metrics, map format, tokio-uring eval |
