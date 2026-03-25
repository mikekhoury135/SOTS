use std::net::SocketAddr;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossbeam_channel::{Receiver, Sender, bounded};
use tokio::net::UdpSocket;
use tracing::{info, warn};

use shared::{
    protocol::{ClientPacket, MAX_PACKET_SIZE, ServerPacket},
    tick::TICK_DURATION,
};

use crate::{config::ServerConfig, game::GameState};

/// Inbound datagram from the IO thread → game loop.
struct Inbound {
    data: Vec<u8>,
    addr: SocketAddr,
}

/// Outbound packet from the game loop → IO thread.
struct Outbound {
    data: Vec<u8>,
    addr: SocketAddr,
}

/// Channel capacity — bounded to apply backpressure if game loop is too slow.
const CHANNEL_CAP: usize = 4096;

pub async fn run_server(config: ServerConfig) -> Result<()> {
    let bind_addr = format!("0.0.0.0:{}", config.port);
    let socket = UdpSocket::bind(&bind_addr).await?;
    info!("Listening on {bind_addr}");

    // Channels between IO (tokio) and game loop (dedicated thread)
    let (in_tx, in_rx) = bounded::<Inbound>(CHANNEL_CAP);
    let (out_tx, out_rx) = bounded::<Outbound>(CHANNEL_CAP);

    // ── Spawn the game loop on a dedicated OS thread ─────────────────────
    let tick_dur = TICK_DURATION;
    thread::Builder::new()
        .name("game-loop".into())
        .spawn(move || {
            game_loop(in_rx, out_tx, tick_dur);
        })?;

    // ── IO loop (tokio): recv datagrams → in_tx, drain out_rx → socket ──
    let mut buf = vec![0u8; MAX_PACKET_SIZE];

    loop {
        tokio::select! {
            biased;

            // Receive incoming UDP datagrams
            result = socket.recv_from(&mut buf) => {
                match result {
                    Ok((len, addr)) => {
                        let data = buf[..len].to_vec();
                        // Non-blocking send; drop packet if channel is full (overload shedding)
                        let _ = in_tx.try_send(Inbound { data, addr });
                    }
                    Err(e) => warn!("recv_from error: {e}"),
                }
            }

            // Drain outbound packets from the game loop
            _ = tokio::task::yield_now() => {
                while let Ok(out) = out_rx.try_recv() {
                    if let Err(e) = socket.send_to(&out.data, out.addr).await {
                        warn!("send_to {} failed: {e}", out.addr);
                    }
                }
            }
        }
    }
}

/// The game loop runs on a dedicated OS thread, isolated from tokio's scheduler.
/// Uses spin_sleep for sub-millisecond tick accuracy.
fn game_loop(in_rx: Receiver<Inbound>, out_tx: Sender<Outbound>, tick_dur: Duration) {
    let mut state = GameState::new();
    let mut next_tick = Instant::now();

    info!("Game loop started ({:.1} Hz)", 1.0 / tick_dur.as_secs_f64());

    loop {
        // ── Drain all pending inbound packets ────────────────────────────
        while let Ok(inbound) = in_rx.try_recv() {
            match decode_client(&inbound.data) {
                Ok(packet) => {
                    let responses = state.handle_client_packet(packet, inbound.addr);
                    for resp in responses {
                        if let Ok(bytes) = encode_server(&resp) {
                            let _ = out_tx.try_send(Outbound {
                                data: bytes,
                                addr: inbound.addr,
                            });
                        }
                    }
                }
                Err(e) => warn!("Bad packet from {}: {e}", inbound.addr),
            }
        }

        // ── Tick the game world ──────────────────────────────────────────
        state.tick();

        // ── Build and send snapshots ─────────────────────────────────────
        let snapshots = state.build_snapshots();
        for (addr, packet) in snapshots {
            if let Ok(bytes) = encode_server(&packet) {
                let _ = out_tx.try_send(Outbound { data: bytes, addr });
            }
        }

        // ── Prune timed-out sessions ─────────────────────────────────────
        state.prune_timed_out();

        // ── Sleep precisely until next tick ───────────────────────────────
        next_tick += tick_dur;
        let now = Instant::now();
        if next_tick > now {
            spin_sleep::sleep(next_tick - now);
        } else {
            // We fell behind — reset to avoid tick burst
            next_tick = now;
        }
    }
}

fn encode_server(packet: &ServerPacket) -> Result<Vec<u8>> {
    let bytes = bincode::serde::encode_to_vec(packet, bincode::config::standard())?;
    Ok(bytes)
}

fn decode_client(data: &[u8]) -> Result<ClientPacket> {
    let (packet, _) = bincode::serde::decode_from_slice(data, bincode::config::standard())
        .map_err(|e| anyhow::anyhow!("decode: {e}"))?;
    Ok(packet)
}
