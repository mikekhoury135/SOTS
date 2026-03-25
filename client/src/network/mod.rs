use std::sync::Arc;

use tokio::net::UdpSocket;
use tracing::{info, warn};

use shared::{
    protocol::{ClientPacket, ServerPacket, MAX_PACKET_SIZE},
    tick::TICK_DURATION,
    types::{InputFrame, PlayerFlags},
};

use crate::state::SharedState;

pub async fn run_client(server_addr: String, shared: Arc<SharedState>) {
    if let Err(e) = connect_and_run(server_addr, shared).await {
        warn!("Network task exited: {e}");
    }
}

async fn connect_and_run(server_addr: String, shared: Arc<SharedState>) -> anyhow::Result<()> {
    let socket = UdpSocket::bind("0.0.0.0:0").await?;
    socket.connect(&server_addr).await?;
    info!("UDP socket connected to {server_addr}");

    // ── Handshake ────────────────────────────────────────────────────────────
    let connect_bytes = encode(&ClientPacket::Connect { token: 0 })?;
    socket.send(&connect_bytes).await?;
    info!("Sent Connect; waiting for ConnectAck…");

    let mut buf = vec![0u8; MAX_PACKET_SIZE];

    // Wait up to 5 s for ConnectAck
    let player_id = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let len = socket.recv(&mut buf).await?;
            if let Ok(ServerPacket::ConnectAck { player_id }) = decode(&buf[..len]) {
                return Ok::<_, anyhow::Error>(player_id);
            }
        }
    })
    .await
    .map_err(|_| anyhow::anyhow!("ConnectAck timed out — is the server running?"))??;

    shared.game.lock().player_id = Some(player_id);
    info!("Connected as {player_id:?}");

    // ── Main loop: send inputs at TICK_RATE, receive state updates ───────────
    let mut interval = tokio::time::interval(TICK_DURATION);
    let mut tick: u16 = 0;

    loop {
        tokio::select! {
            biased;

            // Drain any incoming packets first
            Ok(len) = socket.recv(&mut buf) => {
                match decode(&buf[..len]) {
                    Ok(ServerPacket::StateUpdate { players, .. }) => {
                        shared.game.lock().players = players;
                    }
                    Ok(ServerPacket::Heartbeat { .. }) => {}
                    Ok(ServerPacket::Shutdown) => {
                        info!("Server shut down");
                        break;
                    }
                    _ => {}
                }
            }

            // Send input on each tick
            _ = interval.tick() => {
                let movement = shared.input.lock().movement;
                let frame = InputFrame {
                    tick,
                    movement,
                    yaw_delta: 0,
                    pitch_delta: 0,
                    flags: PlayerFlags::new(),
                };
                let bytes = encode(&ClientPacket::Input { frames: vec![frame] })?;
                if let Err(e) = socket.send(&bytes).await {
                    warn!("send error: {e}");
                }
                tick = tick.wrapping_add(1);
            }
        }
    }

    Ok(())
}

fn encode(packet: &ClientPacket) -> anyhow::Result<Vec<u8>> {
    Ok(bincode::serde::encode_to_vec(packet, bincode::config::standard())?)
}

fn decode(data: &[u8]) -> anyhow::Result<ServerPacket> {
    let (p, _) = bincode::serde::decode_from_slice(data, bincode::config::standard())
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    Ok(p)
}
