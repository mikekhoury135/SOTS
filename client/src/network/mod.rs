// Phase 1: Client-side UDP networking.
// This module will own:
// - Async UDP socket (tokio) for send/recv
// - Packet encode/decode using shared::protocol
// - Input send queue (InputFrame → server)
// - State receive buffer (StateUpdate from server)
// - Prediction buffer for reconciliation
