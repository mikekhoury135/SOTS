use std::net::SocketAddr;

use bytes::Bytes;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum TransportError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Packet too large: {size} bytes (max {max})")]
    PacketTooLarge { size: usize, max: usize },
    #[error("Connection timed out")]
    Timeout,
}

/// Abstraction over the network transport layer.
/// Allows swapping raw UDP for QUIC (quinn) later without touching game logic.
pub trait Transport {
    /// Send a datagram to the given address.
    fn send_to(&self, data: &[u8], addr: SocketAddr) -> Result<usize, TransportError>;

    /// Receive a datagram. Returns the payload and the sender's address.
    fn recv_from(&self, buf: &mut [u8]) -> Result<(usize, SocketAddr), TransportError>;

    /// Local address this transport is bound to.
    fn local_addr(&self) -> Result<SocketAddr, TransportError>;
}

/// A received datagram with its source address.
#[derive(Debug)]
pub struct Datagram {
    pub data: Bytes,
    pub addr: SocketAddr,
}
