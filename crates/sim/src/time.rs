//! Game time: integer game-seconds since the world started (TM9). Never wall-clock time.

pub const SECONDS_PER_MINUTE: i64 = 60;
pub const SECONDS_PER_HOUR: i64 = 60 * SECONDS_PER_MINUTE;
pub const SECONDS_PER_DAY: i64 = 24 * SECONDS_PER_HOUR;

/// A point on the world's game clock, in game-seconds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GameTime(pub i64);

impl GameTime {
    pub const ZERO: Self = Self(0);

    #[must_use]
    pub const fn seconds(self) -> i64 {
        self.0
    }

    /// `None` on overflow.
    #[must_use]
    pub const fn checked_add_seconds(self, seconds: i64) -> Option<Self> {
        match self.0.checked_add(seconds) {
            Some(t) => Some(Self(t)),
            None => None,
        }
    }

    /// Whole game-days since the start; floors, so negative times land on the earlier day.
    #[must_use]
    pub const fn day(self) -> i64 {
        self.0.div_euclid(SECONDS_PER_DAY)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_day_is_86400_seconds() {
        assert_eq!(SECONDS_PER_DAY, 86_400);
    }

    #[test]
    fn day_floors() {
        assert_eq!(GameTime(0).day(), 0);
        assert_eq!(GameTime(SECONDS_PER_DAY - 1).day(), 0);
        assert_eq!(GameTime(SECONDS_PER_DAY).day(), 1);
        assert_eq!(GameTime(-1).day(), -1);
    }

    #[test]
    fn add_reports_overflow() {
        assert_eq!(GameTime(5).checked_add_seconds(10), Some(GameTime(15)));
        assert_eq!(GameTime(i64::MAX).checked_add_seconds(1), None);
    }
}
