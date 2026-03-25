// Phase 2: Authoritative game loop and ECS world.
// This module will own:
// - Fixed-timestep tick loop (64 Hz via spin_sleep)
// - hecs::World with player entities
// - Input application, movement, hit detection
// - State snapshot generation (delta compressed)
