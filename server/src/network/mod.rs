use anyhow::Result;
use std::net::SocketAddr;
use tokio::net::UdpSocket;
use tracing::{info, warn};

use shared::{
    protocol::{ClientPacket, MAX_PACKET_SIZE, ServerPacket},
    tick::TICK_DURATION,
};

use crate::{config::ServerConfig, game::GameState};

pub async fn run_server(config: ServerConfig) -> Result<()> {
    let bind_addr = format!("0.0.0.0:{}", config.port);
    let socket = UdpSocket::bind(&bind_addr).await?;
    info!("Listening on {bind_addr}");

    let mut state = GameState::new();
    let mut interval = tokio::time::interval(TICK_DURATION);
    let mut buf = vec![0u8; MAX_PACKET_SIZE];

    loop {
        tokio::select! {
            biased;

            // Receive an incoming datagram
            result = socket.recv_from(&mut buf) => {
                match result {
                    Ok((len, addr)) => {
                        handle_packet(&buf[..len], addr, &mut state, &socket).await;
                    }
                    Err(e) => warn!("recv_from error: {e}"),
                }
            }

            // Game tick
            _ = interval.tick() => {
                state.tick();
                let snapshots = state.build_snapshots();
                for (addr, packet) in snapshots {
                    send_packet(&socket, &packet, addr).await;
                }
                state.prune_timed_out();
            }
        }
    }
}

async fn handle_packet(data: &[u8], addr: SocketAddr, state: &mut GameState, socket: &UdpSocket) {
    match decode_client(data) {
        Ok(packet) => {
            let responses = state.handle_client_packet(packet, addr);
            for resp in responses {
                send_packet(socket, &resp, addr).await;
            }
        }
        Err(e) => warn!("Bad packet from {addr}: {e}"),
    }
}

async fn send_packet(socket: &UdpSocket, packet: &ServerPacket, addr: SocketAddr) {
    match encode_server(packet) {
        Ok(bytes) => {
            if let Err(e) = socket.send_to(&bytes, addr).await {
                warn!("send_to {addr} failed: {e}");
            }
        }
        Err(e) => warn!("encode error: {e}"),
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
