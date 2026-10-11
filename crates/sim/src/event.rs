//! Every kind of event the world runs, and the one fixed priority table (TM2).

use crate::codec::{DecodeError, Reader, Writer};
use crate::hash::StateHasher;
use crate::id::NationId;
use crate::queue::Priority;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// A nation's recurring daily pass; it re-schedules itself one game-day later.
    DailyPass(NationId),
}

impl Event {
    /// The fixed priority table. Values are spaced so later kinds (e.g. combat phases, which
    /// must run before a pass in the same second) can slot in without renumbering.
    #[must_use]
    pub const fn priority(self) -> Priority {
        match self {
            Self::DailyPass(_) => Priority(100),
        }
    }

    pub(crate) fn hash_into(self, h: &mut StateHasher) {
        match self {
            Self::DailyPass(nation) => {
                h.write_u16(0);
                h.write_u32(nation.0);
            }
        }
    }

    /// Snapshot encoding (TS6). Tags are part of the file format: never reuse or renumber one.
    pub(crate) fn encode(self, w: &mut Writer) {
        match self {
            Self::DailyPass(nation) => {
                w.put_u8(0);
                w.put_u32(nation.0);
            }
        }
    }

    pub(crate) fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        match r.u8()? {
            0 => Ok(Self::DailyPass(NationId(r.u32()?))),
            tag => Err(DecodeError::BadTag(tag)),
        }
    }
}
