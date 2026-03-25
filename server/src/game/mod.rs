use ahash::AHashMap;
use glam::Vec3;
use hecs::{Entity, World};
use std::net::SocketAddr;
use tracing::info;

use shared::{
    physics,
    protocol::{ClientPacket, ServerPacket},
    tick::{TICK_RATE, TickNum},
    types::{InputFrame, PlayerFlags, PlayerId, PlayerState, QuantizedPosition},
};

// ── ECS components ────────────────────────────────────────────────────────────

pub struct WorldPos(pub Vec3);
pub struct WorldYaw(pub f32); // radians
pub struct PlayerTag(pub PlayerId);

// ── Session ───────────────────────────────────────────────────────────────────

struct Session {
    id: PlayerId,
    entity: Entity,
    pending_input: Option<InputFrame>,
    /// The sequence number of the last InputFrame we processed for this client.
    last_processed_seq: u32,
    last_active_tick: u16,
}

// ── Constants ─────────────────────────────────────────────────────────────────

const TIMEOUT_TICKS: u16 = TICK_RATE as u16 * 10; // 10 s

// ── GameState ─────────────────────────────────────────────────────────────────

pub struct GameState {
    world: World,
    sessions: AHashMap<SocketAddr, Session>,
    addr_by_player: AHashMap<u16, SocketAddr>,
    next_id: u16,
    current_tick: TickNum,
}

impl GameState {
    pub fn new() -> Self {
        Self {
            world: World::new(),
            sessions: AHashMap::new(),
            addr_by_player: AHashMap::new(),
            next_id: 1,
            current_tick: TickNum(0),
        }
    }

    /// Handle one incoming client packet; returns any immediate responses.
    pub fn handle_client_packet(
        &mut self,
        packet: ClientPacket,
        addr: SocketAddr,
    ) -> Vec<ServerPacket> {
        match packet {
            ClientPacket::Connect { token: _ } => {
                if self.sessions.contains_key(&addr) {
                    // Already connected — resend ack (idempotent)
                    let id = self.sessions[&addr].id;
                    return vec![ServerPacket::ConnectAck { player_id: id }];
                }

                let id = PlayerId(self.next_id);
                self.next_id += 1;

                let entity = self
                    .world
                    .spawn((WorldPos(Vec3::ZERO), WorldYaw(0.0), PlayerTag(id)));

                self.sessions.insert(
                    addr,
                    Session {
                        id,
                        entity,
                        pending_input: None,
                        last_processed_seq: 0,
                        last_active_tick: self.current_tick.0,
                    },
                );
                self.addr_by_player.insert(id.0, addr);

                info!("Player {id:?} connected from {addr}");
                vec![ServerPacket::ConnectAck { player_id: id }]
            }

            ClientPacket::Input { frames } => {
                if let Some(session) = self.sessions.get_mut(&addr) {
                    session.last_active_tick = self.current_tick.0;
                    if let Some(frame) = frames.last() {
                        session.pending_input = Some(*frame);
                    }
                }
                vec![]
            }

            ClientPacket::Heartbeat => {
                if let Some(session) = self.sessions.get_mut(&addr) {
                    session.last_active_tick = self.current_tick.0;
                }
                vec![ServerPacket::Heartbeat {
                    server_tick: self.current_tick.0,
                }]
            }

            ClientPacket::Disconnect => {
                self.disconnect(addr);
                vec![]
            }
        }
    }

    /// Advance the world by one tick: apply pending inputs.
    pub fn tick(&mut self) {
        // Collect inputs before mutably borrowing world
        let inputs: Vec<(Entity, InputFrame)> = self
            .sessions
            .values()
            .filter_map(|s| s.pending_input.map(|f| (s.entity, f)))
            .collect();

        for (entity, frame) in &inputs {
            if let Ok((pos, yaw)) = self
                .world
                .query_one_mut::<(&mut WorldPos, &mut WorldYaw)>(*entity)
            {
                physics::apply_input(&mut pos.0, &mut yaw.0, frame);
            }
        }

        // Update last_processed_seq for each session that had input
        for session in self.sessions.values_mut() {
            if let Some(frame) = session.pending_input.take() {
                session.last_processed_seq = frame.sequence;
            }
        }

        self.current_tick = self.current_tick.next();
    }

    /// Build a per-client StateUpdate snapshot (each client gets their own last_processed_input).
    pub fn build_snapshots(&self) -> Vec<(SocketAddr, ServerPacket)> {
        let players: Vec<PlayerState> = self
            .world
            .query::<(&WorldPos, &WorldYaw, &PlayerTag)>()
            .iter()
            .map(|(_, (pos, yaw, tag))| PlayerState {
                id: tag.0,
                position: QuantizedPosition::from_vec3(pos.0),
                yaw: (yaw.0.rem_euclid(std::f32::consts::TAU) / std::f32::consts::TAU * 65536.0)
                    as u16,
                pitch: 0,
                health: 100,
                flags: PlayerFlags::new(),
            })
            .collect();

        self.sessions
            .iter()
            .map(|(addr, session)| {
                let packet = ServerPacket::StateUpdate {
                    server_tick: self.current_tick.0,
                    last_processed_input: session.last_processed_seq,
                    players: players.clone(),
                };
                (*addr, packet)
            })
            .collect()
    }

    /// Disconnect clients that haven't sent anything for TIMEOUT_TICKS.
    pub fn prune_timed_out(&mut self) {
        let now = self.current_tick.0;
        let timed_out: Vec<SocketAddr> = self
            .sessions
            .iter()
            .filter(|(_, s)| now.wrapping_sub(s.last_active_tick) > TIMEOUT_TICKS)
            .map(|(addr, _)| *addr)
            .collect();

        for addr in timed_out {
            info!("Session {addr} timed out");
            self.disconnect(addr);
        }
    }

    fn disconnect(&mut self, addr: SocketAddr) {
        if let Some(session) = self.sessions.remove(&addr) {
            self.addr_by_player.remove(&session.id.0);
            let _ = self.world.despawn(session.entity);
            info!("Player {:?} disconnected", session.id);
        }
    }
}
