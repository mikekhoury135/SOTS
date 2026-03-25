// Phase 3: Input capture.
// This module will own:
// - Keyboard state (WASD, jump, crouch, reload)
// - Mouse delta capture (yaw/pitch)
// - Packing raw input into shared::types::InputFrame
// - Input buffering for jitter tolerance (send multiple frames per packet)
