use std::time::Duration;

/// Server tick rate in Hz.
pub const TICK_RATE: u32 = 128;

/// Duration of a single tick.
pub const TICK_DURATION: Duration = Duration::from_micros(1_000_000 / TICK_RATE as u64);

/// Wrapping tick number. u16 gives ~17 minutes before wrap at 64 Hz,
/// which is fine — sequence comparisons use wrapping arithmetic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TickNum(pub u16);

impl TickNum {
    pub fn next(self) -> Self {
        Self(self.0.wrapping_add(1))
    }

    /// Returns true if `self` is more recent than `other`,
    /// accounting for wrapping (half-space comparison).
    pub fn is_newer_than(self, other: Self) -> bool {
        let diff = self.0.wrapping_sub(other.0);
        diff > 0 && diff < 32768
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tick_wrapping() {
        let t = TickNum(u16::MAX);
        assert_eq!(t.next(), TickNum(0));
    }

    #[test]
    fn tick_ordering() {
        assert!(TickNum(10).is_newer_than(TickNum(5)));
        assert!(!TickNum(5).is_newer_than(TickNum(10)));
        // Wrapping case: 0 is newer than 65530
        assert!(TickNum(0).is_newer_than(TickNum(65530)));
    }
}
