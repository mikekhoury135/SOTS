use ahash::AHashMap;
use glam::Vec3;
use hecs::{Entity, World};
use std::net::SocketAddr;
use tracing::info;

use shared::{
    combat::{self, HITSCAN_DAMAGE, MAX_HEALTH, RESPAWN_TICKS},
    physics,
    protocol::{ClientPacket, ServerPacket},
    tick::{TICK_RATE, TickNum},
    types::{InputFrame, PlayerFlags, PlayerId, PlayerState, QuantizedPosition, movement},
};

// ── ECS components ────────────────────────────────────────────────────────────

pub struct WorldPos(pub Vec3);
pub struct WorldYaw(pub f32); // radians
pub struct VerticalVelocity(pub f32); // units/second, positive = up
pub struct PlayerTag(pub PlayerId);
pub struct Health(pub u8);
/// Ticks remaining until respawn. When > 0, the player is dead.
pub struct RespawnTimer(pub u16);

// ── Session ───────────────────────────────────────────────────────────────────

struct Session {
    id: PlayerId,
    entity: Entity,
    pending_input: Option<InputFrame>,
    last_processed_seq: u32,
    last_active_tick: u16,
}

// ── Constants ─────────────────────────────────────────────────────────────────

const TIMEOUT_TICKS: u16 = TICK_RATE as u16 * 10; // 10 s

/// Spawn points — cycle through these for respawning players.
const SPAWN_POINTS: &[Vec3] = &[
    Vec3::new(0.0, 0.0, 0.0),
    Vec3::new(-20.0, 0.0, -20.0),
    Vec3::new(20.0, 0.0, -20.0),
    Vec3::new(-20.0, 0.0, 20.0),
];

// ── GameState ─────────────────────────────────────────────────────────────────

pub struct GameState {
    world: World,
    sessions: AHashMap<SocketAddr, Session>,
    addr_by_player: AHashMap<u16, SocketAddr>,
    next_id: u16,
    current_tick: TickNum,
    spawn_index: usize,
}

impl GameState {
    pub fn new() -> Self {
        Self {
            world: World::new(),
            sessions: AHashMap::new(),
            addr_by_player: AHashMap::new(),
            next_id: 1,
            current_tick: TickNum(0),
            spawn_index: 0,
        }
    }

    fn next_spawn(&mut self) -> Vec3 {
        let pos = SPAWN_POINTS[self.spawn_index % SPAWN_POINTS.len()];
        self.spawn_index += 1;
        pos
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
                    let id = self.sessions[&addr].id;
                    return vec![ServerPacket::ConnectAck { player_id: id }];
                }

                let id = PlayerId(self.next_id);
                self.next_id += 1;

                let spawn = self.next_spawn();
                let entity = self.world.spawn((
                    WorldPos(spawn),
                    WorldYaw(0.0),
                    VerticalVelocity(0.0),
                    PlayerTag(id),
                    Health(MAX_HEALTH),
                    RespawnTimer(0),
                ));

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

    /// Advance the world by one tick: apply pending inputs, process combat, handle respawns.
    pub fn tick(&mut self) {
        // ── Respawn timers ───────────────────────────────────────────────
        let mut respawns: Vec<Entity> = Vec::new();
        for (entity, (timer, _)) in self.world.query_mut::<(&mut RespawnTimer, &Health)>() {
            if timer.0 > 0 {
                timer.0 -= 1;
                if timer.0 == 0 {
                    respawns.push(entity);
                }
            }
        }

        // Actually respawn (set health, move to spawn point)
        for entity in respawns {
            if let Ok((pos, vy, health, timer)) = self.world.query_one_mut::<(
                &mut WorldPos,
                &mut VerticalVelocity,
                &mut Health,
                &mut RespawnTimer,
            )>(entity)
            {
                let spawn = SPAWN_POINTS[self.spawn_index % SPAWN_POINTS.len()];
                self.spawn_index += 1;
                pos.0 = spawn;
                vy.0 = 0.0;
                health.0 = MAX_HEALTH;
                timer.0 = 0;
            }
        }

        // ── Movement ─────────────────────────────────────────────────────
        let inputs: Vec<(Entity, InputFrame)> = self
            .sessions
            .values()
            .filter_map(|s| s.pending_input.map(|f| (s.entity, f)))
            .collect();

        for (entity, frame) in &inputs {
            // Only move if alive
            if let Ok((_, health, timer)) = self
                .world
                .query_one_mut::<(&WorldPos, &Health, &RespawnTimer)>(*entity)
                && (health.0 == 0 || timer.0 > 0)
            {
                continue;
            }

            if let Ok((pos, yaw, vy)) =
                self.world
                    .query_one_mut::<(&mut WorldPos, &mut WorldYaw, &mut VerticalVelocity)>(*entity)
            {
                physics::apply_input(&mut pos.0, &mut yaw.0, &mut vy.0, frame);
            }
        }

        // ── Shooting (hitscan) ───────────────────────────────────────────
        // Collect shooters first, then resolve hits
        let shooters: Vec<(Entity, Vec3, f32)> = inputs
            .iter()
            .filter(|(_, frame)| frame.movement & movement::SHOOT != 0)
            .filter_map(|(entity, _)| {
                self.world
                    .query_one_mut::<(&WorldPos, &WorldYaw, &Health, &RespawnTimer)>(*entity)
                    .ok()
                    .filter(|(_, _, h, t)| h.0 > 0 && t.0 == 0)
                    .map(|(pos, yaw, _, _)| (*entity, pos.0, yaw.0))
            })
            .collect();

        // Build target list: all alive players with their positions
        let alive_players: Vec<(Vec3, Entity)> = self
            .world
            .query::<(&WorldPos, &Health, &RespawnTimer)>()
            .iter()
            .filter(|(_, (_, h, t))| h.0 > 0 && t.0 == 0)
            .map(|(entity, (pos, _, _))| (pos.0, entity))
            .collect();

        for (shooter_entity, origin, yaw) in &shooters {
            let (sin_y, cos_y) = yaw.sin_cos();
            let direction = Vec3::new(sin_y, 0.0, -cos_y); // matches physics::apply_input forward

            // Build targets excluding the shooter
            let targets: Vec<(Vec3, usize)> = alive_players
                .iter()
                .enumerate()
                .filter(|(_, (_, e))| e != shooter_entity)
                .map(|(i, (pos, _))| (*pos, i))
                .collect();

            let (hit_idx, _result) = combat::hitscan(*origin, direction, &targets);

            if let Some(target_idx) = hit_idx {
                // Map target_idx back to entity
                let hit_entity = alive_players
                    .iter()
                    .enumerate()
                    .filter(|(_, (_, e))| e != shooter_entity)
                    .nth(target_idx)
                    .map(|(_, (_, e))| *e);

                #[allow(clippy::collapsible_if)]
                if let Some(entity) = hit_entity {
                    if let Ok((health, timer)) = self
                        .world
                        .query_one_mut::<(&mut Health, &mut RespawnTimer)>(entity)
                    {
                        health.0 = health.0.saturating_sub(HITSCAN_DAMAGE);
                        if health.0 == 0 {
                            timer.0 = RESPAWN_TICKS;
                            if let Ok(tag) = self.world.query_one_mut::<&PlayerTag>(entity) {
                                info!("Player {:?} was killed", tag.0);
                            }
                        }
                    }
                }
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

    /// Build a per-client StateUpdate snapshot.
    pub fn build_snapshots(&self) -> Vec<(SocketAddr, ServerPacket)> {
        let players: Vec<PlayerState> = self
            .world
            .query::<(&WorldPos, &WorldYaw, &PlayerTag, &Health, &RespawnTimer)>()
            .iter()
            .map(|(_, (pos, yaw, tag, health, timer))| {
                let mut flags = PlayerFlags::new();
                if health.0 == 0 || timer.0 > 0 {
                    // Clear ALIVE flag
                    flags.0 &= !PlayerFlags::ALIVE;
                }
                PlayerState {
                    id: tag.0,
                    position: QuantizedPosition::from_vec3(pos.0),
                    yaw: (yaw.0.rem_euclid(std::f32::consts::TAU) / std::f32::consts::TAU * 65536.0)
                        as u16,
                    pitch: 0,
                    health: health.0,
                    flags,
                }
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
