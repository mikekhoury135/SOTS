// Phase 1: UDP socket I/O, session table, reliability layer.
// This module will own:
// - Raw UDP recv thread(s) using socket2
// - Session table (AHashMap<SocketAddr, Session>)
// - Packet encode/decode
// - Outbound send queue
