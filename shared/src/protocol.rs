use serde::{Deserialize, Serialize};

use crate::types::{InputFrame, PlayerId, PlayerState};

/// Packet header prepended to every UDP datagram.
/// Kept minimal to reduce per-packet overhead.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PacketHeader {
    /// Monotonically increasing sequence number (wrapping u16).
    pub sequence: u16,
    /// Bitfield ACK of the last 32 received remote sequences.
    /// Bit 0 = ack_sequence - 1, Bit 1 = ack_sequence - 2, etc.
    pub ack: u16,
    pub ack_bits: u32,
}

/// Packets sent from client to server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClientPacket {
    /// Initial connection request. Contains a stub auth token.
    Connect { token: u64 },
    /// Client input for one or more ticks (input buffering for jitter tolerance).
    Input { frames: Vec<InputFrame> },
    /// Periodic keepalive.
    Heartbeat,
    /// Graceful disconnect.
    Disconnect,
}

/// Packets sent from server to client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServerPacket {
    /// Connection accepted; assigns the client a player ID.
    ConnectAck { player_id: PlayerId },
    /// Connection rejected with a reason.
    ConnectDenied { reason: String },
    /// Delta snapshot: only players whose state changed since client's last ACKed tick.
    StateUpdate {
        server_tick: u16,
        players: Vec<PlayerState>,
    },
    /// Periodic keepalive / latency probe.
    Heartbeat { server_tick: u16 },
    /// Server is shutting down.
    Shutdown,
}

/// Maximum UDP payload size we will send.
/// Stay well under typical MTU (1500) minus IP (20) and UDP (8) headers.
pub const MAX_PACKET_SIZE: usize = 1400;

/// Game server default port.
pub const DEFAULT_PORT: u16 = 7777;
