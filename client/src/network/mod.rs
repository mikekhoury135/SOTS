use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Instant;

use glam::Vec3;
use tokio::net::UdpSocket;
use tracing::{info, warn};

use shared::{
    physics,
    protocol::{ClientPacket, MAX_PACKET_SIZE, ServerPacket},
    tick::TICK_DURATION,
    types::{InputFrame, PlayerFlags},
};

use crate::state::SharedState;

/// Maximum number of unacknowledged inputs to keep for reconciliation.
const MAX_PENDING_INPUTS: usize = 128;

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

    {
        let mut game = shared.game.lock();
        game.player_id = Some(player_id);
    }
    info!("Connected as {player_id:?}");

    // ── Prediction state ─────────────────────────────────────────────────────
    let mut predicted_pos = Vec3::ZERO;
    let mut predicted_yaw: f32 = 0.0;
    let mut predicted_vy: f32 = 0.0;
    let mut input_sequence: u32 = 1;
    let mut pending_inputs: VecDeque<InputFrame> = VecDeque::with_capacity(MAX_PENDING_INPUTS);

    // ── RTT tracking ─────────────────────────────────────────────────────────
    let mut last_input_send_time = Instant::now();
    let mut rtt_ms: f32 = 0.0;

    // ── Delayed send queue for simulated latency ─────────────────────────────
    let mut delayed_queue: VecDeque<(Instant, Vec<u8>)> = VecDeque::new();

    // ── Main loop ────────────────────────────────────────────────────────────
    let mut interval = tokio::time::interval(TICK_DURATION);
    let mut tick: u16 = 0;

    loop {
        // Flush any delayed packets whose time has come
        while let Some((send_at, _)) = delayed_queue.front() {
            if Instant::now() >= *send_at {
                let (_, bytes) = delayed_queue.pop_front().expect("checked front");
                if let Err(e) = socket.send(&bytes).await {
                    warn!("send error: {e}");
                }
            } else {
                break;
            }
        }

        tokio::select! {
            biased;

            // Drain any incoming packets first
            Ok(len) = socket.recv(&mut buf) => {
                match decode(&buf[..len]) {
                    Ok(ServerPacket::StateUpdate { server_tick, last_processed_input, players }) => {
                        let recv_time = Instant::now();

                        // ── RTT estimate (simple: time since we sent the acked input) ──
                        let elapsed = recv_time.duration_since(last_input_send_time);
                        rtt_ms = rtt_ms * 0.9 + elapsed.as_secs_f32() * 1000.0 * 0.1;

                        // ── Find server-authoritative position for local player ──
                        let server_pos = players
                            .iter()
                            .find(|p| p.id == player_id)
                            .map(|p| p.position.to_vec3())
                            .unwrap_or(predicted_pos);

                        let server_yaw = players
                            .iter()
                            .find(|p| p.id == player_id)
                            .map(|p| {
                                p.yaw as f32 / 65536.0 * std::f32::consts::TAU
                            })
                            .unwrap_or(predicted_yaw);

                        // ── Discard acknowledged inputs ──
                        while let Some(front) = pending_inputs.front() {
                            if front.sequence <= last_processed_input {
                                pending_inputs.pop_front();
                            } else {
                                break;
                            }
                        }

                        // ── Reconciliation: rewind to server state, replay unacked inputs ──
                        predicted_pos = server_pos;
                        predicted_yaw = server_yaw;
                        // vy is not reconciled from server (not in snapshot); reset to zero on landing
                        predicted_vy = 0.0;

                        for frame in &pending_inputs {
                            physics::apply_input(
                                &mut predicted_pos,
                                &mut predicted_yaw,
                                &mut predicted_vy,
                                frame,
                            );
                        }

                        // ── Update game view for the renderer ──
                        {
                            let mut game = shared.game.lock();
                            game.players = players;
                            game.predicted_pos = predicted_pos;
                            game.predicted_yaw = predicted_yaw;
                            game.server_pos = server_pos;
                            game.rtt_ms = rtt_ms;
                            game.server_tick = server_tick;
                            game.client_tick = tick;
                            game.pending_inputs = pending_inputs.len();
                        }
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
                // Read keyboard movement + drain accumulated mouse yaw + consume fire request
                let (base_movement, raw_yaw_delta, fire) = {
                    let mut input = shared.input.lock();
                    (input.movement, input.take_yaw_delta(), input.take_fire_request())
                };

                // fire_requested ensures even a sub-tick tap always produces one SHOOT frame
                let movement = if fire {
                    base_movement | shared::types::movement::SHOOT
                } else {
                    base_movement
                };

                // Clamp raw_yaw_delta to i16 range for the wire format
                let yaw_delta = raw_yaw_delta.clamp(i16::MIN as f32, i16::MAX as f32) as i16;

                let frame = InputFrame {
                    tick,
                    sequence: input_sequence,
                    movement,
                    yaw_delta,
                    pitch_delta: 0,
                    flags: PlayerFlags::new(),
                };

                // ── Client-side prediction: apply input immediately ──
                physics::apply_input(&mut predicted_pos, &mut predicted_yaw, &mut predicted_vy, &frame);

                // ── Store in pending buffer for reconciliation ──
                if pending_inputs.len() >= MAX_PENDING_INPUTS {
                    pending_inputs.pop_front();
                }
                pending_inputs.push_back(frame);

                // ── Update predicted position + yaw in game view ──
                {
                    let mut game = shared.game.lock();
                    game.predicted_pos = predicted_pos;
                    game.predicted_yaw = predicted_yaw;
                    game.client_tick = tick;
                    game.pending_inputs = pending_inputs.len();
                    if movement & shared::types::movement::SHOOT != 0 {
                        game.last_shot_time = Some(Instant::now());
                    }
                }

                // ── Send to server (possibly delayed) ──
                let bytes = encode(&ClientPacket::Input { frames: vec![frame] })?;
                let sim_latency_ms = shared.debug.lock().simulated_latency_ms;

                if sim_latency_ms == 0 {
                    if let Err(e) = socket.send(&bytes).await {
                        warn!("send error: {e}");
                    }
                } else {
                    let send_at = Instant::now()
                        + std::time::Duration::from_millis(sim_latency_ms as u64);
                    delayed_queue.push_back((send_at, bytes));
                }

                last_input_send_time = Instant::now();
                input_sequence += 1;
                tick = tick.wrapping_add(1);
            }
        }
    }

    Ok(())
}

fn encode(packet: &ClientPacket) -> anyhow::Result<Vec<u8>> {
    Ok(bincode::serde::encode_to_vec(
        packet,
        bincode::config::standard(),
    )?)
}

fn decode(data: &[u8]) -> anyhow::Result<ServerPacket> {
    let (p, _) = bincode::serde::decode_from_slice(data, bincode::config::standard())
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    Ok(p)
}
