//! Every kind of event the world runs, and the one fixed priority table (TM2).

use crate::codec::{DecodeError, Reader, Sink};
use crate::id::{ArmyId, NationId};
use crate::queue::Priority;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// A nation's recurring daily pass; it re-schedules itself one game-day later.
    DailyPass(NationId),
    /// An army reaches the end of its current road (MP7). Stale, and ignored, once the army's
    /// generation has moved on: the queue cannot remove events, so re-planning leaves the old
    /// one behind.
    Arrive { army: ArmyId, generation: u32 },
    /// Two armies of different nations meet on a road; both halt (contact stub, no combat yet).
    /// Stale unless both generations still match.
    Meet {
        a: ArmyId,
        b: ArmyId,
        generation_a: u32,
        generation_b: u32,
    },
}

impl Event {
    /// The fixed priority table. Values are spaced so later kinds can slot in without
    /// renumbering. In one second: armies meet before they arrive (meeting at the very end of
    /// a road still halts them), and both come before the daily pass.
    #[must_use]
    pub const fn priority(self) -> Priority {
        match self {
            Self::Meet { .. } => Priority(40),
            Self::Arrive { .. } => Priority(50),
            Self::DailyPass(_) => Priority(100),
        }
    }

    /// Snapshot and hash encoding (TS6, TM9). Tags are part of the file format: never reuse or
    /// renumber one.
    pub(crate) fn write(self, s: &mut impl Sink) {
        match self {
            Self::DailyPass(nation) => {
                s.put_u8(0);
                s.put_u32(nation.0);
            }
            Self::Arrive { army, generation } => {
                s.put_u8(1);
                s.put_u32(army.0);
                s.put_u32(generation);
            }
            Self::Meet {
                a,
                b,
                generation_a,
                generation_b,
            } => {
                s.put_u8(2);
                s.put_u32(a.0);
                s.put_u32(b.0);
                s.put_u32(generation_a);
                s.put_u32(generation_b);
            }
        }
    }

    pub(crate) fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        match r.u8()? {
            0 => Ok(Self::DailyPass(NationId(r.u32()?))),
            1 => Ok(Self::Arrive {
                army: ArmyId(r.u32()?),
                generation: r.u32()?,
            }),
            2 => Ok(Self::Meet {
                a: ArmyId(r.u32()?),
                b: ArmyId(r.u32()?),
                generation_a: r.u32()?,
                generation_b: r.u32()?,
            }),
            tag => Err(DecodeError::BadTag(tag)),
        }
    }
}
